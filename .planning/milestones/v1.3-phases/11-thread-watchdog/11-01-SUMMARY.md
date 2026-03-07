---
phase: 11-thread-watchdog
plan: "01"
subsystem: infra
tags: [watchdog, atomics, esp-idf, rust, esp_restart]

# Dependency graph
requires:
  - phase: 09-channel-loop-hardening
    provides: recv_timeout loops with no-op timeout arms (Plan 02 will add heartbeat increments here)
  - phase: 10-memory-diagnostics
    provides: HWM logging pattern (uxTaskGetStackHighWaterMark) used in supervisor_loop
provides:
  - pub static GNSS_RX_HEARTBEAT: AtomicU32 in src/watchdog.rs
  - pub static MQTT_PUMP_HEARTBEAT: AtomicU32 in src/watchdog.rs
  - pub fn spawn_supervisor() in src/watchdog.rs (spawns supervisor with 4096-byte stack)
  - WDT_CHECK_INTERVAL (5s) and WDT_MISS_THRESHOLD (3) constants in config.rs + config.example.rs
affects:
  - 11-02 (Plan 02 will increment the heartbeat counters and call spawn_supervisor)
  - gnss.rs (GNSS RX thread will increment GNSS_RX_HEARTBEAT)
  - mqtt.rs (MQTT pump thread will increment MQTT_PUMP_HEARTBEAT)
  - main.rs (Plan 02 adds spawn_supervisor() call)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "AtomicU32 pub static heartbeat counters matching UART_TX_ERRORS pattern in gnss.rs"
    - "supervisor_loop -> ! diverging function with loop + thread::sleep"
    - "spawn_supervisor returns anyhow::Result<()> matching other spawn_ functions"

key-files:
  created:
    - src/watchdog.rs
  modified:
    - src/config.example.rs
    - src/main.rs

key-decisions:
  - "spawn_supervisor() call deferred to Plan 02 — only mod watchdog declared here so Plan 02 compiler errors are isolated to wiring, not module visibility"
  - "4096-byte stack for supervisor: no I/O, no buffers, only loop + u32 arithmetic + log calls"
  - "Ordering::Relaxed for heartbeat loads: counter monotonically increases, no synchronization needed beyond detecting staleness"

patterns-established:
  - "Watchdog pattern: pub AtomicU32 statics incremented by threads, polled by supervisor; esp_restart() on threshold"

requirements-completed: [WDT-01, WDT-02]

# Metrics
duration: 2min
completed: 2026-03-07
---

# Phase 11 Plan 01: Thread Watchdog Module Summary

**Software watchdog module with two AtomicU32 heartbeat counters and a supervisor thread that calls esp_restart() after 15s of missed beats (3 x 5s checks)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-07T12:14:27Z
- **Completed:** 2026-03-07T12:16:33Z
- **Tasks:** 2
- **Files modified:** 3 (config.example.rs, watchdog.rs, main.rs)

## Accomplishments
- Created src/watchdog.rs with GNSS_RX_HEARTBEAT and MQTT_PUMP_HEARTBEAT pub AtomicU32 statics
- spawn_supervisor() spawns a 4096-byte stack thread running supervisor_loop()
- supervisor_loop polls both counters every 5s; calls unsafe esp_idf_svc::sys::esp_restart() after 3 consecutive misses (15s window)
- WDT_CHECK_INTERVAL and WDT_MISS_THRESHOLD constants added to both config files with documented rationale
- mod watchdog declared in main.rs; cargo build --release passes with no errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Add WDT constants to config.example.rs and config.rs** - `7d54c81` (feat)
2. **Task 2: Create src/watchdog.rs with heartbeat counters and supervisor** - `d41d8e1` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/watchdog.rs` - Watchdog module: GNSS_RX_HEARTBEAT, MQTT_PUMP_HEARTBEAT statics and spawn_supervisor() + supervisor_loop()
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/config.example.rs` - Added WDT_CHECK_INTERVAL (5s) and WDT_MISS_THRESHOLD (3)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/main.rs` - Added `mod watchdog;` declaration

## Decisions Made
- spawn_supervisor() call deferred to Plan 02 — only the module declaration is added here, so Plan 02 compiler errors are isolated to wiring and not module visibility
- 4096-byte stack chosen for supervisor: no I/O, no heap buffers, only arithmetic + log calls; HWM log at entry will confirm headroom at runtime
- Ordering::Relaxed for heartbeat loads: the counter only needs to be seen as changed (not zero), so acquire/release synchronization is unnecessary

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- src/watchdog.rs compiles cleanly; GNSS_RX_HEARTBEAT and MQTT_PUMP_HEARTBEAT are accessible as crate::watchdog::GNSS_RX_HEARTBEAT from gnss.rs and mqtt.rs
- Plan 02 can proceed immediately: wire heartbeat increments in gnss.rs and mqtt.rs, then call spawn_supervisor() from main.rs
- No blockers

---
*Phase: 11-thread-watchdog*
*Completed: 2026-03-07*
