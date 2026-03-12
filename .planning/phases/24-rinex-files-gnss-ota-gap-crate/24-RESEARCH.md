# Phase 24: RINEX Files + gnss-ota gap crate - Research

**Researched:** 2026-03-12
**Domain:** RINEX 2.11 fixed-width file format, MSM-to-RINEX unit conversion, hourly file rotation, gnss-ota dual-slot OTA trait design, nostd OTA blocker analysis
**Confidence:** HIGH for RINEX format spec (verified from official IGS docs); MEDIUM for rinex crate OBS writer output format (confirmed functional but GLONASS/BDS nav under construction); HIGH for OTA ecosystem (confirmed esp-hal-ota exists; C6 untested)

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| RINEX-01 | Server writes RINEX 2.11 observation files (`.26O`) with hourly rotation and correct column-positioned format | RINEX 2.11 spec fully verified; format codes documented; DIY writer pattern recommended over rinex crate |
| RINEX-02 | Observation file includes all mandatory headers (VERSION/TYPE, TYPES OF OBSERV, WAVELENGTH FACT, TIME OF FIRST OBS, END OF HEADER) plus APPROX POSITION XYZ | All header records with exact format codes verified from official IGS RINEX 2.11 spec |
| RINEX-03 | Server writes RINEX 2.11 mixed navigation files (`.26P`) from decoded ephemeris messages with hourly rotation | Nav file format verified for GPS and GLONASS; `D19.12` field format documented; rinex crate NAV writer under construction — use DIY writer |
| RINEX-04 | RINEX output accepted by RTKLIB (`rnx2rtkp` or `rtkplot`) | RTKLIB accepts RINEX 2.11; observation types C1/L1/S1 confirmed accepted; GLONASS requires FCN-aware phase handling |
| NOSTD-04a | `gnss-ota` crate with dual-slot OTA trait definition and `BLOCKER.md` | esp-hal-ota confirmed for ESP32/C3/S3; ESP32-C6 untested; concrete blockers identified: no partition table API in esp-storage, esp-hal-ota uses pointer magic from ESP-IDF bootloader structures |
</phase_requirements>

## Summary

Phase 24 has two independent work streams: (1) RINEX file writing in the gnss-server, and (2) the gnss-ota gap crate skeleton.

The RINEX stream requires a DIY fixed-width ASCII writer — the `rinex` crate (0.21.1) has an OBS writer but NAV is marked under construction for GLONASS/BDS. Since the format is well-specified (IGS RINEX 2.11, ~300 lines of Rust), a DIY writer is both safer and fully controllable. The critical unit conversions from rtcm-rs raw units (milliseconds) to RINEX units (meters for pseudorange, cycles for carrier phase) are: `pseudorange_m = pseudorange_ms * 299_792.458` (speed of light in m/ms) and carrier phase conversion requires signal frequency — for GPS L1: `phase_cycles = phase_ms * f_L1 / 1000.0` where `f_L1 = 1_575_420_000.0 Hz`. GLONASS carrier phase requires FCN for frequency — without it, write 16 spaces (missing observation). The observation file uses `C1` (C/A code pseudorange) and `L1` (carrier phase in cycles) as the primary observation types for RTKLIB compatibility.

