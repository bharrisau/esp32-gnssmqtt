---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Field Deployment
status: defining_requirements
stopped_at: —
last_updated: "2026-03-08"
last_activity: "2026-03-08 — Milestone v2.0 started"
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-08)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** v2.0 Field Deployment — defining requirements

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-03-08 — Milestone v2.0 started

## Accumulated Context

### Decisions

All decisions from v1.0–v1.3 logged in PROJECT.md Key Decisions table.

Key carry-forward notes:
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir
- [OTA NOTE]: Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes
- [BLE NOTE]: `esp-idf-svc::bt` BLE GATT API was volatile as of mid-2025 — verify stability before BLE provisioning work

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes
- Verify `esp-idf-svc::bt` BLE GATT API stability before BLE provisioning phase

### Blockers/Concerns

(none at milestone start)

## Session Continuity

Last session: 2026-03-08
Stopped at: Milestone v2.0 start — requirements pending
Resume file: None
Next action: `/gsd:plan-phase <N>` after roadmap is created
