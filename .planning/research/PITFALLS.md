# Pitfalls Research

**Domain:** v2.1 — RTCM3 MSM decoding, RINEX 2.x writing, embassy/nostd migration, WebSocket skyplot, mixed std/nostd Cargo workspace; added to existing ESP32-C6 GNSS firmware
**Researched:** 2026-03-12
**Confidence:** HIGH for MSM cell mask / carrier phase (verified against RTKLIB source and rtklibexplorer post-mortems); HIGH for RINEX format (verified against IGS RINEX 2.11 spec); HIGH for Cargo workspace feature unification (verified against Cargo RFC and official docs); MEDIUM for embassy/nostd panic handler conflicts (verified via esp-rs/esp-hal GitHub discussions); MEDIUM for WebSocket memory patterns (verified against tokio-tungstenite issue tracker)

---

## Critical Pitfalls

### Pitfall 1: MSM Cell Mask Bit Count Is Not nsat × nsig

**What goes wrong:**
The MSM cell mask field is exactly nsat × nsig bits wide, where nsat = popcount(satellite_mask) and nsig = popcount(signal_mask). A naive decoder reads the cell mask as a fixed width (e.g., always 64 bits) and counts the wrong number of cells. The cell count drives how many per-cell data fields (pseudorange fine, carrier phase fine, CNR, lock-time) are read from the bitstream. An incorrect cell count misaligns the bit cursor for every field that follows, producing garbage observations for all satellites in the epoch.

**Why it happens:**
The satellite mask is 64 bits; the signal mask is 32 bits; neither is the cell mask width. Developers read the spec as "satellite mask + signal mask + cell mask" and assume the cell mask is a fixed 64- or 32-bit field. The actual width is dynamic: popcount(sat_mask) × popcount(sig_mask). For GPS MSM7 with 8 satellites and 2 signals, the cell mask is 16 bits; for GPS+GLONASS+Galileo+BeiDou MSM7 with 30 total satellites and 3 signals, it is 90 bits. This requires reading a non-byte-aligned field of variable length from the bitstream.

**How to avoid:**
Count set bits in the satellite mask and signal mask immediately after reading them. Use those counts — call them `nsat` and `nsig` — to determine the cell mask width: `nsat * nsig` bits. Then the number of cells with data is `popcount(cell_mask)`, which drives how many per-cell arrays to read. The per-satellite arrays (rough range integer, rough range modulo-1ms) have `nsat` entries; per-signal arrays (fine pseudorange, fine phase, CNR) have `ncell = popcount(cell_mask)` entries. These are two different array lengths in the same message. Reference: RTKLIB `src/rtcm3.c` function `decode_msm_header()` — it computes nsat, nsig, and ncell explicitly before reading any per-observation data.

**Warning signs:**
- All observations after the first satellite parse as wildly wrong values (>1e9 metres pseudorange)
- Carrier phase values are identical across all satellites (bit cursor stuck)
- Decoder produces the correct number of satellites but wrong signal count, or vice versa
- Debug print of bit cursor position diverges from expected position after cell mask read

**Phase to address:** RTCM3 MSM decode phase — implement and validate the three-mask structure before touching any observation data fields.

---

### Pitfall 2: GLONASS Carrier Phase Is Zero Without FCN

**What goes wrong:**
GLONASS MSM4 messages (type 1084) do not carry the frequency channel number (FCN) for each GLONASS satellite. RTKLIB and any conforming decoder requires the FCN to convert the phase-range (stored in cycles of a nominal frequency) to actual carrier phase in cycles of the satellite's true frequency. Without FCN, the decoder outputs zero carrier phase for all GLONASS satellites, silently. The pseudorange is unaffected — it is derived from the rough range and fine pseudorange fields, which do not require FCN.

**Why it happens:**
MSM4 includes only basic satellite data; extended satellite info (including FCN for GLONASS) appears only in MSM5 and MSM7. A server decoding 1084 messages without accompanying 1020 (GLONASS ephemeris) messages has no FCN source and cannot compute GLONASS carrier phase. The failure is silent — the decoder produces valid-looking RINEX with L1C/L2C observations filled but carrier phase is 0.0 for every GLONASS satellite in every epoch.

