---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Field Deployment
status: planning
stopped_at: Completed 14-02-PLAN.md
last_updated: "2026-03-08T00:12:00.000Z"
last_activity: 2026-03-08 — phase 14 plan 02 complete; command relay + reboot trigger implemented
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 2
  completed_plans: 2
  percent: 10
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-08)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** v2.0 Field Deployment — Phase 14: Quick Additions

## Current Position

Phase: 14 of 18 (Quick Additions)
Plan: 2 of 2 in current phase (phase complete)
Status: Phase 14 complete — ready for phase 15
Last activity: 2026-03-08 — Phase 14 plan 02 executed; command relay + reboot trigger implemented

Progress: [█░░░░░░░░░] 10%

## Performance Metrics

**Velocity:**
- Total plans completed: 0 (v2.0)
- Prior milestone (v1.3): 9 plans, ~30 min avg/plan
- Total v1.x execution time: ~8 hours across 24 plans

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| v2.0 — not started | - | - | - |

**Recent Trend:**
- v1.3 last 5 plans: stable
- Trend: Stable

| Phase 14 P01 | 5 | 2 tasks | 2 files |
| Phase 14 P02 | 12 | 2 tasks | 3 files |

## Accumulated Context

### Decisions

All decisions from v1.0–v1.3 logged in PROJECT.md Key Decisions table.

Key carry-forward notes:
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir
- [OTA NOTE]: Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes
- [BLE NOTE]: BLE provisioning deferred — SoftAP chosen for v2.0; covers WiFi + MQTT in one web UI without custom app
- [Phase 14]: EspSntp handle in main() scope prevents sntp_stop() on drop — mirrors _gnss_cmd_tx keep-alive pattern
- [Phase 14]: CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y in sdkconfig.defaults switches ESP-IDF log from ms-since-boot to HH:MM:SS.mmm wall-clock
- [Phase 14 P02]: QoS 0 (AtMostOnce) for /command subscription — prevents retain replay; old commands must not re-execute (CMD-02)
- [Phase 14 P02]: Reboot check uses json.trim() == "reboot" before extract_json_str — graceful short-circuit for MAINT-01 without parse error noise
- [Phase 14 P02]: command_relay_task uses blocking send() for gnss_cmd_tx to ensure no silent drops to UM980

### Pending Todos

- Verify `esp-idf-svc` SoftAP/captive-portal API availability before Phase 15
- Verify `esp-idf-svc::sntp` API before Phase 14

### Blockers/Concerns

(none at phase 14 start)

## Session Continuity

Last session: 2026-03-08T00:12:00.000Z
Stopped at: Completed 14-02-PLAN.md
Resume file: None
Next action: `/gsd:plan-phase 15` (SoftAP provisioning — verify esp-idf-svc SoftAP API first)
