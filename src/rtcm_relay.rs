//! RTCM relay — publishes verified RTCM3 frames from the GNSS pipeline to MQTT.
//!
//! Consumes the `Receiver<RtcmFrame>` returned by `gnss::spawn_gnss` (third element).
//! For each `(message_type, pool_buffer, frame_len)` tuple, publishes the complete raw RTCM3
//! frame (including preamble, header, payload, and CRC bytes) to `gnss/{device_id}/rtcm`
//! at QoS 0 (AtMostOnce), retain = false.
//!
//! All RTCM message types are published to the single consolidated topic — the message type
//! is encoded in the binary frame header and downstream consumers can parse it.
//!
//! The complete frame is published (not just the payload) so downstream consumers can
//! independently verify the CRC and parse the frame structure.
//!
//! After copying into a `BytesMut` buffer, the pool buffer is returned to the GNSS RX
//! thread via `free_pool_tx` so it can be reused — eliminating per-frame heap allocation
//! in steady state. The `bytes::Bytes` frozen slice is then sent to the publish thread
//! via `SyncSender<MqttMessage>` for zero-copy handoff.
//!
//! Uses `SyncSender<MqttMessage>` (non-blocking try_send) to the publish thread.
//! At 1-4 frames/sec (MSM7 at 1Hz for up to 4 constellations), the channel is unlikely
//! to be full in practice.

use crate::gnss::RtcmFrame;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;

/// Spawn the RTCM relay thread.
///
/// Moves `rtcm_rx` and `free_pool_tx` into the thread — caller must NOT retain references.
/// `rtcm_topic` is a pre-built `Arc<str>` for "gnss/{id}/rtcm" — cloned per frame.
/// `mqtt_tx` is a bounded `SyncSender<MqttMessage>` shared with the publish thread.
/// `free_pool_tx` returns pool buffers to the GNSS RX thread after each frame is processed.
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_relay(
    mqtt_tx: SyncSender<crate::mqtt_publish::MqttMessage>,
    rtcm_topic: Arc<str>,
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

            // Reusable BytesMut buffer — split().freeze() for zero-copy handoff to publish thread.
            // reserve(1029) after each frame attempts to reclaim the backing if the Bytes refcount
            // has dropped to 1 (publish thread consumed it); otherwise allocates fresh (acceptable).
            let mut buf = bytes::BytesMut::with_capacity(1029);

            loop {
                match rtcm_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
                    Ok((_message_type, frame_buf, frame_len)) => {
                        // Write frame bytes into BytesMut, then freeze for zero-copy handoff.
                        buf.extend_from_slice(&frame_buf[..frame_len]);
                        let filled = buf.split().freeze();

                        // Return pool buffer to GNSS RX thread — MUST happen regardless of
                        // publish success/failure. Done before try_send so the pool buffer is
                        // never leaked even if the channel is full or closed.
                        if free_pool_tx.send(frame_buf).is_err() {
                            log::error!("RTCM relay: free pool channel closed — buffer leaked");
                        }

                        // Publish via channel (non-blocking).
                        match mqtt_tx.try_send(crate::mqtt_publish::MqttMessage::Rtcm {
                            topic: rtcm_topic.clone(),
                            payload: filled,
                        }) {
                            Ok(()) => {}
                            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                                log::warn!("RTCM relay: publish channel full — frame dropped");
                                crate::gnss::RTCM_DROPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                log::error!("RTCM relay: publish channel closed — thread exiting");
                                break;
                            }
                        }

                        // Best-effort reclaim: works if Bytes refcount has dropped to 1 (publish
                        // thread consumed it). If not yet consumed, reserve() allocates fresh —
                        // acceptable, not a correctness issue.
                        buf.reserve(1029);
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