**How to avoid:**
For GLONASS carrier phase: require either (a) MSM7 messages (1087) which include extended satellite info with FCN, or (b) GLONASS 1020 ephemeris messages alongside any MSM variant to supply FCN. The UM980 outputs GLONASS 1020 by default when RTK mode is configured; verify this is enabled. When FCN is unknown, write the GLONASS carrier phase field as a blank (space-filled F14.3) rather than 0.0 — a blank is distinguished from a valid zero measurement by downstream RINEX parsers.

**Warning signs:**
- RINEX output shows GLONASS L1C/L2C observations present but carrier phase column is 0.000000000 for every GLONASS epoch
- RTKLIB reports "no glonass freq" warnings in convbin log
- RTK solution degrades to code-only when GLONASS satellites dominate the constellation

**Phase to address:** RTCM3 MSM decode phase, specifically GLONASS handling. Add a test that verifies FCN is sourced before carrier phase is written.

---

### Pitfall 3: Pseudorange Integer Ambiguity Reconstruction Is Not Just a Scale Factor

**What goes wrong:**
MSM messages split pseudorange into two parts: a rough range (integer milliseconds, DF397 for the integer part + DF398 for the sub-millisecond modulo) stored per-satellite, and a fine pseudorange correction (DF400 for MSM4, DF405 for MSM7) stored per-cell. Combining them requires: `pseudorange = (rough_range_integer + rough_range_modulo) * LIGHT_MS + fine_pseudorange * scale`. Developers often apply the fine scale factor to the wrong combination, or forget that rough_range_integer is in integer light-milliseconds (~300km per count) while rough_range_modulo is in units of 2^-10 ms. The two rough range parts must be combined before adding the fine correction, not independently scaled.

**Why it happens:**
The RTCM spec describes DF397 (integer ms), DF398 (modulo 1ms in 2^-10 ms units), and DF400/DF405 (fine pseudorange) as separate data fields without making the combination formula explicit. The scale factors differ by six orders of magnitude: rough integer is ~299,792 metres/count, fine correction is 2^-24 ms * c ≈ 0.0596 metres/count (MSM4) or 2^-29 ms * c ≈ 0.00186 metres/count (MSM7). Applying the MSM4 fine scale to MSM7 data introduces a 32x error in the fine correction.

**How to avoid:**
Implement the pseudorange reconstruction in a single function with explicit named variables. Use the RTKLIB `decode_msm7()` implementation as the reference — it computes `r = rough[i] + rrv[i]` first (combining integer and modulo), then adds `pr[j] * P2_29 * CLIGHT * 1e-3` for the fine correction. Verify against a known RTCM3 frame decoded by RTKLIB or pyrtcm before integrating into RINEX output.

**Warning signs:**
- Pseudorange values are 32x or 1024x too large or too small compared to expected ~20,000km range
- Fine pseudorange is correct (matches reference) but total pseudorange has systematic offset
- Differences between epochs are correct (relative pseudorange is right) but absolute values are wrong by a constant

**Phase to address:** RTCM3 MSM decode phase — write unit tests comparing output against pyrtcm or RTKLIB before any RINEX writing begins.

---

### Pitfall 4: RINEX 2.11 Header Labels Must Be in Exactly Columns 61-80

**What goes wrong:**
RTKLIB, rnx2rtkp, and teqc all reject RINEX 2.11 files where header labels are not positioned in exactly columns 61-80 (1-indexed, fixed-width ASCII). A label like `RINEX VERSION / TYPE` written starting at column 60 or 62 causes the parser to skip the header line entirely and either reject the file or silently misparse the version number. The result is "unknown version" or "unknown file type" errors that do not identify the column error as the root cause.

**Why it happens:**
RINEX 2.11 is a fixed-column format, not delimited. The spec states "header labels in columns 61-80" and provides Fortran-style format strings. Rust string formatting with `format!` and `{:>n}` padding is natural for aligning values but easy to miscalculate when the value field width changes (e.g., "RINEX VERSION / TYPE" is 20 characters, "END OF HEADER" is 13 characters — both must start at column 61 despite different lengths).

**How to avoid:**
Write a single `fn header_line(content: &str, label: &str) -> String` helper that formats content into columns 1-60 and label into columns 61-80, padding with spaces. Assert in tests that the result is exactly 80 characters. Verify every header line type independently. Do not use `println!` or `writeln!` with ad-hoc format strings for header lines — each one should pass through the same helper to guarantee column alignment.

**Warning signs:**
- `rnx2rtkp` reports "invalid rinex file" with no further detail
- teqc reports "label not found" for mandatory headers like `APPROX POSITION XYZ`
- RTKLIB reads the file but treats all epochs as the wrong year (version field misread)

