# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- ✅ **v1.1 GNSS Relay** — Phases 4-6 (shipped 2026-03-07)
- ✅ **v1.2 Observations + OTA** — Phases 7-8 (shipped 2026-03-07)
- ✅ **v1.3 Reliability Hardening** — Phases 9-13 (shipped 2026-03-08)
- ✅ **v2.0 Field Deployment** — Phases 14-21 (shipped 2026-03-12)
- 🚧 **v2.1 Server and nostd Foundation** — Phases 22-25 (in progress)

## Phases

<details>
<summary>✅ v1.0 Foundation (Phases 1-3) — SHIPPED 2026-03-04</summary>

- [x] Phase 1: Scaffold (2/2 plans) — completed 2026-03-03
- [x] Phase 2: Connectivity (4/4 plans) — completed 2026-03-04
- [x] Phase 3: Status LED (3/3 plans) — completed 2026-03-04

Archive: `.planning/milestones/v1.0-ROADMAP.md`

</details>

<details>
<summary>✅ v1.1 GNSS Relay (Phases 4-6) — SHIPPED 2026-03-07</summary>

- [x] Phase 4: UART Pipeline (2/2 plans) — completed 2026-03-06
- [x] Phase 5: NMEA Relay (2/2 plans) — completed 2026-03-07
- [x] Phase 6: Remote Config (2/2 plans) — completed 2026-03-07

Archive: `.planning/milestones/v1.1-ROADMAP.md`

</details>

<details>
<summary>✅ v1.2 Observations + OTA (Phases 7-8) — SHIPPED 2026-03-07</summary>

- [x] Phase 7: RTCM Relay (3/3 plans) — completed 2026-03-07
- [x] Phase 8: OTA (3/3 plans) — completed 2026-03-07

Archive: `.planning/milestones/v1.3-ROADMAP.md`

</details>

<details>
<summary>✅ v1.3 Reliability Hardening (Phases 9-13) — SHIPPED 2026-03-08</summary>

- [x] Phase 9: Channel + Loop Hardening (2/2 plans) — completed 2026-03-07
- [x] Phase 10: Memory + Diagnostics (2/2 plans) — completed 2026-03-07
- [x] Phase 11: Thread Watchdog (2/2 plans) — completed 2026-03-07
- [x] Phase 12: Resilience (2/2 plans) — completed 2026-03-07
- [x] Phase 13: Health Telemetry (1/1 plan) — completed 2026-03-08

Archive: `.planning/milestones/v1.3-ROADMAP.md`

</details>

<details>
<summary>✅ v2.0 Field Deployment (Phases 14-21) — SHIPPED 2026-03-12</summary>

- [x] Phase 14: Quick Additions (2/2 plans) — completed 2026-03-07
- [x] Phase 15: Provisioning (3/3 plans) — completed 2026-03-08
- [x] Phase 16: Remote Logging (2/2 plans) — completed 2026-03-08
- [x] Phase 17: NTRIP Client (4/4 plans) — completed 2026-03-09
- [x] Phase 18: Telemetry and OTA Validation (3/3 plans) — completed 2026-03-09
- [x] Phase 19: Pre-2.0 Bugfix (3/3 plans) — completed 2026-03-10
- [x] Phase 20: Field Testing Fixes (4/4 plans) — completed 2026-03-11
- [x] Phase 21: MQTT Performance (3/3 plans) — completed 2026-03-12

Archive: `.planning/milestones/v2.0-ROADMAP.md`

</details>

### 🚧 v2.1 Server and nostd Foundation (In Progress)

**Milestone Goal:** Build a companion Rust server (RTCM3 → RINEX files + live web UI) and scaffold the embassy/nostd crate ecosystem needed to eventually port the firmware off ESP-IDF.

#### Phase Summary

