# Requirements: esp32-gnssmqtt

**Defined:** 2026-03-07
**Milestone:** v1.3 Reliability Hardening
**Core Value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.

## v1.3 Requirements

### Channel Hardening

- [x] **HARD-01**: All mpsc channels use `sync_channel` with explicit bounded capacities; capacities documented in code comments (fixes: cmd_rx, subscribe_tx, config_tx, ota_tx)
- [x] **HARD-02**: UART TX write failures are logged (not silently ignored via `let _ = ...`); per-failure error counter incremented

### Memory

- [ ] **HARD-03**: RTCM frame delivery uses a pre-allocated buffer pool at startup; no per-frame `Vec` allocation in steady state

### Stack / Diagnostics

- [x] **HARD-04**: FreeRTOS task stack high-water mark (HWM) is logged at startup for every spawned thread

### Loop Safety

- [x] **HARD-05**: All loops with an intended termination condition (retry loops, init sequences) have an explicit maximum iteration or duration counter; exceeding the limit results in a logged error and clean exit rather than infinite spin
- [x] **HARD-06**: All blocking channel receives use `recv_timeout()` with a documented maximum wait; all blocking I/O and mutex lock operations have explicit timeouts (no unbounded `lock()` or blocking `recv()`)

### Thread Watchdog

- [ ] **WDT-01**: Each critical thread (GNSS RX, MQTT pump) feeds a shared atomic watchdog counter at a regular interval (≤ 5s)
- [ ] **WDT-02**: A watchdog supervisor thread detects if any critical thread misses 3 consecutive heartbeats and triggers `esp_restart()`

### Resilience

- [ ] **RESIL-01**: `wifi_supervisor` triggers `esp_restart()` if WiFi has not been connected for a configurable duration (default 10 minutes)
- [ ] **RESIL-02**: MQTT pump signals a reboot timer; if MQTT stays disconnected for a configurable duration after WiFi is up (default 5 minutes), device restarts

### Health Telemetry

- [ ] **METR-01**: Device publishes `{"uptime_s":N,"heap_free":N,"nmea_drops":N,"rtcm_drops":N}` to `gnss/{device_id}/status` every 60 seconds
- [ ] **METR-02**: NMEA and RTCM drop counters are atomic; incremented at each `TrySendError::Full` drop site in gnss.rs

## v2 Requirements

### Hardening (deferred)

- **HARD-07**: All heap allocations moved to startup; steady-state zero-alloc for NMEA path (NMEA strings currently allocated per-sentence)

### Metrics / Telemetry (deferred)

- **METR-03**: Remote log streaming to MQTT

## Out of Scope

| Feature | Reason |
|---------|--------|
| BLE provisioning | Future milestone — esp-idf-svc::bt API volatile as of mid-2025 |
| TLS/mTLS for MQTT | Separate security milestone |
| HTTPS for OTA | Requires mbedTLS + certificate management; defer to security milestone |
| Full NMEA field parsing | Firmware relays raw; consumers parse downstream |
| Remote log streaming | High complexity, deferred to v2 |
| Multi-broker publishing | Single broker only |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| HARD-01 | Phase 9 | Complete (09-01) |
| HARD-02 | Phase 9 | Complete (09-01) |
| HARD-05 | Phase 9 | Complete (09-02) |
| HARD-06 | Phase 9 | Complete (09-02) |
| HARD-03 | Phase 10 | Pending |
| HARD-04 | Phase 10 | Complete |
| WDT-01  | Phase 11 | Pending |
| WDT-02  | Phase 11 | Pending |
| RESIL-01 | Phase 12 | Pending |
| RESIL-02 | Phase 12 | Pending |
| METR-01 | Phase 13 | Pending |
| METR-02 | Phase 13 | Pending |

**Coverage:**
- v1.3 requirements: 12 total
- Mapped to phases: 12
- Unmapped: 0

---
*Requirements defined: 2026-03-07*
*Last updated: 2026-03-07 after roadmap creation (Phases 9-13)*
