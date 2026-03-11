# Project Research Summary

**Project:** esp32-gnssmqtt v2.1
**Domain:** Embedded Rust GNSS/MQTT firmware (ESP32-C6) + companion Tokio server + embassy/nostd foundation
**Researched:** 2026-03-12
**Confidence:** HIGH for server stack and RINEX format; MEDIUM for nostd migration feasibility

## Executive Summary

The v2.1 milestone extends a complete, field-validated ESP32-C6 firmware (v2.0) in three directions simultaneously: a companion server binary that subscribes to the existing MQTT feed to decode RTCM3 MSM frames and write RINEX observation files, a live web UI (skyplot + SNR chart) delivered over WebSocket, and an embassy/nostd audit that creates trait definitions and gap crate skeletons for a future bare-metal port. The recommended server stack is a single Cargo workspace containing the existing firmware (moved to `firmware/`), the new server binary (`server/`), and shared no_std library crates (`crates/`). The server uses tokio + axum 0.8 + rumqttc 0.24 + rtcm-rs 0.11 + rinex 0.21 — all of which share the same async runtime with no conflicts. The firmware is unchanged from v2.0; all new code lives in the server and gap crates.

The primary technical risk is in the RTCM3 MSM decode path. Three decode bugs are documented with HIGH confidence from RTKLIB source inspection: the MSM cell mask width is dynamic (popcount(sat_mask) x popcount(sig_mask)), not fixed; GLONASS carrier phase is silently zero without the frequency channel number (FCN) from 1020 ephemeris or MSM7 messages; and pseudorange reconstruction requires combining the rough integer and rough modulo parts before adding the fine correction. Any one of these bugs produces silently incorrect RINEX files that downstream PPK tools will accept without error but compute wrong positions from. The mitigation is to validate the rtcm-rs decoder output against pyrtcm or RTKLIB convbin before writing any RINEX output. Using rtcm-rs rather than a hand-rolled parser eliminates the cell mask and pseudorange bugs; the GLONASS FCN gap requires explicit handling.

The embassy/nostd migration audit reveals that a full bare-metal port is blocked by at least three critical gaps with no drop-in replacements: NVS key-value credential storage (no equivalent in esp-hal; requires `sequential-storage` over raw flash), password-protected SoftAP (explicitly listed as missing in esp-radio 0.15.x), and DNS hijack for the captive portal (no nostd DNS server crate). The v2.1 scope is correctly limited to trait definitions, gap crate skeletons, and beginning the NVS gap implementation. A complete port is future-milestone work.

## Key Findings

### Recommended Stack

The server binary is a standard Rust std binary for the host, added as a Cargo workspace member alongside the existing firmware. The workspace uses `resolver = "2"` to prevent hidden `std` feature unification from breaking the no_std gap crates. The firmware `.cargo/config.toml` stays in `firmware/` (not the workspace root) so the ESP32-C6 RISC-V target applies only to the firmware member. The `rtcm-rs` crate is used directly in the server; it is already `no_std` compatible and covers all RTCM 3.4 MSM messages needed. For the embassy/nostd audit, esp-hal 1.0.0 (stable, October 2025) supports ESP32-C6 but requires Rust 1.88.0 minimum — a toolchain bump from the current 1.77 target. The audit and gap crate work runs against esp-hal in separate `crates/` members, not in the firmware binary.

**Core technologies:**
- tokio 1.x: async runtime — single runtime for all server tasks, required by both axum and rumqttc, no runtime conflicts
- axum 0.8: HTTP + WebSocket server — tokio-native, built-in WebSocket via `axum::extract::ws`; avoids actix runtime conflict
- rumqttc 0.24 (AsyncClient): MQTT subscriber — from bytebeamio (same org as rumqttd broker); cloneable; MQTT 3.1.1 matches firmware
- rtcm-rs 0.11: RTCM3 MSM decode — all RTCM 3.4 MSM4/MSM7 messages confirmed; `no_std`; `#[forbid(unsafe_code)]`
- rinex 0.21: RINEX observation file writer — OBS writer stable; NAV writer under construction (defer or use DIY)
- sequential-storage 0.8 + embedded-storage 0.3: nostd NVS gap — wear-levelled key-value on raw flash; closest equivalent to ESP-IDF NVS
- esp-hal 1.0.0 + esp-radio 0.15.x: nostd audit target — stable GPIO/UART; WiFi STA functional; SoftAP password-protection missing

