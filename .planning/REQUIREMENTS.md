# Requirements: esp32-gnssmqtt

**Defined:** 2026-03-08
**Milestone:** v2.0 Field Deployment
**Core Value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.

## v1.3 Requirements (Complete)

### Channel Hardening

- [x] **HARD-01**: All mpsc channels use `sync_channel` with explicit bounded capacities; capacities documented in code comments (fixes: cmd_rx, subscribe_tx, config_tx, ota_tx)
- [x] **HARD-02**: UART TX write failures are logged (not silently ignored via `let _ = ...`); per-failure error counter incremented

### Memory

- [x] **HARD-03**: RTCM frame delivery uses a pre-allocated buffer pool at startup; no per-frame `Vec` allocation in steady state

### Stack / Diagnostics

- [x] **HARD-04**: FreeRTOS task stack high-water mark (HWM) is logged at startup for every spawned thread

### Loop Safety

- [x] **HARD-05**: All loops with an intended termination condition (retry loops, init sequences) have an explicit maximum iteration or duration counter; exceeding the limit results in a logged error and clean exit rather than infinite spin
- [x] **HARD-06**: All blocking channel receives use `recv_timeout()` with a documented maximum wait; all blocking I/O and mutex lock operations have explicit timeouts (no unbounded `lock()` or blocking `recv()`)

### Thread Watchdog

- [x] **WDT-01**: Each critical thread (GNSS RX, MQTT pump) feeds a shared atomic watchdog counter at a regular interval (≤ 5s)
- [x] **WDT-02**: A watchdog supervisor thread detects if any critical thread misses 3 consecutive heartbeats and triggers `esp_restart()`

### Resilience

- [x] **RESIL-01**: `wifi_supervisor` triggers `esp_restart()` if WiFi has not been connected for a configurable duration (default 10 minutes)
- [x] **RESIL-02**: MQTT pump signals a reboot timer; if MQTT stays disconnected for a configurable duration after WiFi is up (default 5 minutes), device restarts

### Health Telemetry

- [x] **METR-01**: Device publishes `{"uptime_s":N,"heap_free":N,"nmea_drops":N,"rtcm_drops":N}` to `gnss/{device_id}/status` every 60 seconds
- [x] **METR-02**: NMEA and RTCM drop counters are atomic; incremented at each `TrySendError::Full` drop site in gnss.rs

## v2.0 Requirements

### Provisioning (PROV)

- [ ] **PROV-01**: Device enters SoftAP hotspot mode on first boot when no WiFi credentials exist in NVS
- [ ] **PROV-02**: User can open captive-portal web UI to enter WiFi SSID and password
- [ ] **PROV-03**: User can configure MQTT broker (host, port, username, password) via provisioning web UI
- [ ] **PROV-04**: User can store up to 3 WiFi networks via web UI; all persisted to NVS
- [ ] **PROV-05**: On all WiFi failures, device retries stored networks indefinitely with backoff; does not auto-enter SoftAP
- [ ] **PROV-06**: GPIO9 held low for 3s enters SoftAP mode; device exits back to WiFi mode after 300s with no client connected (timer paused while a client is associated)
- [ ] **PROV-07**: MQTT payload `"softap"` to `gnss/{device_id}/ota/trigger` enters SoftAP mode; same 300s no-client timeout applies
- [ ] **PROV-08**: LED shows a distinct flash pattern while in SoftAP mode (different from connecting/connected/error)

### NTRIP Client (NTRIP)

- [ ] **NTRIP-01**: Device connects to configured NTRIP caster and streams RTCM3 corrections to UM980 UART
- [ ] **NTRIP-02**: NTRIP settings (host, port, mountpoint, user, pass) configurable via retained MQTT topic `gnss/{device_id}/ntrip/config`
- [ ] **NTRIP-03**: NTRIP client reconnects automatically on connection loss
- [ ] **NTRIP-04**: NTRIP connection state included in health heartbeat

### Command Relay (CMD)

- [ ] **CMD-01**: Device subscribes to `gnss/{device_id}/command` and forwards each message as a raw UM980 command over UART
- [ ] **CMD-02**: Command topic is non-retained; each publish triggers exactly one command send with no deduplication

