---
phase: 19-pre-2-0-bugfix
plan: "03"
subsystem: firmware
tags: [gpio, led, button, factory-reset, softap, nvs, esp32]

# Dependency graph
requires:
  - phase: 19-02
    provides: NVS TLS versioning fix enabling reliable post-OTA MQTT; provisioning set_force_softap API
provides:
  - "LedState::ButtonHold (4): 100ms fast flash pattern for 3-10s hold warning"
  - "LedState::Off (5): steady off pattern for 10s+ danger zone"
  - "GPIO9 3-phase state machine: Idle/Warning/Danger with LED feedback and factory reset"
affects: [testing, hardware-sign-off]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "BtnPhase enum defined inside thread closure for minimal scope — no pub visibility needed"
    - "led_state_btn Arc clone created before led_state moves into led_task thread"
    - "nvs_flash_erase + esp_restart for factory reset — does not touch OTA partition"

key-files:
  created: []
  modified:
    - src/led.rs
    - src/main.rs

key-decisions:
  - "BtnPhase enum defined inside GPIO9 thread closure — no module-level visibility required"
  - "Factory reset uses nvs_flash_erase() (all namespaces) not nvs_flash_erase_default_partition — complete credential wipe for field recovery"
  - "led_state_btn cloned at Step 3d alongside led_state_wifi and led_state_mqtt — before led_state moves into led_task at Step 3e"
  - "Hold threshold at 3s transitions Idle→Warning (LED ButtonHold); 10s transitions Warning→Danger (LED Off) — release in each window acts accordingly"

patterns-established:
  - "Button hold LED feedback: use AtomicU8 store to ButtonHold variant during warning phase, Off during danger phase"

requirements-completed: [FEAT-1]

# Metrics
duration: 7min
completed: 2026-03-09
---

# Phase 19 Plan 03: Boot Button Rework (FEAT-1) Summary

**GPIO9 boot button reworked to 3-phase state machine: fast-flash LED warning at 3s, steady-off danger signal at 10s, factory NVS erase on release after 10s**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-09T14:46:32Z
- **Completed:** 2026-03-09T14:48:26Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Added `LedState::ButtonHold` (4) with 100ms/100ms fast flash pattern and `LedState::Off` (5) with steady off — both exhaustively covered in `led_task` match
- Reworked GPIO9 monitor from a single 3s threshold to a 3-phase state machine (Idle/Warning/Danger) with LED feedback at each threshold
- Factory reset path erases all NVS namespaces via `nvs_flash_erase()` then restarts — OTA slot not touched, credentials cleared for field recovery

## Task Commits

Each task was committed atomically:

1. **Task 1: Add ButtonHold and Off variants to led.rs** - `5367bab` (feat)
2. **Task 2: Rework GPIO9 monitor in main.rs to 3-phase state machine** - `7a584ad` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified
- `src/led.rs` - Added ButtonHold=4 and Off=5 variants; updated from_u8 and led_task match arms
- `src/main.rs` - Added led_state_btn clone at Step 3d; replaced single-threshold GPIO9 loop with BtnPhase state machine

## Decisions Made
- BtnPhase enum defined inside the thread closure (not module-level) — sufficient scope, no pub needed
- Factory reset calls `nvs_flash_erase()` (all namespaces), not `nvs_flash_erase_default_partition()` — complete credential wipe intended for field recovery
- `led_state_btn` cloned at Step 3d before `led_state` moves into the LED task thread — available to the GPIO9 thread spawned at Step 19

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- FEAT-1 complete — boot button rework ships with v2.0
- All three Phase 19 plans complete: BUG-1 (DHCP DNS), BUG-3/BUG-4 (NVS TLS versioning), FEAT-1 (boot button)
- Hardware sign-off checklist in `testing.md` remains pending (OTA on FFFEB5, heartbeat GNSS fields, SoftAP captive portal, new boot button behaviour)
- Milestone v2.0 code complete pending hardware testing

---
*Phase: 19-pre-2-0-bugfix*
*Completed: 2026-03-09*

## Self-Check: PASSED
- src/led.rs: FOUND
- src/main.rs: FOUND
- 19-03-SUMMARY.md: FOUND
- Commit 5367bab: FOUND
- Commit 7a584ad: FOUND
