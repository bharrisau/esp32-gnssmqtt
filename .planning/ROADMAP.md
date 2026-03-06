# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- 📋 **v1.1 GNSS Relay** — Phases 4+ (planned)

## Phases

<details>
<summary>✅ v1.0 Foundation (Phases 1-3) — SHIPPED 2026-03-04</summary>

- [x] Phase 1: Scaffold (2/2 plans) — completed 2026-03-03
- [x] Phase 2: Connectivity (4/4 plans) — completed 2026-03-04
- [x] Phase 3: Status LED (3/3 plans) — completed 2026-03-04

Archive: `.planning/milestones/v1.0-ROADMAP.md`

</details>

### 📋 v1.1 GNSS Relay (Planned)

- [x] Phase 4: UART Pipeline — NMEA sentence read loop from UM980 (UART-01 through UART-04) (completed 2026-03-06)
  - **Goal:** Read a continuous stream of NMEA bytes from the UM980 over UART0, assemble complete sentences, deliver as (sentence_type, raw_sentence) tuples via mpsc channel, provide TX Sender<String> for command injection.
  - **Plans:** 2 plans
  - Plans:
    - [ ] 04-01-PLAN.md — Create gnss.rs: exclusive UartDriver owner with RX thread (sentence assembly + stdout mirror) and TX thread (command write)
    - [ ] 04-02-PLAN.md — Refactor uart_bridge.rs to TX-only + wire main.rs Step 7 with gnss::spawn_gnss
- [ ] Phase 5: NMEA Relay — publish each sentence type to `gnss/{device_id}/nmea/{TYPE}` (NMEA-01, NMEA-02)
  - **Goal:** Consume (sentence_type, raw_sentence) tuples from gnss::spawn_gnss and publish each sentence's raw bytes to MQTT topic gnss/{device_id}/nmea/{sentence_type} at QoS 0 with a bounded 64-sentence channel.
  - **Plans:** 2 plans
  - Plans:
    - [ ] 05-01-PLAN.md — Switch gnss.rs to sync_channel(64) with try_send, create src/nmea_relay.rs with spawn_relay()
    - [ ] 05-02-PLAN.md — Wire nmea_relay into main.rs Step 14 + hardware verification on device FFFEB5
- [x] Phase 6: Remote Config — subscribe to `gnss/{device_id}/config`, forward commands to UM980 (CONF-01 through CONF-03) (completed 2026-03-06)
  - **Goal:** Subscribe to `gnss/{device_id}/config` (QoS 1), parse payload (JSON or plain text), apply hash deduplication, and forward each command line-by-line to the UM980 via gnss_cmd_tx with a configurable per-command delay.
  - **Plans:** 2 plans
  - Plans:
    - [ ] 06-01-PLAN.md — Create src/config_relay.rs (spawn_config_relay, djb2 hash, payload parser) + extend pump_mqtt_events with config_tx routing
    - [ ] 06-02-PLAN.md — Wire config relay into main.rs Step 15 + hardware verification on device FFFEB5

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffold | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | v1.0 | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | v1.0 | 3/3 | Complete | 2026-03-04 |
| 4. UART Pipeline | v1.1 | 2/2 | Complete | 2026-03-06 |
| 5. NMEA Relay | 1/2 | In Progress|  | — |
| 6. Remote Config | 2/2 | Complete   | 2026-03-06 | — |
