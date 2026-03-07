# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- ✅ **v1.1 GNSS Relay** — Phases 4-6 (shipped 2026-03-07)
- 🚧 **v1.2 Observations + OTA** — Phases 7-8 (in progress)

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

### 🚧 v1.2 Observations + OTA (In Progress)

- [ ] **Phase 7: RTCM Relay** — Mixed NMEA+RTCM byte stream parsing, binary MQTT publish, topic routing fix
- [ ] **Phase 8: OTA** — Dual-partition table, HTTP firmware pull, rollback safety, status reporting

## Phase Details

### Phase 7: RTCM Relay
**Goal**: UM980 RTCM3 correction frames are reliably delivered to MQTT alongside existing NMEA relay, with correct MQTT topic routing for all message types
**Depends on**: Phase 6 (v1.1 complete)
**Requirements**: RTCM-01, RTCM-02, RTCM-03, RTCM-04, RTCM-05
**Success Criteria** (what must be TRUE):
  1. RTCM3 frames appear on `gnss/{device_id}/rtcm/{message_type}` as raw binary at QoS 0 while NMEA topics continue publishing without interruption
  2. Frames with CRC-24Q failures are silently dropped and the byte-stream re-syncs to the next valid 0xD3 preamble or `$` start without operator intervention
  3. MSM7 frames (up to 1029 bytes) are published without truncation or MQTT buffer overflow
  4. A retained `/config` payload is NOT forwarded to the UM980 when an `/ota/trigger` message arrives (topic discrimination fix verifiable by inspection of UM980 UART TX)
**Plans**: TBD

### Phase 8: OTA
**Goal**: An operator can remotely update firmware by publishing a URL to an MQTT topic; the device downloads, flashes, and reboots into new firmware with automatic rollback if the new firmware fails to confirm itself
**Depends on**: Phase 7 (topic discrimination fix required for OTA trigger routing)
**Requirements**: OTA-01, OTA-02, OTA-03, OTA-04, OTA-05, OTA-06
**Success Criteria** (what must be TRUE):
  1. After publishing `{"url":"..."}` to `gnss/{device_id}/ota/trigger`, the device publishes `{"state":"downloading","progress":N}` updates and eventually `{"state":"complete"}` to `gnss/{device_id}/ota/status`, then reboots into new firmware
  2. MQTT heartbeat continues publishing during OTA download (pump event loop is not blocked by the HTTP transfer)
  3. When a new firmware boot completes without calling `mark_running_slot_valid()` within the watchdog window, the device reboots back into the previous firmware slot on the next boot
  4. OTA can be triggered a second time immediately after a successful update (trigger topic cleared; no retained re-trigger on reconnect)
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
| 7. RTCM Relay | v1.2 | 0/? | Not started | - |
| 8. OTA | v1.2 | 0/? | Not started | - |