- [ ] **Phase 22: Workspace + Nostd Audit** - Establish Cargo workspace layout AND produce the complete ESP-IDF dependency audit mapped to embassy/nostd equivalents or flagged as gaps
- [ ] **Phase 23: MQTT + RTCM3 + gnss-nvs crate** - Server subscribes to MQTT, decodes all RTCM3 MSM and ephemeris messages into verified observation structs, AND implements the gnss-nvs crate with NvsStore trait, ESP-IDF impl, and sequential-storage impl
- [ ] **Phase 24: RINEX Files + gnss-ota gap crate** - Server writes hourly-rotating RINEX 2.11 observation and navigation files accepted by RTKLIB, AND implements the gnss-ota gap crate with dual-slot OTA trait and documented nostd blocker
- [ ] **Phase 25: Web UI + remaining gap crate skeletons** - HTTP + WebSocket server pushes live satellite skyplot, SNR bar chart, and device health panel to browser, AND implements gnss-softap, gnss-dns, and gnss-log gap crate skeletons with trait definitions and BLOCKER.md

## Phase Details

### Phase 22: Workspace + Nostd Audit
**Goal**: Developer can build firmware and server from a single Cargo workspace without target conflicts, and every ESP-IDF dependency usage is mapped to an embassy/nostd equivalent or explicitly flagged as a gap
**Depends on**: Nothing (first v2.1 phase)
**Requirements**: INFRA-01, NOSTD-01
**Success Criteria** (what must be TRUE):
  1. `cargo build -p esp32-gnssmqtt-firmware` succeeds for the ESP32-C6 RISC-V target from the workspace root; `cargo build -p gnss-server` succeeds for the host target from the workspace root
  2. Building the server does not pull `std` features into the no_std gap crate members (resolver="2" verified by inspecting the dependency graph)
  3. The firmware `.cargo/config.toml` applies only to the `firmware/` member — server and gap crate builds are unaffected by embedded-target overrides
  4. Audit document enumerates every `esp-idf-svc`, `esp-idf-hal`, and `esp-idf-sys` usage by category (WiFi, NVS, OTA, UART, TLS, log hook, SoftAP, DNS), with each usage mapped to an esp-hal/embassy equivalent or marked as a gap with the specific blocker recorded
  5. Gap list is prioritised — NVS, OTA, SoftAP, DNS hijack, and log hook explicitly ranked for Phase 23-25 implementation order; document committed to the repo
**Plans**: TBD

### Phase 23: MQTT + RTCM3 + gnss-nvs crate
**Goal**: Server connects to MQTT and decodes all RTCM3 MSM and ephemeris messages into verified observation structs; gnss-nvs crate provides a working NvsStore trait with ESP-IDF and sequential-storage implementations
**Depends on**: Phase 22
**Requirements**: SRVR-01, SRVR-02, RTCM-01, RTCM-02, RTCM-03, RTCM-04, NOSTD-02, NOSTD-03
**Success Criteria** (what must be TRUE):
  1. Server connects to MQTT broker and subscribes to `gnss/{id}/rtcm`, `gnss/{id}/nmea`, and `gnss/{id}/heartbeat` for a configured device ID; reconnects automatically after broker disconnect with exponential backoff (observable via server logs)
  2. Server decodes MSM4/MSM7 pseudorange, carrier phase, and C/N0 for GPS and GLONASS; missing GLONASS carrier phase (no FCN) is represented as None, never 0.0
  3. Server decodes MSM messages for Galileo and BeiDou (best-effort) and ephemeris messages 1019, 1020, 1046, 1044; decoded observations from a single epoch (~10ms window) are grouped before emission with epoch boundary visible in server log output
  4. `gnss-nvs` crate exists with a `NvsStore` trait (namespaced, typed getters/setters, blob support) and a working ESP-IDF NVS concrete implementation that compiles for the ESP32-C6 target
  5. `gnss-nvs` contains a started `sequential-storage`-backed `NvsStore` implementation (compiles; hardware validation deferred to a future milestone)
**Plans**: TBD

