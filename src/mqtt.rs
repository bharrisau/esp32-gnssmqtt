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
pub fn mqtt_connect(
    device_id: &str,
    subscribe_tx: SyncSender<()>,
    config_tx: SyncSender<Vec<u8>>,
    ota_tx: SyncSender<Vec<u8>>,
    led_state: Arc<AtomicU8>,
) -> anyhow::Result<Arc<Mutex<EspMqttClient<'static>>>> {
    let broker_url = format!(
        "mqtt://{}:{}",
        crate::config::MQTT_HOST,
        crate::config::MQTT_PORT
    );

    // IMPORTANT: lwt_topic MUST be declared BEFORE conf in the same scope.
    // LwtConfiguration.topic is &'a str — it must outlive the MqttClientConfiguration.
    let lwt_topic = format!("gnss/{}/status", device_id);

    let conf = MqttClientConfiguration {
        client_id: Some(device_id),
        username: if crate::config::MQTT_USER.is_empty() {
            None
        } else {
            Some(crate::config::MQTT_USER)
        },
        password: if crate::config::MQTT_PASS.is_empty() {
            None
        } else {
            Some(crate::config::MQTT_PASS)
        },
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
                if t.ends_with("/config") {
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
                }
                // All other topics: silently ignored
            }
            m @ _ => {
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

    // Initial delay — give MQTT time to fully connect before first publish.
    std::thread::sleep(std::time::Duration::from_secs(5));

    // ONE-TIME: publish retained "online" to /status — clears the LWT "offline" retained message.
    // This happens once per thread lifetime (= once per reconnect, since heartbeat_loop is
    // re-spawned on each connection cycle — or runs continuously if not re-spawned; either way
    // it correctly clears LWT on reconnect).
    match client.lock() {
        Err(e) => log::warn!("Heartbeat: status mutex poisoned on init: {:?}", e),
        Ok(mut c) => match c.enqueue(&status_topic, QoS::AtLeastOnce, true, b"online") {
            Ok(_) => log::info!("Heartbeat: published retained online to {}", status_topic),
            Err(e) => log::warn!("Heartbeat: status online publish failed: {:?}", e),
        },
    }

    loop {
        // Read cumulative counters — no reset (METR-02: cumulative since boot).
        let nmea_drops = crate::gnss::NMEA_DROPS.load(Ordering::Relaxed);
        let rtcm_drops = crate::gnss::RTCM_DROPS.load(Ordering::Relaxed);
        let uart_tx_errors = crate::gnss::UART_TX_ERRORS.load(Ordering::Relaxed);

        // Monotonic uptime in seconds since boot. esp_timer_get_time() returns i64 microseconds.
        // Divide while i64 to avoid sign truncation; safe for any realistic uptime.
        let uptime_s = unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1_000_000;

        // Current free heap in bytes.
        let heap_free = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };

        // Build JSON manually — no serde (consistent with ota.rs pattern; avoids ~50KB binary cost).
        let json = format!(
            "{{\"uptime_s\":{},\"heap_free\":{},\"nmea_drops\":{},\"rtcm_drops\":{},\"uart_tx_errors\":{}}}",
            uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors
        );

        log::info!("Heartbeat: {}", json);

        match client.lock() {
            Err(e) => log::warn!("Heartbeat mutex poisoned: {:?}", e),
            Ok(mut c) => match c.enqueue(&heartbeat_topic, QoS::AtMostOnce, false, json.as_bytes()) {
                Ok(_) => log::info!("Heartbeat published to {}", heartbeat_topic),
                Err(e) => log::warn!("Heartbeat publish failed: {:?}", e),
            },
        }

        std::thread::sleep(std::time::Duration::from_secs(crate::config::HEARTBEAT_INTERVAL_SECS));
    }
}
