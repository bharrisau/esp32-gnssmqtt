---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Observations + OTA
status: planning
stopped_at: Completed 07-rtcm-relay 07-02-PLAN.md
last_updated: "2026-03-07T03:18:29.262Z"
last_activity: 2026-03-07 — Roadmap created; Phase 7 and Phase 8 defined
progress:
  total_phases: 2
  completed_phases: 0
  total_plans: 3
  completed_plans: 1
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.
**Current focus:** v1.2 Observations + OTA — Phase 7 (RTCM relay)

## Current Position

Phase: 7 — RTCM Relay (not started)
Plan: —
Status: Ready to plan
Last activity: 2026-03-07 — Roadmap created; Phase 7 and Phase 8 defined

```
v1.2 progress: [          ] 0% (0/2 phases)
```

## Accumulated Context

### Decisions

All decisions logged in PROJECT.md Key Decisions table.
- [Phase 07-rtcm-relay]: Box<[u8; 1029]> for RtcmBody buffer to avoid stack overflow even with 12288 stack
- [Phase 07-rtcm-relay]: Complete RTCM frame published (preamble+header+payload+CRC) not just payload for independent CRC verification by consumers

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before Phase 8 implementation (read `~/.cargo/registry/src/.../esp-idf-svc-0.51.0/Cargo.toml`; takes 2 minutes; resolves whether `features = ["ota"]` is needed)

### Blockers/Concerns

- [Phase 8 PREREQUISITE]: `partitions.csv` redesign requires `espflash erase-flash` + USB reflash before any OTA code is testable — existing factory partition leaves zero room for OTA slots. This is the first act of Phase 8, not an optional step.
- [Phase 8 PITFALL]: `mark_running_slot_valid()` must be called early in `main()` after UART init succeeds on every boot — omitting it causes rollback on every reboot when `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`.
- [Phase 8 PITFALL]: OTA download must run in independent `ota.rs` thread — running inside MQTT pump blocks `connection.next()`, causes keep-alive timeout and broker disconnect.
- [Phase 8 PITFALL]: Watchdog fires during OTA partition erase (4-8 seconds); use sequential erase mode from the start.
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)
- [Future]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before BLE provisioning work (future milestone)

## Session Continuity

Last session: 2026-03-07T03:18:29.259Z
Stopped at: Completed 07-rtcm-relay 07-02-PLAN.md
Resume file: None
Next action: `/gsd:plan-phase 7`
