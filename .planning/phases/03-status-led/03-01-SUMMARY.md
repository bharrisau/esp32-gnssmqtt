---
phase: 03-status-led
plan: 01
subsystem: infra
tags: [rust, esp32, gpio, led, atomic, wifi]

# Dependency graph
requires:
  - phase: 02-connectivity
    provides: wifi_supervisor function in src/wifi.rs that this plan extends
provides:
  - LedState enum (Connecting/Connected/Error) with AtomicU8 encoding in src/led.rs
  - led_task function driving GPIO15 active-low with three blink patterns
  - wifi_supervisor updated to accept Arc<AtomicU8> and write Connecting/Error state transitions
affects:
  - 03-02 (mqtt.rs update to write Connected/Connecting on MQTT events)
  - 03-03 (main.rs wiring — create Arc<AtomicU8>, spawn led_task thread, update wifi_supervisor call)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Arc<AtomicU8> for single-writer multi-reader LED state — simpler than Mutex for u8 enum"
    - "50ms polling loop with elapsed_ms counter for blink timing — avoids blocking state changes"
    - "Active-low GPIO15: set_low() = LED on, set_high() = LED off"

key-files:
  created:
    - src/led.rs
  modified:
    - src/wifi.rs
    - src/main.rs

key-decisions:
  - "AtomicU8 chosen over Arc<Mutex<LedState>> — sufficient for single u8 value, no lock contention"
  - "elapsed_ms counter (not sleep-per-blink) so state changes apply within 50ms not at end of blink cycle"
  - "wifi_supervisor writes Connecting on disconnect before backoff sleep, Error after 3 consecutive max-backoff failures"
  - "wifi_supervisor never writes Connected — MQTT pump owns that transition (WiFi up != MQTT up)"
  - "mod led declared in main.rs at Task 1 so cargo check validates led.rs immediately"

patterns-established:
  - "LED state writes: wifi_supervisor owns Connecting/Error; mqtt pump owns Connected/Connecting(disconnect)"
  - "Blink timing via elapsed_ms modular arithmetic, polled every 50ms"

requirements-completed: [LED-01, LED-02, LED-03]

# Metrics
duration: 2min
completed: 2026-03-04
---

# Phase 03 Plan 01: Status LED State Module and WiFi Supervisor Wiring Summary

**LedState enum with active-low GPIO15 blink driver and wifi_supervisor extended with Arc<AtomicU8> for Connecting/Error state writes**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-04T05:05:31Z
- **Completed:** 2026-03-04T05:08:21Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Created src/led.rs with LedState enum (Connecting=0, Connected=1, Error=2), from_u8 conversion, and led_task blink driver for GPIO15 active-low
- Implemented three blink patterns: 200ms on/off (Connecting), steady on (Connected), 3x rapid pulse + 700ms off (Error) via 50ms polling loop
- Updated wifi_supervisor to accept Arc<AtomicU8> second parameter and write Connecting on disconnect, Error after 3 consecutive max-backoff (60s) failures
- Enforced architectural rule: wifi_supervisor never writes Connected (MQTT pump owns that transition)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create src/led.rs — LedState enum and led_task function** - `e3b5f44` (feat)
2. **Task 2: Update src/wifi.rs — add led_state parameter to wifi_supervisor** - `6af0a67` (feat)

**Plan metadata:** TBD (docs commit)

## Files Created/Modified
- `src/led.rs` - LedState enum, from_u8 conversion, led_task blink driver (GPIO15 active-low, 50ms poll)
- `src/wifi.rs` - wifi_supervisor updated: new Arc<AtomicU8> parameter, Connecting/Error state writes
- `src/main.rs` - Added `mod led;` declaration so module is compiled and checked

## Decisions Made
- Used `Arc<AtomicU8>` over `Arc<Mutex<LedState>>` — a single u8 is sufficient for three enum values, avoids lock overhead on the LED polling path
- elapsed_ms counter approach for blink timing rather than thread::sleep for full blink period — state changes reflect within 50ms regardless of where in the blink cycle they occur
- wifi_supervisor writes Connecting immediately on disconnect detection (before backoff sleep) so LED feedback is instant
- Error threshold: backoff_secs must already be at 60 (max) AND 3 consecutive failures — roughly 3+ minutes of unrecoverable WiFi failure

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered
- Cargo check after Task 2 shows one expected E0061 error (main.rs call site passes only 1 argument to wifi_supervisor, which now takes 2). This is correct and will be resolved in Plan 03-03 when main.rs is updated.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- src/led.rs is complete and ready for Plan 03-02 (mqtt.rs update to write Connected/Connecting)
- src/wifi.rs wifi_supervisor has correct signature; Plan 03-03 will update main.rs call site
- Arc<AtomicU8> shared state pattern established for all three state-writing modules

## Self-Check: PASSED

- src/led.rs: FOUND
- src/wifi.rs: FOUND
- .planning/phases/03-status-led/03-01-SUMMARY.md: FOUND
- Commit e3b5f44 (Task 1): FOUND
- Commit 6af0a67 (Task 2): FOUND

---
*Phase: 03-status-led*
*Completed: 2026-03-04*
