---
phase: 23-mqtt-rtcm3-gnss-nvs-crate
plan: "03"
subsystem: server
tags: [rust, rtcm3, rtcm-rs, gnss, tokio, epoch-buffer, msm, ephemeris, tdd]

# Dependency graph
requires:
  - phase: 23-mqtt-rtcm3-gnss-nvs-crate/23-02
    provides: MqttMessage enum (Rtcm/Nmea/Heartbeat), mpsc msg_rx channel, gnss-server binary skeleton
provides:
  - observation.rs: Observation, EpochGroup, EphemerisMsg, RtcmEvent types (Phase 24 RINEX writer contracts)
  - epoch.rs: EpochBuffer with push/flush-on-change logic and ISO8601 epoch boundary logging
  - rtcm_decode.rs: decode_rtcm_payload() dispatching 8 MSM variants and 4 ephemeris variants
  - run_decode_task() Tokio task wired to MqttMessage::Rtcm from mpsc channel
  - tests/fixtures/rtcm_sample.bin: real GPS MSM4 (1074) frame from gnss.log
affects:
  - 23-04 (gnss-nvs crate — consumes gnss-server patterns)
  - 24 (RINEX writer consumes Observation, EpochGroup, EphemerisMsg types)

# Tech tracking
tech-stack:
  added:
    - rtcm-rs 0.11 (RTCM3 frame parsing; next_msg_frame loop; MSM + ephemeris decoding)
    - chrono 0.4 with clock feature (UTC timestamp for epoch boundary log line)
  patterns:
    - next_msg_frame loop: (consumed, Some(frame)) / (0, None) break / (consumed, None) skip
    - EpochBuffer flush-on-change: epoch_key=0 means no epoch yet; different epoch_ms triggers flush
    - Signal extraction inline in match arms (avoids naming private msg1074_sig::DataType)
    - cnr_dbhz MSM4 is Option<u8> (df403 with inv:0); convert to Option<f64> with .map(|v| v as f64)
    - cnr_dbhz MSM7 is Option<f64> (df408 with inv:0)
    - #[allow(dead_code)] on forward-compat types (Observation fields, EpochGroup.observations consumed by Phase 24)

key-files:
  created:
    - gnss-server/src/observation.rs
    - gnss-server/src/epoch.rs
    - gnss-server/src/rtcm_decode.rs
    - gnss-server/tests/fixtures/rtcm_sample.bin
  modified:
    - gnss-server/Cargo.toml
    - gnss-server/src/main.rs

key-decisions:
  - "BeiDou ephemeris is RTCM msg 1042 (Msg1042T), not 1044 (which is QZSS); plan had incorrect type reference"
  - "Signal data extracted inline in match arms — avoids referencing private msg1074_sig::DataType module path"
  - "cnr_dbhz MSM4 field is Option<u8> (df403 inv:0), MSM7 is Option<f64> (df408 inv:0) — convert MSM4 with .map(|v| v as f64)"
  - "EpochBuffer::new() uses epoch_key=0 as sentinel for no-epoch-yet; first push always accumulates without flush"
  - "GLONASS carrier_phase_ms passed through as-is — None when MSM field returns None; FCN conversion deferred to Phase 24"

patterns-established:
  - "Pattern 1: decode_rtcm_payload takes &[u8] + &mut EpochBuffer, returns Vec<RtcmEvent> — stateless from caller perspective"
  - "Pattern 2: EpochBuffer owns accumulated state; caller receives Option<EpochGroup> on each push"
  - "Pattern 3: Observation.constellation + sv_id + signal_id uniquely identifies a measurement; epoch_ms on each Observation for convenience"

requirements-completed: [RTCM-01, RTCM-02, RTCM-03, RTCM-04]

# Metrics
duration: 7min
completed: 2026-03-12
---

# Phase 23 Plan 03: RTCM3 Decode Pipeline Summary

**rtcm-rs 0.11 MSM4/MSM7 decode with EpochBuffer flush-on-change, ISO8601 epoch logging, and Tokio decode task consuming MqttMessage::Rtcm**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-03-12T06:12:43Z
- **Completed:** 2026-03-12T06:19:44Z
- **Tasks:** 1 (TDD: RED tests + GREEN implementation in single commit)
- **Files modified:** 6 (4 created, 2 modified)

