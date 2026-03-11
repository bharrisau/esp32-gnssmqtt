# Feature Research

**Domain:** GNSS server-side processing — RINEX file generation, live web UI, nostd NVS abstraction (v2.1 milestone)
**Researched:** 2026-03-12
**Confidence:** HIGH for RINEX 2.11 format (spec fetched directly from IGS); HIGH for MSM message numbers (multiple consistent sources); HIGH for NMEA GSV fields (spec fetched directly); MEDIUM for ekv/NVS trait landscape (embedded-svc trait definitions not directly fetched; secondary sources used)

---

## Context: What Already Exists

The firmware (v2.0) already delivers these; the server consumes them:

- `gnss/{id}/rtcm/{msg_type}` — raw RTCM3 frames as binary MQTT payload
- `gnss/{id}/nmea/{SENTENCE_TYPE}` — raw NMEA sentences as text
- `gnss/{id}/heartbeat` — JSON with uptime_s, heap_free, fix_type, satellites, hdop
- NTRIP client pushes RTCM3 corrections to UM980 for RTK fix

The server is a new Rust binary, not firmware. It subscribes to these topics and does the heavy processing that the ESP32-C6 cannot.

---

## Feature Landscape — New Features for This Milestone

### Table Stakes (Users Expect These)

Features that must exist for the milestone to deliver value. Missing any makes the server useless for its stated purpose.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| RTCM3 MSM frame identification by message number | Server cannot process MSM without knowing which constellation and MSM level each frame belongs to | LOW | Message number encodes constellation: 107x=GPS, 108x=GLONASS, 109x=Galileo, 112x=BeiDou; last digit = MSM level (4 or 7) |
| MSM4 decode: pseudorange, carrier phase, CNR per satellite | Core RINEX observable — without pseudorange the observation file is worthless for PPK | HIGH | MSM4 (1074/1084/1094/1124): rough pseudorange + fine pseudorange residual → full pseudorange in metres; phase range → carrier phase in cycles; CNR → signal strength |
| MSM7 decode: same fields as MSM4 plus Doppler and higher precision | MSM7 is what the UM980 emits by default; server must handle it | HIGH | MSM7 (1077/1087/1097/1127): same field set as MSM4 with finer resolution encoding; Doppler field present but can be ignored for RINEX obs output |
| Epoch formation from MSM time-of-week | RINEX observation records are epoch-based; must group all satellite observations with the same timestamp into one epoch record | HIGH | MSM header contains GPS time-of-week (ms) for GPS/Galileo/BeiDou, or GLONASS day-of-week + time-of-day; buffer MSM frames within a short window (~10ms) per epoch |
| RINEX 2.11 observation file (.xxO) header — required fields | RTKLIB, RTKPost, and every PPK tool validates the header; missing mandatory fields cause rejection | LOW | Required: RINEX VERSION/TYPE, # / TYPES OF OBSERV, WAVELENGTH FACT L1/2, TIME OF FIRST OBS, END OF HEADER. Recommended: APPROX POSITION XYZ, ANTENNA: DELTA H/E/N, MARKER NAME, REC # / TYPE / VERS |
| RINEX 2.11 GPS observations (G prefix, from MSM 1074/1077) | GPS is the baseline constellation; without it the file is useless | HIGH | Satellite identifier G01–G32; observation types C1 (pseudorange m), L1 (carrier phase cycles + LLI flag), S1 (signal strength 0–9 mapped from CNR 0–99 dB-Hz) |
| RINEX 2.11 GLONASS observations (R prefix, from MSM 1084/1087) | Second most common constellation; expected in any multi-GNSS file | HIGH | Satellite identifier R01–R24; frequency channel number needed for wavelength calculation (embedded in GLONASS MSM satellite cell header); WAVELENGTH FACT header should use 0 0 for GLONASS (unknown at file open) |
| Hourly file rotation with correct RINEX filename | Convention: SSSSDDDHhh.YYo (station 4-char, day-of-year 3-digit, hour letter a-x, 2-digit year, type o/n/p) | LOW | At hour boundary: close current file, open new file with updated epoch; file extension .26O for year 2026 observation |
| RTCM3 GPS ephemeris decode (1019) → RINEX nav record | Required for PPK tools that cannot fetch broadcast ephemeris separately | HIGH | 1019 maps to RINEX 2.11 GPS nav: PRN/epoch/SV_clock header line + 7 broadcast orbit lines (IODE, Crs, delta_n, M0, Cuc, e, Cus, sqrt_A, toe, Cic, OMEGA0, Cis, i0, Crc, omega, OMEGADOT, IDOT, GPS_week, SV_accuracy, health, TGD, IODC) |
| RTCM3 GLONASS ephemeris decode (1020) → RINEX nav record | PPK with GLONASS requires GLONASS nav data | HIGH | 1020 maps to RINEX 2.11 GLONASS nav (.xxG or mixed .xxP): 4-line format — epoch+clock, ECEF position (km), ECEF velocity (km/s), ECEF acceleration (km/s²) + health + frequency channel + age of data |
| Mixed RINEX navigation file (.xxP) with GPS + GLONASS | Single nav file accepted by most PPK tools; avoids managing separate .xxN + .xxG files | MEDIUM | 'M' satellite system in header; GPS records have no leading system letter (blank = GPS); GLONASS records prefixed with R; records interleaved chronologically |
| Skyplot: polar SVG showing elevation and azimuth per satellite | Every GNSS tool from u-blox to Trimble shows a skyplot; its absence makes the web UI feel unfinished | MEDIUM | Data source: NMEA GSV sentences (GPGSV, GLGSV, GAGSV, BDGSV); each provides SV ID, elevation 0–90°, azimuth 0–359°, C/N0. North-up polar: center = zenith (90°), outer ring = horizon (0°) |
| SNR/C/N0 bar chart per satellite | Operators use it to diagnose obstructions and interference; paired with skyplot it shows which satellite positions correlate with poor signal | LOW | Same GSV C/N0 field; vertical bars per satellite; Y-axis 0–99 dB-Hz; threshold lines at 20 and 35 dB-Hz; bar colour by constellation |
| Device health panel from heartbeat MQTT topic | Already provided by firmware; server must surface it visually | LOW | Parse `gnss/{id}/heartbeat` JSON; display: uptime, heap_free, fix_type (0=no fix, 4=RTK fix, 5=RTK float), satellites in use, HDOP |
| NVS trait definition (interface only, with ESP-IDF impl) | Documents the interface shape needed for an embassy/nostd port; validates that the firmware's NVS usage is fully captured | MEDIUM | Trait covers: open namespace, typed get/set (u8/u16/u32/u64/i8/i16/i32/i64/str/blob), remove key, commit; ESP-IDF NVS as the initial concrete impl |