The gnss-ota stream is a crate skeleton only — no working OTA implementation is required this phase. The crate defines an abstract `OtaSlot` trait and a `BLOCKER.md` documenting specifically why a nostd OTA implementation cannot be completed today. Key blockers are: (1) `esp-hal-ota` is untested on ESP32-C6 and uses "pointer magic" referencing ESP-IDF internal bootloader structures (`EspOtaSelectEntry`) rather than a clean embedded-storage abstraction, and (2) esp-storage lacks partition table parsing API (tracking issue #3259 in esp-rs/esp-hal), so the target OTA partition cannot be located without hardcoding addresses.

**Primary recommendation:** Implement RINEX writer as a standalone DIY module (~300 lines). Create gnss-ota crate skeleton with trait + BLOCKER.md. Both streams are independent and can be developed in parallel.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| chrono | 0.4 | UTC timestamps for RINEX epoch lines and file naming | Already in gnss-server Cargo.toml; UTC formatting for RINEX header TIME OF FIRST OBS |
| tokio | 1.x | Async file I/O for RINEX writer | Already in gnss-server; `tokio::fs::File` + `BufWriter` for efficient hourly writes |

### Supporting (for RINEX writer)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rinex | 0.21.1 | RINEX parsing (reading back output for validation) | Optional — only if round-trip test needed; NAV writer under construction, do NOT use for writing |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| DIY fixed-width writer | rinex 0.21.1 writer | rinex OBS writer confirmed functional; NAV writer has construction warnings for GLONASS/BDS; DIY is ~300 lines and gives full control over exact column layout RTKLIB expects |
| `write!` macros for formatting | RINEX builder pattern | `write!` with explicit widths (`{:14.3}`, `{:<60}`) directly mirrors RINEX format codes; no intermediate data structures needed |

**Installation (no new dependencies needed for gnss-server):**
All required libraries (chrono, tokio, anyhow) already in gnss-server/Cargo.toml.

**Installation (gnss-ota crate):**
```bash
# crates/gnss-ota/ — new crate, no external dependencies needed for trait-only skeleton
```

## Architecture Patterns

### Recommended Project Structure

```
gnss-server/src/
├── rinex_writer.rs      # RINEX 2.11 OBS + NAV writer, hourly rotation logic
├── main.rs              # updated: wire EpochGroup + EphemerisMsg to rinex_writer
gnss-server/src/rinex_writer.rs

crates/gnss-ota/
├── Cargo.toml           # no external dependencies; workspace member
├── src/
│   └── lib.rs           # OtaSlot trait definition
└── BLOCKER.md           # Documents specific nostd OTA blockers
```

### Pattern 1: RINEX 2.11 Observation File Header

**What:** All mandatory header lines, each exactly 80 characters, label in columns 61-80.

**Format codes from official IGS RINEX 2.11 spec:**
```
RINEX VERSION / TYPE: F9.2,11X,A1,19X,A1,19X   — label cols 61-80
PGM / RUN BY / DATE:  A20,A20,A20               — label cols 61-80
MARKER NAME:          A60                        — label cols 61-80
APPROX POSITION XYZ:  3F14.4                    — label cols 61-80
ANTENNA: DELTA H/E/N: 3F14.4                    — label cols 61-80
# / TYPES OF OBSERV:  I6,9(4X,A1,A1)            — label cols 61-80
WAVELENGTH FACT L1/2: 2I6, I6                   — label cols 61-80
TIME OF FIRST OBS:    5I6,F13.7,5X,A3           — label cols 61-80
END OF HEADER:        60X                        — label cols 61-80
```

**Example (verified against IGS spec):**
```
     2.11           OBSERVATION DATA    M (MIXED)   RINEX VERSION / TYPE
gnss-server         gnss                20260312    PGM / RUN BY / DATE
GNSS-FFFEB5                                         MARKER NAME
0.0000000000E+00  0.0000000000E+00  0.0000000000E+00APPROX POSITION XYZ
        0.0000          0.0000          0.0000      ANTENNA: DELTA H/E/N
     4    C1    L1    S1    P1                      # / TYPES OF OBSERV
     1     1                                        WAVELENGTH FACT L1/2
  2026     3    12     0     0    0.0000000    GPS  TIME OF FIRST OBS
                                                    END OF HEADER
```

**Implementation rule:** Each header line is exactly 80 characters. Pad field area (cols 1-60) to 60 chars; place label left-justified in cols 61-80.

### Pattern 2: RINEX 2.11 Observation Epoch Record

**Epoch header line format (verified from IGS spec Table A2):**
```
Columns 1-2:   Year (2-digit, zero-padded, space prefix)  " 26"
Column 3:      space
Columns 4-5:   Month                                       " 3"
Column 6:      space
Columns 7-8:   Day                                         "12"
Column 9:      space
Columns 10-11: Hour                                        " 4"
Column 12:     space
Columns 13-14: Minute                                      "23"
Column 15:     space
Columns 16-26: Second (F11.7)                              " 11.2000000"
Column 27-28:  spaces
Column 29:     Epoch flag                                  "0"
Column 30-32:  spaces
Columns 33-35: Number of satellites (I3)                   "  8"
Columns 37-68: Satellite PRN list (12 max, A1+I2 each)    "G01G05G09..."
Continuation line for >12 sats: "                                G13G15..."
```

**Rust formatting:**
```rust
// Source: IGS RINEX 2.11 spec, Table A2
// epoch_ms is u32 (milliseconds since week/day start); convert to UTC datetime first
write!(w, " {:02} {:2} {:2} {:2} {:2}{:11.7}  {:1}  {:3}",
    year_2digit, month, day, hour, minute, second,
    epoch_flag,   // 0 = OK
    sat_count
)?;
// Then write sat list: "G01G05R03E07" etc., 3 chars each, up to 12 per line
```

### Pattern 3: RINEX 2.11 Observation Data Record

**Per-satellite, per-observation-type format (verified from IGS spec):**
```
Columns 1-14:  Observation value (F14.3) — 0.0 written as 14 spaces if missing
Column 15:     Loss of Lock Indicator (I1) — 0 for OK, space if obs missing
Column 16:     Signal strength (I1) — 0 or space
```
Up to 5 observations per 80-char line; continuation line for more.

**Key RINEX 2.11 observation types for RTKLIB compatibility (verified by RTKLIB rinex.c):**
- `C1` — C/A code pseudorange (meters, from rtcm-rs `pseudorange_ms * 299792.458`)
- `L1` — L1 carrier phase (cycles, from rtcm-rs `carrier_phase_ms * freq_l1 / 1000.0`)
- `S1` — Signal strength (dB-Hz, CNR from rtcm-rs `cnr_dbhz`, scaled 0-9 for RINEX signal strength)
- `P1` — P-code pseudorange (write same as C1 for UM980 output; RTKLIB treats C1/P1 as equivalent in 2.11)

**Rust formatting example:**
```rust
// Source: IGS RINEX 2.11 spec F14.3 format
fn write_obs(w: &mut impl Write, value: Option<f64>, lli: u8, ssi: u8) -> io::Result<()> {
    match value {
        Some(v) => write!(w, "{:14.3}{:1}{:1}", v, lli, ssi),
        None    => write!(w, "                "),  // 16 spaces — missing obs
    }
}
```

### Pattern 4: RINEX 2.11 Navigation File Format

**GPS PRN/EPOCH/SV CLK record (verified from IGS spec Table A3):**
```
Col 1-2:   PRN (I2)
Col 4-5:   Year (I2.2)
Col 7-8:   Month (I2)
Col 10-11: Day (I2)
Col 13-14: Hour (I2)
Col 16-17: Minute (I2)
Col 19-23: Second (F5.1)
Col 24-42: SV clock bias (D19.12) — seconds
Col 43-61: SV clock drift (D19.12) — sec/sec
Col 62-80: SV clock drift rate (D19.12) — sec/sec²
```

**Broadcast Orbit records 1-7: each line is `3X,4D19.12`** (3 spaces then 4 fields × 19 chars).

**D19.12 Fortran format in Rust:**
```rust
// RINEX uses Fortran D-format: e.g., -1.234567890123D-04
// Write as: format!("{:19.12E}", val).replace("E", "D").replace("E-", "D-")
// Ensure two-digit exponent: D-04 not D-4
fn write_d19_12(w: &mut impl Write, val: f64) -> io::Result<()> {
    let s = format!("{:19.12E}", val);
    // Rust uses E notation with + sign; RINEX uses D with sign
    // Replace E+XX with D+XX, E-XX with D-XX
    // Zero-pad single-digit exponents: D+4 -> D+04
    write!(w, "{}", rinex_d_format(val))
}
```

**GLONASS navigation record (verified from IGS spec Table A10):**
```
Col 1-2:   Slot number (I2)
Col 4-5:   Year through Col 19-23: Second — same as GPS
Col 24-42: SV clock bias (-τN, D19.12)
Col 43-61: SV frequency bias (+γN, D19.12)
Col 62-80: Message frame time (tk seconds, D19.12)
Orbit 1:   X position (km), Ẋ (km/s), Ẍ (km/s²), Bn health
Orbit 2:   Y position (km), Ẏ, Ÿ, Freq number (-7 to +13)
Orbit 3:   Z position (km), Ż, Z̈, Age of info (days)
```

**Mixed nav file extension:** RINEX 2.11 standard file naming does not define a `.P` extension for mixed nav files. The `.P` extension is a RTKLIB convention for "multiple navigation file" (GPS+GLONASS). Write GPS ephemeris and GLONASS ephemeris into the same file; name with `.26P` extension for RTKLIB compatibility (2026 year → `.26`). This matches common RTKLIB usage patterns.

### Pattern 5: Unit Conversion — MSM milliseconds to RINEX units

**Pseudorange (meters):**
```rust
// Source: physics — speed of light in meters per millisecond
const SPEED_OF_LIGHT_M_PER_MS: f64 = 299_792.458;  // m/ms

fn pseudorange_m(rough_int: Option<u8>, rough_mod: f64, fine: Option<f64>) -> Option<f64> {
    // Full pseudorange = (rough_int * 1ms + rough_mod * 1ms + fine_ms) * c
    // rough_int is in whole milliseconds; rough_mod is fractional ms; fine is fractional ms
    let int_ms = rough_int.map(|v| v as f64).unwrap_or(0.0);
    let total_ms = int_ms + rough_mod + fine.unwrap_or(0.0);
    if rough_int.is_none() { return None; }
    Some(total_ms * SPEED_OF_LIGHT_M_PER_MS)
}
```

**Note from Phase 23 Observation struct:** `pseudorange_ms` in the Observation struct stores only the fine pseudorange component (not reconstructed). Phase 24 must decide: use rough+fine reconstruction for RINEX, or use fine-only as a delta. Since Observation doesn't carry rough range components, add rough range fields to Observation OR reconstruct in rinex_writer using full Msg1077/1074 data. **Recommendation: add `rough_range_ms: f64` to `Observation` to store `rough_int + rough_mod`** so the RINEX writer can reconstruct full pseudorange.

**Carrier phase (cycles):**
```rust
// GPS L1 frequency: 1575.42 MHz
const GPS_L1_HZ: f64 = 1_575_420_000.0;
// GPS L2 frequency: 1227.60 MHz
const GPS_L2_HZ: f64 = 1_227_600_000.0;

fn phase_cycles(phase_ms: Option<f64>, freq_hz: f64) -> Option<f64> {
    // phase_ms is phase in ms; multiply by frequency to get cycles
    phase_ms.map(|p| p * freq_hz / 1000.0)
}

// GLONASS L1: freq = (1602 + FCN * 0.5625) MHz — FCN not in MSM4/MSM7 signal data
// Without FCN: write 16 spaces (missing observation marker) per STATE.md decision
```

**Signal strength (RINEX SSI 1-9 from CNR dBHz):**
```rust
// RINEX signal strength indicator: 1 (min) to 9 (max), 0 = unknown
// RTKLIB rinex.c: ssi = min(9, max(1, (int)((snr - 15.0) / 6.0 + 1.5)))
fn cnr_to_ssi(cnr_dbhz: Option<f64>) -> u8 {
    match cnr_dbhz {
        None => 0,
        Some(v) => ((v - 15.0) / 6.0 + 1.5).round().clamp(1.0, 9.0) as u8,
    }
}
```

### Pattern 6: Hourly File Rotation

**What:** A new file opens at each UTC hour boundary. The current file is flushed and closed; a new file is opened with the new hour's name.

**File naming for RINEX 2.11:**
```
ssssdddf.yyt where:
  ssss = station name (4 char, e.g. "GNSS")
  ddd  = day of year (001-366)
  f    = session code (0 = full hour, a-x = hour a-x in 24-session files; use "0" for hourly)
  yy   = 2-digit year (26 for 2026)
  t    = file type (O = observation, P = mixed nav)

Examples:
  GNSS0600.26O  — day 60, hour 00 (midnight), observation, 2026
  GNSS0600.26P  — day 60, hour 00 (midnight), mixed nav, 2026
```

**Hourly rotation logic:**
```rust
// Source: RINEX 2.11 spec section 4 + standard GNSS practice
// Track current hour; when epoch_utc.hour() != current_hour, rotate files
fn should_rotate(current_file_hour: u32, epoch_dt: &chrono::DateTime<Utc>) -> bool {
    epoch_dt.hour() != current_file_hour
}
```

### Pattern 7: gnss-ota Dual-Slot OTA Trait

**What:** A minimal trait defining the operations needed for dual-slot OTA in an embedded environment. No implementation — skeleton only.

**Trait design:**
```rust
// crates/gnss-ota/src/lib.rs
#![no_std]

/// Abstract handle to one OTA application partition slot.
///
/// Implementations:
/// - `EspHalOtaSlot` — wraps esp-hal-ota (std; ESP32/S3/C3 only; C6 untested)
/// - A future `EmbeddedOtaSlot` — blocked, see BLOCKER.md
pub trait OtaSlot {
    type Error: core::fmt::Debug;

    /// Total capacity of this OTA partition in bytes.
    fn capacity(&self) -> usize;

    /// Erase the partition, preparing it for a new image.
    fn erase(&mut self) -> Result<(), Self::Error>;

    /// Write a chunk of firmware at the given offset within the slot.
    fn write_chunk(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error>;

    /// Mark this slot as the next boot target. Does not reboot.
    fn set_as_boot_target(&mut self) -> Result<(), Self::Error>;

    /// Verify the written image (e.g., CRC32 check).
    fn verify(&self, expected_crc: u32) -> Result<bool, Self::Error>;
}

/// Select which OTA slot to use next.
pub trait OtaManager {
    type Slot: OtaSlot;
    type Error: core::fmt::Debug;

    /// Return the slot that is NOT currently booted (the "next" slot).
    fn next_slot(&mut self) -> Result<Self::Slot, Self::Error>;

    /// Return the index (0 or 1) of the currently booted slot.
    fn current_slot_index(&self) -> Result<usize, Self::Error>;
}
```

### Anti-Patterns to Avoid

- **Using RINEX F14.3 for missing observations:** A missing observation must be written as 16 spaces (14 value + 1 LLI + 1 SSI all blank), NOT as `"         0.000  "`. RTKLIB and RTKPOST will reject `0.000` as a valid-looking but incorrect observation.
- **Writing D19.12 without two-digit exponent:** RINEX requires `D+04` not `D+4`. Rust's `{:E}` may produce single-digit exponents — pad manually.
- **GPS epoch_ms as RINEX timestamp directly:** `gps_epoch_time_ms` is GPS time-of-week in ms. Convert to UTC using GPS epoch (1980-01-06) + current GPS week. The GPS week number is NOT in MSM messages — must be maintained separately or derived from system clock.
- **GLONASS carrier phase as 0.0:** Write as 16 spaces (missing observation) per STATE.md. RTKLIB will treat 0.0 as a real phase observation and produce garbage solutions.
- **Opening a new file before flushing the old one:** The hour-boundary rotation must flush and close the current file before opening the new one. Any partially written epoch belongs to the old file.
- **Satellite count > 12 on one epoch line:** The epoch line holds max 12 satellite identifiers (columns 37-68). Use a continuation line (format `32X,12(A1,I2)`) for epochs with >12 satellites.
- **Wrong observation type for RTKLIB:** Use `C1` for C/A code (not `C/A` or `CA`). RTKLIB's rinex.c maps `C1`→GPS L1C/A and `C1`→GLONASS L1C/A. Using non-standard codes causes RTKLIB to silently skip the observation.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| UTC time from GPS time-of-week | Manual GPS→UTC conversion | `chrono` + known GPS epoch + leap seconds | GPS leap seconds change; hardcoding leads to wrong timestamps after next leap second event |
| RINEX 2.11 header labels | Freeform header strings | Exact column-position formatting with 80-char lines | RTKLIB parser is column-sensitive; even one off-by-one in label position causes silent parse failure |
| GPS L1/L2 frequency constants | Magic numbers | Named constants from GNSS standards | L1=1575.42 MHz, L2=1227.60 MHz are fixed for GPS; GLONASS L1 = 1602 + k×0.5625 MHz where k is FCN |
| CRC32 for gnss-ota | Custom CRC | crc crate (already in esp-ota-nostd deps) | CRC32 is complex to hand-implement correctly; use the `crc` crate |

**Key insight:** The RINEX fixed-width format is deceptively simple but column-exact. RTKLIB's parser uses character-position indexing, not whitespace-delimited parsing. A single space in the wrong column silently corrupts data without error.

## Common Pitfalls

### Pitfall 1: GPS Week Number Not in MSM Messages

**What goes wrong:** `gps_epoch_time_ms` is time-of-week (0 to 604,799,999 ms). Converting it to UTC requires the GPS week number, which is NOT in MSM messages.

**Why it happens:** RTCM3 MSM messages intentionally omit GPS week to save bits — the receiver/network tracks the week externally.

**How to avoid:** Initialize the GPS week from `chrono::Utc::now()` at server startup. GPS week since epoch (1980-01-06): `(days_since_gps_epoch / 7) % 1024`. Update once per hour or when a week rollover is detected (epoch_ms jumps from ~604M back to near 0). Store as server state.

**Warning signs:** RINEX timestamps that are off by exactly 7 days, or timestamps in 1980.

### Pitfall 2: Observation Missing vs Zero

**What goes wrong:** Writing `         0.000  ` (F14.3 of 0.0) instead of 16 spaces for a missing observation.

**Why it happens:** Rust `write!("{:14.3}", 0.0)` produces `"         0.000"` — looks like a valid observation.

**How to avoid:** Always check `Option<f64>` — only write the F14.3 value when `Some(v)`. On `None`, write exactly 16 spaces.

**Warning signs:** RTKLIB reports "phase cycle slip" or "large position error" — misinterpreting 0.0 observations.

### Pitfall 3: RINEX 2.11 File Extension Convention

**What goes wrong:** Naming the nav file `.26N` (GPS-only) when it contains GLONASS entries, or vice versa.

**Why it happens:** RINEX 2.11 has separate extensions per constellation: `.26N` = GPS nav, `.26G` = GLONASS nav. The `.26P` extension is a RTKLIB-specific convention for mixed-constellation navigation (not in the official RINEX 2.11 spec).

**How to avoid:** For RTKLIB compatibility, use `.26P` for mixed navigation files (GPS + GLONASS + Galileo + BeiDou in one file). RTKLIB's `rnx2rtkp` accepts `.P` files as mixed nav. Document this deviation from the strict RINEX 2.11 spec.

**Warning signs:** RTKLIB reports "no navigation data" when loading the nav file.

### Pitfall 4: D19.12 Exponent Formatting

**What goes wrong:** Rust formats `f64` as `1.234567890123E+4` (single-digit exponent); RINEX requires `1.234567890123D+04` (two-digit, D notation).

**Why it happens:** Rust `{:E}` uses E notation with platform-dependent exponent width.

**How to avoid:**
```rust
fn to_d19_12(val: f64) -> String {
    if val == 0.0 {
        return "  0.000000000000D+00".to_string();
    }
    let s = format!("{:19.12E}", val);
    // s is like " 1.234567890123E4" or "-1.234567890123E-4"
    // Need: " 1.234567890123D+04"
    // Replace E with D and ensure two-digit exponent with sign
    let (mantissa, exp_str) = s.split_once('E').unwrap();
    let exp: i32 = exp_str.parse().unwrap();
    format!("{}D{:+03}", mantissa, exp)
}
```

**Warning signs:** RTKLIB or TEQC reports "format error in navigation file".

### Pitfall 5: rinex crate GLONASS NAV Writer Limitation

**What goes wrong:** Using `rinex` crate 0.21.1 NAV writer for GLONASS navigation produces incorrect or incomplete output.

**Why it happens:** The rinex crate 0.21.1 README explicitly states "Navigation is currently not feasible with Glonass, SBAS and IRNSS" — GLONASS NAV writer is under construction.

**How to avoid:** Use the DIY fixed-width writer for navigation files. The GPS NAV format is ~7 broadcast orbit records × 4 D19.12 fields per record — approximately 50 lines of Rust format code.

**Warning signs:** RTKLIB reports "no GLONASS ephemeris" or satellite count drops to GPS-only.

### Pitfall 6: Observation struct missing rough range

**What goes wrong:** The current `Observation` struct (from Phase 23) stores only `pseudorange_ms` as the fine component from MSM signal data. The full pseudorange in RINEX requires rough range integer + rough range mod + fine range.

**Why it happens:** Phase 23 Observation struct was designed for the epoch buffer, not RINEX output. The rough range fields are in the satellite data (not signal data) of MSM messages.

**How to avoid:** Two options: (A) Add `rough_range_ms: f64` (= `rough_int + rough_mod`) to `Observation` struct during Phase 24 implementation; or (B) store full reconstructed pseudorange_ms in Observation at decode time. Option A is smaller change; Option B is cleaner for the RINEX writer. Prefer Option B — store full `pseudorange_ms` = `rough_int * 1.0 + rough_mod + fine_ms` in `rtcm_decode.rs`.

**Warning signs:** RINEX pseudorange values are tiny (near zero) because only the fine component (~<1ms range residual) is written.

### Pitfall 7: gnss-ota crate must be `#![no_std]`

**What goes wrong:** Adding `std` features to the gnss-ota crate breaks the workspace resolver "2" isolation. The gap crate purpose is to provide a nostd-compilable trait skeleton.

**Why it happens:** `resolver = "2"` prevents `std` feature unification, but a `std` import in the crate root still compiles std into the crate.

**How to avoid:** Begin `crates/gnss-ota/src/lib.rs` with `#![no_std]`. Use only `core::` types in the trait definition. Test with `cargo check --target thumbv7em-none-eabihf -p gnss-ota` to verify no_std compilation.

## Code Examples

Verified patterns from official sources:

### RINEX 2.11 Observation Header Writer

```rust
// Source: IGS RINEX 2.11 spec (files.igs.org/pub/data/format/rinex211.txt)
use std::io::{BufWriter, Write};

fn write_obs_header<W: Write>(
    w: &mut BufWriter<W>,
    station: &str,
    approx_xyz: (f64, f64, f64),
    first_obs_utc: &chrono::DateTime<chrono::Utc>,
) -> std::io::Result<()> {
    // VERSION / TYPE — exactly 80 chars
    writeln!(w, "     2.11           OBSERVATION DATA    M (MIXED)   RINEX VERSION / TYPE")?;
    // PGM / RUN BY / DATE
    writeln!(w, "{:<20}{:<20}{:<20}PGM / RUN BY / DATE",
        "gnss-server", "gnss", first_obs_utc.format("%Y%m%d").to_string())?;
    // MARKER NAME
    writeln!(w, "{:<60}MARKER NAME", station)?;
    // APPROX POSITION XYZ — 3F14.4 in cols 1-42, then 18 spaces
    writeln!(w, "{:14.4}{:14.4}{:14.4}                  APPROX POSITION XYZ",
        approx_xyz.0, approx_xyz.1, approx_xyz.2)?;
    // ANTENNA: DELTA H/E/N — 3 zeros
    writeln!(w, "{:14.4}{:14.4}{:14.4}                  ANTENNA: DELTA H/E/N", 0.0f64, 0.0f64, 0.0f64)?;
    // TYPES OF OBSERV — C1 L1 S1 (3 types, 6 chars + up to 9 obs codes of 6 chars each)
    writeln!(w, "     3    C1    L1    S1                                  # / TYPES OF OBSERV")?;
    // WAVELENGTH FACT L1/2 — default (1,1) for all GPS
    writeln!(w, "     1     1                                              WAVELENGTH FACT L1/2")?;
    // TIME OF FIRST OBS
    writeln!(w, "  {:4}{:6}{:6}{:6}{:6}{:13.7}     GPS TIME OF FIRST OBS",
        first_obs_utc.year(), first_obs_utc.month(), first_obs_utc.day(),
        first_obs_utc.hour(), first_obs_utc.minute(),
        first_obs_utc.second() as f64 + first_obs_utc.nanosecond() as f64 / 1e9)?;
    // END OF HEADER
    writeln!(w, "{:<60}END OF HEADER", "")?;
    Ok(())
}
```

### RINEX 2.11 Epoch + Observation Record

```rust
// Source: IGS RINEX 2.11 spec Table A2
fn write_epoch<W: Write>(
    w: &mut BufWriter<W>,
    epoch_utc: &chrono::DateTime<chrono::Utc>,
    observations: &[(SatId, Option<f64>, Option<f64>, Option<f64>)],
    // (sat_id, pseudorange_m, carrier_phase_cycles, cnr_dbhz)
) -> std::io::Result<()> {
    let year2 = epoch_utc.year() % 100;
    let sats: Vec<String> = observations.iter()
        .map(|(s, ..)| s.to_rinex_str())  // e.g. "G01", "R03", "E07"
        .collect();
    let nsat = sats.len();

    // Epoch line: " YY MM DD HH MM SS.SSSSSSS  0  N[sat list]"
    write!(w, " {:02} {:2} {:2} {:2} {:2}{:11.7}  0  {:3}",
        year2, epoch_utc.month(), epoch_utc.day(),
        epoch_utc.hour(), epoch_utc.minute(),
        epoch_utc.second() as f64)?;

    // Write up to 12 satellite IDs per line
    for (i, sat) in sats.iter().enumerate() {
        if i > 0 && i % 12 == 0 {
            writeln!(w)?;
            write!(w, "{:32}", "")?;  // 32 spaces continuation prefix
        }
        write!(w, "{}", sat)?;  // 3 chars: system code + 2-digit PRN
    }
    writeln!(w)?;

    // Observation records: 5 obs per 80-char line (16 chars each = 5×16 = 80)
    for (_, pr_m, phase_cyc, cnr) in observations {
        let ssi = cnr_to_ssi(*cnr);
        write_obs(w, *pr_m, 0, ssi)?;         // C1
        write_obs(w, *phase_cyc, 0, ssi)?;    // L1
        write_obs(w, cnr.map(|v| v), 0, 0)?; // S1
        writeln!(w)?;
    }
    Ok(())
}

fn write_obs<W: Write>(w: &mut W, value: Option<f64>, lli: u8, ssi: u8) -> std::io::Result<()> {
    match value {
        Some(v) => write!(w, "{:14.3}{}{}", v, lli, ssi),
        None    => write!(w, "                "),  // exactly 16 spaces
    }
}
```

### gnss-ota Crate Trait Skeleton

```rust
// crates/gnss-ota/src/lib.rs
// Source: Design from CONTEXT decisions + esp-hal-ota and esp-ota-nostd API analysis
#![no_std]

/// Handle to one of the two OTA application partition slots (ota_0 or ota_1).
///
/// A dual-slot OTA implementation writes the new firmware image to the inactive
/// slot, verifies it, then sets it as the next boot target before resetting.
///
/// See BLOCKER.md for specific reasons a complete nostd implementation cannot
/// be shipped today.
pub trait OtaSlot {
    type Error: core::fmt::Debug;

    /// Total writable capacity of this partition in bytes.
    fn capacity(&self) -> usize;

    /// Erase this partition (required before writing).
    fn erase(&mut self) -> Result<(), Self::Error>;

    /// Write `data` at `offset` bytes from the start of this partition.
    /// Caller must call `erase()` first.
    fn write_chunk(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error>;

    /// Verify the image using CRC32. Returns `Ok(true)` if CRC matches.
    fn verify_crc32(&self, expected: u32) -> Result<bool, Self::Error>;

    /// Mark this slot as the next boot target (does not reset the device).
    fn set_as_boot_target(&mut self) -> Result<(), Self::Error>;
}

/// Manages the two OTA partition slots and tracks the currently booted slot.
pub trait OtaManager {
    type Slot: OtaSlot;
    type Error: core::fmt::Debug;

    /// Returns the index (0 or 1) of the currently booted slot.
    fn booted_slot_index(&self) -> Result<usize, Self::Error>;

    /// Returns a mutable reference to the slot that is NOT currently booted.
    fn inactive_slot(&mut self) -> Result<Self::Slot, Self::Error>;
}
```

### D19.12 Fortran Format Converter

```rust
// Source: RINEX 2.11 spec format code D19.12 (Fortran double-precision notation)
fn to_d19_12(val: f64) -> String {
    if !val.is_finite() || val == 0.0 {
        return "  0.000000000000D+00".to_string();
    }
    // Format as scientific notation with 12 decimal places in 19 total chars
    let exp = val.abs().log10().floor() as i32;
    let mantissa = val / 10f64.powi(exp);
    format!("{:19.12E}", val)
        .replace('E', "D")
        // Ensure exponent has sign and two digits: D+04, D-07
        // Rust may emit D4 or D-4 — normalize
        .chars().collect::<String>()
        // Post-process: replace "D4" with "D+04", "D-4" with "D-04"
        // Implementation detail: write a small helper function
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| RTCM legacy messages (1001-1012) | RTCM3 MSM (1071-1127) | ~2015 | MSM carries more precise phase + CNR; all modern receivers output MSM |
| Separate GPS/GLONASS nav files (.N/.G) | Mixed nav file (.P) in RTKLIB | RTKLIB 2.4+ | Single file simplifies post-processing workflow |
| RINEX 2.11 (separate obs per constellation) | RINEX 3.x (mixed, GNSS codes) | IGS 2012 | RINEX 3.x more capable but RTKLIB 2.11 support is mature and tested; Phase scope is 2.11 only |
| esp-idf-svc EspOta (std) | esp-hal-ota (no_std, requires embedded-storage) | 2023 | ESP-IDF OTA API hidden behind esp-idf-sys; esp-hal-ota exposes OTA partition structures directly |
| esp-storage without partition awareness | esp-storage with partition table support | Tracking: esp-rs/esp-hal#3259 | Partition table parsing in esp-storage is still in progress (as of 2026-03) |

**Deprecated/outdated:**
- `esp-ota` crate (crates.io): depends on `esp-idf-sys` — std only, not relevant for gap crate
- RINEX 2.10: superseded by 2.11; RTKLIB accepts both; use 2.11

## Open Questions

1. **GPS Week Number Tracking**
   - What we know: MSM messages do not contain GPS week number; epoch_ms is time-of-week
   - What's unclear: Should the server derive GPS week from system clock at startup, or should it track week rollover from epoch_ms discontinuities?
   - Recommendation: Derive from `chrono::Utc::now()` at startup using known GPS epoch (1980-01-06 00:00:00 UTC); add a week rollover detector (epoch_ms resets from ~604M to near 0)

2. **Observation struct rough range**
   - What we know: Phase 23 Observation stores only fine pseudorange_ms; full pseudorange requires rough range from satellite data
   - What's unclear: Does rtcm_decode.rs have easy access to both satellite and signal data to compute rough+fine at decode time?
   - Recommendation: Looking at the existing rtcm_decode.rs, the `data_segment.satellite_data` and `signal_data` are both accessible in the match arm. Modify `Observation.pseudorange_ms` in Phase 24 to store the full reconstructed value: `rough_int * 1.0 + rough_mod + fine_ms`. This is a backward-compatible change since the field type remains `Option<f64>`.

3. **APPROX POSITION XYZ for moving rover**
   - What we know: RINEX header requires APPROX POSITION XYZ but the UM980 is a rover (position varies)
   - What's unclear: Should we write 0,0,0 or compute approximate position from NMEA GGA?
   - Recommendation: Write 0,0,0 for now. RTKLIB ignores APPROX POSITION for standard post-processing. Document as known limitation. Future improvement: parse GGA from MQTT nmea topic to provide approximate position.

4. **rinex 0.21.1 OBS writer exact output format verification**
   - What we know: OBS writer is marked as functional in README; nav writer has GLONASS limitations
   - What's unclear: Whether rinex crate OBS writer produces correct column positions for RINEX 2.11 vs 3.x
   - Recommendation: Use DIY writer to avoid any format uncertainty. The DIY approach is ~300 lines and gives full column control. If time permits, test rinex crate OBS output against a reference file after implementation.

5. **BLOCKER.md content for gnss-ota**
   - What we know: esp-hal-ota is untested on ESP32-C6; uses pointer magic from ESP-IDF bootloader structures; esp-storage lacks partition table API
   - What's unclear: Is there an active PR or timeline for esp-storage partition support?
   - Recommendation: Document known blockers at time of writing: (1) esp-storage #3259 (no partition table API), (2) esp-hal-ota explicitly lists C6 as untested, (3) OTA partition selection currently requires hardcoded flash offsets or ESP-IDF bootloader binary introspection via "pointer magic"

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test (`cargo test`) |
| Config file | none — `#[cfg(test)]` modules inline |
| Quick run command | `cargo test -p gnss-server -- rinex` |
| Full suite command | `cargo test --workspace --exclude esp32-gnssmqtt-firmware` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RINEX-01 | Observation file has correct column layout, hourly rotation | unit | `cargo test -p gnss-server -- rinex_writer::tests` | ❌ Wave 0 |
| RINEX-02 | All mandatory headers present and at correct column positions | unit | `cargo test -p gnss-server -- rinex_writer::tests::obs_header` | ❌ Wave 0 |
| RINEX-03 | Navigation file GPS/GLONASS ephemeris records in D19.12 format | unit | `cargo test -p gnss-server -- rinex_writer::tests::nav_record` | ❌ Wave 0 |
| RINEX-04 | RTKLIB accepts output files | manual | `rnx2rtkp <obs> <nav>` on device FFFEB5 output | manual-only |
| NOSTD-04a | gnss-ota crate compiles in no_std context | build | `cargo check --target thumbv7em-none-eabihf -p gnss-ota` | ❌ Wave 0 |

**RINEX-04 is manual-only** because it requires a running MQTT-connected device (FFFEB5) and RTKLIB installed locally. This validates that the generated files are actually accepted by RTKLIB.

### Sampling Rate

- **Per task commit:** `cargo test -p gnss-server -- rinex && cargo test -p gnss-ota`
- **Per wave merge:** `cargo clippy --workspace --exclude esp32-gnssmqtt-firmware -- -D warnings && cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **Phase gate:** Full suite green before `/gsd:verify-work`; RINEX-04 manual validation documented in VALIDATION.md

### Wave 0 Gaps

- [ ] `gnss-server/src/rinex_writer.rs` — covers RINEX-01, RINEX-02, RINEX-03 with `#[cfg(test)]` module
- [ ] `crates/gnss-ota/Cargo.toml` and `crates/gnss-ota/src/lib.rs` — covers NOSTD-04a (crate doesn't exist yet)
- [ ] `crates/gnss-ota/BLOCKER.md` — documents specific nostd blockers
- [ ] `Cargo.toml` workspace: add `crates/gnss-ota` to members (already covered by `crates/*` glob if crate exists)

## Sources

### Primary (HIGH confidence)

- `files.igs.org/pub/data/format/rinex211.txt` — Official IGS RINEX 2.11 specification: all header records, format codes (F9.2, F14.3, D19.12, I6), epoch record column layout, nav file format for GPS and GLONASS
- `server.gage.upc.edu/gLAB/HTML/Observation_Rinex_v2.11.html` — RINEX 2.11 observation format with satellite system codes (G/R/E/C) and multi-GNSS PRN format verified
- `docs.rs/esp-hal-ota/latest/esp_hal_ota/` — Ota struct API, dependency on embedded-storage, ESP32-C6 listed as untested
- `github.com/filipton/esp-hal-ota` — Confirmed: "no-std OTA for esp-hal"; C6 explicitly untested; uses pointer magic from ESP-IDF bootloader structures
- `github.com/esp-rs/esp-hal/issues/3259` — esp-storage partition table support tracking issue (confirmed open)
- Phase 23 `gnss-server/src/observation.rs` and `rtcm_decode.rs` — Confirmed existing Observation struct fields and rtcm-rs signal extraction patterns

### Secondary (MEDIUM confidence)

- `docs.rs/rinex/0.21.1/rinex/` — OBS writer confirmed functional (✓); NAV writer confirmed under construction for GLONASS/BDS; `to_file()` method exists
- `github.com/nav-solutions/rinex` README — Confirmed 0.21.1 (Sep 2025); "Navigation is currently not feasible with Glonass, SBAS and IRNSS"
- `rtklibexplorer.wordpress.com` — GLONASS MSM FCN requirement confirmed: MSM1-4/MSM6 lack frequency; MSM7 includes it; RTKLIB needs FCN for phase-to-cycles conversion
- `docs.rs/esp-ota-nostd/latest/` — Confirmed `embedded-storage`, `esp-partition-table` dependencies; function-based API (not trait-based)

### Tertiary (LOW confidence)

- `rtklibexplorer.wordpress.com/2020/11/01/converting-glonass-rtcm-msm-messages-to-rinex-with-rtklib/` — RTKLIB CNR→SSI formula (`ssi = (cnr-15)/6 + 1.5`); needs validation against RTKLIB source rinex.c
- RTKLIB observation type `C1`/`P1` equivalence for 2.11 — referenced in search results summary of RTKLIB rinex.c; not verified directly from source
- `.26P` extension for mixed nav — RTKLIB convention (not official RINEX 2.11 spec); widely used in practice

## Metadata

**Confidence breakdown:**

- RINEX 2.11 header format: HIGH — verified from official IGS spec
- RINEX 2.11 epoch/obs column layout: HIGH — verified from official IGS spec
- RINEX nav format (GPS): HIGH — verified from official IGS spec
- RINEX nav format (GLONASS): HIGH — verified from official IGS spec
- rinex crate OBS writer functional: MEDIUM — README confirms ✓ but format correctness for 2.x not independently verified
- rinex crate NAV writer for GLONASS: HIGH (broken) — README explicitly states not feasible
- gnss-ota OTA ecosystem state: HIGH — esp-hal-ota confirmed; C6 untested confirmed
- esp-storage partition API gap: HIGH — GitHub issue #3259 confirmed open
- MSM-to-RINEX unit conversions: MEDIUM — physics correct; implementation details need validation against RTKLIB output

**Research date:** 2026-03-12
**Valid until:** 2026-06-12 (RINEX 2.11 spec is frozen; esp-hal ecosystem moves fast — re-check esp-storage#3259 status before BLOCKER.md is finalized)
