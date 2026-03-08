# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- ✅ **v1.1 GNSS Relay** — Phases 4-6 (shipped 2026-03-07)
- ✅ **v1.2 Observations + OTA** — Phases 7-8 (shipped 2026-03-07)
- ✅ **v1.3 Reliability Hardening** — Phases 9-13 (shipped 2026-03-08)
- 🚧 **v2.0 Field Deployment** — Phases 14-18 (in progress)

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

### 🚧 v2.0 Field Deployment (In Progress)

**Milestone Goal:** Enable unattended outdoor RTK operation with runtime WiFi/MQTT provisioning, NTRIP corrections pipeline, remote log streaming, and command relay — no firmware recompile needed for field configuration.

- [x] **Phase 14: Quick Additions** — SNTP time sync, command relay topic, and reboot trigger extend existing subsystems with minimal new code (completed 2026-03-07)
- [x] **Phase 15: Provisioning** — SoftAP web UI lets users configure WiFi and MQTT credentials without recompiling firmware; stored in NVS with multi-AP failover (completed 2026-03-08)
- [x] **Phase 16: Remote Logging** — ESP-IDF log output forwarded to MQTT with re-entrancy guard preventing feedback loops; level configurable at runtime (completed 2026-03-08)
- [ ] **Phase 17: NTRIP Client** — TCP connection to NTRIP caster streams RTCM3 corrections directly to UM980 UART, enabling RTK fix
- [ ] **Phase 18: Telemetry and OTA Validation** — GNSS fix quality added to heartbeat; OTA pipeline validated on hardware before v2.0 milestone sign-off

## Phase Details

### Phase 14: Quick Additions
**Goal**: Users can sync wall-clock time automatically, send arbitrary UM980 commands remotely, and trigger remote reboot — all using existing MQTT infrastructure with no new connection types
**Depends on**: Phase 13
**Requirements**: MAINT-01, MAINT-02, CMD-01, CMD-02
**Success Criteria** (what must be TRUE):
  1. Log output shows ISO timestamps (not relative ms ticks) after WiFi connects
  2. Publishing any string to `gnss/{device_id}/command` causes the UM980 to execute that command exactly once, with no deduplication
  3. Publishing `"reboot"` to `gnss/{device_id}/ota/trigger` causes the device to restart within 5 seconds
  4. The command topic is non-retained; replaying the MQTT session does not re-send old commands
**Plans**: 2 plans

Plans:
- [ ] 14-01-PLAN.md — SNTP time sync on WiFi connect (sdkconfig + EspSntp init)
- [ ] 14-02-PLAN.md — Command relay topic + reboot trigger (mqtt.rs + ota.rs + main.rs)

### Phase 15: Provisioning
**Goal**: Users can configure WiFi and MQTT credentials from any browser via the device's SoftAP hotspot, with up to 3 networks stored in NVS and tried automatically on connection failure
**Depends on**: Phase 14
**Requirements**: PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, PROV-07, PROV-08
**Success Criteria** (what must be TRUE):
  1. A freshly flashed device with no NVS credentials broadcasts a SoftAP hotspot and shows a web UI at its IP address
  2. User can enter WiFi SSID/password and MQTT host/port/credentials in the web UI; device saves them to NVS and reboots into station mode
  3. User can store up to 3 WiFi networks; device tries each in order on connection failure without entering SoftAP
  4. Holding GPIO9 low for 3 seconds re-enters SoftAP mode from any state; device returns to WiFi mode after 300 seconds with no client connected
  5. Publishing `"softap"` to the OTA trigger topic enters SoftAP mode with the same 300-second no-client timeout
  6. LED shows a distinct pattern while in SoftAP mode, visually distinct from connecting, connected, and error states
**Plans**: 3 plans

Plans:
- [ ] 15-01-PLAN.md — provisioning.rs module: NVS credential storage, SoftAP WiFi mode, HTTP portal form (PROV-01, PROV-02, PROV-03, PROV-04)
- [ ] 15-02-PLAN.md — Boot-path decision, wifi_connect_any, mqtt_connect runtime credentials (PROV-01, PROV-05)
- [ ] 15-03-PLAN.md — GPIO9 monitor, MQTT "softap" trigger, LedState::SoftAP blink pattern (PROV-06, PROV-07, PROV-08)

