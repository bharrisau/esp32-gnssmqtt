# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-03)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.
**Current focus:** Phase 1 - Scaffold

## Current Position

Phase: 1 of 3 (Scaffold)
Plan: 0 of TBD in current phase
Status: Ready to plan
Last activity: 2026-03-03 — Roadmap created; ready to begin Phase 1 planning

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: -
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: -
- Trend: -

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Init]: Use `esp-idf-hal` + `esp-idf-svc` (IDF std path) — not bare-metal esp-hal; required for WiFi, BLE, NVS, MQTT on ESP32-C6
- [Init]: UM980 init commands delivered via retained MQTT topic — enables remote reconfiguration without reflash
- [Init]: Per-sentence MQTT topics (`nmea/{TYPE}`) — consumers subscribe selectively
- [Init]: Device ID from ESP32 hardware serial — unique per-device without manual configuration

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1]: Verify current coordinated versions of `esp-idf-hal`/`esp-idf-svc`/`esp-idf-sys` from latest `esp-idf-template` before pinning — training data versions may have incremented since Aug 2025
- [Phase 2]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before Phase 3 BLE provisioning work (v2 milestone)

## Session Continuity

Last session: 2026-03-03
Stopped at: Roadmap created; Phase 1 ready to plan
Resume file: None
