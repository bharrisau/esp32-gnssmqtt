# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-03)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.
**Current focus:** Phase 1 - Scaffold

## Current Position

Phase: 1 of 3 (Scaffold)
Plan: 1 of 2 in current phase
Status: In progress
Last activity: 2026-03-03 — Plan 01-01 complete: scaffold compiled, binary artifact verified

Progress: [█░░░░░░░░░] 10%

## Performance Metrics

**Velocity:**
- Total plans completed: 1
- Average duration: ~60 min
- Total execution time: ~1 hour

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-scaffold | 1 | ~60min | ~60min |

**Recent Trend:**
- Last 5 plans: 01-01 (~60min)
- Trend: baseline established

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Init]: Use `esp-idf-hal` + `esp-idf-svc` (IDF std path) — not bare-metal esp-hal; required for WiFi, BLE, NVS, MQTT on ESP32-C6
- [Init]: UM980 init commands delivered via retained MQTT topic — enables remote reconfiguration without reflash
- [Init]: Per-sentence MQTT topics (`nmea/{TYPE}`) — consumers subscribe selectively
- [Init]: Device ID from ESP32 hardware serial — unique per-device without manual configuration
- [01-01]: esp-idf-svc =0.51.0 / esp-idf-hal =0.45.2 / esp-idf-sys =0.36.1 with = pinning for build reproducibility
- [01-01]: embuild manages ESP-IDF v5.3.3 download — no manual SDK setup required
- [01-01]: Device ID from last 3 MAC bytes (first 3 are Espressif OUI, not unique)
- [01-01]: nightly Rust toolchain required for RISC-V esp-idf-sys build-std support

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1 RESOLVED]: Pinned esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1 — build confirmed working
- [Phase 1 DEFERRED]: Flash to hardware and stable device ID verification (01-02-PLAN.md)
- [Phase 2]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before Phase 3 BLE provisioning work (v2 milestone)
- [01-01 NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-03
Stopped at: Completed 01-01-PLAN.md (scaffold: config files + source files + cargo build verified)
Resume file: .planning/phases/01-scaffold/01-02-PLAN.md