### Differentiators (Competitive Advantage)

Features that go beyond baseline and add real operational value.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Galileo observations (E prefix, MSM 1094/1097) | UM980 tracks Galileo; including it improves PPP and PPK accuracy; four-constellation coverage matches what the receiver actually produces | HIGH | 'E' satellite identifier is a de-facto RINEX 2.11 extension (not in spec but accepted by RTKLIB and teqc); 1045 = Galileo F/NAV ephemeris; 1046 = I/NAV (either works) |
| BeiDou observations (C prefix, MSM 1124/1127) | BDS-3 constellation now rivals GPS in global coverage; omitting it understates what UM980 provides | HIGH | 'C' satellite identifier is NOT in RINEX 2.11 spec; RTKLIB accepts it via extended-2.11 mode; downstream toolchain must be verified; consider offering RINEX 3.x as an option for BeiDou consumers |
| Satellite trail history on skyplot | Shows satellite paths across the observation session; useful for sky mask planning and obstruction diagnosis | LOW | Accumulate (azimuth, elevation) per PRN over time; render as SVG polyline per satellite; cap history at session start or 60 minutes |
| Signal quality colour coding on skyplot dots | Correlates sky position with signal quality — immediately shows which sectors are obstructed | LOW | Map C/N0 to colour: green (>35 dB-Hz), amber (20–35 dB-Hz), red (<20 dB-Hz); apply as SVG fill colour on satellite dot |
| ekv-backed NVS trait implementation (nostd) | Second concrete implementation of the NVS trait; proves the abstraction is portable; unblocks embassy port | HIGH | ekv provides `Map<Vec<u8>, Vec<u8>>` on raw NOR flash with LSM-tree; namespace key prefix (e.g. `b"ntrip:host"`); serialise typed values to/from bytes; requires embedded-hal NorFlash trait from flash driver |
| Multi-constellation GSV aggregation (all four sentence prefixes) | UM980 emits GPGSV + GLGSV + GAGSV + BDGSV; showing only GPS satellites makes the chart misleading | LOW | Maintain per-PRN state keyed by (constellation, prn); update on each GSV sentence type; clear stale satellites after ~5 seconds of no update |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| RINEX 3.x output instead of RINEX 2.11 | Better official multi-constellation support; BeiDou/Galileo have proper identifiers | RINEX 3 observation files require 3-character observation codes per signal (C1C, L1C, S1C, etc.); MSM signal ID → RINEX 3 tracking mode code table is large and constellation-specific; significantly higher implementation complexity; most existing toolchains already accept RINEX 2.11 with 'C'/'E' extensions | Output RINEX 2.11 now with de-facto extensions for Galileo; add RINEX 3.x option in a future milestone; use a comment header line to document the non-standard identifiers |
| Storing all raw RTCM frames server-side as a ring buffer | Insurance against bugs in MSM decode | Unbounded memory growth at 5 Hz with multi-constellation; the MQTT relay already provides real-time delivery; if reprocessing is needed, the broker's message history or a separate recorder handles it | Hourly RINEX files are the persistence layer; if raw frame replay is needed, use a separate MQTT message store (e.g. EMQX persistence) not the server process |
| Doppler observations (D1/D2) in RINEX obs file | MSM7 provides Doppler; "might as well use it" | RINEX Doppler adds complexity to epoch records; downstream PPK tools (RTKLIB, RTKPost) rarely use Doppler from observation files for position computation; increases file size | Include C1/L1/S1 (and C2/L2/S2 for dual-frequency) only; Doppler can be added as a P2 enhancement if a specific toolchain requires it |
| Full-rate skyplot updates (5 Hz matching GNSS output) | "More real-time" | At 5 Hz with 40+ satellites across four constellations, the browser receives hundreds of SVG patch updates per second via WebSocket; renders poorly on low-power devices and overwhelms the WebSocket channel | Downsample skyplot WebSocket pushes to 1 Hz; compute delta state (only changed satellites) and push those |
| Per-epoch NMEA parsing for position/map display | "Show the rover position on a map" | Full GGA/RMC parsing in the server duplicates firmware work and adds a mapping library dependency; the server's purpose is RINEX generation and satellite display — position mapping is a separate concern | Display fix quality from heartbeat (already parses fix_type and HDOP); if position display is needed, add it as a separate page component in a later milestone |
| Writing RINEX files to MQTT topics | "Keep everything in MQTT" | RINEX files are multi-kilobyte structured text; MQTT is not a file transfer protocol; consumers expect RINEX as files on disk or HTTP endpoints | Write RINEX files to local disk; expose via HTTP GET endpoint for download |

