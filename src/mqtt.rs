//! MQTT client, LWT, connection pump, and heartbeat. Uses EspMqttClient from esp-idf-svc.

use esp_idf_svc::mqtt::client::{
    EspMqttClient, LwtConfiguration, MqttClientConfiguration,
};
use embedded_svc::mqtt::client::{EventPayload, QoS};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use crate::led::LedState;

/// Create an MQTT client with LWT configured and event dispatch via callback.
///
/// Uses `EspMqttClient::new_cb` so no blocking pump thread is needed — events are
/// dispatched from the ESP-IDF C MQTT task thread directly into the callback closure.
///
/// The callback MUST NOT call any `EspMqttClient` methods: the C MQTT task holds its
/// internal mutex during event dispatch and any re-entrant call will deadlock.
/// Atomic stores and `SyncSender::try_send` are safe from within the callback.
///
/// LWT: publishes `offline` to `gnss/{device_id}/status` with retain=true on unexpected disconnect.
/// MQTT output buffer is set to 2048 bytes to support RTCM MSM7 frames up to 1029 bytes.
#[allow(clippy::too_many_arguments)]
pub fn mqtt_connect(
    device_id: &str,
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    subscribe_tx: SyncSender<()>,
    status_tx: SyncSender<()>,
    config_tx: SyncSender<Vec<u8>>,
    ota_tx: SyncSender<Vec<u8>>,
    cmd_relay_tx: SyncSender<Vec<u8>>,   // NEW — CMD-01
    log_level_tx: SyncSender<Vec<u8>>,  // NEW — LOG-02
    ntrip_config_tx: SyncSender<Vec<u8>>,  // NEW — NTRIP-02
    led_state: Arc<AtomicU8>,
) -> anyhow::Result<Arc<Mutex<EspMqttClient<'static>>>> {
    let broker_url = format!("mqtt://{}:{}", host, port);

    // IMPORTANT: lwt_topic MUST be declared BEFORE conf in the same scope.
    // LwtConfiguration.topic is &'a str — it must outlive the MqttClientConfiguration.
    let lwt_topic = format!("gnss/{}/status", device_id);

    let conf = MqttClientConfiguration {
        client_id: Some(device_id),
        username: if user.is_empty() { None } else { Some(user) },
        password: if pass.is_empty() { None } else { Some(pass) },
        lwt: Some(LwtConfiguration {
            topic: &lwt_topic,
            payload: b"offline",
            qos: QoS::AtLeastOnce,
            retain: true,
        }),
        keep_alive_interval: Some(std::time::Duration::from_secs(10)),
        reconnect_timeout: Some(std::time::Duration::from_secs(5)),
        disable_clean_session: true,
        out_buffer_size: 2048,  // covers 1029-byte RTCM MSM7 frame + MQTT fixed header + topic overhead
        ..Default::default()
    };

    let client = EspMqttClient::new_cb(&broker_url, &conf, move |event| {
        match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("MQTT connected");
                led_state.store(LedState::Connected as u8, Ordering::Relaxed);
                // RESIL-02: clear disconnect timer — MQTT is now connected.
                // Safe from callback: atomic store only — no EspMqttClient methods called.
                crate::resil::MQTT_DISCONNECTED_AT.store(0, std::sync::atomic::Ordering::Relaxed);
                // Try to signal subscriber; if channel is full, subscriber is already queued to
                // re-subscribe on the previous Connected event — this signal can be dropped safely.
                match subscribe_tx.try_send(()) {
                    Ok(_) => {}
                    Err(TrySendError::Full(_)) => {
                        log::warn!("mqtt cb: subscribe signal channel full — subscriber already queued");
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        log::error!("mqtt cb: subscribe channel closed");
                    }
                }
                // Signal heartbeat to publish retained "online" to /status (clears LWT on every reconnect).
                match status_tx.try_send(()) {
                    Ok(_) => {}
                    Err(TrySendError::Full(_)) => {
                        log::warn!("mqtt cb: status signal channel full — online already queued");
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        log::error!("mqtt cb: status channel closed");
                    }
                }
            }
            EventPayload::Disconnected => {
                log::warn!("MQTT disconnected");
                led_state.store(LedState::Connecting as u8, Ordering::Relaxed);
                // RESIL-02: record when MQTT disconnected so wifi_supervisor can measure elapsed time.
                // compare_exchange: only set if currently 0 (not already mid-disconnect tracking).
                // Safe from callback: atomic store only — no EspMqttClient methods called.
                crate::resil::MQTT_DISCONNECTED_AT
                    .compare_exchange(0, crate::resil::now_secs(),
                        std::sync::atomic::Ordering::Relaxed,
                        std::sync::atomic::Ordering::Relaxed)
                    .ok();
            }
            EventPayload::Error(e) => {
                log::error!("MQTT error: {:?}", e);
            }
            EventPayload::Received { topic, data, .. } => {
                // topic: Option<&str> — None on chunked subsequent frames; Some(...) for complete messages.
                // Config and OTA payloads arrive as Details::Complete (single chunk), so topic is always Some here.
                let t = topic.unwrap_or("");
                if t.ends_with("/ntrip/config") {
                    // NTRIP-02: runtime caster config update — checked before /config to prevent
                    // /ntrip/config from being routed to the device config channel.
                    match ntrip_config_tx.try_send(data.to_vec()) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: ntrip config channel full — payload dropped"),
                        Err(TrySendError::Disconnected(_)) => {}
                    }
                } else if t.ends_with("/config") {
                    match config_tx.try_send(data.to_vec()) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: config channel full — payload dropped"),
                        Err(TrySendError::Disconnected(_)) => log::warn!("mqtt cb: config channel closed"),
                    }
                } else if t.ends_with("/ota/trigger") {
                    match ota_tx.try_send(data.to_vec()) {
                        Ok(_) => log::info!("OTA trigger received, payload len={}", data.len()),
                        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: OTA channel full — trigger dropped (OTA in progress?)"),
                        Err(TrySendError::Disconnected(_)) => log::warn!("mqtt cb: OTA channel closed"),
                    }
                } else if t.ends_with("/command") {
                    // CMD-01: forward payload to command relay task.
                    // CMD-02: QoS 0 subscription (see subscriber_loop) prevents retain replay.
                    match cmd_relay_tx.try_send(data.to_vec()) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: command channel full — command dropped"),
                        Err(TrySendError::Disconnected(_)) => log::warn!("mqtt cb: command channel closed"),
                    }
                } else if t.ends_with("/log/level") {
                    // LOG-02: runtime log level change — silently drop if channel full.
                    match log_level_tx.try_send(data.to_vec()) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => {}    // silently drop — level change can wait
                        Err(TrySendError::Disconnected(_)) => {}
                    }
                }
                // All other topics: silently ignored
            }
            EventPayload::Subscribed(_) | EventPayload::Published(_) => {
                // Normal MQTT ACKs for subscribe and QoS>=1 publish operations — not errors.
                // Log at debug level so they are filtered at the default info level.
                // (These fire for every subscribe in subscriber_loop and every retained publish.)
            }
            m => {
                log::warn!("Unhandled MQTT event: {:?}", m);
            }
        }
    })?;

    Ok(Arc::new(Mutex::new(client)))
}

