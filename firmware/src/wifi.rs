//! WiFi connection and reconnect supervisor. Uses BlockingWifi<EspWifi> from esp-idf-svc.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use embedded_svc::wifi::AuthMethod;

use crate::led::LedState;

/// Connect to the configured WiFi network.
///
/// Consumes the modem peripheral, wraps it in BlockingWifi, sets client configuration,
/// and performs the full start → connect → wait_netif_up sequence.
///
/// The returned `BlockingWifi` handle must be kept alive (e.g. passed to `wifi_supervisor`)
/// for the WiFi driver to remain active.
#[allow(dead_code)]
pub fn wifi_connect(
    modem: impl esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: crate::config::WIFI_SSID.try_into().unwrap(),
        password: crate::config::WIFI_PASS.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    log::info!("WiFi connected to SSID: {}", crate::config::WIFI_SSID);

    Ok(wifi)
}

/// Connect to the first available network from a list of stored credentials (PROV-05).
///
/// Cycles through all networks up to 3 times before giving up.
/// For each attempt: stop → reconfigure → start → connect → wait_netif_up.
/// stop() + start() before each reconfigure is required for reliable network switching.
///
/// PROV-05: Does NOT enter SoftAP mode on failure. The RESIL-01 reboot timer in
/// wifi_supervisor handles sustained WiFi failure recovery.
pub fn wifi_connect_any(
    modem: impl esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    networks: Vec<(String, String)>,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;

    if networks.is_empty() {
        anyhow::bail!("wifi_connect_any: no networks provided");
    }

    let max_attempts = networks.len() * 3;
    for (attempt, (ssid, pass)) in networks.iter().cycle().take(max_attempts).enumerate() {
        log::info!(
            "WiFi: attempt {}/{} — SSID '{}'",
            attempt + 1,
            max_attempts,
            ssid
        );

        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid.as_str().try_into().unwrap_or_default(),
            password: pass.as_str().try_into().unwrap_or_default(),
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        }))?;

        // start() required before connect(); stop() required before next reconfigure.
        let _ = wifi.start();

        match wifi.connect() {
            Ok(_) => match wifi.wait_netif_up() {
                Ok(_) => {
                    log::info!("WiFi connected to '{}'", ssid);
                    return Ok(wifi);
                }
                Err(e) => log::warn!("WiFi netif_up failed for '{}': {:?}", ssid, e),
            },
            Err(e) => log::warn!("WiFi connect failed for '{}': {:?}", ssid, e),
        }

        let _ = wifi.stop();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    anyhow::bail!("wifi_connect_any: all {} attempts failed", max_attempts)
}

/// WiFi reconnect supervisor.
///
/// Runs forever in a dedicated thread. Polls connection state every 5 seconds.
/// On disconnect, writes LedState::Connecting, waits `backoff_secs`, calls `wifi.connect()`,
/// and on success resets the backoff and error counters.  On failure, doubles the backoff
/// (capped at 60 seconds).  After 3 consecutive failures at maximum backoff, writes
/// LedState::Error.
///
/// Note: `wifi.start()` is NOT called here — the driver is already started by
/// `wifi_connect`. Only `wifi.connect()` is retried on reconnect.
///
/// Note: LedState::Connected is NOT written here — that is the MQTT pump's responsibility.
/// WiFi up != MQTT up; writing Connected here would show a false green before MQTT is ready.
pub fn wifi_supervisor(mut wifi: BlockingWifi<EspWifi<'static>>, led_state: Arc<AtomicU8>) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "WiFi sup", hwm_words, hwm_words * 4);
    let mut backoff_secs: u64 = 1;
    let mut consecutive_failures: u32 = 0;
    let mut disconnected_since: Option<std::time::Instant> = None;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));

        let connected = wifi.is_connected().unwrap_or(false);

        if !connected {
            // RESIL-01: reboot if WiFi has been down for too long.
            let since = disconnected_since.get_or_insert_with(std::time::Instant::now);
            let elapsed = since.elapsed();
            if elapsed >= crate::config::WIFI_DISCONNECT_REBOOT_TIMEOUT {
                log::error!("[RESIL-01] WiFi disconnected for {}s — rebooting",
                    elapsed.as_secs());
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }

            // RESIL-02: clear MQTT disconnect timer while WiFi is down.
            // Prevents combined-outage false trigger (Pitfall 2 in research).
            crate::resil::MQTT_DISCONNECTED_AT.store(0, std::sync::atomic::Ordering::Relaxed);

            log::warn!(
                "WiFi disconnected. Reconnecting in {}s (attempt {}/{})...",
                backoff_secs,
                consecutive_failures + 1,
                crate::config::MAX_WIFI_RECONNECT_ATTEMPTS
            );

            // Signal "working on reconnect" immediately before the backoff sleep.
            led_state.store(LedState::Connecting as u8, Ordering::Relaxed);

            std::thread::sleep(std::time::Duration::from_secs(backoff_secs));

            let reconnect_ok = match wifi.connect() {
                Err(e) => {
                    log::error!("WiFi reconnect failed: {:?}", e);
                    false
                }
                Ok(_) => match wifi.wait_netif_up() {
                    Ok(_) => true,
                    Err(e) => {
                        log::error!("WiFi netif up failed after reconnect: {:?}", e);
                        false
                    }
                }
            };

            if reconnect_ok {
                log::info!("WiFi reconnected");
                backoff_secs = 1;
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
                if consecutive_failures >= crate::config::MAX_WIFI_RECONNECT_ATTEMPTS {
                    log::error!(
                        "WiFi: {} consecutive reconnect failures — LedState::Error set",
                        consecutive_failures
                    );
                    led_state.store(LedState::Error as u8, Ordering::Relaxed);
                }
                backoff_secs = (backoff_secs * 2).min(60);
            }
        } else {
            // WiFi is connected: clear disconnect timer and check MQTT resilience.
            disconnected_since = None;
            // RESIL-02: check if MQTT has been disconnected too long while WiFi is up.
            // Only evaluate here (WiFi-connected arm) to avoid counting MQTT-down-during-WiFi-outage.
            let mqtt_disc_at = crate::resil::MQTT_DISCONNECTED_AT.load(std::sync::atomic::Ordering::Relaxed);
            if mqtt_disc_at != 0 {
                let elapsed_secs = crate::resil::now_secs().saturating_sub(mqtt_disc_at);
                if elapsed_secs >= crate::config::MQTT_DISCONNECT_REBOOT_SECS {
                    log::error!("[RESIL-02] MQTT disconnected for {}s (WiFi up) — rebooting",
                        elapsed_secs);
                    unsafe { esp_idf_svc::sys::esp_restart(); }
                }
            }
        }
    }
}