---

## Feature Dependencies

```
[MQTT subscriber — rtcm/# and nmea/# and heartbeat]
    └──feeds──> [RTCM3 MSM frame identification by message number]
    └──feeds──> [NMEA GSV parsing (skyplot + bar chart)]
    └──feeds──> [Device health panel]

[RTCM3 MSM frame identification]
    └──required by──> [MSM4/MSM7 decode (pseudorange, phase, CNR)]
                          └──required by──> [Epoch formation]
                                                └──required by──> [RINEX .xxO observation file writer]
                                                                      └──required by──> [Hourly rotation + RINEX naming]

[RTCM3 1019 GPS ephemeris decode]
    └──required by──> [RINEX mixed nav file .xxP GPS records]

[RTCM3 1020 GLONASS ephemeris decode]
    └──required by──> [RINEX mixed nav file .xxP GLONASS records]

[NMEA GSV parsing — all constellation prefixes]
    └──required by──> [Skyplot polar SVG]
    └──required by──> [SNR bar chart]
    └──both require──> [WebSocket push to browser]

[ESP-IDF NVS API audit]
    └──informs──> [NVS trait definition]
                      └──has impl──> [ESP-IDF NVS concrete implementation]
                      └──future impl──> [ekv nostd backing implementation]

[MSM satellite system flag (from message number)]
    └──determines──> [RINEX satellite identifier prefix: G/R/E/C]

[GLONASS frequency channel from MSM satellite cell]
    └──required by──> [Correct GLONASS L1/L2 wavelength in RINEX]
```

