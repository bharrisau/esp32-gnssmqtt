# Requirements: esp32-gnssmqtt

**Defined:** 2026-03-12
**Core Value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.

## v2.1 Requirements

Requirements for the Server and nostd Foundation milestone. Each maps to roadmap phases.

### Infrastructure

- [x] **INFRA-01**: Developer can build firmware and server binary from the same Cargo workspace without target conflicts (`resolver = "2"`, firmware/ + server/ + crates/ layout)

### Server

- [x] **SRVR-01**: Server binary subscribes to MQTT `gnss/{id}/rtcm`, `gnss/{id}/nmea`, and `gnss/{id}/heartbeat` for a configured device ID
- [x] **SRVR-02**: Server reconnects to MQTT broker after disconnect with exponential backoff

### RTCM3 Decode

- [x] **RTCM-01**: Server decodes RTCM3 MSM4/MSM7 messages for GPS and GLONASS (pseudorange, carrier phase, C/N0)
- [x] **RTCM-02**: Server decodes RTCM3 MSM messages for Galileo and BeiDou (best-effort; RINEX 2.11 extension)
- [x] **RTCM-03**: Server decodes RTCM3 ephemeris messages 1019 (GPS), 1020 (GLONASS), 1046 (Galileo), 1044 (BeiDou)
- [x] **RTCM-04**: Server buffers MSM frames within a ~10ms epoch window before emitting an observation epoch

### RINEX Files

- [x] **RINEX-01**: Server writes RINEX 2.11 observation files (`.26O`) with hourly rotation and correct column-positioned format
- [x] **RINEX-02**: Observation file includes all mandatory headers (VERSION/TYPE, TYPES OF OBSERV, WAVELENGTH FACT, TIME OF FIRST OBS, END OF HEADER) plus APPROX POSITION XYZ
- [x] **RINEX-03**: Server writes RINEX 2.11 mixed navigation files (`.26P`) from decoded ephemeris messages with hourly rotation
- [x] **RINEX-04**: RINEX output is accepted by RTKLIB (validated with `rnx2rtkp` or `rtkplot`)

### Web UI

- [x] **UI-01**: HTTP server serves a static HTML page; WebSocket pushes satellite state at ~1 Hz
- [x] **UI-02**: Browser renders polar skyplot SVG showing satellite elevation, azimuth, and PRN from NMEA GSV
- [x] **UI-03**: Browser renders SNR/C/N0 bar chart per satellite from NMEA GSV
- [x] **UI-04**: Browser shows device health panel (uptime, fix type, satellites, HDOP, heap free) from MQTT heartbeat

### nostd / Embassy Gap Work

- [x] **NOSTD-01**: Complete audit of all `esp-idf-svc`, `esp-idf-hal`, and `esp-idf-sys` usages in the firmware mapped to embassy/esp-hal equivalents or flagged as gaps
- [x] **NOSTD-02**: `gnss-nvs` crate created with a `NvsStore` trait (namespaced, typed getters/setters, blob support) and ESP-IDF NVS backing implementation
- [x] **NOSTD-03**: `sequential-storage` backed `NvsStore` implementation started (nostd flash backing for embassy port)
- [x] **NOSTD-04a**: `gnss-ota` gap crate — dual-slot OTA trait definition and `BLOCKER.md` documenting the specific nostd blocker preventing implementation today
- [x] **NOSTD-04b**: `gnss-softap` + `gnss-dns` + `gnss-log` gap crate skeletons — trait definitions and `BLOCKER.md` for each documenting specific nostd blockers

## Future Requirements

### RINEX

- **RINEX-F01**: RINEX output validated against teqc and rnx2rtkp for edge cases (missing constellations, session boundary epochs)
- **RINEX-F02**: RINEX 3.x output as an alternative format option

### nostd / Embassy

- **NOSTD-F01**: Full embassy port of firmware using gap crates — replaces esp-idf-svc/hal/sys entirely
- **NOSTD-F02**: `sequential-storage` NVS implementation hardware-validated on device FFFEB5
- **NOSTD-F03**: nostd NTRIP TLS client using embedded-tls

### Server

- **SRVR-F01**: Multi-device support (subscribe to multiple device IDs simultaneously)
- **SRVR-F02**: RINEX file upload to remote FTP/SFTP endpoint after hourly rotation

## Out of Scope

| Feature | Reason |
|---------|--------|
| Full embassy firmware port | Blocked by SoftAP password, DNS hijack, and log hook gaps — future milestone after gap crates mature |
| RINEX 3.x format | RINEX 2.11 sufficient for RTKLIB/PPP workflows; 3.x adds complexity without clear benefit for v2.1 |
| BLE provisioning | SoftAP covers WiFi+MQTT+NTRIP without custom app; BLE deferred |
| Multi-broker publishing | Single broker only |
| TLS for server MQTT connection | Defer — broker is local/trusted network |

## Traceability

Which phases cover which requirements. Updated during roadmap revision 2026-03-12.

| Requirement | Phase | Status |
|-------------|-------|--------|
| INFRA-01 | Phase 22 | Complete |
| NOSTD-01 | Phase 22 | Complete |
| SRVR-01 | Phase 23 | Complete |
| SRVR-02 | Phase 23 | Complete |
| RTCM-01 | Phase 23 | Complete |
| RTCM-02 | Phase 23 | Complete |
| RTCM-03 | Phase 23 | Complete |
| RTCM-04 | Phase 23 | Complete |
| NOSTD-02 | Phase 23 | Complete |
| NOSTD-03 | Phase 23 | Complete |
| RINEX-01 | Phase 24 | Complete |
| RINEX-02 | Phase 24 | Complete |
| RINEX-03 | Phase 24 | Complete |
| RINEX-04 | Phase 24 | Complete |
| NOSTD-04a | Phase 24 | Complete |
| UI-01 | Phase 25 | Complete |
| UI-02 | Phase 25 | Complete |
| UI-03 | Phase 25 | Complete |
| UI-04 | Phase 25 | Complete |
| NOSTD-04b | Phase 25 | Complete |

**Coverage:**
- v2.1 requirements: 20 total (NOSTD-04 split into NOSTD-04a + NOSTD-04b)
- Mapped to phases: 20
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-12*
*Last updated: 2026-03-12 — roadmap revised to 4-phase interleaved structure (22-25); NOSTD-04 split into NOSTD-04a (Phase 24) and NOSTD-04b (Phase 25); gnss-nvs crate work (NOSTD-02, NOSTD-03) moved to Phase 23 alongside MQTT/RTCM3*
