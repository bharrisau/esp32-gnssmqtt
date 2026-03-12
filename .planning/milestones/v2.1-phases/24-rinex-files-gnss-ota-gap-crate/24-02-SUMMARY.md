---
phase: 24-rinex-files-gnss-ota-gap-crate
plan: "02"
subsystem: gnss-server
tags: [rinex, gnss, rust, navigation, gps, glonass, chrono, d19.12]

# Dependency graph
requires:
  - phase: 24-01
    provides: RinexObsWriter, write_obs_header, write_epoch, EphemerisMsg types
provides:
  - to_d19_12() formatter — 19-char Fortran D notation with two-digit signed exponent
  - gps_tow_to_utc() — GPS week + TOW to UTC DateTime (18s leap second correction)
  - current_gps_week() — current GPS week from system clock
  - write_nav_header() — RINEX 2.11 GPS nav file header writer
  - write_gps_nav() — 8-line GPS navigation record writer from Msg1019T
  - write_glo_nav() — 4-line GLONASS navigation record writer from Msg1020T
  - RinexNavWriter struct with hourly file rotation (.26P naming)
  - run_decode_task fully wired — forwards EpochGroup to obs writer, EphemerisMsg to nav writer
  - output_dir config field with './rinex_output' default
affects: [25-web-ui-gap-skeletons]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - D19.12 formatter: split mantissa/exponent from Rust {:E} notation, reformat as D+{:+03}
    - GPS week tracking: compute from system clock at startup; store in writer structs for file rotation
    - GLONASS nav record tk_s = tk_h*3600 + tk_min*60 + tk_s (field from Msg1020T)
    - tau_n_s negated for RINEX convention (RINEX clock_bias = -tau_n)
    - gps_week field stored but not yet used in file naming — allowed dead_code; reserved for rollover handling

key-files:
  created: []
  modified:
    - gnss-server/src/rinex_writer.rs
    - gnss-server/src/main.rs
    - gnss-server/src/config.rs

key-decisions:
  - "to_d19_12 zero case returns ' 0.000000000000D+00' (19 chars, 1 leading space) not '  0.000000000000D+00' (20 chars) — plan had a typo in the example string; RINEX D19.12 is 19 chars total"
  - "to_d19_12 tested for exponents in -99..+99 range (GPS/GLONASS data); f64::MIN_POSITIVE exponent -308 is out of scope for RINEX nav values"
  - "GLONASS tau_n_s negated via -msg.tau_n_s (f64 negation) not -(msg.tau_n_s as f64) — clippy unnecessary_cast"
  - "write_nav_header uses 'NAVIGATION DATA' label (not 'N: GPS NAV DATA') to match RINEX 2.11 label area at col 61"
  - "gps_week field in both RinexObsWriter and RinexNavWriter allowed dead_code — stored for potential GPS week rollover handling in future"

requirements-completed: [RINEX-03, RINEX-04]

# Metrics
duration: 7min
completed: 2026-03-12
---

# Phase 24 Plan 02: RINEX 2.11 Navigation Writer and End-to-End Wiring Summary

