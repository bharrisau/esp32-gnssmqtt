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
- [ ] Phase 6: Remote Config — subscribe to `gnss/{device_id}/config`, forward commands to UM980 (CONF-01 through CONF-03)

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffold | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | v1.0 | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | v1.0 | 3/3 | Complete | 2026-03-04 |
| 4. UART Pipeline | 2/2 | Complete   | 2026-03-06 | — |
| 5. NMEA Relay | v1.1 | 0/? | Not started | — |
| 6. Remote Config | v1.1 | 0/? | Not started | — |
