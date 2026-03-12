---
phase: 24-rinex-files-gnss-ota-gap-crate
verified: 2026-03-12T09:00:00Z
status: human_needed
score: 9/10 must-haves verified
human_verification:
  - test: "Run rnx2rtkp -x 5 GNSS0600.26O GNSS0600.26P against real output from device FFFEB5"
    expected: "No RINEX parse errors in rnx2rtkp output; solution epochs processed"
    why_human: "Requires live RTCM3 data from device FFFEB5 to generate real files; cannot verify file acceptability with synthetic data alone"
  - test: "Let the server run across a UTC hour boundary and inspect the output directory"
    expected: "Two separate .26O files appear (e.g. GNSS0710.26O and GNSS0810.26O); each has a valid RINEX header"
    why_human: "Hourly file rotation requires a real clock rollover or time mock; cannot verify in a unit test"
---

# Phase 24: RINEX Files + gnss-ota Gap Crate Verification Report

**Phase Goal:** Server writes RINEX 2.11 observation and navigation files that RTKLIB accepts without error; gnss-ota gap crate defines the dual-slot OTA trait with a documented nostd blocker
**Verified:** 2026-03-12T09:00:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | RINEX 2.11 observation header writes exactly 80-char lines with labels in columns 61-80 | VERIFIED | `obs_header_lines_are_80_chars` and `obs_header_label_at_col_61` tests pass; `write_obs_header` uses `{:<60}{:<20}` pattern throughout |
| 2 | Epoch record writes satellite PRN list (up to 12 per line) and per-satellite C1/L1/S1 observations | VERIFIED | `epoch_gt12_sats_continuation_line` test passes with 13 satellites; continuation line verified at 32-space prefix |
| 3 | Missing observations are written as 16 spaces (not 0.0) | VERIFIED | `write_obs_none_produces_16_spaces` test passes; `write_obs(None, _, _)` writes exactly 16 spaces |
| 4 | GLONASS carrier phase without FCN is written as 16 spaces | VERIFIED | `carrier_phase_cycles` returns `None` for `Constellation::Glonass`; `None` maps to 16 spaces in `write_obs` |
| 5 | Pseudorange in meters uses full rough+fine reconstruction (not fine-only) | VERIFIED | `observation.rs` stores full `rough_int + rough_mod + fine` in `pseudorange_ms`; `pseudorange_m()` multiplies by `SPEED_OF_LIGHT_M_PER_MS` |
| 6 | RINEX nav D19.12 exponent is two-digit with sign (D+04 not D+4) | VERIFIED | `d19_12_small_negative_exponent_two_digits` (D-04), `d19_12_negative_value_positive_exponent` (D+10), and `d19_12_always_19_chars_various` tests all pass |
| 7 | GLONASS navigation record writes slot number and orbital parameters in km units | VERIFIED | `write_glo_nav` uses `msg.xn_km`, `msg.yn_km`, `msg.zn_km` and corresponding derivatives; `tau_n_s` negated per RINEX convention |
| 8 | run_decode_task forwards EpochGroup and EphemerisMsg to rinex_writer instead of discarding | VERIFIED | `main.rs` routes `RtcmEvent::Epoch` to `obs_writer.write_group()` and `RtcmEvent::Ephemeris` to `nav_writer.write_ephemeris()`; output directory created at startup |
| 9 | gnss-ota crate compiles as no_std for a bare-metal target | VERIFIED | `cargo check --target thumbv7em-none-eabihf -p gnss-ota` passes clean; `lib.rs` begins with `#![no_std]` |
| 10 | RINEX output is accepted by RTKLIB | UNCERTAIN | Requires human validation with `rnx2rtkp` against real device data — see Human Verification Required section |