### Dependency Notes

- **MSM message number encodes constellation:** The server must map message numbers to constellation before any field decode. 107x → GPS, 108x → GLONASS, 109x → Galileo, 112x → BeiDou. Last digit → MSM level (4 = no Doppler, moderate precision; 7 = Doppler + higher precision).
- **Epoch formation requires buffering:** MSM messages for multiple constellations within the same epoch arrive as separate frames (one per constellation per epoch). A short time window (~10ms) is needed to collect all frames sharing the same reference time before writing the epoch record.
- **GLONASS frequency channel is only in MSM satellite cell:** The GLONASS frequency slot (−7 to +6) affects the L1/L2 carrier wavelength. It is embedded in the MSM4/MSM7 satellite cell bitmask and must be stored per PRN slot to correctly compute carrier phase observations.
- **Skyplot and bar chart share the same GSV parsing state:** A single GSV aggregator keyed by (constellation, prn) feeds both views. No duplication needed.
- **NVS trait is independent of server features:** The trait definition and ESP-IDF impl are firmware-support work, not server features. They can proceed in parallel with server development.

---

## RTCM3 MSM Message Number Reference (Confidence: HIGH)

| Constellation | MSM4 | MSM7 | Ephemeris |
|---------------|------|------|-----------|
| GPS | 1074 | 1077 | 1019 |
| GLONASS | 1084 | 1087 | 1020 |
| Galileo | 1094 | 1097 | 1045 (F/NAV), 1046 (I/NAV) |
| BeiDou | 1124 | 1127 | 1042 |
| QZSS | 1114 | 1117 | 1044 |

**MSM4 fields:** rough pseudorange (m) + fine pseudorange residual + carrier phase (cycles) + CNR (dB-Hz). No Doppler. Standard precision.
**MSM7 fields:** same as MSM4 + Doppler (Hz) + higher-resolution encoding of pseudorange and phase.
**Practical note:** MSM7 is what most modern receivers (including UM980) emit by default. MSM4 is still common in bandwidth-limited configurations. Both are sufficient for 2cm RTK accuracy. The server must handle both.

---

## RINEX 2.11 Observation File: Minimum Viable Specification (Confidence: HIGH)

### Required Header Records

1. `RINEX VERSION / TYPE` — "2.11" + "O" (observation) + satellite system: " " (GPS), "M" (mixed), "R" (GLONASS)
2. `# / TYPES OF OBSERV` — integer count + list of 2-char observation codes
3. `WAVELENGTH FACT L1/2` — default wavelength factors; use `1  1` for GPS; `0  0` for mixed (unknown GLONASS slot)
4. `TIME OF FIRST OBS` — 4-digit year, month, day, hour, min, sec + time system (GPS or GLO)
5. `END OF HEADER` — mandatory final header line

### Observation Type Codes for MSM-Derived Data

| Code | Description | Source |
|------|-------------|--------|
| C1 | Pseudorange L1 C/A (metres) | MSM pseudorange field |
| L1 | Carrier phase L1 (cycles) + LLI flag | MSM phase range field |
| S1 | Signal strength L1 (RINEX 1–9; map from CNR dB-Hz) | MSM CNR field: 0-9 = 0; 10–19 = 1; … 40+ = 9 |
| C2 | Pseudorange L2 (metres) | MSM L2 pseudorange (if dual-frequency) |
| L2 | Carrier phase L2 (cycles) | MSM L2 phase (if dual-frequency) |
| S2 | Signal strength L2 | MSM L2 CNR |

**Note:** Use C1 not P1 for MSM-sourced pseudorange. P-code pseudorange is distinct; MSM measures C/A-equivalent pseudorange which maps to C1 in RINEX 2.11 convention.

### Satellite System Identifiers in RINEX 2.11

| Identifier | Constellation | Status |
|------------|---------------|--------|
| G (or blank) | GPS | Official in spec |
| R | GLONASS | Official in spec |
| S | SBAS/geostationary | Official in spec |
| E | Galileo | De-facto extension; accepted by RTKLIB, teqc |
| C | BeiDou | NOT in spec; informal extension; RTKLIB extended mode only |

### Epoch Record Format