**What not to add:**
- paho-mqtt (C FFI, native lib dep) — use rumqttc
- actix-web (separate runtime) — use axum
- Custom RTCM parser on server — rtcm-rs covers all needed message types
- embassy-executor in the firmware binary — breaks FreeRTOS; audit only, gap crates only

### Expected Features

**Must have (table stakes — P1 for v2.1 launch):**
- RTCM3 MSM4/MSM7 decode (GPS + GLONASS): pseudorange, carrier phase, CNR per satellite
- Epoch formation from MSM time-of-week (buffer multi-constellation frames within ~10ms window)
- RINEX 2.11 observation file (.26O) for GPS + GLONASS with hourly rotation and correct RINEX naming
- RTCM3 1019 + 1020 ephemeris decode to RINEX mixed nav file (.26P)
- Multi-constellation NMEA GSV parsing (GPGSV, GLGSV, GAGSV, BDGSV)
- Skyplot polar SVG from GSV elevation/azimuth data; SNR bar chart from GSV C/N0
- Device health panel from heartbeat MQTT topic (fix_type, satellites, HDOP, heap_free)
- NVS trait definition with ESP-IDF NVS concrete implementation
- WebSocket endpoint at 1 Hz pushing satellite state JSON to browser

**Should have (P2 — after launch validation):**
- Galileo MSM decode (1094/1097) and 'E' observations in RINEX (de-facto extension, accepted by RTKLIB)
- BeiDou MSM decode (1124/1127) and 'C' observations (requires toolchain compatibility check)
- Galileo/BeiDou ephemeris decode (1045, 1042) added to mixed nav file
- Satellite trail history on skyplot
- Signal quality colour coding on skyplot dots (green/amber/red by C/N0 threshold)
- ekv-backed NVS gap crate implementation (nostd path)

**Defer (v2.2+ or anti-features):**
- RINEX 3.x output — 3-char observation codes; significantly higher complexity; existing toolchains accept RINEX 2.11 with extensions
- Doppler (D1/D2) in RINEX observation file — MSM7 provides it but PPK tools rarely use it
- Full-rate WebSocket at 5 Hz — browser DOM becomes the bottleneck; 1 Hz is correct
- Storing raw RTCM frames as a ring buffer — unbounded memory; RINEX files are the persistence layer
- Multi-device MQTT aggregation

### Architecture Approach

The system is organized as a single Cargo workspace with three layers: the existing ESP32-C6 firmware in `firmware/` (unchanged from v2.0), the new Tokio server in `server/`, and shared no_std library crates in `crates/`. The server runs six root async tasks: rumqttc event loop, rtcm_decoder, nmea_parser, rinex_writer, ws_aggregator (1 Hz interval), and axum HTTP+WebSocket server. State is shared via `Arc<RwLock<SatelliteState>>` (written by decoder/parser tasks, read by the WebSocket aggregator) and a `tokio::sync::broadcast::Sender<SkyplotUpdate>` for fan-out to browser clients. Each WebSocket connection runs in its own spawned task with a send timeout to prevent slow clients from stalling the server. No framing state machine is needed in the server — the firmware already publishes complete pre-framed RTCM3 bytes as MQTT payloads.

**Major components:**
1. `firmware/` — ESP32-C6 firmware (unchanged from v2.0); publishes RTCM3 frames, NMEA sentences, heartbeat to MQTT
2. `server/src/rtcm_decoder.rs` — receives complete RTCM3 bytes from MQTT; calls rtcm-rs; extracts MSM observations and ephemeris; updates satellite_state
3. `server/src/nmea_parser.rs` — parses GSV sentences; updates elevation/azimuth/C/N0 per satellite in satellite_state
4. `server/src/rinex_writer.rs` — receives decoded observations; writes RINEX 2.11 epoch records; hourly file rotation
5. `server/src/web_server.rs` — axum HTTP + WebSocket; static HTML/JS embedded via `include_str!`; broadcast fan-out at 1 Hz
6. `crates/gnss-hal-traits/` + `crates/gnss-nvs/` — no_std trait definitions and NVS gap crate skeleton

### Critical Pitfalls

