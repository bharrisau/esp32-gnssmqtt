//! RTCM relay — publishes verified RTCM3 frames from the GNSS pipeline to MQTT.
//!
//! Consumes the `Receiver<(u16, Vec<u8>)>` returned by `gnss::spawn_gnss` (third element).
//! For each `(message_type, frame)` tuple, publishes the complete raw RTCM3 frame (including
//! preamble, header, payload, and CRC bytes) to `gnss/{device_id}/rtcm/{message_type}` at
//! QoS 0 (AtMostOnce), retain = false.
//!
//! The complete frame is published (not just the payload) so downstream consumers can
//! independently verify the CRC and parse the frame structure.
//!
//! Uses `enqueue()` (non-blocking) — the MQTT pump thread drains the outbox.
//! At 1-4 frames/sec (MSM7 at 1Hz for up to 4 constellations), enqueue latency is negligible.

use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::mqtt::client::EspMqttClient;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};

/// Spawn the RTCM relay thread.
///
/// Moves `rtcm_rx` into the thread — caller must NOT retain a reference to it.
/// `device_id` is cloned into the thread for topic construction.
/// `client` is an `Arc<Mutex<>>` shared with nmea_relay, heartbeat, and subscriber threads.
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    rtcm_rx: Receiver<(u16, Vec<u8>)>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            log::info!("RTCM relay thread started");
            loop {
                match rtcm_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
                    Ok((message_type, frame)) => {
                        let topic = format!("gnss/{}/rtcm/{}", device_id, message_type);
                        // Acquire Mutex per frame — do NOT hold across loop iterations.
                        // Holding across iterations would starve heartbeat/subscriber threads.
                        match client.lock() {
                            Err(e) => log::warn!("RTCM relay: mutex poisoned: {:?}", e),
                            Ok(mut c) => match c.enqueue(&topic, QoS::AtMostOnce, false, &frame) {
                                Ok(_) => {}
                                Err(e) => log::warn!("RTCM relay: enqueue failed: {:?}", e),
                            },
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // No RTCM frame within 5s — expected at low update rates. Continue.
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        log::error!("RTCM relay: receiver closed — thread exiting");
                        break;
                    }
                }
            }
            // Dead-end park (gnss RX thread has exited; this thread has nothing to do).
            loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
        })
        .expect("rtcm relay thread spawn failed");
    Ok(())
}