### Remote Logging (LOG)

- [ ] **LOG-01**: ESP-IDF log output forwarded to `gnss/{device_id}/log` MQTT topic at QoS 0; log hook uses re-entrancy guard so MQTT enqueue/send paths are excluded from capture, preventing feedback loops
- [ ] **LOG-02**: Log level threshold configurable via retained MQTT topic
- [ ] **LOG-03**: Log publishing is non-blocking; messages dropped silently when MQTT is disconnected or channel is full

### Maintenance (MAINT)

- [ ] **MAINT-01**: Device reboots when `gnss/{device_id}/ota/trigger` payload is `"reboot"`
- [ ] **MAINT-02**: Device syncs wall-clock time via SNTP on WiFi connect; timestamps appear in log output
- [ ] **MAINT-03**: OTA firmware update validated on hardware (device FFFEB5) as explicit sign-off gate before v2.0 milestone is marked complete

### Telemetry (TELEM)

- [ ] **TELEM-01**: Health heartbeat includes GNSS fix type, satellite count, and HDOP parsed from the most recent GGA sentence

## Future Requirements

### Hardening

- **HARD-07**: All heap allocations moved to startup; steady-state zero-alloc for NMEA path (NMEA strings currently allocated per-sentence)

### Security

- **SEC-F01**: TLS/mTLS for MQTT — separate milestone
- **SEC-F02**: Authenticated provisioning web UI (portal password)

### Provisioning

- **PROV-F01**: BLE provisioning if a standard tool (no custom app) can configure both WiFi and MQTT credentials

## Out of Scope

| Feature | Reason |
|---------|--------|
| Full NMEA field parsing | Firmware relays raw; consumers parse downstream |
| Local NMEA buffering across power cycles | Real-time relay only |
| JSON-wrapped NMEA publish | Raw NMEA preferred |
| Multi-broker publishing | Single broker only |
| TLS/mTLS for MQTT | Separate security milestone |
| BLE provisioning | Requires custom app to also configure MQTT; SoftAP covers both in web UI |
| HTTPS for OTA | Requires mbedTLS + certificate management; defer to security milestone |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| HARD-01 | Phase 9 | Complete |
| HARD-02 | Phase 9 | Complete |
| HARD-05 | Phase 9 | Complete |
| HARD-06 | Phase 9 | Complete |
| HARD-03 | Phase 10 | Complete |
| HARD-04 | Phase 10 | Complete |
| WDT-01  | Phase 11 | Complete |
| WDT-02  | Phase 11 | Complete |
| RESIL-01 | Phase 12 | Complete |
| RESIL-02 | Phase 12 | Complete |
| METR-01 | Phase 13 | Complete |
| METR-02 | Phase 13 | Complete |
| MAINT-01 | Phase 14 | Pending |
| MAINT-02 | Phase 14 | Pending |
| CMD-01 | Phase 14 | Pending |
| CMD-02 | Phase 14 | Pending |
| PROV-01 | Phase 15 | Pending |
| PROV-02 | Phase 15 | Pending |
| PROV-03 | Phase 15 | Pending |
| PROV-04 | Phase 15 | Pending |
| PROV-05 | Phase 15 | Pending |
| PROV-06 | Phase 15 | Pending |
| PROV-07 | Phase 15 | Pending |
| PROV-08 | Phase 15 | Pending |
| LOG-01 | Phase 16 | Pending |
| LOG-02 | Phase 16 | Pending |
| LOG-03 | Phase 16 | Pending |
| NTRIP-01 | Phase 17 | Pending |
| NTRIP-02 | Phase 17 | Pending |
| NTRIP-03 | Phase 17 | Pending |
| NTRIP-04 | Phase 17 | Pending |
| TELEM-01 | Phase 18 | Pending |
| MAINT-03 | Phase 18 | Pending |

**Coverage:**
- v2.0 requirements: 21 total
- Mapped to phases: 21
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-08*
*Last updated: 2026-03-08 after v2.0 roadmap creation (phases 14-18)*