### Phase 24: RINEX Files + gnss-ota gap crate
**Goal**: Server writes RINEX 2.11 observation and navigation files that RTKLIB accepts without error; gnss-ota gap crate defines the dual-slot OTA trait with a documented nostd blocker
**Depends on**: Phase 23
**Requirements**: RINEX-01, RINEX-02, RINEX-03, RINEX-04, NOSTD-04a
**Success Criteria** (what must be TRUE):
  1. Server produces `.26O` observation files with correct RINEX 2.11 column-positioned format; files rotate to a new file at each UTC hour boundary
  2. Observation file headers contain all mandatory records (VERSION/TYPE, TYPES OF OBSERV, WAVELENGTH FACT, TIME OF FIRST OBS, APPROX POSITION XYZ, END OF HEADER) with labels in columns 61-80
  3. Server produces `.26P` mixed navigation files from decoded ephemeris messages with hourly rotation
  4. `rnx2rtkp` or `rtkplot` processes the output files without parse errors (validated manually against a real RTCM3 stream)
  5. `gnss-ota` crate exists with a dual-slot OTA trait definition and a `BLOCKER.md` documenting specifically what prevents a nostd implementation today
**Plans**: TBD

### Phase 25: Web UI + remaining gap crate skeletons
**Goal**: Browser shows a live satellite skyplot, SNR bar chart, and device health panel updated from the running server; gnss-softap, gnss-dns, and gnss-log gap crate skeletons exist with trait definitions and documented blockers
**Depends on**: Phase 23
**Requirements**: UI-01, UI-02, UI-03, UI-04, NOSTD-04b
**Success Criteria** (what must be TRUE):
  1. HTTP GET to the server root returns an HTML page; a WebSocket connection from that page receives satellite state JSON at approximately 1 Hz
  2. Browser renders a polar SVG skyplot showing each satellite's PRN label at its elevation/azimuth position, updated from NMEA GSV sentences
  3. Browser renders an SNR/C/N0 bar chart with one bar per tracked satellite, coloured or labelled by constellation
  4. Browser shows a device health panel with uptime, fix type, satellite count, HDOP, and heap free from the MQTT heartbeat topic; panel updates within 35 seconds of a heartbeat change
  5. `gnss-softap`, `gnss-dns`, and `gnss-log` crates each exist with a trait definition file and a `BLOCKER.md` documenting specifically what prevents a nostd implementation today
**Plans**: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffold | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | v1.0 | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | v1.0 | 3/3 | Complete | 2026-03-04 |
| 4. UART Pipeline | v1.1 | 2/2 | Complete | 2026-03-06 |
| 5. NMEA Relay | v1.1 | 2/2 | Complete | 2026-03-07 |
| 6. Remote Config | v1.1 | 2/2 | Complete | 2026-03-07 |
| 7. RTCM Relay | v1.2 | 3/3 | Complete | 2026-03-07 |
| 8. OTA | v1.2 | 3/3 | Complete | 2026-03-07 |
| 9. Channel + Loop Hardening | v1.3 | 2/2 | Complete | 2026-03-07 |
| 10. Memory + Diagnostics | v1.3 | 2/2 | Complete | 2026-03-07 |
| 11. Thread Watchdog | v1.3 | 2/2 | Complete | 2026-03-07 |
| 12. Resilience | v1.3 | 2/2 | Complete | 2026-03-07 |
| 13. Health Telemetry | v1.3 | 1/1 | Complete | 2026-03-08 |
| 14. Quick Additions | v2.0 | 2/2 | Complete | 2026-03-07 |
| 15. Provisioning | v2.0 | 3/3 | Complete | 2026-03-08 |
| 16. Remote Logging | v2.0 | 2/2 | Complete | 2026-03-08 |
| 17. NTRIP Client | v2.0 | 4/4 | Complete | 2026-03-09 |
| 18. Telemetry and OTA Validation | v2.0 | 3/3 | Complete | 2026-03-09 |
| 19. Pre-2.0 Bugfix | v2.0 | 3/3 | Complete | 2026-03-10 |
| 20. Field Testing Fixes | v2.0 | 4/4 | Complete | 2026-03-11 |
| 21. MQTT Performance | v2.0 | 3/3 | Complete | 2026-03-12 |
| 22. Workspace + Nostd Audit | v2.1 | 0/TBD | Not started | - |
| 23. MQTT + RTCM3 + gnss-nvs crate | v2.1 | 0/TBD | Not started | - |
| 24. RINEX Files + gnss-ota gap crate | v2.1 | 0/TBD | Not started | - |
| 25. Web UI + remaining gap crate skeletons | v2.1 | 0/TBD | Not started | - |
