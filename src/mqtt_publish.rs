//! Dedicated MQTT publish thread — owns `EspMqttClient` exclusively and dispatches
//! typed `MqttMessage` variants to `enqueue()`.
//!
//! # Architecture
//!
//! All relay threads (NMEA, RTCM, Log, heartbeat, status) send `MqttMessage` values
//! into a `std::sync::mpsc::SyncSender<MqttMessage>`. The publish thread drains the
//! channel and calls `client.enqueue()` for each message. This eliminates per-relay
//! `Arc<Mutex<EspMqttClient>>` locking and the contention it causes at high NMEA rates.
//!
//! # Re-entrancy guard
//!
//! `LOG_REENTERING` (in `log_relay`) is set to `true` **only** when publishing a
//! `MqttMessage::Log` variant — preventing the log relay from re-entering itself.
//! It is **not** set for other variants; doing so at 40 msg/s (5 Hz × 8 sentence types)
//! would suppress almost all MQTT log output.
//!
//! # Thread stack
//!
//! Callers (in `main.rs`) must spawn this thread with at least 8192 bytes of stack —
//! the same as `nmea_relay` and `rtcm_relay`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::mqtt::client::EspMqttClient;

// TODO(plan02): make LOG_REENTERING pub in log_relay.rs — required for the guard below.
// Until Plan 02 runs, this module will not compile unless LOG_REENTERING is pub.
// Plan 02 is the immediate successor; this comment marks the compile dependency.

/// Number of times `client.enqueue()` has returned an error.
/// Incremented with `Ordering::Relaxed` — approximate; used for diagnostics only.
pub static MQTT_ENQUEUE_ERRORS: AtomicU32 = AtomicU32::new(0);

/// Number of messages dropped because the outbox was full at enqueue time.
/// This counter is incremented separately from `MQTT_ENQUEUE_ERRORS` when we can
/// distinguish an outbox-full condition; currently incremented on any enqueue error
/// alongside `MQTT_ENQUEUE_ERRORS` as a conservative over-count until finer error
/// classification is available.
pub static MQTT_OUTBOX_DROPS: AtomicU32 = AtomicU32::new(0);

/// Typed MQTT message — carries topic, payload, and variant-specific flags.
///
/// Each relay thread constructs the appropriate variant and sends it via
/// `SyncSender<MqttMessage>` to the publish thread.
#[allow(dead_code)]
pub enum MqttMessage {
    /// NMEA sentence — QoS 0, no retain. High-frequency path (up to 40 msg/s).
    Nmea {
        topic: Arc<str>,
        payload: Vec<u8>,
    },
    /// RTCM3 correction frame — QoS 0, no retain. Uses `bytes::Bytes` for zero-copy
    /// transfer from the pool buffer used by the GNSS RX thread.
    Rtcm {
        topic: Arc<str>,
        payload: bytes::Bytes,
    },
    /// Log line forwarded from the log relay — QoS 0, no retain.
    /// Sets `LOG_REENTERING` during enqueue to prevent feedback loops.
    Log {
        topic: Arc<str>,
        payload: Vec<u8>,
    },
    /// Heartbeat JSON — configurable retain flag (retained "online" on reconnect).
    Heartbeat {
        topic: Arc<str>,
        payload: Vec<u8>,
        retain: bool,
    },
    /// Status message (e.g. LWT "offline") — static payload, configurable QoS and retain.
    Status {
        topic: Arc<str>,
        payload: &'static [u8],
        qos: QoS,
        retain: bool,
    },
    /// Benchmark / diagnostic message — QoS 0, no retain.
    Bench {
        topic: Arc<str>,
        payload: Vec<u8>,
    },
}

/// Dedicated MQTT publish thread entry point.
///
/// Owns `client` exclusively (no Arc/Mutex). Receives `MqttMessage` values from
/// `mqtt_rx`, dispatches to `client.enqueue()`, and handles re-entrancy for the
/// Log variant.
///
/// # Panics
///
/// Never panics — on channel disconnect, parks in a 60-second sleep loop.
///
/// # Stack
///
/// Caller must spawn with at least 8192 bytes of stack.
#[allow(dead_code)]
pub fn publish_thread(
    mut client: EspMqttClient<'static>,
    mqtt_rx: std::sync::mpsc::Receiver<MqttMessage>,
) -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!(
        "[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "MQTT publish",
        hwm_words,
        hwm_words * 4
    );

    loop {
        match mqtt_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
            Ok(msg) => dispatch(&mut client, msg),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Normal when no messages pending — continue.
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::error!("MQTT publish thread: channel closed — parking");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            }
        }
    }
}

/// Dispatch a single `MqttMessage` to `client.enqueue()`.
///
/// Sets `LOG_REENTERING` only for the `Log` variant; all other variants leave the
/// guard clear to avoid suppressing ~40 enqueue calls/second of GNSS telemetry.
fn dispatch(client: &mut EspMqttClient<'static>, msg: MqttMessage) {
    match msg {
        MqttMessage::Nmea { topic, payload } => {
            if client.enqueue(&topic, QoS::AtMostOnce, false, &payload).is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        MqttMessage::Rtcm { topic, payload } => {
            if client.enqueue(&topic, QoS::AtMostOnce, false, &payload).is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        MqttMessage::Log { topic, payload } => {
            // TODO(plan02): LOG_REENTERING is now pub in log_relay (made pub in Plan 21-01).
            crate::log_relay::LOG_REENTERING.store(true, std::sync::atomic::Ordering::Relaxed);
            let result = client.enqueue(&topic, QoS::AtMostOnce, false, &payload);
            crate::log_relay::LOG_REENTERING.store(false, std::sync::atomic::Ordering::Relaxed);
            if result.is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        MqttMessage::Heartbeat { topic, payload, retain } => {
            if client.enqueue(&topic, QoS::AtMostOnce, retain, &payload).is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        MqttMessage::Status { topic, payload, qos, retain } => {
            if client.enqueue(&topic, qos, retain, payload).is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        MqttMessage::Bench { topic, payload } => {
            if client.enqueue(&topic, QoS::AtMostOnce, false, &payload).is_err() {
                MQTT_ENQUEUE_ERRORS.fetch_add(1, Ordering::Relaxed);
                MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
