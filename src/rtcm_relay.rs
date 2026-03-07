//! RTCM relay — publishes verified RTCM3 frames from the GNSS pipeline to MQTT.
//!
//! Consumes the `Receiver<RtcmFrame>` returned by `gnss::spawn_gnss` (third element).
//! For each `(message_type, pool_buffer, frame_len)` tuple, publishes the complete raw RTCM3
//! frame (including preamble, header, payload, and CRC bytes) to
//! `gnss/{device_id}/rtcm/{message_type}` at QoS 0 (AtMostOnce), retain = false.
//!
//! The complete frame is published (not just the payload) so downstream consumers can
//! independently verify the CRC and parse the frame structure.
//!
//! After publishing, the pool buffer is returned to the GNSS RX thread via `free_pool_tx`
//! so it can be reused — eliminating per-frame heap allocation in steady state.
//!
//! Uses `enqueue()` (non-blocking) — the MQTT pump thread drains the outbox.
//! At 1-4 frames/sec (MSM7 at 1Hz for up to 4 constellations), enqueue latency is negligible.

use crate::gnss::RtcmFrame;
use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::mqtt::client::EspMqttClient;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};

/// Spawn the RTCM relay thread.
///
/// Moves `rtcm_rx` and `free_pool_tx` into the thread — caller must NOT retain references.
/// `device_id` is cloned into the thread for topic construction.
/// `client` is an `Arc<Mutex<>>` shared with nmea_relay, heartbeat, and subscriber threads.
/// `free_pool_tx` is used to return pool buffers after each publish (both success and failure).
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    rtcm_rx: Receiver<RtcmFrame>,
    free_pool_tx: SyncSender<Box<[u8; 1029]>>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "RTCM relay", hwm_words, hwm_words * 4);
            log::info!("RTCM relay thread started");
            loop {
                match rtcm_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
                    Ok((message_type, frame_buf, frame_len)) => {
                        let topic = format!("gnss/{}/rtcm/{}", device_id, message_type);
                        // Acquire Mutex per frame — do NOT hold across loop iterations.
                        // Holding across iterations would starve heartbeat/subscriber threads.
                        match client.lock() {
                            Err(e) => {
                                log::warn!("RTCM relay: mutex poisoned: {:?}", e);
                                // Return buffer to pool even on mutex failure — must not leak.
                                if free_pool_tx.send(frame_buf).is_err() {
                                    log::error!("RTCM relay: free pool channel closed — buffer leaked");
                                }
                            }
                            Ok(mut c) => {
                                match c.enqueue(&topic, QoS::AtMostOnce, false, &frame_buf[..frame_len]) {
                                    Ok(_) => {}
                                    Err(e) => log::warn!("RTCM relay: enqueue failed: {:?}", e),
                                }
                                // Return buffer to pool — MUST happen regardless of enqueue success/failure.
                                if free_pool_tx.send(frame_buf).is_err() {
                                    log::error!("RTCM relay: free pool channel closed — buffer leaked");
                                }
                            }
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
