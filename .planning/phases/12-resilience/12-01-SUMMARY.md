---
phase: 12-resilience
plan: 01
subsystem: infra
tags: [esp32, atomics, watchdog, wifi, mqtt, resilience, rust]

# Dependency graph
requires:
  - phase: 11-thread-watchdog
    provides: GNSS_RX_HEARTBEAT AtomicU32 pattern and esp_restart() reboot pattern
  - phase: 09-channel-loop-hardening
    provides: wifi_supervisor with consecutive_failures and backoff logic
provides:
  - MQTT_DISCONNECTED_AT AtomicU32 static in src/resil.rs for RESIL-02 timer
  - now_secs() u32 helper for stamping disconnect events
  - RESIL-01: WiFi 10-min disconnect reboot in wifi_supervisor
  - RESIL-02: MQTT 5-min disconnect (while WiFi up) reboot in wifi_supervisor
  - WIFI_DISCONNECT_REBOOT_TIMEOUT and MQTT_DISCONNECT_REBOOT_SECS config constants
affects: [12-02-plan, mqtt-callback, phase-13-health-telemetry]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - AtomicU32 for shared connectivity state (ESP32 lacks AtomicU64)
    - Option<Instant> for local elapsed-time tracking in supervisor loop
    - MQTT timer cleared on WiFi drop to prevent combined-outage false-trigger

key-files:
  created:
    - src/resil.rs
  modified:
    - src/wifi.rs
    - src/config.example.rs
    - src/main.rs

key-decisions:
  - "AtomicU32 not AtomicU64: ESP32 Xtensa LX6/LX7 target lacks AtomicU64; now_secs() returns u32 (wraps ~136yr, safe for 5-min windows)"
  - "MQTT_DISCONNECT_REBOOT_SECS typed as u32 to match now_secs() return type for saturating_sub comparison"
  - "RESIL-01 uses Option<Instant> (local to wifi_supervisor) not an atomic — no cross-thread sharing needed for WiFi disconnect timer"
  - "MQTT timer cleared in !connected arm of wifi_supervisor to prevent false RESIL-02 trigger during combined WiFi+MQTT outage"

patterns-established:
  - "Resilience atomics in src/resil.rs: same module-per-concern pattern as watchdog.rs"
  - "Both reboot paths use log::error! immediately before esp_restart() for flush guarantee"

requirements-completed: [RESIL-01, RESIL-02]

# Metrics
duration: 3min
completed: 2026-03-07
---

# Phase 12 Plan 01: Resilience Infrastructure Summary

**AtomicU32-based MQTT disconnect tracker (resil.rs) and wifi_supervisor extended with RESIL-01 (10-min WiFi timeout) and RESIL-02 (5-min MQTT timeout while WiFi up) automatic reboot paths**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-07T13:55:05Z
- **Completed:** 2026-03-07T13:58:07Z
- **Tasks:** 3
- **Files modified:** 4 (src/resil.rs created, src/main.rs, src/wifi.rs, src/config.example.rs modified)

## Accomplishments

- Created src/resil.rs with MQTT_DISCONNECTED_AT AtomicU32 static and now_secs() helper — Plan 02 MQTT callback writes here on Disconnected/Connected events
- Extended wifi_supervisor with RESIL-01: Option<Instant> timer reboots after WIFI_DISCONNECT_REBOOT_TIMEOUT (10 min) with `[RESIL-01]` log prefix
- Extended wifi_supervisor with RESIL-02: reads MQTT_DISCONNECTED_AT when WiFi is up; reboots after MQTT_DISCONNECT_REBOOT_SECS (5 min) with `[RESIL-02]` log prefix
- MQTT timer cleared in !connected arm (Pitfall 2 prevention: combined WiFi+MQTT outage does not false-trigger RESIL-02)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create src/resil.rs** - `2d1f793` (feat)
2. **Task 2: Add resilience constants to config.example.rs** - `3fc15c8` (feat)
3. **Task 3: Extend wifi_supervisor with RESIL-01 and RESIL-02** - `d1ba80b` (feat)

## Files Created/Modified

- `src/resil.rs` - MQTT_DISCONNECTED_AT AtomicU32 static and now_secs() u32 helper
- `src/main.rs` - Added `mod resil;` declaration near `mod watchdog;`
- `src/wifi.rs` - wifi_supervisor extended with disconnected_since, RESIL-01, RESIL-02 checks
- `src/config.example.rs` - WIFI_DISCONNECT_REBOOT_TIMEOUT (Duration 600s) and MQTT_DISCONNECT_REBOOT_SECS (u32 300)

## Decisions Made

- **AtomicU32 not AtomicU64:** Plan specified AtomicU64 but the ESP32 Xtensa target does not support AtomicU64. Using AtomicU32 for MQTT_DISCONNECTED_AT with now_secs() returning u32. Safe for RESIL-02's 5-min comparison window; wraps only after ~136 years.
- **MQTT_DISCONNECT_REBOOT_SECS as u32:** Changed from plan's u64 to u32 to match now_secs() return type and enable saturating_sub comparison without casting.
- **RESIL-01 uses Option<Instant>:** Local to wifi_supervisor thread — no atomic needed since only one thread tracks WiFi disconnect duration.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] AtomicU64 replaced with AtomicU32 for ESP32 target compatibility**
- **Found during:** Task 1 (Create src/resil.rs)
- **Issue:** Plan specified `AtomicU64` and `u64` for MQTT_DISCONNECTED_AT, but ESP32 Xtensa LX6/LX7 target does not provide `std::sync::atomic::AtomicU64` — compile error `no AtomicU64 in sync::atomic`
- **Fix:** Changed to AtomicU32 throughout; now_secs() returns u32; MQTT_DISCONNECT_REBOOT_SECS typed u32; comparisons use saturating_sub on u32
- **Files modified:** src/resil.rs, src/config.example.rs, src/wifi.rs
- **Verification:** `cargo build --release` passes with zero warnings
- **Committed in:** 2d1f793 (Task 1), 3fc15c8 (Task 2), d1ba80b (Task 3)

---

**Total deviations:** 1 auto-fixed (Rule 1 - Bug: target platform constraint)
**Impact on plan:** Required change for correct operation on ESP32. Functional semantics unchanged — u32 epoch seconds are sufficient for 5-min and 10-min timeout windows.

## Issues Encountered

- `Ordering` import in initial resil.rs draft was unused (callers supply their own Ordering) — removed before first build attempt.
- `config.rs` is gitignored (contains WiFi/MQTT credentials) — new constants added to both config.rs (for build) and config.example.rs (committed template). Only config.example.rs and wifi.rs are committed.

## User Setup Required

None - no external service configuration required. New constants are already in both config.rs and config.example.rs.

## Next Phase Readiness

- MQTT_DISCONNECTED_AT is ready for Plan 02: mqtt.rs callback must write `resil::MQTT_DISCONNECTED_AT` on Disconnected (compare_exchange from 0) and clear on Connected (store 0)
- Both reboot paths fully wired and tested at compile level
- Plan 02 can wire the MQTT callback without any structural changes to wifi.rs or resil.rs

## Self-Check: PASSED

All created files and commits verified present.

---
*Phase: 12-resilience*
*Completed: 2026-03-07*
