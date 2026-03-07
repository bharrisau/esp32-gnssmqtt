---
phase: 10-memory-diagnostics
plan: 01
subsystem: infra
tags: [freertos, stack, hwm, logging, esp-idf]

# Dependency graph
requires:
  - phase: 09-channel-loop-hardening
    provides: all thread spawn sites finalized with recv_timeout loops and bounded channels
provides:
  - FreeRTOS stack HWM log at entry of all 11 thread entry points
  - Startup log visibility into per-thread stack headroom
affects: [11-watchdog, 12-resilience, 13-health-telemetry]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) } at top of every thread closure/function"
    - "[HWM] <thread-name>: N words (N bytes) stack remaining at entry log format"

key-files:
  created: []
  modified:
    - src/gnss.rs
    - src/nmea_relay.rs
    - src/rtcm_relay.rs
    - src/config_relay.rs
    - src/uart_bridge.rs
    - src/mqtt.rs
    - src/wifi.rs
    - src/ota.rs
    - src/led.rs

key-decisions:
  - "Used full path esp_idf_svc::sys::uxTaskGetStackHighWaterMark without adding new use imports — direct dependency in Cargo.toml makes full path resolution valid in all files"
  - "HWM call placed as absolute first statement in each thread function/closure before any existing log lines to ensure measurement reflects true entry-point headroom"

patterns-established:
  - "HWM pattern: let hwm_words = unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) }; log::info!(\"[HWM] ...\") at top of every spawned thread"

requirements-completed: [HARD-04]

# Metrics
duration: 3min
completed: 2026-03-07
---

# Phase 10 Plan 01: Stack HWM Logging Summary

**FreeRTOS stack high-water mark logged at entry of all 12 thread sites (11 named threads) across 9 source files using uxTaskGetStackHighWaterMark via esp_idf_svc::sys**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-07T11:14:19Z
- **Completed:** 2026-03-07T11:17:45Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- All 11 thread entry points now emit `[HWM] <name>: N words (N bytes) stack remaining at entry` as their very first log line
- Startup log now shows per-thread stack headroom, enabling operators to detect threads with inadequate configured stack sizes (values below ~500 words are concerning)
- Zero behavioral changes — purely additive logging; `cargo build --release` exits clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Add HWM log to GNSS, NMEA, RTCM, Config, UART bridge threads** - `65f94ed` (feat)
2. **Task 2: Add HWM log to MQTT, WiFi, OTA, LED threads** - `1e5d778` (feat)

**Plan metadata:** (docs commit — this summary)

## Files Created/Modified

- `src/gnss.rs` — HWM at GNSS RX closure entry and GNSS TX closure entry (2 call sites)
- `src/nmea_relay.rs` — HWM at NMEA relay closure entry
- `src/rtcm_relay.rs` — HWM at RTCM relay closure entry (before existing thread-started log)
- `src/config_relay.rs` — HWM at Config relay closure entry
- `src/uart_bridge.rs` — HWM at UART bridge closure entry
- `src/mqtt.rs` — HWM at pump_mqtt_events, subscriber_loop, heartbeat_loop function tops (3 call sites)
- `src/wifi.rs` — HWM at wifi_supervisor function top
- `src/ota.rs` — HWM at ota_task function top (called via spawn_ota wrapper)
- `src/led.rs` — HWM at led_task function top

## Decisions Made

- Used full path `esp_idf_svc::sys::uxTaskGetStackHighWaterMark` rather than adding `use` imports — direct Cargo.toml dependency makes full path resolution valid in Rust 2021 edition without any `use` declaration
- Placed HWM call as the absolute first statement in each entry point, including before existing `log::info!("... thread started")` lines, to measure true entry headroom

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None — all 9 files compiled cleanly on first build attempt after each task's edits.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- All thread entry points instrumented; startup log provides full HWM visibility
- HARD-04 requirement satisfied
- Phase 10 Plan 02 (periodic HWM reporting) can proceed

---
*Phase: 10-memory-diagnostics*
*Completed: 2026-03-07*
