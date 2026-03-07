---
phase: 07-rtcm-relay
plan: 03
subsystem: firmware
tags: [rust, esp32, main.rs, rtcm, mqtt, wiring]

# Dependency graph
requires:
  - phase: 07-01
    provides: "topic-discriminated pump_mqtt_events with MQTT out_buffer_size 2048"
  - phase: 07-02
    provides: "rtcm_relay.rs module and updated gnss.rs spawn_gnss returning (cmd_tx, nmea_rx, rtcm_rx)"
provides:
  - "main.rs wired with mod rtcm_relay declaration"
  - "spawn_gnss call site destructures three return values (gnss_cmd_tx, nmea_rx, rtcm_rx)"
  - "rtcm_relay::spawn_relay called after NMEA relay with rtcm_rx receiver"
  - "Complete Phase 7 firmware compiles and links (cargo build --release exits 0)"
affects:
  - "08-ota-update"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Module wiring pattern: add mod declaration, update call site destructure, spawn relay thread after NMEA relay"

key-files:
  created: []
  modified:
    - src/main.rs

key-decisions:
  - "No code changes required in this plan — all three main.rs changes (mod rtcm_relay, three-value destructure, spawn_relay call) were implemented as an auto-fix deviation during plan 07-01 to unblock compilation"

patterns-established:
  - "Step 16 in main.rs initialization order: RTCM relay spawned after NMEA relay (Step 14) and Config relay (Step 15)"

requirements-completed:
  - RTCM-03
  - RTCM-04

# Metrics
duration: 5min
completed: 2026-03-07
---

# Phase 7 Plan 03: Wire rtcm_relay into main.rs Summary

**main.rs wired with mod rtcm_relay, three-value spawn_gnss destructure, and rtcm_relay::spawn_relay — completing Phase 7 RTCM relay integration; firmware compiles and links at release profile**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-07T03:25:00Z
- **Completed:** 2026-03-07T03:30:00Z
- **Tasks:** 1
- **Files modified:** 0 (work was pre-completed)

## Accomplishments

- Verified all three required main.rs changes are in place: `mod rtcm_relay` at line 37, `(gnss_cmd_tx, nmea_rx, rtcm_rx)` destructure at line 91, `rtcm_relay::spawn_relay` call at line 159
- Confirmed `cargo build --release` exits 0 with no errors — full Phase 7 firmware compiles and links
- Completed full Phase 7 integration checks: `grep -rn "rtcm_relay" src/` shows mod+spawn+self; `grep -n "out_buffer_size"` shows 2048; `grep -n "RxState"` shows state machine; `grep -n "ends_with.*config"` shows topic discrimination

## Task Commits

No new code commits required for this plan — the main.rs changes were implemented during plan 07-01 as an auto-fix deviation:

1. **Task 1: Wire rtcm_relay into main.rs** - pre-completed in `54e02b8` (fix(07-01): topic-discriminated routing in pump_mqtt_events)

**Plan metadata:** (see final commit in this plan)

## Files Created/Modified

- `src/main.rs` - Pre-modified in 07-01: mod rtcm_relay declaration (line 37), spawn_gnss three-value destructure (line 91), Step 16 rtcm_relay::spawn_relay call (line 159)

## Decisions Made

None - the wiring was performed automatically as a deviation during plan 07-01 to unblock compilation. The implementation exactly matches the plan specification.

## Deviations from Plan

### Pre-completed Work

The sole task of this plan (wiring rtcm_relay into main.rs) was completed in plan 07-01 as a Rule 3 auto-fix (blocking issue: the three-value destructure was required for the firmware to compile after gnss.rs was updated to return an rtcm_rx channel). All success criteria verified correct:

- `mod rtcm_relay;` present at line 37
- `let (gnss_cmd_tx, nmea_rx, rtcm_rx) = gnss::spawn_gnss(...)` at line 91
- `rtcm_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), rtcm_rx)` at line 159
- Step 16 comment present in initialization comment header

No further code changes were needed in this plan.

---

**Total deviations:** 0 in this plan (pre-existing work from 07-01 auto-fix)
**Impact on plan:** All success criteria satisfied. No scope creep.

## Issues Encountered

None — build passes cleanly, all verification checks pass.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 7 (RTCM relay) is fully complete: all five requirements (RTCM-01 through RTCM-05) implemented and verified
- Phase 8 (OTA firmware update) is unblocked — partition table redesign is the first required step
- Prerequisite reminder: `espflash erase-flash` + USB reflash required before any OTA code is testable (existing factory partition leaves no room for OTA slots)

---
*Phase: 07-rtcm-relay*
*Completed: 2026-03-07*
