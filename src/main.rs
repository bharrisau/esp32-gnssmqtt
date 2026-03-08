//! esp32-gnssmqtt firmware entry point.
//!
//! Initialization order is MANDATORY:
//! 1. esp_idf_svc::sys::link_patches() — MUST be first, applies linker patches
//! 2. log_relay::MqttLogger::initialize() — MUST be before any log:: calls
//! 3. Peripherals::take() — take hardware ownership
//!    3b-3e) LED state Arc + GPIO15 PinDriver + LED thread spawn
//! 4. EspSystemEventLoop::take() — required by WiFi
//! 5. EspDefaultNvsPartition::take() — required by WiFi
//! 6. wifi::wifi_connect — WiFi BEFORE MQTT (IP required for TCP)
//! 7. gnss::spawn_gnss — GNSS pipeline (UART owner, RX + TX threads)
//!    uart_bridge::spawn_bridge — stdin bridge → GNSS TX channel
//! 8. Create subscribe_tx/rx, config_tx/rx, ota_tx/rx channels
//! 9. mqtt::mqtt_connect — MQTT AFTER WiFi (TCP must be up); callback handles events inline
//!    9.5) log_relay::spawn_log_relay — activates MQTT log forwarding (LOG-01)
//!    9.6) Log level relay thread — applies /log/level runtime changes (LOG-02)
//! 10. Spawn subscriber thread (subscribes on Connected signal)
//! 11. Spawn heartbeat thread
//! 12. Spawn wifi supervisor thread
//! 13. NMEA relay: spawn_relay(mqtt_client clone, device_id clone, nmea_rx)
//! 14. Config relay: spawn_config_relay(gnss_cmd_tx clone, config_rx)
//! 15. RTCM relay: rtcm_relay::spawn_relay(mqtt_client clone, device_id clone, rtcm_rx)
//!
//!    15b) mark_running_slot_valid() — called after mqtt_connect, before relay threads
//!
//! 16. OTA task: spawn_ota(mqtt_client clone, device_id clone, ota_rx, nvs clone)
//!
//!    17b) NTRIP client: spawn_ntrip_client(uart_arc clone, ntrip_config_rx, nvs clone)
//!
//! 17. Watchdog supervisor (spawned last of critical threads)
//! 18. GPIO9 monitor thread — hold 3s triggers SoftAP re-entry (PROV-06)

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;

mod config;
mod device_id;
mod gnss;
mod gnss_state;
mod led;
mod log_relay;
mod mqtt;
mod ntrip_client;
mod config_relay;
mod nmea_relay;
mod rtcm_relay;
mod ota;
mod provisioning;
mod uart_bridge;
mod resil;
mod watchdog;
mod wifi;