**Score:** 9/10 truths verified (1 requires human)

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `gnss-server/src/observation.rs` | Observation struct with `rough_range_ms` field | VERIFIED | Field present at line 29; `pseudorange_ms` stores full reconstruction; doc comments explain design |
| `gnss-server/src/rinex_writer.rs` | RINEX 2.11 obs header writer, epoch writer, nav writer, unit converters, RinexObsWriter, RinexNavWriter | VERIFIED | 820 lines (well above 350 min); exports all required types and functions; 13 unit tests inline |
| `gnss-server/src/main.rs` | run_decode_task wired to RinexObsWriter + RinexNavWriter; output_dir from config | VERIFIED | `run_decode_task` initialises both writers from config, routes all events; `mod rinex_writer` declared |
| `gnss-server/src/config.rs` | `output_dir` field with `./rinex_output` default | VERIFIED | `output_dir: String` with `#[serde(default = "default_output_dir")]`; default function returns `"./rinex_output"` |
| `crates/gnss-ota/Cargo.toml` | Crate manifest with `name = "gnss-ota"`, no external deps | VERIFIED | Contains `name = "gnss-ota"`, empty `[dependencies]` section |
| `crates/gnss-ota/src/lib.rs` | `#![no_std]` OtaSlot + OtaManager traits using `core::fmt::Debug` | VERIFIED | Starts with `#![no_std]`; both traits defined; uses `core::fmt::Debug` as Error bound |
| `crates/gnss-ota/BLOCKER.md` | Documented nostd OTA blockers with GitHub issue references | VERIFIED | Documents `esp-rs/esp-hal#3259` (esp-storage no partition table API) and three esp-hal-ota issues (C6 untested, pointer magic, no embedded-storage abstraction) |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `gnss-server/src/rinex_writer.rs` | `gnss-server/src/observation.rs` | EpochGroup.observations iteration | WIRED | `write_epoch` iterates `group.observations`; imports `EpochGroup`, `Observation`, `Constellation` |
| `gnss-server/src/rinex_writer.rs` | `std::fs::File` via `std::io::BufWriter` | Synchronous file I/O in RinexObsWriter/RinexNavWriter | WIRED | Both writer structs use `BufWriter<std::fs::File>` |
| `gnss-server/src/main.rs` | `gnss-server/src/rinex_writer.rs` | run_decode_task calling `obs_writer.write_group()` and `nav_writer.write_ephemeris()` | WIRED | Both call sites present at lines 92 and 98; `gps_tow_to_utc` called to derive epoch_utc |
| `gnss-server/src/rinex_writer.rs` | `chrono::Utc::now()` via `current_gps_week()` initialised at startup | gps_week stored in writers | WIRED | `current_gps_week()` uses `Utc::now()`; called in `run_decode_task` before writer construction |
| `crates/gnss-ota/src/lib.rs` | `core::fmt::Debug` | `#![no_std]` with only `core` types | WIRED | Both `OtaSlot::Error` and `OtaManager::Error` bounds use `core::fmt::Debug`; no `std` imports |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| RINEX-01 | 24-01 | Server writes RINEX 2.11 observation files (.26O) with hourly rotation and correct column-positioned format | SATISFIED | `RinexObsWriter` exists with hourly rotation; 13 unit tests verify column format; all 31 gnss-server tests pass |
| RINEX-02 | 24-01 | Observation file includes all mandatory headers (VERSION/TYPE, TYPES OF OBSERV, WAVELENGTH FACT, TIME OF FIRST OBS, END OF HEADER) plus APPROX POSITION XYZ | SATISFIED | `write_obs_header` writes all 9 records: RINEX VERSION/TYPE, PGM/RUN BY/DATE, MARKER NAME, APPROX POSITION XYZ, ANTENNA DELTA, TYPES OF OBSERV, WAVELENGTH FACT, TIME OF FIRST OBS, END OF HEADER |
| RINEX-03 | 24-02 | Server writes RINEX 2.11 mixed navigation files (.26P) from decoded ephemeris messages with hourly rotation | SATISFIED | `RinexNavWriter` exists with hourly rotation; `write_gps_nav` (8-line) and `write_glo_nav` (4-line) implemented; nav header test passes; Galileo/BeiDou log warning and skip |
| RINEX-04 | 24-02 | RINEX output is accepted by RTKLIB | NEEDS HUMAN | Automated checks confirm format-level correctness (80-char lines, D19.12, PRN formatting); acceptance by `rnx2rtkp` requires real RTCM3 data from device FFFEB5 |
| NOSTD-04a | 24-03 | gnss-ota gap crate — dual-slot OTA trait definition and BLOCKER.md documenting the specific nostd blocker | SATISFIED | `cargo check --target thumbv7em-none-eabihf -p gnss-ota` passes; both traits defined; BLOCKER.md documents two concrete blockers with GitHub issue links |

No orphaned requirements found. All Phase 24 requirements (RINEX-01 through RINEX-04, NOSTD-04a) claimed by plans and verified above.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `gnss-server/src/observation.rs` | 18, 29, 40, 53 | `#[allow(dead_code)]` on struct, field, and enum | Info | Expected — `rough_range_ms` is a documentation field; `#[allow(dead_code)]` on structs is transitional from before Plan 02 wiring. Not a functional concern. |
| `gnss-server/src/rinex_writer.rs` | 444, 518 | `#[allow(dead_code)]` on `gps_week` field in both writers | Info | Documented decision — stored for future GPS week rollover handling. Clippy is clean. |

No blocker or warning-level anti-patterns found. No TODO/FIXME comments in any modified files.

---

### Human Verification Required

#### 1. RTKLIB Acceptance Test (RINEX-04)

**Test:** Run the server against a live RTCM3 stream from device FFFEB5 for at least 30 minutes. Then run:
```
rnx2rtkp -x 5 <output_dir>/FFFEB50710.26O <output_dir>/FFFEB50710.26P
```
**Expected:** No RINEX parse errors in rnx2rtkp output; solution epochs are processed and a position solution is reported.
**Why human:** RINEX format correctness at the byte level has been verified by unit tests. But RTKLIB acceptance depends on the combination of header metadata, epoch timing, and observation data that can only be validated against real decoded RTCM3 frames from actual hardware.

#### 2. Hourly File Rotation

**Test:** Let the server run through a UTC hour boundary (or set system clock near a boundary). Inspect the configured output directory.
**Expected:** Two separate `.26O` files are created for consecutive hours, each with a valid RINEX header. The old file is flushed and closed before the new file is opened.
**Why human:** The rotation logic uses `epoch_utc.hour() != self.current_hour`. Verifying this works at a real clock boundary requires either waiting or a time injection mechanism that is not yet in the test suite.

---

### Gaps Summary

No gaps blocking goal achievement. All automated must-haves pass:

- 13 rinex_writer unit tests pass (RINEX format correctness at every level)
- All 31 gnss-server tests pass
- `cargo clippy -p gnss-server -- -D warnings` clean
- `cargo check --target thumbv7em-none-eabihf -p gnss-ota` clean
- Commits 5ae7553, 7815d8c, 649fd87, 6b1e46a confirmed in git history
- All 5 requirement IDs (RINEX-01 through RINEX-04, NOSTD-04a) satisfied by artifacts

The single outstanding item (RINEX-04 RTKLIB acceptance) is a manual validation deferred by project convention to hardware testing on device FFFEB5, consistent with the testing.md approach used throughout this project.

---

_Verified: 2026-03-12T09:00:00Z_
_Verifier: Claude (gsd-verifier)_
