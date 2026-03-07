---
phase: 12-resilience
plan: "02"
subsystem: infra
tags: [mqtt, resilience, atomic, esp32, watchdog]

# Dependency graph
requires:
  - phase: 12-01
    provides: "MQTT_DISCONNECTED_AT AtomicU32, now_secs(), RESIL-01 and RESIL-02 read paths in wifi_supervisor"
provides:
  - "MQTT callback writes MQTT_DISCONNECTED_AT on EventPayload::Disconnected (compare_exchange)"
  - "MQTT callback clears MQTT_DISCONNECTED_AT on EventPayload::Connected (store 0)"
  - "Complete RESIL-02 feedback loop: write side now wired to match read side from Plan 01"
affects:
  - phase-13-health-telemetry

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "crate::resil:: full path used inside callbacks — no use imports, consistent with crate::config:: convention in wifi.rs"
    - "compare_exchange(0, now_secs()) for idempotent disconnect stamping — only first disconnect within a session sets the timer"

key-files:
  created: []
  modified:
    - src/mqtt.rs

key-decisions:
  - "compare_exchange(0, now_secs()) in Disconnected arm — only stamps if currently 0 so repeated Disconnected events do not reset the timer"
  - "store(0, Relaxed) in Connected arm placed before subscribe_tx.try_send() — clears timer as early as possible once connection is confirmed"
  - "No use imports added — crate::resil:: inline path avoids polluting callback closure import namespace, matches existing crate::config:: pattern"

patterns-established:
  - "Atomic stores only inside MQTT callbacks — EspMqttClient methods are never called inside new_cb closure (deadlock prevention)"

requirements-completed: [RESIL-02]

# Metrics
duration: 15min
completed: 2026-03-07
---

# Phase 12 Plan 02: Resilience — MQTT Callback Wiring Summary

**MQTT callback now stamps MQTT_DISCONNECTED_AT on disconnect and clears it on reconnect, completing the RESIL-02 feedback loop started in Plan 01**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-07
- **Completed:** 2026-03-07
- **Tasks:** 2 (1 auto + 1 checkpoint:human-verify)
- **Files modified:** 1

## Accomplishments

- Added `compare_exchange(0, now_secs(), Relaxed, Relaxed).ok()` to `EventPayload::Disconnected` arm in MQTT callback — stamps disconnect time without overwriting a mid-session timer
- Added `store(0, Relaxed)` to `EventPayload::Connected` arm — clears disconnect timer the moment MQTT reconnects
- No EspMqttClient methods called from within callback — re-entrancy constraint preserved as documented in mqtt.rs module comment
- Human verification confirmed all four RESIL sites (wifi.rs x7, mqtt.rs x4, resil.rs x1) are correctly wired

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire MQTT_DISCONNECTED_AT writes into the MQTT callback** - `3419b9e` (feat)
2. **Task 2: Checkpoint — human verification approved** - no code commit (verification only)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/mqtt.rs` - Added RESIL-02 atomic writes in EventPayload::Disconnected and EventPayload::Connected arms

## Decisions Made

- `compare_exchange(0, now_secs())` used in Disconnected arm (not plain `store`) — if multiple Disconnected events fire, only the first stamps the timer; subsequent events are no-ops via `.ok()` discarding the Err
- `store(0, Relaxed)` used in Connected arm — unconditional clear is correct; any residual non-zero value is stale and should be reset
- `crate::resil::` full path used inline — consistent with `crate::config::` pattern elsewhere in the codebase, avoids adding a `use` import inside the closure

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Phase 12 (Resilience) is now complete: RESIL-01 (WiFi disconnect reboot after 10 min) and RESIL-02 (MQTT disconnect reboot after 5 min) are both fully wired
- Both paths compile cleanly; build verified in Task 1
- Hardware test with shortened timeouts (30s each) can be performed before or after next phase — no blockers

---
*Phase: 12-resilience*
*Completed: 2026-03-07*
