---
phase: 24-rinex-files-gnss-ota-gap-crate
plan: "01"
subsystem: gnss-server
tags: [rinex, rtcm, gnss, rust, chrono, observation, pseudorange]

# Dependency graph
requires:
  - phase: 23-mqtt-rtcm3-gnss-nvs-crate
    provides: Observation struct, EpochGroup, rtcm_decode.rs with MSM4/MSM7 signal extraction
provides:
  - RINEX 2.11 observation file header writer with exact 80-char column-positioned lines
  - RINEX 2.11 epoch record writer with >12 satellite continuation line support
  - Full pseudorange reconstruction (rough_range_ms + fine_ms) in Observation struct
  - RinexObsWriter struct with hourly file rotation logic
  - Unit converters: pseudorange_m, carrier_phase_cycles, cnr_to_ssi, to_rinex_prn
affects: [24-02, 25-web-ui-gap-skeletons]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - RINEX 2.11 fixed-width formatting: each header line = 60-char data field + 20-char label, exactly 80 chars total
    - Observation struct stores full reconstructed pseudorange_ms = rough_int + rough_mod + fine; rinex_writer reads it directly
    - GLONASS carrier phase written as None (16 spaces) because FCN not available in MSM signal data
    - #![allow(dead_code)] at module level for rinex_writer until wired to main in later plan

key-files:
  created:
    - gnss-server/src/rinex_writer.rs
  modified:
    - gnss-server/src/observation.rs
    - gnss-server/src/rtcm_decode.rs
    - gnss-server/src/epoch.rs
    - gnss-server/src/main.rs

key-decisions:
  - "write_obs(lli=0, ssi=7) correctly produces 16 chars ('  23514789.12307') not 17 — plan test expectation was wrong (extra space); RINEX 2.11 I1 format for both LLI and SSI means 0 is written as '0' not space"
  - "pseudorange_ms stores full reconstructed value in Observation; rough_range_ms field kept for documentation/debugging"
  - "GLONASS carrier phase returns None from carrier_phase_cycles() — FCN required but absent in MSM4/MSM7 signal data; written as 16 spaces per RINEX 2.11 spec"
  - "RinexObsWriter uses std::fs::File + std::io::BufWriter (synchronous) — RINEX writer runs inside async task, file I/O fast enough at 1 Hz epoch rate"

patterns-established:
  - "RINEX header line format: writeln!(w, \"{:<60}{:<20}\", data_field, label) produces exactly 80-char lines"
  - "MSM satellite lookup: signal_data entry has satellite_id matching satellite_data entry; use iter().find() to join"

requirements-completed: [RINEX-01, RINEX-02]

# Metrics
duration: 11min
completed: 2026-03-12
---

# Phase 24 Plan 01: RINEX 2.11 Observation Writer Summary

**RINEX 2.11 obs writer with hourly rotation, 80-char column-exact headers, and full MSM pseudorange reconstruction (rough+fine) stored in Observation struct**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-12T06:28:41Z
- **Completed:** 2026-03-12T06:40:00Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- Added `rough_range_ms: f64` field to `Observation` struct and updated all 8 MSM4/MSM7 match arms in `rtcm_decode.rs` to reconstruct full pseudorange (rough_int + rough_mod + fine)
- Implemented `gnss-server/src/rinex_writer.rs` with all required functions: `write_obs_header`, `write_epoch`, `write_obs`, `cnr_to_ssi`, `pseudorange_m`, `carrier_phase_cycles`, `to_rinex_prn`, and `RinexObsWriter` struct
- All 7 `rinex_writer` unit tests pass: 80-char header lines, label at col 61, write_obs None/Some, PRN formatting, cnr_to_ssi clamping, >12 satellite continuation line
- `cargo clippy -p gnss-server -- -D warnings` clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend Observation struct and implement RINEX obs writer** - `5ae7553` (feat)

## Files Created/Modified
- `gnss-server/src/rinex_writer.rs` — RINEX 2.11 obs header + epoch writer, RinexObsWriter hourly rotation struct, unit converters, 7 unit tests
- `gnss-server/src/observation.rs` — Added `rough_range_ms: f64` field to `Observation` struct
- `gnss-server/src/rtcm_decode.rs` — Updated all 8 MSM4/MSM7 match arms to reconstruct full pseudorange; updated 2 test constructors
- `gnss-server/src/epoch.rs` — Updated 3 test Observation constructors with `rough_range_ms: 0.0`
- `gnss-server/src/main.rs` — Added `mod rinex_writer;`

## Decisions Made
- `write_obs(lli=0, ssi=7)` correctly produces `"  23514789.12307"` (16 chars); the plan test expectation `"  23514789.123 07"` (17 chars) had an extra space — fixed the test assertion to match RINEX 2.11 I1 format
- `pseudorange_ms` stores full reconstructed range; `rough_range_ms` kept as a documentation field with `#[allow(dead_code)]`
- GLONASS `carrier_phase_cycles()` returns `None` unconditionally — FCN not available in MSM signal data
- `#![allow(dead_code)]` at module level for `rinex_writer` since it's not yet wired to `main.rs`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed incorrect test expectation for write_obs**
- **Found during:** Task 1 (GREEN phase — test was failing with wrong expected value)
- **Issue:** Plan specified `write_obs(Some(23514789.123), 0, 7)` should produce `"  23514789.123 07"` (17 chars). RINEX 2.11 I1 format for LLI and SSI means each is 1 char: `"0"` and `"7"`. The correct output is `"  23514789.12307"` (16 chars). The plan had an extra space between the decimal digits and LLI digit.
- **Fix:** Updated the test assertion to `"  23514789.12307"` with explanatory comment citing RINEX 2.11 spec
- **Files modified:** gnss-server/src/rinex_writer.rs
- **Verification:** Test passes; 16-char field satisfies RINEX 2.11 column layout for observations
- **Committed in:** 5ae7553 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — incorrect test expectation)
**Impact on plan:** The fix aligns the test with the RINEX 2.11 spec. No functional change to the writer output; 16-char fields are correct for RTKLIB compatibility.

## Issues Encountered
- The RESEARCH.md code example header line `"     2.11           OBSERVATION DATA    M (MIXED)   RINEX VERSION / TYPE"` was only 72 chars. Reconstructed the correct 80-char format by padding the data area to 60 chars using `{:<60}{:<20}` format pattern for all header lines.

## Next Phase Readiness
- `rinex_writer.rs` is complete and tested; ready to be wired to the decode task in a subsequent plan
- `RinexObsWriter.write_group()` accepts `&DateTime<Utc>` + `&EpochGroup` — same types already flowing through `run_decode_task`
- RINEX-03 (navigation file writer) and RINEX-04 (RTKLIB acceptance test) still pending

---
*Phase: 24-rinex-files-gnss-ota-gap-crate*
*Completed: 2026-03-12*