```
 YY MM DD HH MM SS.SSSSSSS  0 NN SSSSSSSSSSS...  [clock_offset]
```
Where: YY=2-digit year, epoch flag 0=normal, NN=satellite count (max 12 per line; continuation lines for >12), SSS=satellite IDs (G01, R07, E03, C01…).

Observation record per satellite (for each declared observation type in order):
```
F14.3 I1 I1 [repeated per obs type, max 5 per 80-char line, continuation lines for more]
```
Value (14.3), LLI flag (0=ok, 1=lost lock, 4=half-cycle), signal strength (0=unknown, 5=good, 9=max).

---

## RINEX 2.11 Navigation Files: Format Summary (Confidence: HIGH)

### GPS Mixed Nav (.xxP) — from RTCM3 1019

Each record: header line (PRN, epoch, af0, af1, af2) + 7 broadcast orbit lines with D19.12 fields, 4 per line:
- Line 1: IODE, Crs, Δn, M0
- Line 2: Cuc, e, Cus, √A
- Line 3: toe, Cic, Ω0, Cis
- Line 4: i0, Crc, ω, Ω̇
- Line 5: IDOT, L2 codes, GPS week, L2 P flag
- Line 6: SV accuracy (URA), SV health, TGD, IODC
- Line 7: transmission time of message, fit interval

### GLONASS Mixed Nav (.xxP) — from RTCM3 1020

Header line (PRN with R prefix, epoch, −τn, +γn, message frame time) + 3 orbit lines:
- Line 1: X position (km), X velocity (km/s), X acceleration (km/s²), health
- Line 2: Y position (km), Y velocity (km/s), Y acceleration (km/s²), frequency number
- Line 3: Z position (km), Z velocity (km/s), Z acceleration (km/s²), age of data

### RINEX 2.11 BeiDou/Galileo Limitation

BeiDou ('C') and Galileo ('E') are not in the official RINEX 2.11 specification. Practical situation:
- RTKLIB accepts 'E' and 'C' via an extended-2.11 compatibility mode
- teqc explicitly supports them as an extension
- Commercial tools may reject files containing these identifiers

**Recommendation:** Build GPS + GLONASS first (fully compliant). Add Galileo ('E') as a tested extension. Treat BeiDou as requiring explicit compatibility verification with the target toolchain, or upgrade to RINEX 3.x output for BeiDou.

---

## Skyplot Implementation (Confidence: HIGH)

### Data Source

NMEA GSV sentences, emitted per-constellation: `$GPGSV` (GPS), `$GLGSV` (GLONASS), `$GAGSV` (Galileo), `$BDGSV` (BeiDou). Each sentence covers up to 4 satellites:
- SV PRN (integer)
- Elevation: 0–90° (integer degrees)
- Azimuth: 000–359° (integer degrees, true north)
- C/N0: 00–99 dB-Hz (integer) or null if not tracking

### Polar Plot Conventions

- Center = zenith (elevation 90°); outer ring = horizon (elevation 0°)
- North = 12 o'clock; azimuth increases clockwise
- Concentric rings at 15° or 30° elevation intervals
- SVG coordinate mapping for satellite dot:
  - `r_fraction = (90 - elevation_deg) / 90.0`
  - `x = cx + R * r_fraction * sin(azimuth_rad)`
  - `y = cy - R * r_fraction * cos(azimuth_rad)`

### Standard Visual Conventions

- Satellites as filled circles labelled with system+PRN (e.g. G07, R12, E04)
- Constellation colour coding: GPS=green, GLONASS=blue, Galileo=red, BeiDou=orange (conventional; not standardised)
- Not-tracking satellites (null C/N0): hollow/grey circle, still plotted at elevation/azimuth if available
- Azimuth labels at N/E/S/W minimum; every 30° optional
- Elevation labels on concentric rings

---

## SNR Bar Chart (Confidence: HIGH)

- X-axis: satellite identifiers (e.g. G01, G07, R03, E04, C12) sorted by constellation then PRN
- Y-axis: C/N0 0–99 dB-Hz
- Threshold reference lines: 20 dB-Hz (minimum usable), 35 dB-Hz (good signal)
- Bar colour: matches constellation colour from skyplot
- Null C/N0 (satellite in view but not tracked): greyed bar at height 0 or bar omitted

---