**Phase to address:** RINEX writer phase — implement the `header_line()` helper first, before any header fields, and write a test that checks every required header line for exact 80-character length and correct label position.

---

### Pitfall 5: RINEX 2.11 Observation Records Truncate at Column 80 — More Than 5 Obs Types Need Continuation Lines

**What goes wrong:**
Each RINEX 2.11 observation data record can hold at most 5 observation values per line (F14.3, I1, I1 = 16 characters each × 5 = 80 characters). If the # TYPES OF OBSERV header declares more than 5 types (common for multi-frequency: C1, P1, P2, L1, L2, S1, S2 = 7 types), each epoch requires a continuation line for the remaining observations. Omitting continuation lines causes downstream parsers to attribute the values to the wrong observation type or fail to find expected observation counts.

**Why it happens:**
MSM7 decoding produces rich data: pseudorange, carrier phase, Doppler, CNR — often 6-8 observation types per signal. Developers writing the RINEX encoder loop over all observations in one pass without checking the 5-observation-per-line limit.

**How to avoid:**
Structure the observation encoder as: write up to 5 obs values on the first line (pad missing observations with 16 spaces), then for each subsequent group of 5, write a continuation line starting at column 1 (not a new epoch line). The continuation line format uses columns 1-80 only; there is no epoch header on continuation lines. Count your declared observation types and verify the encoder generates exactly `ceil(n_types / 5)` lines per satellite per epoch.

**Warning signs:**
- RTKLIB reports wrong number of observations per epoch
- Observation type offsets are shifted by exactly 5 positions (continuation line missing — values attributed to wrong type)
- File passes `teqc -qc` but RTK solution uses only the first 5 observation types

**Phase to address:** RINEX writer phase — write a test that reads back a generated RINEX file with a RINEX 2.11 parser (e.g., RTKLIB convbin or a minimal Rust parser) and compares observation values round-trip.

---

### Pitfall 6: RINEX 2.11 File Naming — Session Code and Compression Convention

**What goes wrong:**
IGS data archives and tools like teqc enforce the 8.3 naming convention: `SSSSddds.yyO` where SSSS is 4-char station name, ddd is day-of-year (zero-padded to 3 digits), s is the session identifier (0 for a single daily file, or A-X for hourly sessions where A=hour 0, B=hour 1, ..., X=hour 23), yy is 2-digit year, and O is file type (O=observation, N=navigation). The common mistake is using a lowercase session code, using a decimal hour instead of the letter code, or using a 2-digit day-of-year (missing leading zero). Tools that use filename parsing to determine time coverage silently misattribute hours when session codes are wrong.

**Why it happens:**
Generating a filename at rotation time typically involves formatting the current UTC time. The session letter mapping (hour 0 = 'A', hour 23 = 'X') is a lookup table, not a natural format. Using `format!("{hour:02}h")` instead of `char::from(b'A' + hour as u8)` produces the wrong session field.

**How to avoid:**
Implement a `fn rinex_filename(station: &str, doy: u16, hour: u8, year: u16, obs_type: char) -> String` function with the session letter computed as `(b'A' + hour).into():char()` for hourly files or '0' for daily. Validate station names are exactly 4 characters (pad with 'X' or truncate). For the mixed navigation file extension use `.yyP` (mixed), `.yyN` (GPS only), `.yyG` (GLONASS only); the UM980 produces all four constellations so `.yyP` is correct.

**Warning signs:**
- teqc refuses to merge hourly files into a daily file (session code mismatch)
- IGS archive ingestion tools log "unexpected filename format" and skip the file
- Hourly rotation produces files where hour 10 is named `...10h.26O` instead of `...K.26O`

**Phase to address:** RINEX writer phase, specifically the rotation and naming logic. Test hourly rotation across midnight (day rollover) and across year boundaries.

---

### Pitfall 7: Hidden `std` Dependency Pulled In by Workspace Feature Unification

**What goes wrong:**
A no_std gap crate in the workspace compiles cleanly in isolation (`cargo build -p gap-crate --target riscv32imac-unknown-none-elf`). After another workspace member (the server binary, targeting x86_64) is added as a dependency on a shared library, Cargo's feature unification enables the `std` feature on that shared library for all workspace members. The no_std gap crate now transitively depends on `std` via the shared library, and the embedded build fails with "can't find crate for `std`" — which does not name the shared library as the source of the problem.

