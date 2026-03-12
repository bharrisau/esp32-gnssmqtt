---
gsd_state_version: 1.0
milestone: v2.1
milestone_name: Server and nostd Foundation
status: ready_to_plan
stopped_at: Phase 22 ready to plan
last_updated: "2026-03-12"
last_activity: "2026-03-12 — v2.1 roadmap revised to 4-phase interleaved structure (22-25); gap crate work distributed across server feature phases; NOSTD-04 split into NOSTD-04a (Phase 24) and NOSTD-04b (Phase 25); gnss-nvs moved to Phase 23"
progress:
  total_phases: 4
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-12)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** Phase 22 — Workspace + Nostd Audit (v2.1 start)

## Current Position

Phase: 22 of 25 (Workspace + Nostd Audit)
Plan: 0 of TBD in current phase
Status: Ready to plan
Last activity: 2026-03-12 — v2.1 roadmap revised to 4 phases (22-25); gap crate work interleaved with server feature phases; 20/20 requirements mapped (NOSTD-04 split into NOSTD-04a + NOSTD-04b)

Progress: [████████████████████░░░░░] 84% (21/25 phases complete across all milestones)

## Execution Path

Phase dependencies for v2.1:

```
22 (Workspace + Audit) → 23 (MQTT + RTCM3 + gnss-nvs) → 24 (RINEX + gnss-ota)
                                                        → 25 (Web UI + gap skeletons)
```

Phase 24 and Phase 25 both depend on Phase 23 and can run in parallel with each other once Phase 23 completes.

## Performance Metrics

**Velocity (v2.0 reference):**
- Total plans completed: 48 (v1.0-v2.0 combined)
- v2.0: 24 plans across 8 phases
- Trend: Stable

**By Milestone:**

| Milestone | Phases | Plans | Status |
|-----------|--------|-------|--------|
| v1.0-v2.0 | 21 | 48 | Complete |
| v2.1 | 4 | TBD | Not started |

## Accumulated Context

### Decisions

Key carry-forward decisions affecting v2.1:
- [v2.0]: Single publish thread owns EspMqttClient; SyncSender<MqttMessage> pattern — server follows similar message-passing design
- [v2.0]: bytes crate for zero-copy RTCM on publish path — server receives these Bytes payloads from MQTT
- [v2.1 planning]: resolver="2" mandatory in workspace root — prevents std feature unification into no_std gap crates (Cargo pitfall)
- [v2.1 planning]: rtcm-rs 0.11 for server decode — avoids hand-rolled MSM cell mask and pseudorange bugs
- [v2.1 planning]: GLONASS carrier phase without FCN is Option::None, written as 16 spaces in RINEX, never 0.0
- [v2.1 revised]: Workspace restructure and nostd audit merged into Phase 22 — both produce no user-facing features and are tightly coupled groundwork; single phase avoids artificial split of interdependent setup work
- [v2.1 revised 2]: Gap crate work interleaved with server feature phases — gnss-nvs in Phase 23, gnss-ota in Phase 24, remaining skeletons in Phase 25; avoids one large gap crate phase blocking delivery feedback; NOSTD-04 split into NOSTD-04a (gnss-ota, Phase 24) and NOSTD-04b (gnss-softap/dns/log, Phase 25)

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 24]: rinex 0.21 OBS output format (2.x vs 3.x) unverified without running code — evaluate at Phase 24 start; DIY fallback is ~200-300 lines
- [Phase 24]: rinex 0.21 NAV writer marked under construction — may need DIY fixed-width writer
- [Phase 23]: esp-hal ecosystem moved fast in 2025; re-check esp-radio SoftAP password-protection and embedded-tls TLS 1.2 status before finalising gap table (Phase 22 audit will surface this)
- [Phase 23]: sequential-storage + esp-hal flash driver on ESP32-C6 unverified — include minimal build test in phase

## Session Continuity

Last session: 2026-03-12
Stopped at: v2.1 roadmap revised (4 phases, 22-25); ready to plan Phase 22
Resume file: None
Next action: /gsd:plan-phase 22
