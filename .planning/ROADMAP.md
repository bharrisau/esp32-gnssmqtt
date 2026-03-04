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

- [ ] Phase 4: UART Pipeline — NMEA sentence read loop from UM980 (UART-01 through UART-04)
- [ ] Phase 5: NMEA Relay — publish each sentence type to `gnss/{device_id}/nmea/{TYPE}` (NMEA-01, NMEA-02)
- [ ] Phase 6: Remote Config — subscribe to `gnss/{device_id}/config`, forward commands to UM980 (CONF-01 through CONF-03)

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffold | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | v1.0 | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | v1.0 | 3/3 | Complete | 2026-03-04 |
| 4. UART Pipeline | v1.1 | 0/? | Not started | — |
| 5. NMEA Relay | v1.1 | 0/? | Not started | — |
| 6. Remote Config | v1.1 | 0/? | Not started | — |