## NVS Trait Abstraction (Confidence: MEDIUM)

### ESP-IDF NVS Model (What the Trait Must Abstract)

- Namespace handle: open a named namespace (up to 15 chars) → returns a handle
- Typed get/set per handle: u8, u16, u32, u64, i8, i16, i32, i64, str (null-terminated), blob (arbitrary bytes)
- Key constraint: max 15 characters
- Value size: integers trivial; str/blob up to ~4000 bytes (partition-constrained in practice)
- Operations: get (returns Option — key may not exist), set, remove, commit (required after writes in some ESP-IDF versions)
- Errors: key not found, wrong type, no space remaining, invalid argument

### embedded-svc Current State

The `embedded-svc` crate defines `RawStorage` (blob get/set) but does not provide a full typed NVS trait with namespaces. The firmware currently uses `esp_idf_svc::nvs` directly. A gap crate is needed.

### Recommended Trait Interface Shape

```rust
// NVS namespace handle — scoped to a partition + namespace
trait NvsNamespace {
    type Error;
    fn get_u32(&self, key: &str) -> Result<Option<u32>, Self::Error>;
    fn set_u32(&mut self, key: &str, val: u32) -> Result<(), Self::Error>;
    // ... u8/u16/u64/i8/i16/i32/i64 variants
    fn get_str<'a>(&self, key: &str, buf: &'a mut [u8]) -> Result<Option<&'a str>, Self::Error>;
    fn set_str(&mut self, key: &str, val: &str) -> Result<(), Self::Error>;
    fn get_blob<'a>(&self, key: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Self::Error>;
    fn set_blob(&mut self, key: &str, val: &[u8]) -> Result<(), Self::Error>;
    fn remove(&mut self, key: &str) -> Result<(), Self::Error>;
    fn commit(&mut self) -> Result<(), Self::Error>;
}

trait NvsPartition {
    type Namespace<'a>: NvsNamespace where Self: 'a;
    fn open_namespace(&self, name: &str) -> Result<Self::Namespace<'_>, Self::Error>;
}
```

### ekv as Planned nostd Backing Implementation

ekv (embassy-rs) provides `Map<Vec<u8>, Vec<u8>>` on raw NOR flash using an LSM-tree. To implement `NvsNamespace` on top: prefix keys with namespace (e.g. `b"ntrip\0host"`) and serialise typed values to/from little-endian bytes. Limitations relevant to this use case:
- ekv is optimal for >1000 keys; firmware NVS has ~10–50 keys — functionally correct but architecturally over-engineered
- ekv write transactions use at least one full flash page erase even for a single key update (costly for small writes)
- On-disk format is not stable across major versions (acceptable for firmware that can re-provision)
- ekv requires an `embedded-storage` NorFlash impl from the specific flash chip driver

The trait abstraction matters more than the initial backing implementation. Define the trait cleanly against ESP-IDF NVS first; prove portability with ekv second.

---

## MVP Definition

### Launch With (v2.1)

Minimum to prove the server concept and deliver RINEX output.

- [ ] MQTT subscriber connects, subscribes to `gnss/{id}/rtcm/#` and `gnss/{id}/nmea/#` and `gnss/{id}/heartbeat`
- [ ] RTCM3 MSM frame identification: map message number → (constellation, MSM level 4 or 7)
- [ ] MSM4 and MSM7 decode: pseudorange (m), carrier phase (cycles), CNR (dB-Hz) per satellite+signal
- [ ] Epoch formation: buffer MSM frames by time-of-week within a short window; emit epoch when window closes
- [ ] RINEX 2.11 observation file writer: GPS + GLONASS, C1/L1/S1 observations, hourly rotation, correct filename
- [ ] RTCM3 1019 + 1020 decode → RINEX mixed nav file (.xxP) writer, hourly rotation
- [ ] HTTP server with single page: skyplot SVG + SNR bar chart + device health panel
- [ ] WebSocket endpoint: push satellite state (elevation, azimuth, CNR per PRN) from parsed GSV at 1 Hz
- [ ] Multi-constellation GSV parsing (GPGSV, GLGSV, GAGSV, BDGSV)
- [ ] NVS trait: define interface, ESP-IDF NVS concrete implementation, document ekv as planned nostd impl

### Add After Validation (v2.1.x)

