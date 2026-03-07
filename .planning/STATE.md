---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Reliability Hardening
status: in_progress
stopped_at: null
last_updated: "2026-03-07T00:00:00.000Z"
last_activity: 2026-03-07 — Roadmap created; 5 phases defined (9-13)
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.
**Current focus:** v1.3 Reliability Hardening — roadmap defined, ready to plan Phase 9

## Current Position

Phase: 9 — Channel + Loop Hardening (not started)
Plan: —
Status: Roadmap created; awaiting plan-phase
Last activity: 2026-03-07 — Roadmap created (Phases 9-13)

```
v1.3 Progress: [          ] 0/5 phases complete
```

## Accumulated Context

### Decisions

All decisions from v1.0–v1.2 logged in PROJECT.md Key Decisions table.

Key v1.2 decisions carried forward:
- [Phase 07-rtcm-relay]: Box<[u8; 1029]> for RtcmBody buffer to avoid stack overflow even with 12288 stack
- [Phase 07-rtcm-relay]: Complete RTCM frame published (preamble+header+payload+CRC) for independent CRC verification by consumers
- [Phase 08-ota]: mark_running_slot_valid() non-fatal — factory partition has no OTA slot; warn and continue
- [Phase 08-ota]: espflash.toml [idf_format_args] partition_table required — cargo espflash flash silently uses default partition layout without it

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes (read `~/.cargo/registry/src/.../esp-idf-svc-0.51.0/Cargo.toml`)

### Blockers/Concerns

- [Future]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before BLE provisioning work (future milestone)
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-07
Stopped at: Roadmap created for v1.3 (Phases 9-13)
Resume file: None
Next action: `/gsd:plan-phase 9` — Channel + Loop Hardening (HARD-01, HARD-02, HARD-05, HARD-06)