**Why it happens:**
Cargo resolver v1 unifies all feature sets across all workspace members for a given dependency. If the server binary enables `serde/std` and the gap crate depends on `serde/no_std`, resolver v1 enables `serde/std` for the gap crate too, breaking the no_std build. This applies to any dependency with an optional `std` feature.

**How to avoid:**
Set `resolver = "2"` (or `"3"` for edition 2024) in the workspace root `Cargo.toml` immediately when the workspace is created. Resolver v2 separates build-dependency features from normal dependency features and target-specific features from host-specific features. Additionally, for shared library crates that must compile in both std and no_std contexts, use `#![cfg_attr(not(feature = "std"), no_std)]` and guard `alloc` imports behind `#[cfg(feature = "alloc")]` or `#[cfg(not(feature = "std"))]`. Run `cargo build -p gap-crate --target riscv32imac-unknown-none-elf` in CI to catch regressions before they reach the firmware build.

**Warning signs:**
- A crate that previously compiled as no_std fails after a new workspace member is added
- Error is "can't find crate for `std`" in a crate you did not touch
- `cargo tree -f '{f}' -e features` shows `std` feature enabled on a dependency that should be no_std
- Build succeeds when building the gap crate in isolation but fails in the workspace

**Phase to address:** Cargo workspace setup phase — the first commit that creates the workspace must include `resolver = "2"` and a CI step that builds the embedded target.

---

### Pitfall 8: Panic Handler Conflict Between esp-backtrace and Any Crate That Also Defines `#[panic_handler]`

**What goes wrong:**
The no_std gap crate defines a `#[panic_handler]` for unit tests (to satisfy the linker in test mode). When the embedded firmware links against the gap crate and also depends on `esp-backtrace` (which defines its own `#[panic_handler]`), the linker errors with "duplicate lang item: panic_impl". This blocks firmware builds whenever the gap crate's test infrastructure leaks into the firmware dependency graph.

**Why it happens:**
In Rust, only one `#[panic_handler]` may exist per final binary. A no_std crate that defines a panic handler for its own tests exposes that handler to any binary that links against it unless the handler is gated behind a `cfg(test)` or a feature flag. The Rust compiler does not automatically suppress test-only panic handlers when the crate is used as a library.

**How to avoid:**
Gate test panic handlers behind `#[cfg(test)]` exclusively. Never define a `#[panic_handler]` in a library crate's non-test code. The canonical pattern for no_std library unit tests on a host target is to use `#[cfg(test)]` with a test harness that provides `std`, not a custom panic handler. If the gap crate must be tested on the embedded target (no host std), use `cargo test --target riscv32imac-unknown-none-elf` with `defmt-test` or `embedded-test` — those frameworks provide the panic handler for the test binary without exposing it to dependent crates.

**Warning signs:**
- `error[E0152]: duplicate lang item found: panic_impl` during firmware build
- Error appears only after adding a gap crate as a dependency, not when building the gap crate alone
- `cargo test` for the gap crate passes but `cargo build` for the firmware fails

**Phase to address:** Gap crate implementation phase — establish panic handler isolation policy before any gap crate defines test infrastructure.

---

### Pitfall 9: Embassy Executor on ESP32-C6 Requires `esp-hal`, Not `esp-idf-hal`

**What goes wrong:**
The existing firmware uses `esp-idf-hal` (the std-based ESP-IDF wrapper). Embassy's executor for ESP32 targets is designed for `esp-hal` (the no_std bare-metal HAL). Attempting to use `embassy-executor` with `esp-idf-hal` in a single binary causes linker failures (missing `__pender` or `__EMBASSY_EXECUTOR` symbols) or runtime panics because `embassy-executor` expects its own interrupt configuration, not FreeRTOS.

**Why it happens:**
Both HALs target the same hardware but have incompatible execution models. `esp-idf-hal` runs on FreeRTOS tasks; `embassy-executor` provides its own cooperative scheduler that bypasses FreeRTOS entirely. They cannot coexist in the same binary. The v2.1 milestone is an audit and scaffolding exercise — the actual Embassy migration is future work — but it is tempting to try running Embassy alongside esp-idf-hal for testing, which breaks the firmware build.

