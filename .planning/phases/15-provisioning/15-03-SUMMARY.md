---
phase: 15-provisioning
plan: "03"
subsystem: provisioning
tags: [gpio, led, ota, softap, esp32, rust]

# Dependency graph
requires:
  - phase: 15-02
    provides: provisioning wired into main.rs boot-path, set_force_softap function in provisioning.rs
provides:
  - LedState::SoftAP = 3 with 500ms/500ms blink pattern
  - GPIO9 hold-3s trigger for SoftAP re-entry
  - MQTT "softap" payload trigger for SoftAP re-entry
  - SoftAP LED signaling in boot path before run_softap_portal
affects: [future-ota-changes, led-state-consumers]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Short-circuit payload check before JSON parse in ota_task (mirrors MAINT-01 reboot pattern)"
    - "GPIO polling with Instant elapsed timer; reset on release for debounce-free hold detection"
    - "SoftAP LED state set before blocking portal call so user gets visual feedback immediately"

key-files:
  created: []
  modified:
    - src/led.rs
    - src/ota.rs
    - src/main.rs

key-decisions:
  - "GPIO9 polled every 100ms with 3s hold threshold; timer resets on release preventing accidental re-entry"
  - "GPIO9 monitor spawned after watchdog supervisor (last of all threads) so all subsystems are up before monitoring begins"
  - "nvs passed by clone to spawn_ota (not Arc) because EspNvsPartition<NvsDefault> is already Clone and cheap"
  - "SoftAP LED state signaled via led_state_wifi clone which is still in scope at boot-path decision point"

patterns-established:
  - "Payload short-circuit pattern: check for special string values (.trim() == value) before JSON parse, then call set_force_softap + restart"

requirements-completed: [PROV-06, PROV-07, PROV-08]

# Metrics
duration: 3min
completed: 2026-03-08
---

# Phase 15 Plan 03: SoftAP Re-entry Triggers and LED Pattern Summary

**GPIO9 hold-3s and MQTT "softap" payload triggers for SoftAP re-entry, plus LedState::SoftAP 500ms blink pattern completing all 8 PROV requirements**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-03-08T00:43:00Z
- **Completed:** 2026-03-08T00:46:00Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added LedState::SoftAP = 3 with 500ms/500ms blink arm in led_task (PROV-08), visually distinct from Connecting (400ms) and Error (1300ms triple-pulse)
- Added "softap" payload check in ota_task before "reboot" check and before JSON parse — calls set_force_softap + restart (PROV-07)
- Spawned GPIO9 monitor thread (last, after watchdog) with 100ms polling, 3s hold detection, and instant timer reset on release (PROV-06)
- Set SoftAP LED state before run_softap_portal in boot path so user gets visual feedback immediately

## Task Commits

Each task was committed atomically:

1. **Task 1: LedState::SoftAP and ota_task "softap" trigger** - `37f54c8` (feat)
2. **Task 2: GPIO9 monitor thread** - `d3bfc46` (feat)

## Files Created/Modified
- `src/led.rs` - Added SoftAP = 3 variant, from_u8 arm, 500ms/500ms blink arm in led_task
- `src/ota.rs` - Added EspNvsPartition import, nvs parameter to ota_task/spawn_ota, "softap" payload check
- `src/main.rs` - Pass nvs to spawn_ota, set SoftAP LED before run_softap_portal, GPIO9 monitor thread

## Decisions Made
- GPIO9 polled every 100ms (not interrupt-driven) — simpler, adequate responsiveness for a 3s hold
- Timer resets on release to prevent accidental re-entry from intermittent contact
- GPIO9 monitor spawned last (after watchdog) so all subsystems are fully operational before GPIO monitoring begins
- nvs passed by clone to spawn_ota since EspNvsPartition<NvsDefault> implements Clone cheaply

## Deviations from Plan

None - plan executed exactly as written. The LED state signaling in the SoftAP boot branch was already part of Task 2's specification.

## Issues Encountered

None. Build passed on first attempt for both tasks.

## Next Phase Readiness
- All 8 PROV requirements fully implemented across plans 15-01, 15-02, 15-03
- Phase 15 provisioning complete: SoftAP portal, WiFi credential storage, MQTT config, boot-path selection, GPIO9 trigger, MQTT trigger, LED state
- Ready for next milestone phase

## Self-Check: PASSED

- FOUND: src/led.rs (LedState::SoftAP = 3 present)
- FOUND: src/ota.rs ("softap" check present, nvs param present)
- FOUND: src/main.rs (GPIO9 monitor thread present)
- FOUND: 15-03-SUMMARY.md
- FOUND commit 37f54c8: feat(15-03): LedState::SoftAP and ota_task "softap" trigger
- FOUND commit d3bfc46: feat(15-03): GPIO9 monitor thread for SoftAP re-entry trigger

---
*Phase: 15-provisioning*
*Completed: 2026-03-08*
