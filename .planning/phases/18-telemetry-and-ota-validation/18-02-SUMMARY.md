---
phase: 18-telemetry-and-ota-validation
plan: 02
subsystem: infra
tags: [ota, esp32, firmware, captive-portal, softap, hardware-validation]

# Dependency graph
requires:
  - phase: 18-01
    provides: heartbeat JSON with fix_type/satellites/hdop fields (confirmed Plan 01 integration)
  - phase: 17-04
    provides: captive portal DNS hijack and SoftAP provisioning (deferred hardware verify)
provides:
  - Canary firmware image (esp32-gnssmqtt-canary.bin) built and SHA-256 recorded
  - testing.md checklist at project root for deferred hardware sign-off session
affects: [milestone-v2.0-signoff]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Deferred hardware validation pattern: build artefacts + testing.md checklist allow sign-off session independent of code execution"

key-files:
  created:
    - testing.md
  modified:
    - src/main.rs

key-decisions:
  - "Hardware validation (OTA end-to-end + captive portal SoftAP) deferred to a dedicated sign-off session before v2.0 milestone tag — MAINT-03 not yet closed"
  - "Canary log line retained in src/main.rs (not reverted); final decision on permanent vs temporary deferred to sign-off session"
  - "testing.md written to project root with full step-by-step checklist covering OTA, heartbeat GNSS fields, and SoftAP portal — self-contained for operator use"

patterns-established:
  - "Hardware-deferred pattern: produce build artefact + checklist in same plan, defer sign-off to end-of-milestone session"

requirements-completed: []

# Metrics
duration: 5min
completed: 2026-03-08
---

# Phase 18 Plan 02: OTA Validation Build and Deferred Hardware Sign-Off Summary

**Canary firmware built (SHA-256 recorded) and full hardware validation checklist written to testing.md; OTA and captive portal sign-off deferred to end-of-milestone session**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-03-08T22:02:00Z
- **Completed:** 2026-03-08T22:07:32Z
- **Tasks:** 1 completed, 1 deferred
- **Files modified:** 2

## Accomplishments

- Built canary firmware image with distinguishable `v2.0-ota-canary` startup log line in `src/main.rs`
- `cargo build --release` succeeded; binary SHA-256 recorded: `a395675b9d8fc951070100dfedacedc27881eb0585be11a6d52543aeac611dda`
- Wrote `testing.md` to project root — self-contained hardware validation checklist covering Part A (OTA firmware update), Part B (heartbeat GNSS fields), Part C (SoftAP captive portal), and the canary version line decision
- Hardware validation session on device FFFEB5 deferred to a dedicated sign-off session before the v2.0 milestone tag

## Task Commits

Each task was committed atomically:

1. **Task 1: Build canary firmware image for OTA validation** - `dbbf794` (feat)
2. **Task 2: Hardware validation — OTA + captive portal on device FFFEB5** - DEFERRED (no commit; checklist in testing.md)

**Plan metadata:** (this commit)

## Files Created/Modified

- `src/main.rs` - Added canary startup log line `esp32-gnssmqtt v2.0-ota-canary — OTA validation build`
- `testing.md` - Full hardware validation checklist; SHA-256 of canary binary recorded; sign-off checklist for OTA, TELEM-01 heartbeat, and SoftAP portal

## Decisions Made

- Hardware validation (OTA end-to-end + captive portal SoftAP detection) deferred to a dedicated hands-on session before the v2.0 milestone tag. The build artefact and checklist are ready; no hardware commands were attempted.
- The canary log line in `src/main.rs` is left in place (not reverted); the decision on whether to keep it as a permanent version marker or revert it before tagging is deferred to the sign-off session.
- `testing.md` consolidates all three deferred hardware validations (OTA, heartbeat field verification, SoftAP captive portal) into one operator-facing checklist to be completed in a single session.

## Deviations from Plan

### Deferred Hardware Validation

**Task 2 deferred by user — not a failure**
- **What was deferred:** Full hardware validation session on device FFFEB5 — OTA end-to-end update (MAINT-03) and SoftAP captive portal detection (deferred from Phase 17 Plan 04)
- **Reason:** User explicitly approved deferral; hardware testing to be conducted at end of milestone before v2.0 sign-off
- **Artefact produced:** `testing.md` in project root provides complete step-by-step instructions and a sign-off checklist
- **MAINT-03 status:** Not yet closed — remains open until hardware session completes

---

**Total deviations:** 1 deferred task (by user approval)
**Impact on plan:** Canary build is ready; sign-off deferred to end of milestone. No code quality concerns — clippy clean.

## Issues Encountered

None during automation. Hardware session not attempted.

## User Setup Required

See `testing.md` at project root for the hardware validation checklist. Must be completed before tagging v2.0:

- Part A: OTA firmware update on device FFFEB5 (MAINT-03)
- Part B: Heartbeat GNSS fields verification (fix_type/satellites/hdop)
- Part C: SoftAP captive portal detection on mobile device (deferred from Phase 17)
- Post-session: decide whether to keep or revert canary log line in `src/main.rs`

## Next Phase Readiness

- Phase 18 Plan 03 (README) is already complete (committed in a prior session)
- Remaining for milestone sign-off: hardware validation session using `testing.md`
- MAINT-03 requires hardware evidence before v2.0 can be tagged

---
*Phase: 18-telemetry-and-ota-validation*
*Completed: 2026-03-08*
