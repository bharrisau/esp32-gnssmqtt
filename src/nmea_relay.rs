//! NMEA relay — publishes each NMEA sentence from the GNSS pipeline to MQTT.
//!
//! Consumes the `Receiver<(String, String)>` returned by `gnss::spawn_gnss`.
//! For each `(sentence_type, raw_sentence)` tuple, publishes `raw_sentence` as
//! bytes to `gnss/{device_id}/nmea` at QoS 0 (AtMostOnce), retain = false.
//!
//! All sentence types are published to the single consolidated topic — the sentence
//! type is visible from the `$GNGGA...` / `$GNRMC...` payload prefix.
//!
//! Uses `SyncSender<MqttMessage>` (non-blocking try_send) to the publish thread.
//! If the channel is full, the sentence is dropped and NMEA_DROPS is incremented.
//! This prevents backpressure stalling the relay thread at 40+ sentences/sec.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};

/// Spawn the NMEA relay thread.
///
/// Moves `nmea_rx` into the thread — caller must NOT retain a reference to it.
/// `nmea_topic` is a pre-built `Arc<str>` for "gnss/{id}/nmea" — cloned per sentence
/// with zero allocation (Arc clone is a refcount increment).
/// `mqtt_tx` is a bounded `SyncSender<MqttMessage>` shared with the publish thread.
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_relay(
    mqtt_tx: SyncSender<crate::mqtt_publish::MqttMessage>,
    nmea_topic: std::sync::Arc<str>,
    nmea_rx: Receiver<(String, String)>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "NMEA relay", hwm_words, hwm_words * 4);
            log::info!("NMEA relay thread started");
            let mut sentence_count: u64 = 0;
            let mut throughput_tick = std::time::Instant::now();
            loop {
                match nmea_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
                    Ok((sentence_type, raw)) => {
                        // TELEM-01: parse GGA fix quality into shared atomics for heartbeat.
                        if sentence_type.ends_with("GGA") {
                            parse_gga_into_atomics(&raw);
                        }
                        // Topic consolidation: all NMEA types → single "gnss/{id}/nmea" topic.
                        // Sentence type visible from payload prefix ($GNGGA, $GNRMC, etc.).
                        let payload = raw.into_bytes();
                        match mqtt_tx.try_send(crate::mqtt_publish::MqttMessage::Nmea {
                            topic: nmea_topic.clone(),
                            payload,
                        }) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                log::warn!("NMEA relay: publish channel full — sentence dropped");
                                crate::gnss::NMEA_DROPS.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(TrySendError::Disconnected(_)) => {
                                log::error!("NMEA relay: publish channel closed — thread exiting");
                                break;
                            }
                        }
                        sentence_count += 1;
                        if sentence_count % 100 == 0 {
                            let elapsed = throughput_tick.elapsed();
                            log::info!("NMEA relay: {} sentences in {:.1}s ({:.1} msg/s)",
                                100, elapsed.as_secs_f32(),
                                100.0 / elapsed.as_secs_f32().max(0.001));
                            throughput_tick = std::time::Instant::now();
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // No NMEA sentence within 5s — GNSS may be idle or pipeline stalled. Continue.
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        log::error!("NMEA relay: receiver closed — thread exiting");
                        break;
                    }
                }
            }
            // Dead-end park (gnss RX thread has exited; this thread has nothing to do).
            loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
        })
        .expect("nmea relay thread spawn failed");
    Ok(())
}

/// Parse a GGA sentence and update the shared gnss_state atomics.
///
/// Field layout (comma-delimited):
///   [0] $GNGGA  [1] time  [2] lat  [3] N/S  [4] lon  [5] E/W
///   [6] fix_quality  [7] num_sats  [8] hdop  ...
///
/// Only updates an atomic if the corresponding field is non-empty and parseable.
/// Sentences with fewer than 9 fields are silently ignored (malformed/truncated).
fn parse_gga_into_atomics(raw: &str) {
    let fields: Vec<&str> = raw.split(',').collect();
    if fields.len() < 9 {
        return; // malformed or truncated GGA — leave atomics unchanged
    }
    // Field 6: fix quality (0=no fix, 1=SPS, 2=DGPS, 4=RTK Fixed, 5=RTK Float)
    if !fields[6].is_empty() {
        if let Ok(fix) = fields[6].parse::<u8>() {
            crate::gnss_state::GGA_FIX_TYPE.store(fix, Ordering::Relaxed);
        }
    }
    // Field 7: satellite count
    if !fields[7].is_empty() {
        if let Ok(sats) = fields[7].parse::<u8>() {
            crate::gnss_state::GGA_SATELLITES.store(sats, Ordering::Relaxed);
        }
    }
    // Field 8: HDOP — store as ×10 integer (no AtomicF32 in std)
    // Guard: only write if non-empty and parse succeeds.
    if !fields[8].is_empty() {
        if let Ok(hdop) = fields[8].parse::<f32>() {
            crate::gnss_state::GGA_HDOP_X10.store((hdop * 10.0) as u32, Ordering::Relaxed);
        }
    }
}