- [ ] Galileo MSM decode + 'E' observations in RINEX obs file; 1045 ephemeris → mixed nav file
- [ ] BeiDou MSM decode + 'C' observations (with toolchain compatibility note); 1042 ephemeris
- [ ] Satellite trail history on skyplot (accumulate last N positions per PRN)
- [ ] Signal quality colour coding on skyplot dots
- [ ] ekv-backed NVS trait implementation (nostd)

### Future Consideration (v2.2+)

- [ ] RINEX 3.x output option (3-character observation codes, proper multi-constellation identifiers)
- [ ] PPP-AR post-processing pipeline (feed RINEX to online PPP service)
- [ ] Multi-device aggregation (server subscribes to multiple device IDs)
- [ ] RTCM3 NTRIP server output (act as caster from received base data)

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| MSM4/MSM7 decode (GPS + GLONASS) | HIGH | HIGH | P1 |
| Epoch formation from MSM time-of-week | HIGH | HIGH | P1 |
| RINEX 2.11 obs file (.xxO) GPS + GLONASS | HIGH | HIGH | P1 |
| RINEX 2.11 mixed nav file (.xxP) GPS + GLONASS | HIGH | MEDIUM | P1 |
| Hourly file rotation + RINEX naming | HIGH | LOW | P1 |
| Skyplot polar SVG from NMEA GSV | HIGH | MEDIUM | P1 |
| SNR bar chart from NMEA GSV | MEDIUM | LOW | P1 |
| Device health panel (heartbeat) | MEDIUM | LOW | P1 |
| NVS trait definition + ESP-IDF impl | HIGH | MEDIUM | P1 |
| Galileo MSM + ephemeris (1094/1097, 1045) | MEDIUM | MEDIUM | P2 |
| BeiDou MSM + ephemeris (1124/1127, 1042) | MEDIUM | MEDIUM | P2 |
| Satellite trail history on skyplot | LOW | LOW | P2 |
| Signal quality colour on skyplot dots | LOW | LOW | P2 |
| Multi-constellation GSV aggregation | MEDIUM | LOW | P1 |
| ekv NVS backing implementation | MEDIUM | HIGH | P2 |
| RINEX 3.x output | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

---

## Sources

- [RINEX 2.11 specification (IGS official)](https://files.igs.org/pub/data/format/rinex211.txt) — HIGH confidence, fetched directly
- [RINEX 2.11 Observation format reference (gLAB/UPC)](https://server.gage.upc.edu/gLAB/HTML/Observation_Rinex_v2.11.html) — HIGH
- [RTCM3 MSM message numbers and fields (Tersus GNSS)](https://www.tersus-gnss.com/tech_blog/new-additions-in-rtcm3-and-What-is-msm) — MEDIUM-HIGH
- [RTCM3 message cheat sheet (SNIP)](https://www.use-snip.com/kb/knowledge-base/an-rtcm-message-cheat-sheet/) — HIGH (consistent with Tersus)
- [MSM decoding overview (SNIP)](https://www.use-snip.com/kb/knowledge-base/decoding-msm-messages/) — MEDIUM
- [NMEA GSV field reference (gps-wizard/logiqx)](https://logiqx.github.io/gps-wizard/nmea/messages/gsv.html) — HIGH, fetched directly
- [GLONASS RTCM MSM to RINEX notes (rtklibexplorer)](https://rtklibexplorer.wordpress.com/2020/11/01/converting-glonass-rtcm-msm-messages-to-rinex-with-rtklib/) — MEDIUM
- [rtcm3torinex real-time converter (nunojpg)](https://github.com/nunojpg/rtcm3torinex) — MEDIUM (implementation reference)
- [embassy ekv key-value store](https://github.com/embassy-rs/ekv) — MEDIUM (docs fetched; no_std status not explicitly confirmed)
- [RINEX 2.11 vs 3.x multi-constellation (GIS Resources)](https://gisresources.com/from-rinex-4-0-a-guide-to-gnss-data/) — MEDIUM
- [RTKLIB multi-constellation RINEX support](https://www.rtklib.com/) — MEDIUM (support matrix confirmed from multiple secondary sources)

---
*Feature research for: esp32-gnssmqtt v2.1 — server RINEX generation, live web UI, nostd NVS abstraction*
*Researched: 2026-03-12*
