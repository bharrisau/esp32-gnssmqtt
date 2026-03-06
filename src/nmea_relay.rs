//! NMEA relay — publishes each NMEA sentence from the GNSS pipeline to MQTT.
//!
//! Consumes the `Receiver<(String, String)>` returned by `gnss::spawn_gnss`.
//! For each `(sentence_type, raw_sentence)` tuple, publishes `raw_sentence` as
//! bytes to `gnss/{device_id}/nmea/{sentence_type}` at QoS 0 (AtMostOnce),
//! retain = false.
//!
//! Uses `enqueue()` (non-blocking) not `publish()` (blocking) — the MQTT pump
//! thread drains the outbox. This prevents backpressure stalling the relay
//! thread at 10+ sentences/sec. (Mirrors the heartbeat_loop pattern in mqtt.rs.)
//!
//! If `enqueue()` fails (e.g. MQTT disconnected), logs WARN and continues —
//! the pump thread will reconnect and publishes will resume.

use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::mqtt::client::EspMqttClient;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;

/// Spawn the NMEA relay thread.
///
/// Moves `nmea_rx` into the thread — caller must NOT retain a reference to it.
/// `device_id` is cloned into the thread for topic construction.
/// `client` is an `Arc<Mutex<>>` shared with heartbeat and subscriber threads.
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    nmea_rx: Receiver<(String, String)>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            log::info!("NMEA relay thread started");
            // `for x in &receiver` blocks until a tuple arrives, then processes it.
            // Exits only when all SyncSenders are dropped (gnss.rs RX thread exits).
            for (sentence_type, raw) in &nmea_rx {
                let topic = format!("gnss/{}/nmea/{}", device_id, sentence_type);
                // Acquire Mutex per sentence — do NOT hold across loop iterations.
                // Holding across iterations would starve heartbeat/subscriber threads.
                match client.lock() {
                    Err(e) => log::warn!("NMEA relay: mutex poisoned: {:?}", e),
                    Ok(mut c) => {
                        match c.enqueue(&topic, QoS::AtMostOnce, false, raw.as_bytes()) {
                            Ok(_) => {}
                            Err(e) => log::warn!("NMEA relay: enqueue failed: {:?}", e),
                        }
                    }
                }
            }
            // All SyncSenders dropped — gnss RX thread has exited.
            log::error!("NMEA relay: receiver closed — thread exiting");
        })
        .expect("nmea relay thread spawn failed");
    Ok(())
}