**How to avoid:**
Keep Embassy usage strictly in no_std gap crates compiled for `riscv32imac-unknown-none-elf`, not in the firmware binary (which targets `riscv32imac-esp-espidf`). The firmware target triple `riscv32imac-esp-espidf` includes the ESP-IDF C runtime; `riscv32imac-unknown-none-elf` is bare-metal. They are different targets and cannot share executors. The v2.1 audit output should be trait definitions and crate skeletons — not running Embassy code in the existing firmware.

**Warning signs:**
- Linker errors referencing `__pender`, `EMBASSY_EXECUTOR_TASK_POOL`, or `__heap_start` when adding `embassy-executor` to the firmware Cargo.toml
- Firmware boots but hangs immediately at runtime (Embassy executor initializes and blocks FreeRTOS scheduler)
- `cargo build` for the firmware succeeds but `espflash flash` produces a device that does not start

**Phase to address:** Dependency audit phase — clearly separate "things we will use now" from "things we are auditing for future use". Embassy crates go in gap crates only, not in the firmware Cargo.toml.

---

### Pitfall 10: WebSocket Broadcast Channel Grows Without Bound for Slow Browser Clients

**What goes wrong:**
A `tokio::sync::broadcast` channel with a fixed capacity (e.g., 100 messages) is used to push GNSS state to WebSocket clients. When a client is slow (mobile browser, high-latency connection), its receiver falls behind. In tokio's broadcast, a lagged receiver gets a `RecvError::Lagged(n)` error and drops `n` missed messages. This is the intended behaviour. However, if the server task awaits `ws_sender.send(msg)` without a timeout, a slow client blocks the server task indefinitely, which in turn blocks all other clients sharing that task, causing the broadcast to queue up in memory for the blocked sender.

**Why it happens:**
The WebSocket write path in axum (`ws.send()`) is `async` and back-pressures the caller. If a single client's TCP send buffer is full and the server awaits `ws.send()` without a timeout, the entire handler task stalls. This matters for the skyplot: GNSS state updates every second or faster; a client that takes more than 1 second to drain its TCP buffer will cause the handler task to queue outgoing messages in the tokio runtime, growing memory.

**How to avoid:**
Use `tokio::time::timeout(Duration::from_secs(1), ws.send(msg))` and disconnect the client if the timeout fires. Alternatively, use `try_send` on a per-client bounded channel and drop the client on failure. Do not use a single server task that awaits sends to multiple clients sequentially — use a `tokio::task::spawn` per client. Each client task reads from the broadcast receiver and sends to its own WebSocket; a slow client only kills its own task.

**Warning signs:**
- Server memory grows at ~1MB/min when a WebSocket client is open but not actively reading (e.g., browser tab in background)
- Server works with 1 browser client but hangs with 2 when one tab goes to background
- tokio task count (via `tokio-console` or metrics) grows over time and does not decrease when clients disconnect

**Phase to address:** HTTP/WebSocket server phase — implement per-client tasks with bounded send timeouts from the start, not as a later optimisation.

---

### Pitfall 11: RINEX Observation Value of 0.0 Is Valid — Do Not Use It to Represent "No Data"

**What goes wrong:**
A carrier phase of exactly 0.0 cycles is written when the decoder cannot produce a value (e.g., GLONASS FCN unknown, signal not tracked this epoch, lock-time indicator not yet initialised). Downstream tools interpret 0.0 as a valid observation, not as missing data. RINEX 2.11 missing observations are represented by a blank field (16 spaces: `                `) — not by 0.0 or NaN. An observation file with 0.0 for missing carrier phase causes RTKLIB to attempt to use those observations, producing large residuals and degraded RTK solutions.

**Why it happens:**
Rust's default numeric value is 0, and the natural representation of "I have no data here" in a numeric field is 0.0 or `f64::NAN`. Neither maps to RINEX's blank convention. Developers write the RINEX encoder to format the observation value and append the LLI and signal strength flags, forgetting that the entire 16-character slot must be blank when data is absent.

**How to avoid:**
Represent observation presence with `Option<f64>`. When the `Option` is `None`, write 16 spaces (`format!("{:>16}", "")` or `"                ".to_string()`). When it is `Some(v)`, write `format!("{:14.3}{:1}{:1}", v, lli, sig)`. Never write `Some(0.0)` for missing data. Validate the encoder with a test that checks a RINEX file containing `None` observations can be parsed back by a reference implementation without false observations.

**Warning signs:**
- RTKLIB logs "large residual" or "outlier rejected" for GLONASS satellites in every epoch
- RTK baseline is correct when GLONASS is excluded but wrong when included
- Observation file has 0.000000000 carrier phase for entire GLONASS constellations