1. **MSM cell mask bit count is dynamic, not fixed** — cell mask width = popcount(sat_mask) x popcount(sig_mask) bits; cells with data = popcount(cell_mask). Wrong count misaligns the bit cursor and corrupts every observation that follows. Use rtcm-rs rather than a custom parser; the crate handles this internally.

2. **GLONASS carrier phase is silently zero without FCN** — MSM4 (1084) does not carry the frequency channel number; carrier phase is zero unless 1020 ephemeris messages supply FCN. Represent missing carrier phase as `Option::None`; write blank (16 spaces) in RINEX, never 0.0.

3. **RINEX 2.11 header labels must be in exactly columns 61-80** — a label at column 60 or 62 causes silent rejection by RTKLIB, teqc, rnx2rtkp. Write a single `header_line(content, label)` helper function and assert `line.len() == 80` in unit tests.

4. **Cargo workspace feature unification silently enables `std` in no_std crates** — set `resolver = "2"` in workspace `Cargo.toml` on first commit; verify in CI with `cargo build -p gap-crate --target riscv32imac-unknown-none-elf`.

5. **RINEX missing observations must be blank, not 0.0** — represent observations as `Option<f64>`; format `None` as 16 spaces; 0.0 is treated as a valid measurement by PPK tools and produces wrong RTK solutions without any error.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Cargo Workspace Restructure
**Rationale:** Everything downstream depends on the workspace existing with the correct layout. The per-member `.cargo/config.toml` placement and `resolver = "2"` must be established first or all subsequent builds are unreliable.
**Delivers:** Working workspace where `cargo build -p esp32-gnssmqtt-firmware` and `cargo build -p gnss-server` both succeed; CI validates no_std gap crate compilation for embedded target.
**Addresses:** Pitfall 7 (workspace feature unification); Pitfall 8 (panic handler conflict); Pitfall 9 (embassy executor in firmware binary).
**Avoids:** All three pitfalls are structural and impossible to fix cheaply once other code is written against a broken workspace layout.

### Phase 2: MQTT Subscriber + RTCM3 MSM Decode
**Rationale:** RTCM3 decode is the data source for RINEX files and satellite state. It must be validated independently before RINEX writing begins — bugs here produce silently wrong output that is hard to isolate once the RINEX layer is added. Validation against pyrtcm or RTKLIB convbin is the acceptance criterion for this phase.
**Delivers:** Server connects to MQTT broker, receives RTCM3 frames, and decodes MSM4/MSM7 observations to pseudorange/carrier-phase/CNR structs with verified correctness.
**Uses:** rumqttc 0.24, rtcm-rs 0.11.
**Implements:** rtcm_decoder task; satellite_state (Arc<RwLock>) and EphemerisStore (HashMap per constellation).
**Avoids:** Pitfall 1 (MSM cell mask), Pitfall 2 (GLONASS FCN), Pitfall 3 (pseudorange reconstruction).

### Phase 3: NMEA GSV Parsing + Satellite State
**Rationale:** Can proceed in parallel with Phase 2 conceptually but requires the MQTT subscriber from Phase 2. The shared `Arc<RwLock<SatelliteState>>` is the integration point; both the RTCM decoder and the NMEA parser write to it. Establishing this shared state structure before the WebSocket server removes rework.
**Delivers:** satellite_state populated with elevation, azimuth, CNR per satellite per constellation from parsed GSV sentences; heartbeat data integrated into device health state.
**Addresses:** Multi-constellation GSV aggregation (P1), device health panel (P1).

### Phase 4: RINEX 2.11 Observation File Writer
**Rationale:** Depends on validated MSM decode from Phase 2. The observation writer (rinex crate OBS) is stable; the navigation writer is under construction and may need a DIY fallback. Building RINEX before the WebSocket UI means bugs can be diagnosed against reference RINEX parsers (RTKLIB convbin, teqc) without UI involvement.
**Delivers:** Hourly-rotating RINEX 2.11 observation files (.26O) for GPS + GLONASS validated against RTKLIB; RTCM3 1019+1020 ephemeris decoded to mixed nav file (.26P) if rinex NAV writer is usable, else deferred.
**Uses:** rinex 0.21 (OBS writer), chrono 0.4 (UTC time and filenames).
**Implements:** rinex_writer task; hourly rotation logic; RINEX filename generator.
**Avoids:** Pitfall 4 (header column alignment), Pitfall 5 (continuation lines for >5 obs types), Pitfall 6 (filename session codes), Pitfall 11 (0.0 vs blank for missing observations).

