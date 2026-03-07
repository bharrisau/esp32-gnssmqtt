# Requirements: esp32-gnssmqtt

**Defined:** 2026-03-07
**Core Value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.

## v1 Requirements

### RTCM Binary Relay

- [x] **RTCM-01**: gnss.rs RX thread handles mixed NMEA+RTCM byte stream via `RxState` state machine (Idle / NmeaLine / RtcmHeader / RtcmBody); 1029-byte RTCM frame buffer
- [x] **RTCM-02**: RTCM3 frames detected by 0xD3 preamble, 10-bit length parsed, CRC-24Q verified; invalid frames trigger resync (scan for next 0xD3/$)
- [x] **RTCM-03**: Verified RTCM frames delivered via bounded `sync_channel(32)` as `(u16, Vec<u8>)` (message_type, complete_frame) to `rtcm_relay.rs`
- [x] **RTCM-04**: Raw RTCM frames published to `gnss/{device_id}/rtcm/{message_type}` at QoS 0, retain=false; MQTT `out_buffer_size` bumped to 2048
- [ ] **RTCM-05**: `pump_mqtt_events` routes by topic (`/config` vs `/ota/trigger`) — fixes latent bug where all `Received` events route to `config_tx`

### OTA Firmware Update

- [ ] **OTA-01**: Partition table redesigned to `otadata + ota_0 + ota_1` (each ~1.875MB) for 4MB flash; requires `espflash erase-flash` + USB reflash
- [ ] **OTA-02**: Device subscribes to `gnss/{device_id}/ota/trigger` (QoS 1); payload `{"url":"...","sha256":"..."}` triggers update
- [ ] **OTA-03**: Device HTTP-pulls firmware binary, verifies SHA256 during streaming download, writes to inactive OTA partition via `EspOta`
- [ ] **OTA-04**: Device reboots into new partition; calls `mark_running_slot_valid()` early in `main()` after WiFi+MQTT confirmed; rolls back to previous slot if not called within watchdog window
- [ ] **OTA-05**: OTA download runs in dedicated task receiving trigger via `mpsc::channel`; MQTT pump and keep-alive remain active during download
- [ ] **OTA-06**: Device reports status to `gnss/{device_id}/ota/status` — `{"state":"downloading","progress":N}` / `{"state":"complete"}` / `{"state":"failed","reason":"..."}`

## v2 Requirements

### Hardening

- **HARD-01**: All mpsc channels bounded with explicit capacities documented
- **HARD-02**: All loop exit conditions explicit; no unbounded retry loops
- **HARD-03**: All heap allocations moved to startup; steady-state zero-alloc
- **HARD-04**: FreeRTOS task stack high-water mark logged at startup

### Metrics / Telemetry

- **METR-01**: Device publishes temperature, voltage, uptime to `gnss/{device_id}/status` periodically
- **METR-02**: Queue depths and stack HWM reported in status payload
- **METR-03**: Remote log streaming to MQTT

## Out of Scope

| Feature | Reason |
|---------|--------|
| Base64 RTCM encoding | 33% overhead with no benefit — MQTT is a binary protocol |
| HTTPS for OTA | Requires mbedTLS + certificate management; defer to security milestone |
| Baud rate change from 115200 | Not needed — RTCM MSM4 at 1Hz adds ~9% UART load; 115200 sufficient |
| BLE provisioning | Deferred to future milestone |
| TLS/mTLS for MQTT | Separate security milestone |
| Full NMEA field parsing | Firmware relays raw; consumers parse downstream |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| RTCM-01 | Phase 7 | Complete |
| RTCM-02 | Phase 7 | Complete |
| RTCM-03 | Phase 7 | Complete |
| RTCM-04 | Phase 7 | Complete |
| RTCM-05 | Phase 7 | Pending |
| OTA-01 | Phase 8 | Pending |
| OTA-02 | Phase 8 | Pending |
| OTA-03 | Phase 8 | Pending |
| OTA-04 | Phase 8 | Pending |
| OTA-05 | Phase 8 | Pending |
| OTA-06 | Phase 8 | Pending |

**Coverage:**
- v1 requirements: 11 total
- Mapped to phases: 11
- Unmapped: 0 ✓

---
*Requirements defined: 2026-03-07*
*Last updated: 2026-03-07 after initial definition*
