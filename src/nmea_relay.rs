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
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, RecvTimeoutError};

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