### Phase 5: HTTP + WebSocket Server + Browser UI
**Rationale:** The WebSocket server pushes data from satellite_state which is fully populated by Phases 2 and 3. Building the UI last means it is wired to real, validated data from the first commit. The skyplot and SNR chart are SVG+DOM operations that are straightforward once the data pipeline is correct.
**Delivers:** Browser skyplot polar SVG, SNR bar chart, device health panel; all updated at 1 Hz via WebSocket; static HTML/JS embedded in server binary.
**Uses:** axum 0.8 (HTTP + WebSocket), tokio::sync::broadcast, serde_json.
**Implements:** web_server task; per-client WebSocket handler with send timeout; SkyplotUpdate JSON struct.
**Avoids:** Pitfall 10 (per-client tasks with send timeouts, not sequential blocking awaits).

### Phase 6: Nostd Audit + Gap Crates
**Rationale:** The audit is read-only with respect to the firmware and can run in parallel with Phases 3-5, but its output (gap crate skeletons) is most accurate when it captures the full v2.0 firmware surface. Running it last ensures the audit reflects the final shipped state. Gap crates are scaffolding for a future port milestone, not blocking for any v2.1 server feature.
**Delivers:** Full enumeration of esp-idf-svc/hal/sys usages by category; gap table mapping each to esp-hal equivalent or "no equivalent"; `gnss-hal-traits` no_std trait definitions; `gnss-nvs` gap crate with partial implementation using sequential-storage.
**Uses:** sequential-storage 0.8, embedded-storage 0.3; esp-hal 1.0.0 as reference (not in firmware binary).
**Implements:** NvsStorage trait; NvsNamespace trait; EspNvsStore concrete implementation; sequential-storage backing implementation skeleton.
**Avoids:** Pitfall 8 (panic handler conflict — gate test handlers behind `#[cfg(test)]`), Pitfall 9 (embassy executor never added to firmware Cargo.toml).

### Phase Ordering Rationale

- Workspace restructure (Phase 1) is a prerequisite for all other phases; it takes one day and eliminates an entire class of subtle build failures.
- RTCM3 decode (Phase 2) is the critical dependency for both RINEX writing and the WebSocket UI; the architecture research explicitly recommends decode-first, validate, then write as the correct sequence.
- NMEA parsing (Phase 3) populates the same shared satellite state as RTCM decode and is naturally paired with it; decoupled enough to build incrementally.
- RINEX writing (Phase 4) comes before the UI (Phase 5) so the data pipeline can be validated via reference tools before the browser layer adds visual debugging complexity.
- Nostd audit (Phase 6) is last because it is future-port scaffolding and does not block any user-facing v2.1 deliverable.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4 (RINEX writer):** At phase start, run a quick integration test to determine whether the rinex 0.21 crate produces RINEX 2.x or 3.x output by default, and whether the NAV writer is usable. If the OBS writer produces 3.x or the NAV writer is unusable, a DIY fixed-width writer is required (~200-300 lines for OBS; format fully specified in FEATURES.md). This is the highest-uncertainty decision in the milestone.
- **Phase 6 (Nostd audit):** The esp-hal ecosystem moved fast in 2025; re-check esp-radio SoftAP password-protection status and embedded-tls TLS 1.2 support before declaring gaps final. The audit document should note the date and which gaps may have closed.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Workspace):** Cargo workspace layout with per-member `.cargo/config.toml` is confirmed from Cargo documentation and community practice.
- **Phase 2 (RTCM3 decode):** rtcm-rs API is confirmed and stable; decode pitfalls are documented with concrete prevention strategies.
- **Phase 5 (WebSocket UI):** axum broadcast pattern is canonical and well-documented; skyplot SVG coordinate math is fully specified in FEATURES.md.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All server crates verified against GitHub sources; version compatibility confirmed; tokio runtime alignment confirmed; rumqttc/axum runtime conflict analysis is definitive |
| Features | HIGH | RINEX 2.11 spec fetched from IGS official source; MSM message numbers confirmed from multiple consistent sources; NMEA GSV field reference fetched directly |
| Architecture | HIGH | Firmware architecture is shipped and validated (v2.0); server task architecture is established tokio/axum pattern with axum discussion confirming the broadcast approach |
| Pitfalls | HIGH | Critical decode pitfalls verified against RTKLIB source and rtklibexplorer post-mortems; RINEX format pitfalls verified against IGS spec; Cargo feature unification verified against official RFC and Cargo Book |