**Phase to address:** RTCM3 MSM decode phase (set Option correctly at decode time) and RINEX writer phase (format None as blank).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Write 0.0 for missing RINEX observations instead of blank | Simpler encoder (no Option) | RTKLIB uses spurious observations; RTK solution degrades silently | Never |
| Hard-code MSM satellite mask width as 64 bits | Simpler bit-reader | Wrong cell count for multi-constellation messages; all observations corrupted | Never |
| Skip GLONASS FCN tracking (always write 0.0 phase) | Avoids 1020 ephemeris parsing | Silent GLONASS carrier phase loss; post-processing cannot use GLONASS | Acceptable only if RINEX files are GPS-only; document the limitation |
| Use resolver = "1" (default) in mixed workspace | No action required | First shared dependency with optional `std` feature will silently enable std in no_std crates | Never — set resolver = "2" on workspace creation |
| Define `#[panic_handler]` in library crate (not gated by cfg(test)) | Simpler test setup | Breaks any firmware that links against the library | Never |
| Use `tokio::sync::broadcast` with one receiver per client and no timeout on send | Simpler code | Memory growth per slow client; server stalls when any client goes to background | Only for internal tooling with one client; never for production |
| Single RINEX file (no hourly rotation) | Simpler file management | Files grow unbounded; tools like teqc have line count limits; IGS rejects >24h files | Only during development/testing; never for production archive |
| Use MSM4 (1084) for GLONASS without 1020 ephemeris | Fewer messages to parse | Zero GLONASS carrier phase; post-processing cannot fix ambiguities | Never if RTK or PPP is the intended use |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| RTKLIB convbin | Providing RINEX 2.11 with missing `MARKER NAME` header | `MARKER NAME` is mandatory in RTKLIB's rinex.c; omitting it causes silent rejection; use station name from config |
| teqc | Header label at wrong column (e.g., col 60 instead of 61) | Every header label must be in columns 61-80; write a helper function, not ad-hoc format strings |
| rnx2rtkp | Observation file missing receiver clock offset column when receiver sends it | Receiver clock offset is optional but if present must be in columns 69-80; if unknown, leave blank (not 0.0) |
| MQTT broker | Server subscribing to wildcard `gnss/+/rtcm` receives binary RTCM frames | MQTT payload is `Vec<u8>`; do not attempt UTF-8 decode; existing firmware publishes raw bytes, server must treat payload as `&[u8]` |
| tokio-tungstenite | Not handling `Message::Ping` — browser disconnects after 30s keep-alive timeout | axum's built-in WebSocket handler auto-pongs; tokio-tungstenite does not — must call `ws.send(Message::Pong(...))` explicitly |
| GNSS MSM over MQTT | MQTT publishes raw RTCM3 frames including preamble and CRC; server must re-verify CRC | Server cannot assume frames are valid; re-run CRC24Q on received payload before decoding |
| Cargo workspace cross-target | `cargo build` at workspace root tries to build all members for the host target | Embedded crates will fail on host; use `-p` to build specific members or add target-specific `[package.metadata]` to gate build |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Allocating `Vec<Observation>` per MSM epoch | Heap fragmentation in long-running server | Pre-allocate observation arrays sized to max satellites (64) per constellation per epoch; reuse across epochs | After 24h continuous operation with active heap fragmentation |
| Writing RINEX file line by line with `File::write_all` per line | 1000+ syscalls per epoch at 1Hz GNSS rate | Use `BufWriter<File>` with at least 64KB buffer; flush once per epoch or on rotation | Above 10Hz GNSS output rate; on slow storage |
| Parsing all MQTT topics as UTF-8 before type check | Allocates String for every RTCM binary message | Match topic prefix first, then parse payload; avoid UTF-8 coercion on binary payloads | At 1Hz RTCM the cost is negligible; at 10Hz it appears in profiles |
| WebSocket broadcasting full GNSS state on every NMEA sentence | ~10 messages/sec per 5 NMEA types; browser can't redraw that fast | Debounce: merge GNSS state updates, push to browser at fixed rate (1Hz for skyplot, 5Hz for SNR bars) | Immediate with any browser receiving data; DOM updates become the bottleneck |
| RINEX observation sort order wrong | Tools read obs in declared order; wrong order means obs attributed to wrong types | Sort observations in the data record to match the order declared in `# TYPES OF OBSERV` header | At parse time — any mismatch produces silent wrong values |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| WebSocket endpoint open without auth | Any client on the network reads live GNSS position | Acceptable for LAN-only deployment; document clearly; add auth before any internet exposure |
| RINEX files written to world-readable path | GNSS position history exposed to other local users | Write to a path with mode 0600 or application-private directory |
| Server accepts MQTT messages without topic filtering | Forged MQTT messages from any broker client could inject data | Validate `device_id` in topic matches configured device; reject messages from unexpected device IDs |