/// Subscribe to the device config and OTA trigger topics on every Connected signal from the pump.
///
/// By the time this thread receives a signal, the pump has already called
/// `connection.next()` again, which releases the C MQTT internal mutex — so
/// `subscribe()` here is safe (no deadlock).
///
/// Subscribes to both /config and /ota/trigger at QoS::AtLeastOnce on each Connected
/// signal. AtLeastOnce for /ota/trigger ensures retained trigger messages are delivered
/// on reconnect (and cleared by the empty-payload publish in ota.rs after successful OTA).
///
/// Handles both initial connection and broker restarts (CONN-04).
pub fn subscriber_loop(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    subscribe_rx: Receiver<()>,
) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "MQTT sub", hwm_words, hwm_words * 4);
    let config_topic = format!("gnss/{}/config", device_id);
    let ota_topic = format!("gnss/{}/ota/trigger", device_id);
    loop {
        match subscribe_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
            Ok(()) => {
                match client.lock() {
                    Err(e) => log::warn!("Subscriber mutex poisoned: {:?}", e),
                    Ok(mut c) => {
                        match c.subscribe(&config_topic, QoS::AtLeastOnce) {
                            Ok(_) => log::info!("Subscribed to {}", config_topic),
                            Err(e) => log::warn!("Subscribe /config failed: {:?}", e),
                        }
                        match c.subscribe(&ota_topic, QoS::AtLeastOnce) {
                            Ok(_) => log::info!("Subscribed to {}", ota_topic),
                            Err(e) => log::warn!("Subscribe /ota/trigger failed: {:?}", e),
                        }
                        let command_topic = format!("gnss/{}/command", device_id);
                        match c.subscribe(&command_topic, QoS::AtMostOnce) {  // QoS 0 — no retain replay (CMD-02)
                            Ok(_) => log::info!("Subscribed to {}", command_topic),
                            Err(e) => log::warn!("Subscribe /command failed: {:?}", e),
                        }
                        // LOG-02: subscribe to runtime log level topic.
                        // AtLeastOnce so a retained level setting persists across broker reconnects.
                        let log_level_topic = format!("gnss/{}/log/level", device_id);
                        match c.subscribe(&log_level_topic, QoS::AtLeastOnce) {
                            Ok(_) => log::info!("Subscribed to {}", log_level_topic),
                            Err(e) => log::warn!("Subscribe /log/level failed: {:?}", e),
                        }
                        // NTRIP-02: subscribe to NTRIP caster config topic.
                        // AtLeastOnce so the retained config is re-delivered on broker reconnect,
                        // ensuring the NTRIP client reconnects with the latest caster settings.
                        let ntrip_config_topic = format!("gnss/{}/ntrip/config", device_id);
                        match c.subscribe(&ntrip_config_topic, QoS::AtLeastOnce) {
                            Ok(_) => log::info!("Subscribed to {}", ntrip_config_topic),
                            Err(e) => log::warn!("Subscribe /ntrip/config failed: {:?}", e),
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // No Connected signal within 30s — normal when MQTT is stable. Continue.
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Subscriber: channel closed — thread exiting");
                break;
            }
        }
    }
    // Dead-end park (pump exited; thread has nothing to do).
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Forward MQTT /command payloads to the UM980 via gnss_cmd_tx.
///
/// CMD-01: each payload is forwarded as one raw UM980 command — no deduplication.
/// CMD-02: no deduplication by design; caller ensures QoS 0 subscription (no retain replay).
/// Uses recv_timeout to stay alive when idle; logs and continues on UTF-8 errors.
pub fn command_relay_task(
    gnss_cmd_tx: SyncSender<String>,
    cmd_relay_rx: Receiver<Vec<u8>>,
) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "CMD relay", hwm_words, hwm_words * 4);
    loop {
        match cmd_relay_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
            Ok(payload) => {
                match std::str::from_utf8(&payload) {
                    Ok(cmd) => {
                        log::info!("Command relay: forwarding {:?}", cmd);
                        if let Err(e) = gnss_cmd_tx.send(cmd.to_string()) {
                            log::error!("Command relay: gnss_cmd_tx send failed: {:?}", e);
                        }
                    }
                    Err(e) => log::warn!("Command relay: payload not valid UTF-8: {:?}", e),
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // No command within 30s — normal when idle. Continue.
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Command relay: channel closed — thread parking");
                loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
            }
        }
    }
}

