//! Software watchdog: heartbeat counter for the GNSS RX thread + supervisor loop.
//!
//! The GNSS RX thread increments GNSS_RX_HEARTBEAT at every outer loop iteration (~10ms).
//! The supervisor checks every WDT_CHECK_INTERVAL (5s); if the counter is unchanged for
//! WDT_MISS_THRESHOLD (3) consecutive checks (15s window), it calls esp_restart().
//!
//! MQTT is not supervised here: event delivery runs via a callback on the ESP-IDF C MQTT
//! task thread. No C-level event fires during keepalive ping/pong quiet periods, so a
//! software counter would false-trigger. Hardware TWDT (30s, PANIC=y) covers catastrophic
//! MQTT task hangs.
//!
//! Hardware TWDT (CONFIG_ESP_TASK_WDT_TIMEOUT_S=30, CONFIG_ESP_TASK_WDT_PANIC=y) acts as
//! the backstop: if the supervisor itself hangs, the idle task stops being scheduled
//! and the hardware TWDT fires within 30s.
//!
//! Pattern: matches existing UART_TX_ERRORS AtomicU32 static in gnss.rs.

use std::sync::atomic::{AtomicU32, Ordering};

/// Heartbeat counter for the GNSS RX thread.
/// Incremented at the top of the outer `loop {}` in the GNSS RX thread (gnss.rs).
/// Updated every ~10ms (NON_BLOCK poll + sleep) during normal GNSS operation.
pub static GNSS_RX_HEARTBEAT: AtomicU32 = AtomicU32::new(0);

/// Spawn the watchdog supervisor thread.
///
/// Stack size: 4096 bytes — the supervisor does no I/O, no large buffers; only loop +
/// u32 arithmetic + log calls. HWM at entry will confirm headroom.
pub fn spawn_supervisor() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || supervisor_loop())
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("watchdog supervisor spawn failed: {:?}", e))
}

fn supervisor_loop() -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "WDT sup", hwm_words, hwm_words * 4);
    log::info!("[WDT] supervisor started — check interval {}s, miss threshold {}",
        crate::config::WDT_CHECK_INTERVAL.as_secs(),
        crate::config::WDT_MISS_THRESHOLD);

    let mut last_gnss: u32 = 0;
    let mut gnss_misses: u32 = 0;

    loop {
        std::thread::sleep(crate::config::WDT_CHECK_INTERVAL);

        let gnss_now = GNSS_RX_HEARTBEAT.load(Ordering::Relaxed);

        if gnss_now == last_gnss {
            gnss_misses += 1;
            log::warn!("[WDT] GNSS RX heartbeat missed ({}/{})", gnss_misses, crate::config::WDT_MISS_THRESHOLD);
            if gnss_misses >= crate::config::WDT_MISS_THRESHOLD {
                log::error!("[WDT] GNSS RX thread hung for {}s — rebooting",
                    crate::config::WDT_CHECK_INTERVAL.as_secs() * crate::config::WDT_MISS_THRESHOLD as u64);
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
        } else {
            gnss_misses = 0;
            last_gnss = gnss_now;
        }
    }
}