fn main() {
    // Step 1: Apply ESP-IDF linker patches — MUST be called before anything else.
    // Omitting this causes a hard fault at boot.
    esp_idf_svc::sys::link_patches();

    // Step 2: Install composite logger — MUST be before any log:: calls.
    // MqttLogger wraps EspLogger for UART output and also forwards to the MQTT log channel
    // once spawn_log_relay initializes LOG_TX. Rust log:: calls bypass esp_log_vprintf_func
    // entirely (EspLogger writes directly to newlib stdout), so the vprintf hook alone is
    // insufficient for Rust module logs — this composite logger is required.
    log_relay::MqttLogger::initialize();

    // Step 2b: Install vprintf hook — captures C component logs (wifi, tcp/ip, etc.) that
    // go through esp_log_write → esp_log_vprintf_func. MUST be after MqttLogger::initialize()
    // so the original vprintf is already set. Complements MqttLogger (which covers Rust logs).
    // Early log messages before MQTT connects are silently dropped (LOG_TX not yet initialized).
    extern "C" {
        fn install_mqtt_log_hook();
    }
    unsafe { install_mqtt_log_hook(); }

    let device_id = device_id::get();
    log::info!("=== esp32-gnssmqtt booting ===");
    log::info!("esp32-gnssmqtt v2.0-ota-canary — OTA validation build");
    log::info!("Device ID: {}", device_id);
    log::info!("Build: {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // Step 3: Take hardware peripherals
    let peripherals = Peripherals::take().expect("peripherals already taken");

    // Step 3b: Create shared LED state — initial state is Connecting
    let led_state = Arc::new(AtomicU8::new(led::LedState::Connecting as u8));

    // Step 3c: Create GPIO15 output driver for LED (active-low: set_low = LED ON)
    let led_pin = PinDriver::output(peripherals.pins.gpio15)
        .expect("GPIO15 PinDriver init failed");

    // Step 3d: Clone led_state for wifi and mqtt threads
    let led_state_wifi = led_state.clone();
    let led_state_mqtt = led_state.clone();
    // led_state itself will be moved into led_task below

    // Step 3e: Spawn LED thread — must be before wifi/mqtt threads start writing state
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || led::led_task(led_pin, led_state))
        .expect("LED thread spawn failed");
    log::info!("LED task started");

    // Step 4: System event loop (required by WiFi)
    let sysloop = EspSystemEventLoop::take().expect("sysloop already taken");

    // Step 5: NVS partition (required by WiFi)
    let nvs = EspDefaultNvsPartition::take().expect("NVS already taken");

    // Step 5b: Boot-path decision — SoftAP vs STA.
    // Clone nvs before any function that might consume it. EspNvsPartition<NvsDefault> is Clone.
    let force_softap = provisioning::check_and_clear_force_softap(&nvs);
    let has_credentials = provisioning::has_wifi_credentials(&nvs);
    log::info!("Boot: force_softap={}, has_credentials={}", force_softap, has_credentials);

    // Step 6: WiFi — SoftAP if no credentials or force_softap flag; STA otherwise.
    let wifi = if force_softap || !has_credentials {
        log::info!("Entering SoftAP provisioning mode...");
        // Signal SoftAP LED pattern (PROV-08) before blocking in portal.
        // led_state_wifi is still in scope here (not yet moved into wifi_supervisor thread).
        led_state_wifi.store(crate::led::LedState::SoftAP as u8, std::sync::atomic::Ordering::Relaxed);
        let mut softap_wifi = esp_idf_svc::wifi::BlockingWifi::wrap(
            esp_idf_svc::wifi::EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))
                .expect("EspWifi new failed in SoftAP path"),
            sysloop.clone(),
        ).expect("BlockingWifi wrap failed");
        // run_softap_portal never returns: calls esp_restart() on form submit or 300s timeout.
        provisioning::run_softap_portal(&mut softap_wifi, nvs.clone())
            .expect("SoftAP portal error");
        unreachable!("run_softap_portal always restarts via esp_restart()")
    } else {
        let networks = provisioning::load_wifi_networks(&nvs);
        log::info!("Connecting to WiFi ({} stored network(s))...", networks.len());
        wifi::wifi_connect_any(peripherals.modem, sysloop.clone(), nvs.clone(), networks)
            .expect("WiFi connect failed")
    };
    log::info!("WiFi connected");

    // Load MQTT config from NVS; fall back to compile-time constants if not provisioned.
    let (mqtt_host, mqtt_port, mqtt_user_str, mqtt_pass_str) =
        provisioning::load_mqtt_config(&nvs).unwrap_or_else(|| {
            log::warn!("No MQTT config in NVS — using compile-time defaults");
            (
                crate::config::MQTT_HOST.to_string(),
                crate::config::MQTT_PORT,
                crate::config::MQTT_USER.to_string(),
                crate::config::MQTT_PASS.to_string(),
            )
        });
    log::info!("MQTT config: {}:{}", mqtt_host, mqtt_port);

    // Step 6.5: SNTP — start background time sync after WiFi is up.
    // _sntp MUST remain in main() scope for the firmware lifetime.
    // Dropping it calls sntp_stop(), reverting timestamps to boot-relative ms.
    // new_default() returns immediately; first NTP response arrives within 1-5s.
    let _sntp = sntp::EspSntp::new_default().expect("SNTP init failed");
    log::info!("SNTP initialized — wall-clock time will sync in background");

    // UM980 reboot detection: GNSS RX thread signals when '$devicename' banner seen.
    // Bounded to 1: one pending re-apply is enough (extra signals coalesce).
    let (um980_reboot_tx, um980_reboot_rx) = std::sync::mpsc::sync_channel::<()>(1);

    // Step 7: GNSS pipeline — exclusive UART ownership, RX + TX threads
    // spawn_gnss returns (cmd_tx, nmea_rx, rtcm_rx, free_pool_tx, uart_arc);
    // uart_arc passed to spawn_ntrip_client (Step 17b) for direct RTCM byte writes.
    let (gnss_cmd_tx, nmea_rx, rtcm_rx, free_pool_tx, uart_arc) = gnss::spawn_gnss(
        peripherals.uart0,
        peripherals.pins.gpio16,  // TX line to UM980
        peripherals.pins.gpio17,  // RX line from UM980
        um980_reboot_tx,
    )
    .expect("GNSS init failed");
    log::info!("GNSS pipeline started");

    // stdin bridge: forwards typed commands to UM980 via GNSS TX channel
    uart_bridge::spawn_bridge(gnss_cmd_tx.clone())
        .expect("UART bridge init failed");
    log::info!("UART bridge started");

    // Step 8: Channels — created before mqtt_connect so they can be passed into the callback.
    // subscribe signal — callback → subscriber
    // Bounded to 2: at most one Connected event queued while subscriber processes the previous one.
    let (subscribe_tx, subscribe_rx) = std::sync::mpsc::sync_channel::<()>(2);

    // status signal — callback → heartbeat (publishes retained "online" on every reconnect)
    // Bounded to 2: same reasoning as subscribe channel.
    let (status_tx, status_rx) = std::sync::mpsc::sync_channel::<()>(2);

    // config payload — callback → config_relay
    // Bounded to 4: config is operator-triggered (rare). 4 covers a retained message on reconnect
    // plus a small burst. callback uses try_send() so it never blocks.
    let (config_tx, config_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);

    // OTA trigger — callback → OTA task
    // Bounded to 1: at most one OTA operation can be queued. A second trigger while OTA is running
    // is dropped (callback uses try_send). Prevents double-flash from re-delivered retained triggers.
    let (ota_tx, ota_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);

    // command relay — callback → command_relay_task
    // Bounded to 4: operator-triggered and rare; try_send() in callback never blocks.
    // CMD-02: command_relay_task performs no deduplication by design.
    let (cmd_relay_tx, cmd_relay_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);

    // log level config — callback → log_level_relay_task
    // Bounded to 4: operator-triggered and rare; retained level message on reconnect + burst.
    let (log_level_tx, log_level_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);

    // ntrip config — callback → ntrip_client thread
    // Bounded to 4: operator-triggered; retained message on reconnect + burst.
    let (ntrip_config_tx, ntrip_config_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);

    // Step 9: MQTT — after WiFi (IP must be up). Event dispatch runs in the ESP-IDF C MQTT
    // task thread via callback; no blocking pump thread needed.
    log::info!("Connecting to MQTT broker...");
    let mqtt_client = mqtt::mqtt_connect(
        &device_id,
        &mqtt_host,
        mqtt_port,
        &mqtt_user_str,
        &mqtt_pass_str,
        subscribe_tx, status_tx, config_tx, ota_tx, cmd_relay_tx, log_level_tx,
        ntrip_config_tx,   // NTRIP-02
        led_state_mqtt,
    ).expect("MQTT connect failed");
    log::info!("MQTT client created");

    // Step 9.5: Log relay — forwards captured ESP-IDF log lines to MQTT gnss/{id}/log.
    // Must be after mqtt_connect so the client Arc exists. LOG_TX is initialized inside
    // spawn_log_relay; early log messages before this point are silently dropped (by design).
    log_relay::spawn_log_relay(mqtt_client.clone(), device_id.clone())
        .expect("log relay spawn failed");
    log::info!("Log relay started");

    // Step 9.6: Log level relay — applies runtime log level changes from MQTT /log/level topic.
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || mqtt::log_level_relay_task(log_level_rx))
        .expect("log level relay spawn failed");
    log::info!("Log level relay started");

    // Mark running slot valid — confirms this firmware is functional.
    // MUST be called after WiFi+MQTT confirms connectivity, before spawning threads.
    // Safe to call unconditionally: no-op when slot is already VALID (normal boots).
    // On first boot after OTA, slot is PENDING_VERIFY — this call cancels rollback.
    // EspOta must be dropped (block scope) before OTA thread calls EspOta::new() later.
    {
        match esp_idf_svc::ota::EspOta::new() {
            Ok(mut ota_marker) => match ota_marker.mark_running_slot_valid() {
                Ok(()) => log::info!("Running slot marked valid"),
                Err(e) => log::warn!("mark_running_slot_valid skipped: {} (no OTA partition or factory boot)", e),
            },
            Err(e) => log::warn!("EspOta::new() failed: {} (skip mark_valid)", e),
        }
    }

    // Step 10: Subscriber thread — subscribes on Connected (initial + broker restart)
    let sub_client = mqtt_client.clone();
    let sub_device_id = device_id.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::subscriber_loop(sub_client, sub_device_id, subscribe_rx))
        .expect("subscriber thread spawn failed");

    // Step 12: Heartbeat thread
    let hb_client = mqtt_client.clone();
    let hb_device_id = device_id.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::heartbeat_loop(hb_client, hb_device_id, status_rx))
        .expect("heartbeat thread spawn failed");

    // Step 13: WiFi supervisor thread (reconnect on drop)
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || wifi::wifi_supervisor(wifi, led_state_wifi))
        .expect("wifi supervisor spawn failed");

    // Step 14: NMEA relay — consumes nmea_rx, publishes each sentence to MQTT.
    // nmea_rx is moved into spawn_relay — do NOT retain a reference here.
    // gnss_cmd_tx is kept alive here; Phase 6 will clone it for MQTT config forwarding.
    nmea_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), nmea_rx)
        .expect("NMEA relay thread spawn failed");
    log::info!("NMEA relay started");

    // Step 15: Config relay — receives MQTT config payloads from pump, forwards to UM980 via gnss_cmd_tx.
    // gnss_cmd_tx.clone() is passed here; the original is kept alive in the idle loop below.
    config_relay::spawn_config_relay(gnss_cmd_tx.clone(), config_rx)
        .expect("Config relay thread spawn failed");
    log::info!("Config relay started");

    // UM980 reboot monitor: re-applies startup configuration when UM980 resets.
    // Listens on um980_reboot_rx; on signal, waits 500ms then logs a prominent warning.
    // Full automatic re-apply requires NVS-backed UM980 config storage (not yet implemented);
    // the detection and signal path is the primary value of this feature.
    {
        std::thread::Builder::new()
            .stack_size(4096)
            .spawn(move || {
                let hwm_words = unsafe {
                    esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
                };
                log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                    "UM980 reboot monitor", hwm_words, hwm_words * 4);
                loop {
                    match um980_reboot_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
                        Ok(()) => {
                            log::warn!("UM980 reboot monitor: reboot detected — waiting 500ms before action");
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            // Automatic config re-apply requires NVS-backed UM980 config storage.
                            // Until that is implemented, direct the operator to re-send config via MQTT.
                            log::warn!("UM980 rebooted — automatic config re-apply not yet implemented; \
                                re-send UM980 config via MQTT /config topic to restore configuration");
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            log::error!("UM980 reboot monitor: channel closed — thread parking");
                            loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
                        }
                    }
                }
            })
            .expect("UM980 reboot monitor spawn failed");
    }
    log::info!("UM980 reboot monitor started");

    // Step 15b: Command relay — forwards MQTT /command payloads to UM980.
    let cmd_gnss_tx = gnss_cmd_tx.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::command_relay_task(cmd_gnss_tx, cmd_relay_rx))
        .expect("Command relay thread spawn failed");
    log::info!("Command relay started");

    // Step 16: RTCM relay — receives verified RTCM3 frames from gnss RX thread, publishes to MQTT.
    // rtcm_rx and free_pool_tx are moved into spawn_relay — do NOT retain references here.
    rtcm_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), rtcm_rx, free_pool_tx)
        .expect("RTCM relay thread spawn failed");
    log::info!("RTCM relay started");

    // Step 17: OTA task — receives /ota/trigger payloads, performs HTTP download + flash + reboot.
    // ota_rx is moved into spawn_ota — do NOT retain a reference here.
    // nvs clone passed for PROV-07: "softap" payload triggers set_force_softap + restart.
    ota::spawn_ota(mqtt_client.clone(), device_id.clone(), ota_rx, nvs.clone())
        .expect("OTA task spawn failed");
    log::info!("OTA task started");

    // Step 17b: NTRIP client — streams RTCM3 corrections from configured caster to UM980 UART (NTRIP-01..04)
    ntrip_client::spawn_ntrip_client(Arc::clone(&uart_arc), ntrip_config_rx, nvs.clone())
        .expect("NTRIP client spawn failed");
    log::info!("NTRIP client started");

    // Step 18: Watchdog supervisor — spawned last so all critical threads are running before monitoring begins.
    // Detects silent hangs in GNSS RX and MQTT pump threads; calls esp_restart() after 3 missed beats (15s).
    watchdog::spawn_supervisor()
        .expect("watchdog supervisor spawn failed");
    log::info!("Watchdog supervisor started");

    // Step 19: GPIO9 monitor — spawned after all other subsystems so the device is fully operational.
    // GPIO9 is the BOOT button on XIAO ESP32-C6; safe to use as GPIO input after firmware starts.
    // WARNING: do not hold GPIO9 during hardware reset — that enters serial download mode.
    // Holding GPIO9 low for 3 continuous seconds triggers SoftAP re-entry (PROV-06).
    let gpio9_pin = peripherals.pins.gpio9;
    let nvs_for_gpio = nvs.clone();
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || {
            use esp_idf_svc::hal::gpio::{PinDriver, Pull};

            // HWM at thread entry: confirms stack size is adequate.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] GPIO9 monitor: {} words ({} bytes) stack remaining",
                hwm_words, hwm_words * 4);

            let mut pin = match PinDriver::input(gpio9_pin) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("GPIO9 PinDriver init failed: {:?}", e);
                    return;
                }
            };
            if let Err(e) = pin.set_pull(Pull::Up) {
                log::error!("GPIO9 pull-up failed: {:?}", e);
                return;
            }

            let mut low_since: Option<std::time::Instant> = None;

            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));

                if pin.is_low() {
                    let since = low_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() >= std::time::Duration::from_secs(3) {
                        log::info!("GPIO9: held low 3s — entering SoftAP mode (PROV-06)");
                        crate::provisioning::set_force_softap(&nvs_for_gpio);
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        unsafe { esp_idf_svc::sys::esp_restart(); }
                    }
                } else {
                    low_since = None; // reset timer on release
                }
            }
        })
        .expect("GPIO9 monitor thread spawn failed");
    log::info!("GPIO9 monitor started");

    log::info!("All subsystems started — device operational");
    let _gnss_cmd_tx = gnss_cmd_tx; // keep Sender alive — TX thread exits if all Senders drop
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
