//! Resilience infrastructure: shared atomics for connectivity-loss reboot timers.
//!
//! RESIL-01: wifi_supervisor tracks WiFi disconnection duration locally (Option<Instant>).
//! RESIL-02: MQTT callback writes MQTT_DISCONNECTED_AT on disconnect; wifi_supervisor reads it.
//!
//! Pattern: matches GNSS_RX_HEARTBEAT AtomicU32 static in watchdog.rs.
//!
//! Note: AtomicU64 is not available on the ESP32 target (Xtensa LX6/LX7).
//! AtomicU32 is used; now_secs() returns u32 (wraps after ~136 years — safe for elapsed checks).

use std::sync::atomic::AtomicU32;

/// Stores the Unix epoch second (as u32) when MQTT last disconnected.
///
/// Convention:
///   0 = MQTT is currently connected (or has never disconnected since boot).
///   Non-zero = seconds since UNIX_EPOCH at moment of Disconnected event (truncated to u32).
///
/// Written by: MQTT callback on EventPayload::Disconnected (compare_exchange: only sets if 0)
///             and EventPayload::Connected (store 0 to clear).
/// Written by: wifi_supervisor stores 0 when WiFi disconnects (RESIL-02 anti-pitfall: reset
///             MQTT timer on WiFi loss so combined-outage does not false-trigger RESIL-02).
/// Read by:    wifi_supervisor — only evaluated when WiFi is connected.
pub static MQTT_DISCONNECTED_AT: AtomicU32 = AtomicU32::new(0);

/// Returns current Unix epoch seconds as u32. Used to stamp MQTT_DISCONNECTED_AT.
///
/// Falls back to 1 (non-zero) if SystemTime is unavailable — ensures the value
/// still signals "disconnected" even without a valid clock.
///
/// Wraps every ~136 years; safe for short elapsed-time comparisons (RESIL-02 uses 5min window).
pub fn now_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(1)
}