**RINEX 2.11 nav writer (GPS + GLONASS, D19.12 format) with GPS week tracking, wired into run_decode_task alongside obs writer to produce .26O and .26P files from RTCM3 stream**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-12T07:45:26Z
- **Completed:** 2026-03-12T07:52:33Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Implemented `to_d19_12()` producing exactly 19-char Fortran D notation with two-digit signed exponent (`D+04` not `D+4`); handles zero, positive, and negative values
- Implemented `current_gps_week()` and `gps_tow_to_utc()` using GPS epoch 1980-01-06 with 18-second leap second correction
- Implemented `write_nav_header()` with RINEX 2.11 three-line GPS nav header at 80-char column-exact format
- Implemented `write_gps_nav()` — 8-line GPS navigation record mapping all Msg1019T fields to RINEX orbit records 1-7
- Implemented `write_glo_nav()` — 4-line GLONASS navigation record with tk_s frame time, -tau_n clock bias, and km-unit orbital parameters from Msg1020T
- `RinexNavWriter` struct with hourly rotation to `.{yy}P` files (same convention as obs writer's `.{yy}O`)
- Galileo and BeiDou ephemeris log a warning and skip — RINEX 2.11 requires GPS and GLONASS only
- Added `output_dir` field to `ServerConfig` with `serde(default)` defaulting to `./rinex_output`
- `run_decode_task` updated: initializes both writers from config, forwards `EpochGroup` to `obs_writer.write_group()` with GPS-TOW-derived UTC epoch, forwards `EphemerisMsg` to `nav_writer.write_ephemeris()` with `Utc::now()`
- Output directory created with `std::fs::create_dir_all` at server startup
- All 31 gnss-server tests pass; `cargo clippy -p gnss-server -- -D warnings` clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Navigation writer and D19.12 formatter** - `7815d8c` (feat)
2. **Task 2: Wire RINEX writers into run_decode_task and config** - `649fd87` (feat)

## Files Created/Modified

- `gnss-server/src/rinex_writer.rs` — Added to_d19_12, current_gps_week, gps_tow_to_utc, write_nav_header, write_gps_nav, write_glo_nav, RinexNavWriter; removed module-level allow(dead_code); added field-level allow(dead_code) for gps_week; 5 new TDD unit tests
- `gnss-server/src/main.rs` — run_decode_task accepts output_dir+station, initializes both writers, routes epoch/ephemeris events; output directory created at startup
- `gnss-server/src/config.rs` — Added output_dir field with serde default "./rinex_output" to ServerConfig

## Decisions Made

- `to_d19_12(0.0)` returns `" 0.000000000000D+00"` (19 chars, 1 leading space) — the plan example `"  0.000000000000D+00"` had 2 leading spaces (20 chars), which was a typo; RINEX D19.12 is 19 chars total
- `to_d19_12` test uses exponents -99..+99; `f64::MIN_POSITIVE` (exponent -308) is out of scope for GNSS nav data and was removed from the test array
- `write_nav_header` uses `"NAVIGATION DATA"` label — the plan draft `"N: GPS NAV DATA"` fails the "contains 'NAVIGATION DATA'" test assertion; RINEX 2.11 sec 5.3 uses "NAVIGATION DATA"
- `gps_week` fields in `RinexNavWriter` and `RinexObsWriter` marked `#[allow(dead_code)]` — stored for potential rollover detection without breaking clippy

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed to_d19_12 zero string length (20 chars → 19 chars)**
- **Found during:** Task 1 (GREEN phase — test failure)
- **Issue:** Plan example `"  0.000000000000D+00"` has 2 leading spaces = 20 chars. RINEX D19.12 specifies 19-char total width. Correct zero representation is `" 0.000000000000D+00"` (1 leading space).
- **Fix:** Updated both the function return value and the test assertion to 19-char string
- **Files modified:** gnss-server/src/rinex_writer.rs
- **Committed in:** 7815d8c

**2. [Rule 1 - Bug] Fixed nav header label area — plan used "N: GPS NAV DATA" which doesn't contain "NAVIGATION DATA"**
- **Found during:** Task 1 (GREEN phase — test failure)
- **Issue:** Plan suggested `"N: GPS NAV DATA"` but the test asserts `data_area.contains("NAVIGATION DATA")`. RINEX 2.11 section 5.3 uses `"NAVIGATION DATA"` as the file type identifier.
- **Fix:** Changed nav header to use `"NAVIGATION DATA"` label
- **Files modified:** gnss-server/src/rinex_writer.rs
- **Committed in:** 7815d8c

---

**Total deviations:** 2 auto-fixed (Rule 1 — incorrect test expectation and spec wording)
**Impact on plan:** Both fixes align with RINEX 2.11 spec. No functional change to file format correctness.

## Manual Validation Note (RINEX-04)

Per plan success criteria: after running server against live FFFEB5 RTCM3 stream, run:
```
rnx2rtkp -x 5 GNSS0600.26O GNSS0600.26P
```
Document result in VALIDATION.md. This requires hardware access to device FFFEB5 and is deferred to the hardware testing phase per project convention (see testing.md).

---

## Self-Check: PASSED

- FOUND: gnss-server/src/rinex_writer.rs
- FOUND: gnss-server/src/main.rs
- FOUND: gnss-server/src/config.rs
- FOUND commit 7815d8c (Task 1 — nav writer implementation)
- FOUND commit 649fd87 (Task 2 — wiring)

---
*Phase: 24-rinex-files-gnss-ota-gap-crate*
*Completed: 2026-03-12*
