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

/// Publish a retained heartbeat to `gnss/{device_id}/heartbeat` every 30 seconds.
///
/// Waits 5 seconds before the first publish to give the MQTT stack time to fully connect
/// and for the pump thread to process the initial `Connected` event.
///
/// Uses `client.publish()` (blocking) — acceptable here because this runs in its own
/// dedicated thread and the pump thread keeps the outbox moving.
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
    let topic = format!("gnss/{}/heartbeat", device_id);
    log::info!("Heartbeat thread started, topic: {}", topic);

    // Initial delay — give MQTT time to fully connect before first heartbeat.
    std::thread::sleep(std::time::Duration::from_secs(5));

    loop {
        log::info!("Heartbeat attempting publish...");

        match client.lock() {
            Err(e) => log::warn!("Heartbeat mutex poisoned: {:?}", e),
            Ok(mut c) =>  match c.enqueue(&topic, QoS::AtMostOnce, true, b"online") {
                Ok(_) => log::info!("Heartbeat published to {}", topic),
                Err(e) => log::warn!("Heartbeat publish failed: {:?}", e),
            },
        }

        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
