# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- ✅ **v1.1 GNSS Relay** — Phases 4-6 (shipped 2026-03-07)
- ✅ **v1.2 Observations + OTA** — Phases 7-8 (shipped 2026-03-07)
- ✅ **v1.3 Reliability Hardening** — Phases 9-13 (shipped 2026-03-08)
- ✅ **v2.0 Field Deployment** — Phases 14-21 (shipped 2026-03-12)
- ✅ **v2.1 Server and nostd Foundation** — Phases 22-25 (shipped 2026-03-12)

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

<details>
<summary>✅ v2.0 Field Deployment (Phases 14-21) — SHIPPED 2026-03-12</summary>

- [x] Phase 14: Quick Additions (2/2 plans) — completed 2026-03-07
- [x] Phase 15: Provisioning (3/3 plans) — completed 2026-03-08
- [x] Phase 16: Remote Logging (2/2 plans) — completed 2026-03-08
- [x] Phase 17: NTRIP Client (4/4 plans) — completed 2026-03-09
- [x] Phase 18: Telemetry and OTA Validation (3/3 plans) — completed 2026-03-09
- [x] Phase 19: Pre-2.0 Bugfix (3/3 plans) — completed 2026-03-10
- [x] Phase 20: Field Testing Fixes (4/4 plans) — completed 2026-03-11
- [x] Phase 21: MQTT Performance (3/3 plans) — completed 2026-03-12

Archive: `.planning/milestones/v2.0-ROADMAP.md`

</details>

<details>
<summary>✅ v2.1 Server and nostd Foundation (Phases 22-25) — SHIPPED 2026-03-12</summary>

- [x] Phase 22: Workspace + Nostd Audit (2/2 plans) — completed 2026-03-12
- [x] Phase 23: MQTT + RTCM3 + gnss-nvs crate (3/3 plans) — completed 2026-03-12
- [x] Phase 24: RINEX Files + gnss-ota gap crate (3/3 plans) — completed 2026-03-12
- [x] Phase 25: Web UI + remaining gap crate skeletons (3/3 plans) — completed 2026-03-12

Archive: `.planning/milestones/v2.1-ROADMAP.md`

</details>

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
| 17. NTRIP Client | v2.0 | 4/4 | Complete | 2026-03-09 |
| 18. Telemetry and OTA Validation | v2.0 | 3/3 | Complete | 2026-03-09 |
| 19. Pre-2.0 Bugfix | v2.0 | 3/3 | Complete | 2026-03-10 |
| 20. Field Testing Fixes | v2.0 | 4/4 | Complete | 2026-03-11 |
| 21. MQTT Performance | v2.0 | 3/3 | Complete | 2026-03-12 |
| 22. Workspace + Nostd Audit | v2.1 | 2/2 | Complete | 2026-03-12 |
| 23. MQTT + RTCM3 + gnss-nvs crate | v2.1 | 3/3 | Complete | 2026-03-12 |
| 24. RINEX Files + gnss-ota gap crate | v2.1 | 3/3 | Complete | 2026-03-12 |
| 25. Web UI + remaining gap crate skeletons | v2.1 | 3/3 | Complete | 2026-03-12 |
