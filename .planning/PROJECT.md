# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences from the UM980 over UART, publishes each sentence type to a dedicated MQTT topic, and receives its UM980 initialization commands from the broker via a retained config topic. WiFi and MQTT credentials are provisioned via BLE on first boot.

## Core Value

NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Device provisions WiFi + MQTT credentials via BLE on first boot; credentials persisted to NVS flash
- [ ] Web portal fallback for provisioning if BLE is unavailable
- [ ] On boot, device connects to WiFi and MQTT broker using stored credentials
- [ ] Device subscribes to `gnss/{device_id}/config` (retained) and sends received payload as UART commands to the UM980
- [ ] Device reads all NMEA sentences from UM980 UART at 115200 baud and publishes each to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` (e.g. `gnss/ABC123/nmea/GNGLL`)
- [ ] Device publishes periodic heartbeat to `gnss/{device_id}/heartbeat` for device discovery
- [ ] MQTT auth uses username + password
- [ ] Device reconnects automatically to WiFi and MQTT on disconnect; status LED reflects connectivity state
- [ ] Device ID derived from ESP32 built-in serial number

### Out of Scope

- TLS/mTLS — username/password auth is sufficient for v1
- Local NMEA buffering across power cycles — real-time relay only
- OTA firmware update — defer to future milestone
- Mobile app — BLE provisioning via standard BLE client tools

## Context

- **Target hardware**: ESP32-C6 (RISC-V, WiFi 6, BLE 5)
- **GNSS module**: UM980 multi-band RTK receiver, UART interface at 115200 baud
- **Language**: Rust (embedded, no_std or std via esp-idf-hal)
- **NMEA example**: `$GNGLL,4004.73885655,N,11614.19746477,E,023842.00,A,A*75`
- **UM980 init commands**: Known by developer; delivered to device via retained MQTT config topic rather than hardcoded — enables remote reconfiguration without reflashing
- **MQTT broker**: External broker (e.g. Mosquitto, HiveMQ); username/password auth

## Constraints

- **Tech stack**: Rust — embedded Rust ecosystem for ESP32 (esp-idf-hal, esp-idf-sys, or esp-hal bare metal)
- **Hardware**: ESP32-C6 only; no portability requirement to other targets
- **UART**: UM980 fixed at 115200 baud, 8N1
- **MQTT**: No TLS in v1; standard MQTT 3.1.1 or 5.0

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| UM980 init commands delivered via retained MQTT topic | Enables remote reconfiguration without reflash; decouples firmware from GNSS config | — Pending |
| Per-sentence MQTT topics (`nmea/{TYPE}`) | Consumers can subscribe selectively to sentence types they care about | — Pending |
| Device ID from ESP32 hardware serial | Unique per-device without manual configuration | — Pending |
| BLE provisioning primary, web portal fallback | BLE is frictionless on mobile; web portal covers edge cases | — Pending |

---
*Last updated: 2026-03-03 after initialization*