---

## "Looks Done But Isn't" Checklist

- [ ] **MSM decoder:** Cell count is `popcount(cell_mask)`, not `nsat * nsig` — verify by decoding a real MSM7 frame and comparing observation count to pyrtcm output
- [ ] **MSM decoder:** GLONASS carrier phase is `None` (not 0.0) when FCN is unavailable — verify with a GLONASS-only MSM4 frame without 1020 ephemeris
- [ ] **MSM decoder:** Pseudorange reconstruction uses rough_integer + rough_modulo before adding fine correction — verify absolute pseudorange matches reference within 1 metre
- [ ] **RINEX writer:** Every header line is exactly 80 characters — verify with `assert_eq!(line.len(), 80)` in tests
- [ ] **RINEX writer:** Header labels are in columns 61-80 — spot-check with `&line[60..80]` == label in tests
- [ ] **RINEX writer:** Missing observations are written as 16 spaces, not 0.0 — verify by observing GLONASS phase field for a frame without FCN
- [ ] **RINEX writer:** More than 5 observation types generates continuation lines — verify by declaring 7 types and checking epoch line count per satellite
- [ ] **RINEX writer:** File name uses letter session codes (A-X) for hourly rotation — verify hour 0 produces 'A', hour 23 produces 'X'
- [ ] **Cargo workspace:** `resolver = "2"` present in workspace `[workspace]` table — verify `cat Cargo.toml | grep resolver`
- [ ] **Gap crates:** Compile clean as no_std on `riscv32imac-unknown-none-elf` — verify in CI with `cargo build -p gap-crate --target riscv32imac-unknown-none-elf`
- [ ] **WebSocket server:** Each client runs in its own task — verify by connecting two browser clients and pausing one; confirm the other continues receiving updates
- [ ] **WebSocket server:** Slow client send uses timeout, not blocking await — verify by stalling a test client and confirming server memory is stable

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| MSM cell mask wrong — all observations corrupt | HIGH | Rewrite decode logic from scratch using RTKLIB reference; all RINEX files generated with old decoder are invalid and must be regenerated |
| GLONASS carrier phase zeroed in RINEX archive | MEDIUM | Re-decode RTCM frames from MQTT (if retained or stored); regenerate RINEX files with corrected decoder; no hardware changes needed |
| RINEX header column alignment wrong | MEDIUM | Fix encoder, re-generate all files; any already-processed PPK sessions must be re-run |
| std feature bleeding into no_std crate | LOW | Add `resolver = "2"` to workspace manifest; rebuild; all dependencies will re-compile with correct feature sets |
| Panic handler conflict | LOW | Gate test panic handler behind `#[cfg(test)]`; rebuild |
| WebSocket memory leak | LOW | Add per-client send timeout; redeploy server; no data loss |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| MSM cell mask bit count wrong | RTCM3 MSM decode | Decode real MSM7 frame; compare satellite/cell counts to pyrtcm |
| GLONASS carrier phase zero without FCN | RTCM3 MSM decode | Decode MSM4 without 1020; assert carrier phase is None, not 0.0 |
| Pseudorange reconstruction scale error | RTCM3 MSM decode | Compare pseudorange to RTKLIB convbin output for same frame; delta < 1m |
| RINEX header column misalignment | RINEX writer | `assert_eq!(line.len(), 80)` for all header lines; spot-check label field |
| Observation continuation lines missing | RINEX writer | Declare 7 obs types; verify 2 lines per satellite per epoch |
| RINEX file naming session code wrong | RINEX writer / hourly rotation | Assert hour 0='A', hour 23='X', hour 10='K' |
| Missing obs written as 0.0 not blank | RINEX writer | Parse generated RINEX with RTKLIB; verify GLONASS phase blank when FCN unknown |
| Workspace feature unification enables std | Cargo workspace setup | CI: `cargo build -p gap-crate --target riscv32imac-unknown-none-elf` |
| Panic handler conflict | Gap crate implementation | CI firmware build must succeed after adding gap crate as dependency |
| Embassy executor in firmware binary | Dependency audit | Firmware Cargo.toml must not list `embassy-executor`; enforce by policy |
| WebSocket slow client memory growth | HTTP/WebSocket server | Load test: connect 3 clients, pause 1, monitor server RSS for 60s |
| Missing obs 0.0 corrupts RTK solution | RTCM3 MSM decode + RINEX writer | End-to-end: generate RINEX from known RTCM3 capture, run rnx2rtkp, check solution residuals |