**Overall confidence:** HIGH

### Gaps to Address

- **rinex crate OBS output format (2.x vs 3.x):** Unverifiable without running the code. Must be tested at Phase 4 start; if the crate produces 3.x format or is otherwise insufficient, fall back to DIY writer. Format is fully specified in FEATURES.md so the fallback is low-risk.
- **rinex crate NAV writer maturity:** Marked under construction. Evaluate at Phase 4; defer navigation file writing to a follow-on if not usable.
- **esp-ota-nostd viability on ESP32-C6:** LOW confidence; do not commit to this for the nostd OTA gap until Phase 6 evaluation confirms it links without esp-idf-sys.
- **sequential-storage + esp-hal flash driver combination on ESP32-C6:** MEDIUM confidence; the crates are real and functional but the specific combination is unverified without building. Phase 6 should include a minimal build test.
- **AUSCORS TLS version support:** embedded-tls supports TLS 1.3 only. Verify AUSCORS NTRIP caster supports TLS 1.3 before committing to embedded-tls as the nostd NTRIP TLS replacement.

## Sources

### Primary (HIGH confidence)
- RTKLIB `src/rtcm3.c` — canonical MSM cell mask calculation and pseudorange reconstruction
- IGS RINEX 2.11 specification (files.igs.org) — header column positions, observation record format, continuation line rules
- Cargo RFC 2957 + Cargo Book "Features" — resolver v2 feature unification semantics
- GitHub: esp-rs/esp-hal — 1.0.0 stable October 2025; ESP32-C6 supported; NVS/OTA/SoftAP not in stable scope confirmed
- GitHub: martinhakansson/rtcm-rs — v0.11.0 confirmed; all RTCM 3.4 MSM4/MSM7 messages confirmed
- Tokio blog: Announcing axum 0.8.0 (January 2025) — WebSocket via `extract::ws` confirmed
- GitHub: tokio-rs/axum releases — v0.8.8 latest stable
- docs.rs: esp-wifi 0.15.x — SoftAP non-open listed as missing; renamed to esp-radio
- lib.rs: minimq 0.10.0 — MQTT5 only; broker must support MQTT5 to use minimq in nostd firmware
- rtklibexplorer: "Limitations of the RTCM raw measurement format" — MSM carrier phase and timestamp limitations
- rtklibexplorer: "Converting GLONASS RTCM MSM messages to RINEX with RTKLIB" — FCN requirement for GLONASS carrier phase confirmed
- RTKLIB GitHub issue #129 — zero GLONASS carrier phase from MSM4 without 1020 confirmed
- Cargo RFC 2957 + cargo-features issue #5730 — feature unification failure mode documented

### Secondary (MEDIUM confidence)
- GitHub: nav-solutions/rinex v0.21.1 — OBS writer available; NAV writer under construction; 2.x vs 3.x output format unverified without running the code
- GitHub: bytebeamio/rumqtt — rumqttc 0.24 AsyncClient confirmed; 0.25+ not yet validated
- Rust Users Forum: Cargo workspace per-member targets — directory-local `.cargo/config.toml` confirmed as stable solution on stable Rust
- tokio-rs/axum discussion #1335 — broadcast::Sender pattern for WebSocket fan-out confirmed
- GitHub: embassy-rs/ekv — provides key-value on raw NOR flash; no_std status confirmed from docs
- tokio-tungstenite issue #195 — per-client memory growth without send timeouts documented
- esp-rs/esp-hal discussion #738 — `__pender` linker error when mixing embassy-executor with non-esp-hal runtimes
- Espressif Developer Portal (Feb 2025) — esp-hal 1.0 beta stabilized scope confirmed; std crates community-maintained

### Tertiary (LOW confidence)
- lib.rs: esp-ota-nostd — exists; from-scratch bootloader-compatible OTA; no verified ESP32-C6 integration without ESP-IDF

---
*Research completed: 2026-03-12*
*Ready for roadmap: yes*
