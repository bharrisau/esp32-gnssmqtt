# Requirements: esp32-gnssmqtt

**Defined:** 2026-03-03
**Core Value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.

## v1 Requirements

Milestone 1: Foundation — scaffold, WiFi/MQTT connectivity with hardcoded credentials, status LED.

### Scaffold

- [ ] **SCAF-01**: Project compiles for ESP32-C6 target (`riscv32imc-esp-espidf`) via `cargo build` and flashes via `espflash`
- [ ] **SCAF-02**: `esp-idf-hal`, `esp-idf-svc`, and `esp-idf-sys` crate versions are pinned with `=` specifiers in `Cargo.toml` from a known-good `esp-idf-template` scaffold
- [ ] **SCAF-03**: `sdkconfig.defaults` sets UART RX ring buffer to 4096+ bytes and enables FreeRTOS stack overflow detection
- [ ] **SCAF-04**: `partitions.csv` defines a NVS partition of at least 64KB
- [ ] **SCAF-05**: Device ID module reads hardware eFuse/MAC at runtime and returns a stable unique string

### Connectivity

- [ ] **CONN-01**: Device connects to WiFi using compile-time hardcoded SSID and password
- [ ] **CONN-02**: Device connects to MQTT broker using compile-time hardcoded host, port, username, and password
- [ ] **CONN-03**: Device automatically reconnects to WiFi after a connection drop, with exponential backoff
- [ ] **CONN-04**: Device automatically reconnects to MQTT broker after a connection drop; re-subscribes to all topics inside the `Connected` event handler
- [ ] **CONN-05**: Device publishes a periodic heartbeat message to `gnss/{device_id}/heartbeat` with the MQTT retain flag set
- [ ] **CONN-06**: Device registers an MQTT Last Will and Testament message to `gnss/{device_id}/status` with payload `offline` and retain flag set at connect time
- [ ] **CONN-07**: Device bridges USB debug serial (UART0 / USB CDC) to the UM980 UART — lines received from USB are forwarded to the UM980, and UM980 replies are echoed back over USB

### Status LED

- [ ] **LED-01**: LED shows a distinct blink pattern while the device is attempting to connect to WiFi or MQTT
- [ ] **LED-02**: LED shows a steady-on (or slow blink) pattern when WiFi and MQTT are both connected
- [ ] **LED-03**: LED shows an error pattern (e.g. rapid blink or off) when connectivity cannot be established after repeated retries

## v2 Requirements

Milestone 2: GNSS relay — UART pipeline, NMEA-to-MQTT publishing, and remote UM980 configuration.

### UART Pipeline

- **UART-01**: Device reads raw bytes from UM980 UART at 115200 baud 8N1 using a dedicated high-priority FreeRTOS task
- **UART-02**: Device accumulates bytes into complete NMEA sentences terminated by `\n`, correctly handling fragmented reads across multiple UART read calls
- **UART-03**: Device extracts the sentence type from the NMEA prefix (e.g. `$GNGLL` → `GNGLL`) for use in MQTT topic construction
- **UART-04**: Device validates NMEA checksum (XOR of bytes between `$` and `*`) and drops sentences that fail validation

### NMEA Relay

- **NMEA-01**: Device publishes each valid NMEA sentence to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` (e.g. `gnss/ABC123/nmea/GNGLL`)
- **NMEA-02**: UART reader and MQTT publisher are decoupled via a bounded channel (max 64 sentences); if the channel is full, sentences are dropped rather than blocking the UART task

### Remote Config

- **CONF-01**: Device subscribes to `gnss/{device_id}/config` (QoS 1) and forwards received payload line-by-line to the UM980 over UART TX
- **CONF-02**: Device queues received config messages and only applies them after the UART driver has been fully initialized and is ready to accept writes
- **CONF-03**: Device applies a per-command delay between UART TX writes to allow the UM980 processing window

## Out of Scope

| Feature | Reason |
|---------|--------|
| BLE provisioning | Deferred post-v1; hardcoded credentials sufficient for development and early deployment |
| Web portal (SoftAP) provisioning | Deferred; depends on BLE provisioning milestone |
| TLS/mTLS for MQTT | High complexity (cert management, mbedTLS tuning); separate milestone |
| OTA firmware update | Wrong implementation bricks devices; requires dual-partition design; separate milestone |
| Full NMEA field parsing (lat/lon decode) | Firmware job is relay, not parse; consumers parse downstream |
| Local NMEA buffering across power cycles | Stale positions misleading; flash wear from 10Hz writes unacceptable |
| JSON-wrapped NMEA publish | Parsing overhead on MCU adds no value; consumers prefer raw NMEA |
| Multi-broker publishing | Multiplies state management complexity; single broker only |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| SCAF-01 | Phase 1 | Pending |
| SCAF-02 | Phase 1 | Pending |
| SCAF-03 | Phase 1 | Pending |
| SCAF-04 | Phase 1 | Pending |
| SCAF-05 | Phase 1 | Pending |
| CONN-01 | Phase 2 | Pending |
| CONN-02 | Phase 2 | Pending |
| CONN-03 | Phase 2 | Pending |
| CONN-04 | Phase 2 | Pending |
| CONN-05 | Phase 2 | Pending |
| CONN-06 | Phase 2 | Pending |
| CONN-07 | Phase 2 | Pending |
| LED-01 | Phase 3 | Pending |
| LED-02 | Phase 3 | Pending |
| LED-03 | Phase 3 | Pending |

**Coverage:**
- v1 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0

---
*Requirements defined: 2026-03-03*
*Last updated: 2026-03-03 after roadmap creation*