/// Parse a log level string payload and apply it via EspLogger::set_target_level.
///
/// Accepts "error", "warn", "info", "debug", "verbose" (case-sensitive).
/// Unknown values are logged and ignored. All errors from set_target_level are logged.
///
/// Two filters must be updated together:
///   - esp_idf_svc::log::set_target_level: updates ESP-IDF's C-level tag filter (used by
///     EspLogger::should_log and the vprintf hook path for C component logs).
///   - log::set_max_level: updates Rust's log crate filter. Without this, Rust log:: calls
///     at the old level still reach MqttLogger::log() because the crate checks its own
///     max_level before even calling the logger — esp_log_level_set alone has no effect.
fn apply_log_level(payload: &[u8]) {
    let level_str = match std::str::from_utf8(payload) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    let filter = match level_str {
        "error"   => log::LevelFilter::Error,
        "warn"    => log::LevelFilter::Warn,
        "info"    => log::LevelFilter::Info,
        "debug"   => log::LevelFilter::Debug,
        "verbose" => log::LevelFilter::Trace,
        _ => {
            log::warn!("log level: unknown value {:?}", level_str);
            return;
        }
    };
    if let Err(e) = esp_idf_svc::log::set_target_level("*", filter) {
        log::warn!("log level set failed: {:?}", e);
        return;
    }
    // Sync Rust's own filter — must happen after set_target_level succeeds.
    // Confirmation uses warn! so it remains visible when transitioning to warn level.
    // (At error level the warn confirmation is also suppressed — acceptable.)
    log::set_max_level(filter);
    log::warn!("Log level → {}", level_str);
}

