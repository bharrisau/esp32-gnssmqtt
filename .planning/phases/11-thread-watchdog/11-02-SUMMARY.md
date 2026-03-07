---
phase: 11-thread-watchdog
plan: "02"
subsystem: infra
tags: [watchdog, atomics, esp-idf, sdkconfig, reliability]

# Dependency graph
requires:
  - phase: 11-01
    provides: "watchdog module with GNSS_RX_HEARTBEAT, MQTT_PUMP_HEARTBEAT, spawn_supervisor()"

provides:
  - "GNSS RX thread increments GNSS_RX_HEARTBEAT at top of outer polling loop"
  - "MQTT pump thread increments MQTT_PUMP_HEARTBEAT at top of while-let event body"
  - "watchdog::spawn_supervisor() called as Step 18 in main.rs (final thread spawn)"
  - "CONFIG_ESP_TASK_WDT_PANIC=y in sdkconfig.defaults (hardware TWDT reboots on supervisor hang)"
  - "Fully operational software watchdog with 15s hang detection and hardware TWDT backstop"

affects: [future-reliability, health-telemetry, phase-13]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Heartbeat counter at top of outer loop (not inside match arms) — ensures idle/stall paths also update"
    - "Spawn order discipline: supervisor spawned last so all monitored threads are running at first check"
    - "Hardware TWDT panic mode as backstop for software watchdog supervisor hang"

key-files:
  created: []
  modified:
    - src/gnss.rs
    - src/mqtt.rs
    - src/main.rs
    - sdkconfig.defaults

key-decisions:
  - "Heartbeat in GNSS RX placed at top of loop{} not inside match arm — UART stall returning Ok(0) would stop updates if inside Ok(n) arm only"
  - "Heartbeat in MQTT pump placed before match event.payload() — fires on every MQTT event including internal ping/pong (no Timeout arm exists)"
  - "spawn_supervisor() placed after all other thread spawns (Step 18) — supervisor sees valid initial heartbeat values immediately"
  - "CONFIG_ESP_TASK_WDT_PANIC=y added with comment explaining 15s software / 30s hardware layered defense"

patterns-established:
  - "Watchdog wiring pattern: single-line fetch_add at outer loop entry, full crate:: path, no extra imports"

requirements-completed:
  - WDT-01
  - WDT-02

# Metrics
duration: 3min
completed: 2026-03-07
---

# Phase 11 Plan 02: Thread Watchdog Wiring Summary

**Heartbeat counters wired into GNSS RX and MQTT pump threads, supervisor spawned last in main.rs, hardware TWDT panic mode enabled — software watchdog fully operational with 15s detection and 30s hardware backstop**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-07T12:19:06Z
- **Completed:** 2026-03-07T12:22:08Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- GNSS RX thread now increments GNSS_RX_HEARTBEAT at the top of its outer polling loop — updates every ~10ms whether or not UART data is flowing, so a UART stall is detected correctly
- MQTT pump thread increments MQTT_PUMP_HEARTBEAT at the top of every `while let Ok(event) = connection.next()` iteration — fires on internal ping/pong events without requiring a separate timeout path
- watchdog::spawn_supervisor() added as Step 18 in main.rs — final spawn ensures supervisor observes non-zero heartbeat values on its first 5s check
- CONFIG_ESP_TASK_WDT_PANIC=y added to sdkconfig.defaults — hardware TWDT will now reboot the device if the supervisor itself stops being scheduled, completing WDT-02 criterion 3

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire heartbeat fetch_add into GNSS RX loop and MQTT pump loop** - `c743cbf` (feat)
2. **Task 2: Spawn watchdog supervisor in main.rs and enable TWDT panic in sdkconfig.defaults** - `1d0ac88` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/gnss.rs` - Added `crate::watchdog::GNSS_RX_HEARTBEAT.fetch_add(1, Ordering::Relaxed)` at top of outer `loop {}` in RX thread
- `src/mqtt.rs` - Added `crate::watchdog::MQTT_PUMP_HEARTBEAT.fetch_add(1, Ordering::Relaxed)` at top of `while let Ok(event) = connection.next()` body
- `src/main.rs` - Added Step 18 `watchdog::spawn_supervisor()` call between OTA spawn and operational log line
- `sdkconfig.defaults` - Added `CONFIG_ESP_TASK_WDT_PANIC=y` with explanatory comment after `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30`

## Decisions Made

- Heartbeat placed at top of GNSS RX `loop{}` not inside any `match uart_rx.read(...)` arm — if placed inside `Ok(n) if n > 0`, a UART stall returning `Ok(0)` continuously would freeze the counter and trigger false hang detection
- No new `use` imports needed in either file — `Ordering` already imported in both gnss.rs (line 31) and mqtt.rs (line 9); crate:: full paths used
- spawn_supervisor() placed as Step 18 (last spawn) per plan spec — ensures all monitored threads are live before the supervisor's first 5s check interval elapses

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 11 complete: software watchdog fully operational (both plans done)
- On-device verification recommended: boot device, confirm "[WDT] supervisor started" log line, observe no spurious reboots during 60s nominal operation
- Phase 13 (health telemetry) can read UART_TX_ERRORS (gnss.rs) and the heartbeat counters for diagnostic data

---
*Phase: 11-thread-watchdog*
*Completed: 2026-03-07*
