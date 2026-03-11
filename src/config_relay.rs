//! Config relay — receives MQTT payloads from the pump thread and forwards
//! parsed commands to the UM980 GNSS module via the GNSS TX channel.
//!
//! # Responsibilities
//! - Hash-deduplicate payloads (djb2) — retained MQTT messages are re-delivered
//!   on reconnect; identical payloads must NOT be re-applied to avoid disrupting
//!   the GNSS module.
//! - Parse JSON `{"delay_ms": N, "commands": ["CMD1", ...]}` or fall back to
//!   plain-text newline-delimited commands.
//! - Forward each command via `gnss_cmd_tx.send()` with a per-command delay.
//!   If send fails (TX thread dead), log and abandon remaining commands.
//!
//! # Limitation
//! JSON parsing handles the fixed schema above only. Escaped quotes inside
//! command strings are not supported — UM980 commands contain no special
//! characters so this is not a practical constraint.

use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};

/// Spawn the config relay thread.
///
/// Moves `config_rx` and `gnss_cmd_tx` into the thread — caller must NOT
/// retain references to them (other than independent clones for other uses).
///
/// `nvs_partition` is cloned cheaply into the thread closure for NVS persistence.
///
/// Returns `Ok(())` immediately after spawning (non-blocking).
pub fn spawn_config_relay(
    gnss_cmd_tx: SyncSender<String>,
    config_rx: Receiver<Vec<u8>>,
    nvs_partition: EspNvsPartition<NvsDefault>,
) -> anyhow::Result<()> {
    let nvs_for_relay = nvs_partition.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "Config relay", hwm_words, hwm_words * 4);
            log::info!("Config relay thread started");
            let mut last_hash: u32 = 0;

            loop {
                match config_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
                    Ok(payload) => {
                        // Guard: empty payload means the retained message was cleared.
                        if payload.is_empty() {
                            log::info!("Config relay: empty payload — retained message cleared, skipping");
                            continue;
                        }

                        let hash = djb2_hash(&payload);

                        if hash == last_hash {
                            log::info!(
                                "Config relay: payload unchanged (hash {:#010x}), skipping",
                                hash
                            );
                            continue;
                        }

                        last_hash = hash;
                        log::info!("Config relay: new config payload, hash {:#010x}", hash);
                        apply_config(&payload, &gnss_cmd_tx);
                        // Persist raw payload for automatic re-apply on UM980 reboot (FEAT-2).
                        save_gnss_config(&payload, &nvs_for_relay);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // No config payload within 30s — config is operator-triggered and rare. Continue.
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        log::error!("Config relay: channel closed — thread exiting");
                        break;
                    }
                }
            }

            // Dead-end park (pump exited; thread has nothing to do).
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        })
        .expect("config relay thread spawn failed");
    Ok(())
}

/// Save raw GNSS config payload to NVS "gnss" namespace, key "gnss_config".
///
/// Called after each successful `apply_config` so the last-known-good config
/// survives UM980 power cycles. Errors are logged but not propagated — a save
/// failure is non-fatal; the operator can re-send config via MQTT.
fn save_gnss_config(payload: &[u8], nvs_partition: &EspNvsPartition<NvsDefault>) {
    match EspNvs::new(nvs_partition.clone(), "gnss", true) {
        Ok(mut nvs) => {
            if let Err(e) = nvs.set_blob("gnss_config", payload) {
                log::warn!("Config relay: NVS save failed: {:?}", e);
            } else {
                log::info!("Config relay: saved {} bytes to NVS (gnss/gnss_config)", payload.len());
            }
        }
        Err(e) => log::warn!("Config relay: NVS open failed: {:?}", e),
    }
}

/// DJB2 hash — fast, non-cryptographic, adequate for deduplication.
fn djb2_hash(data: &[u8]) -> u32 {
    let mut hash: u32 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}

/// Parse payload and dispatch commands to the GNSS TX channel.
///
/// Supports two payload formats:
/// 1. JSON: `{"delay_ms": N, "commands": ["CMD1", "CMD2"]}`
/// 2. Plain text: newline-delimited commands (one per line)
///
/// On `gnss_cmd_tx.send()` failure, logs an error and returns immediately —
/// remaining commands in the batch are abandoned (no panic).
///
/// Public so main.rs UM980 reboot monitor can call this directly after reading
/// the saved config from NVS.
pub fn apply_config(payload: &[u8], gnss_cmd_tx: &SyncSender<String>) {
    let text = match std::str::from_utf8(payload) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Config relay: payload is not valid UTF-8: {:?}", e);
            return;
        }
    };

    let (delay_ms, commands): (u64, Vec<&str>) = if text.trim_start().starts_with('{') {
        // JSON path
        match parse_config_json(text) {
            None => {
                log::warn!("Config relay: JSON parse failed, discarding payload");
                return;
            }
            Some(parsed) => parsed,
        }
    } else {
        // Plain text fallback — one command per non-empty line
        let commands: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        let delay_ms: u64 = 100;
        (delay_ms, commands)
    };

    for cmd in commands {
        log::info!("Config relay: sending command: {:?}", cmd);
        // gnss.rs TX thread appends \r\n — do NOT include it here.
        match gnss_cmd_tx.send(cmd.to_string()) {
            Ok(_) => {}
            Err(e) => {
                log::error!(
                    "Config relay: gnss_cmd_tx send failed (TX thread dead?): {:?}",
                    e
                );
                return; // abandon remaining commands
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
}

/// Parse `{"delay_ms": N, "commands": ["CMD1", "CMD2"]}` without serde.
///
/// Limitation: does not handle escaped quotes inside command strings.
/// UM980 commands contain no special characters, so this is not a constraint.
///
/// Returns `None` if the commands array is missing, empty, or malformed.
fn parse_config_json(text: &str) -> Option<(u64, Vec<&str>)> {
    // Extract delay_ms (optional, default 100)
    let delay_ms: u64 = (|| -> Option<u64> {
        let key_pos = text.find("\"delay_ms\"")?;
        let colon_pos = text[key_pos..].find(':')? + key_pos;
        let after_colon = text[colon_pos + 1..].trim_start();
        let end = after_colon
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_colon.len());
        after_colon[..end].parse::<u64>().ok()
    })()
    .unwrap_or(100);

    // Extract commands array
    let array_start = text.find('[')? + 1;
    let array_end = text.find(']')?;
    if array_end <= array_start {
        return None;
    }
    let array_content = &text[array_start..array_end];

    // Split on ',' and strip surrounding whitespace + quotes from each item
    let commands: Vec<&str> = array_content
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
            Some(inner)
        })
        .collect();

    if commands.is_empty() {
        return None;
    }

    Some((delay_ms, commands))
}