/// Drain the log_level channel and apply level changes via apply_log_level.
///
/// Mirrors the pattern of command_relay_task: recv_timeout loop, HWM at entry,
/// park on channel close. LOG-02: each received payload is applied immediately.
pub fn log_level_relay_task(log_level_rx: Receiver<Vec<u8>>) -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "log level relay", hwm_words, hwm_words * 4);
    loop {
        match log_level_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
            Ok(payload) => apply_log_level(&payload),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("log_level_relay: channel closed — thread parking");
                loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
            }
        }
    }
}

/// Publish health telemetry to `gnss/{device_id}/heartbeat` every HEARTBEAT_INTERVAL_SECS.
///
/// On startup (after 5s initial delay), publishes a retained "online" to /status to clear
/// the LWT "offline" retained message left by a previous disconnect.
///
/// Then on every tick, collects system metrics (uptime, heap, drop counters) and publishes
/// a JSON health snapshot to /heartbeat with retain=false.
///
/// Uses `enqueue()` (non-blocking enqueue to MQTT outbox) — acceptable here because this
/// runs in its own dedicated thread and the pump thread keeps the outbox moving.
pub fn heartbeat_loop(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    status_rx: Receiver<()>,
) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "MQTT hb", hwm_words, hwm_words * 4);

    let heartbeat_topic = format!("gnss/{}/heartbeat", device_id);
    let status_topic = format!("gnss/{}/status", device_id);
    log::info!("Heartbeat thread started, heartbeat topic: {}, status topic: {}",
        heartbeat_topic, status_topic);

    // Use recv_timeout to wake on either a Connected event or the heartbeat interval.
    // Connected event → publish retained "online" to /status (clears LWT on every reconnect).
    // Timeout → publish JSON health snapshot to /heartbeat.
    loop {
        match status_rx.recv_timeout(std::time::Duration::from_secs(crate::config::HEARTBEAT_INTERVAL_SECS)) {
            Ok(()) => {
                // MQTT (re)connected — publish retained "online" to clear the LWT "offline" message.
                match client.lock() {
                    Err(e) => log::warn!("Heartbeat: status mutex poisoned: {:?}", e),
                    Ok(mut c) => match c.enqueue(&status_topic, QoS::AtLeastOnce, true, b"online") {
                        Ok(_) => log::info!("Heartbeat: published retained online to {}", status_topic),
                        Err(e) => log::warn!("Heartbeat: status online publish failed: {:?}", e),
                    },
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // Timer tick — collect metrics and publish heartbeat JSON.
                let nmea_drops = crate::gnss::NMEA_DROPS.load(Ordering::Relaxed);
                let rtcm_drops = crate::gnss::RTCM_DROPS.load(Ordering::Relaxed);
                let uart_tx_errors = crate::gnss::UART_TX_ERRORS.load(Ordering::Relaxed);

                // Monotonic uptime in seconds since boot. esp_timer_get_time() returns i64 microseconds.
                let uptime_s = unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1_000_000;

                // Current free heap in bytes.
                let heap_free = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };

                // NTRIP-04: include NTRIP connection state in heartbeat.
                let ntrip_state = crate::ntrip_client::NTRIP_STATE.load(Ordering::Relaxed);
                let ntrip_str = if ntrip_state == 1 { "connected" } else { "disconnected" };

                let json = format!(
                    "{{\"uptime_s\":{},\"heap_free\":{},\"nmea_drops\":{},\"rtcm_drops\":{},\
                     \"uart_tx_errors\":{},\"ntrip\":\"{}\"}}",
                    uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors, ntrip_str
                );

                log::info!("Heartbeat: {}", json);

                match client.lock() {
                    Err(e) => log::warn!("Heartbeat mutex poisoned: {:?}", e),
                    Ok(mut c) => match c.enqueue(&heartbeat_topic, QoS::AtMostOnce, false, json.as_bytes()) {
                        Ok(_) => log::info!("Heartbeat published to {}", heartbeat_topic),
                        Err(e) => log::warn!("Heartbeat publish failed: {:?}", e),
                    },
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Heartbeat: status channel closed — thread parking");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            }
        }
    }
}