## Accomplishments
- decode_rtcm_payload handles all 8 MSM variants (GPS/GLO/GAL/BDS MSM4+MSM7) and 4 ephemeris types (1019/1020/1046/1042)
- EpochBuffer accumulates observations by epoch_ms, flushes when epoch changes, logs `Epoch {ISO8601} GPS:{n} GLO:{n} GAL:{n} BDS:{n}`
- 8 unit tests pass across epoch::tests (4) and rtcm_decode::tests (4) using real GPS MSM4 fixture from gnss.log
- Tokio run_decode_task wired to mpsc::Receiver<MqttMessage> — Phase 24 RINEX writer will replace discard logic

## Task Commits

Single TDD commit (RED + GREEN + clippy fixes combined):

1. **Task 1: RTCM3 decode pipeline** - `73016a7` (feat)

## Files Created/Modified
- `gnss-server/src/observation.rs` — Constellation, Observation, EpochGroup, EphemerisMsg, RtcmEvent types
- `gnss-server/src/epoch.rs` — EpochBuffer with push/flush logic, chrono epoch log, 4 unit tests
- `gnss-server/src/rtcm_decode.rs` — decode_rtcm_payload dispatching all MSM and ephemeris types, 4 unit tests
- `gnss-server/src/main.rs` — added mod declarations + run_decode_task async fn
- `gnss-server/Cargo.toml` — added rtcm-rs 0.11 and chrono 0.4 dependencies
- `gnss-server/tests/fixtures/rtcm_sample.bin` — 148-byte GPS MSM4 (type 1074) frame from gnss.log

## Decisions Made
- BeiDou ephemeris uses msg1042 (Msg1042T) not 1044 (QZSS) — plan frontmatter had wrong type; corrected
- Signal extraction inline in match arms (not helper functions) to avoid naming private `msg1074_sig::DataType`
- MSM4 CNR is `Option<u8>` (df403 with inv=0); MSM7 CNR is `Option<f64>` (df408 with inv=0) — MSM4 converted with `.map(|v| v as f64)` for uniform Observation type
- `#[allow(dead_code)]` on Observation fields and EpochGroup.observations — consumed by Phase 24 RINEX writer

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] BeiDou ephemeris is msg1042 (Msg1042T), not msg1044 (QZSS)**
- **Found during:** Task 1 (observation.rs type definitions)
- **Issue:** Plan specified `Beidou(Msg1044T)` but msg1044 is QZSS ephemeris; BeiDou ephemeris is msg1042 (Msg1042T)
- **Fix:** `EphemerisMsg::Beidou` wraps `Msg1042T` and `handle_message` matches `Message::Msg1042`
- **Files modified:** gnss-server/src/observation.rs, gnss-server/src/rtcm_decode.rs
- **Verification:** `cargo build -p gnss-server` passes; all tests pass
- **Committed in:** 73016a7 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - incorrect type in plan)
**Impact on plan:** Fix required for correctness — QZSS is a different constellation than BeiDou.

## Issues Encountered
- rtcm-rs msg modules are private sub-modules; signal data type (`msg1074_sig::DataType`) not directly nameable — resolved by extracting observations inline in match arms rather than via helper functions with typed parameters

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- Plan 23-04 (gnss-nvs crate) can proceed — no dependencies on this plan
- Plan 24 (RINEX writer) can consume: Observation, EpochGroup, EphemerisMsg from observation.rs; run_decode_task can be modified to forward EpochGroup instead of discarding
- GLONASS FCN conversion from raw carrier_phase_ms to cycles is deferred to Phase 24

## Self-Check: PASSED

- gnss-server/src/observation.rs: FOUND
- gnss-server/src/epoch.rs: FOUND
- gnss-server/src/rtcm_decode.rs: FOUND
- gnss-server/tests/fixtures/rtcm_sample.bin: FOUND
- Commit 73016a7 (Task 1): FOUND

---
*Phase: 23-mqtt-rtcm3-gnss-nvs-crate*
*Completed: 2026-03-12*