### Phase 16: Remote Logging
**Goal**: All ESP-IDF log output is forwarded to an MQTT topic in real time, with a re-entrancy guard that prevents the logging path itself from generating log events, and a runtime-configurable level threshold
**Depends on**: Phase 15
**Requirements**: LOG-01, LOG-02, LOG-03
**Success Criteria** (what must be TRUE):
  1. Log messages appear on `gnss/{device_id}/log` within one second of being emitted by any firmware component
  2. Publishing MQTT or processing subscriptions does not generate additional log entries that appear on the log topic (no feedback loop)
  3. Publishing a log level string to the log config topic changes which messages are forwarded immediately, without reboot
  4. Log publishing does not stall the calling thread when MQTT is disconnected or the channel is full; messages are silently dropped
**Plans**: 2 plans

Plans:
- [ ] 16-01-PLAN.md — C vprintf hook (log_shim.c), Rust relay module (log_relay.rs), build system integration (LOG-01, LOG-03)
- [ ] 16-02-PLAN.md — Wire log relay into main.rs, /log/level subscription and runtime level apply in mqtt.rs (LOG-01, LOG-02, LOG-03)

### Phase 17: NTRIP Client
**Goal**: The device connects to a configured NTRIP caster over TCP and streams RTCM3 correction data to the UM980 UART, enabling RTK fix; connection settings are configurable at runtime via MQTT without reboot
**Depends on**: Phase 16
**Requirements**: NTRIP-01, NTRIP-02, NTRIP-03, NTRIP-04
**Success Criteria** (what must be TRUE):
  1. After publishing NTRIP settings (host, port, mountpoint, credentials) to the retained config topic, the device establishes a TCP connection to the caster and the UM980 receives RTCM3 correction bytes
  2. The UM980 achieves RTK Float or RTK Fix status when a valid mountpoint with coverage is configured
  3. If the NTRIP TCP connection drops, the device reconnects automatically without a reboot
  4. The health heartbeat includes an NTRIP connection state field (`connected` / `disconnected`)
**Plans**: 4 plans

Plans:
- [ ] 17-01-PLAN.md — ntrip_client.rs module: NtripConfig, NVS persistence, TCP session loop, RTCM forwarding to UART, reconnect backoff (NTRIP-01, NTRIP-03)
- [ ] 17-02-PLAN.md — Wire into main.rs + mqtt.rs: ntrip_config channel, /ntrip/config dispatch + subscription, heartbeat ntrip field (NTRIP-02, NTRIP-04)
- [ ] 17-03-PLAN.md — Log quality fixes + UM980 reboot detection (channel 32→128, ANSI strip, MQTT event log levels, config re-apply on UM980 restart)
- [ ] 17-04-PLAN.md — Captive portal DNS hijack for SoftAP (DNS server on port 53, probe URL handling)

### Phase 18: Telemetry and OTA Validation
**Goal**: The health heartbeat reports live GNSS fix quality so operators can assess RTK performance remotely; the OTA update pipeline is validated end-to-end on hardware before v2.0 is marked complete
**Depends on**: Phase 17
**Requirements**: TELEM-01, MAINT-03
**Success Criteria** (what must be TRUE):
  1. The heartbeat JSON includes `fix_type`, `satellites`, and `hdop` fields populated from the most recent GGA sentence
  2. When no GGA sentence has been received, heartbeat fields show null or sentinel values rather than stale data
  3. An OTA firmware update is triggered via MQTT on device FFFEB5, the new image downloads, SHA-256 is verified, the device reboots into the new image, and marks valid — completing the v2.0 hardware sign-off
**Plans**: TBD

Plans:
- [ ] 18-01: GGA parsing for fix quality in heartbeat
- [ ] 18-02: OTA hardware validation on device FFFEB5

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
| 17. NTRIP Client | 1/4 | In Progress|  | - |
| 18. Telemetry and OTA Validation | v2.0 | 0/2 | Not started | - |
