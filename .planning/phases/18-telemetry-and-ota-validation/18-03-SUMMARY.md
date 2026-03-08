---
phase: 18-telemetry-and-ota-validation
plan: "03"
subsystem: docs

tags: [readme, documentation, mqtt, ntrip, ota, provisioning, gnss]

requires:
  - phase: 17-ntrip-client
    provides: NTRIP client implementation, captive portal, log relay, all subsystems documented
  - phase: 18-telemetry-and-ota-validation
    provides: heartbeat GNSS state fields (fix_type/satellites/hdop with null sentinel semantics)

provides:
  - Open-source README.md at project root covering all v2.0 features

affects: [future-contributors, operators, end-users]

tech-stack:
  added: []
  patterns:
    - "README structure: Overview → Hardware → Features → MQTT Topic Reference → Setup → Build → Subsystem sections → Troubleshooting"

key-files:
  created:
    - README.md
  modified: []

key-decisions:
  - "README authored from source inspection (led.rs timing, heartbeat null sentinel semantics, NTRIP NVS persistence) rather than from documentation alone — ensures accuracy"

patterns-established:
  - "MQTT Topic Reference table as authoritative operator reference: Topic, Direction, Payload, QoS, Retain, Notes"

requirements-completed:
  - TELEM-01
  - MAINT-03

duration: 2min
completed: 2026-03-08
---

# Phase 18 Plan 03: README.md Summary

**Open-source project README covering all v2.0 features: MQTT topic reference, SoftAP provisioning steps, OTA procedure, NTRIP config, LED state table, and heartbeat field reference with null semantics**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-08T21:51:24Z
- **Completed:** 2026-03-08T21:53:19Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- Complete README.md at project root (244 lines) covering all v2.0 subsystems
- MQTT Topic Reference table with all 11 topics (direction, payload, QoS, retain, notes)
- First-time setup section documenting SoftAP captive portal provisioning step by step
- OTA update section with build, SHA-256, HTTP serve, and retained-trigger cleanup steps
- LED state timing table sourced directly from `src/led.rs` (Connecting 400 ms, Connected steady, Error 1300 ms triple-pulse, SoftAP 1000 ms)
- Health heartbeat field reference with fix_type table and null sentinel semantics documented
- Troubleshooting covering WiFi, MQTT, OTA, NTRIP, and GNSS failure modes

## Task Commits

1. **Task 1: Write README.md covering all v2.0 features** — `c5f2ec3` (docs)

**Plan metadata:** (final commit follows)

## Files Created/Modified

- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/README.md` — Open-source project documentation for all v2.0 features

## Decisions Made

- LED state table timing sourced from `src/led.rs` comments rather than plan approximations (e.g. Error is triple-pulse 1300 ms cycle not "rapid double-blink")
- fix_type null semantics explained with disambiguation: `null` = no GGA received, `0` = GGA received but no fix

## Deviations from Plan

None — plan executed exactly as written. LED States section sourced from `src/led.rs` to provide accurate timings rather than the approximate descriptions in the plan.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Phase 18 plan 03 (README) complete
- README is the final deliverable for phase 18 plan 03
- Hardware verification of captive portal detection remains deferred to end of milestone

---
*Phase: 18-telemetry-and-ota-validation*
*Completed: 2026-03-08*