---

## Sources

- RTKLIB `src/rtcm3.c` — `decode_msm_header()` and `decode_msm7()` implementations; canonical MSM cell mask calculation and pseudorange reconstruction. Source at https://github.com/tomojitakasu/RTKLIB/blob/master/src/rtcm3.c — HIGH confidence (direct source inspection by proxy)
- rtklibexplorer: "Limitations of the RTCM raw measurement format" (2017-01-30) — documents MSM carrier phase integer cycle bug and timestamp resolution issue. https://rtklibexplorer.wordpress.com/2017/01/30/limitations-of-the-rtcm-raw-measurement-format/ — HIGH confidence
- rtklibexplorer: "Converting GLONASS RTCM MSM messages to RINEX with RTKLIB" (2020-11-01) — documents FCN requirement, MSM4 vs MSM7 for GLONASS phase, ephemeris workaround. https://rtklibexplorer.wordpress.com/2020/11/01/converting-glonass-rtcm-msm-messages-to-rinex-with-rtklib/ — HIGH confidence
- RTKLIB GitHub issue #129 "Convbin, No GLO obs L1, L2 from RTCM MT1084" — confirms zero carrier phase for GLONASS MSM4 without 1020. https://github.com/tomojitakasu/RTKLIB/issues/129 — HIGH confidence
- IGS RINEX 2.11 format specification — header label column positions, observation record format, continuation line rules. https://files.igs.org/pub/data/format/rinex211.txt — HIGH confidence (official standard)
- Cargo RFC 2957 "new feature resolver" — documents feature unification behaviour and resolver v2 semantics. https://rust-lang.github.io/rfcs/2957-cargo-features2.html — HIGH confidence (official RFC)
- Cargo Book "Features" — documents resolver = "2" and build-dependency feature isolation. https://doc.rust-lang.org/cargo/reference/features.html — HIGH confidence (official documentation)
- Cargo issue #5730 "Features of dependencies are enabled if they're enabled in build-dependencies; breaks no_std libs" — documents the specific failure mode. https://github.com/rust-lang/cargo/issues/5730 — HIGH confidence
- nickb.dev "Cargo Workspace and the Feature Unification Pitfall" — practical example of unwanted feature unification and resolver v2 as fix. https://nickb.dev/blog/cargo-workspace-and-the-feature-unification-pitfall/ — MEDIUM confidence (secondary source, consistent with official docs)
- esp-rs/esp-hal GitHub discussion #738 "Need help solving linker error when using embassy" — documents `__pender` linker error when mixing embassy-executor with non-esp-hal runtimes. https://github.com/esp-rs/esp-hal/discussions/738 — MEDIUM confidence
- tokio-tungstenite GitHub issue #195 "Potential memory leak on sender?" — documents 50KB per-client memory growth that is not released on disconnect. https://github.com/snapview/tokio-tungstenite/issues/195 — MEDIUM confidence
- Rust users forum "How to disable my no_std panic handler when I'm testing with std?" — documents `#[cfg(test)]` gating pattern for panic handlers. https://users.rust-lang.org/t/how-to-disable-my-no-std-panic-handler-when-im-testing-with-std/121698 — MEDIUM confidence
- Existing project: `src/rtcm_relay.rs` — confirms server will receive complete raw RTCM3 frames (preamble + payload + CRC) via MQTT; server must re-verify CRC. HIGH confidence (direct inspection)
- RINEX 2.11 NGS mirror — confirms END OF HEADER format, 60X spacing, mandatory label list. https://www.ngs.noaa.gov/CORS/RINEX211.txt — HIGH confidence (official mirror)

---
*Pitfalls research for: v2.1 Server + nostd Foundation — RTCM3 MSM decode, RINEX 2.x write, embassy/nostd migration, WebSocket skyplot, mixed Cargo workspace*
*Researched: 2026-03-12*
