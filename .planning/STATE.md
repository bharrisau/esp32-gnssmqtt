---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Reliability Hardening
status: in_progress
stopped_at: null
last_updated: "2026-03-07T10:10:36Z"
last_activity: 2026-03-07 — Phase 9 plan 09-01 executed (channels bounded + UART TX error logging)
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 2
  completed_plans: 1
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.
**Current focus:** v1.3 Reliability Hardening — executing Phase 9 (09-02 next)

## Current Position

Phase: 9 — Channel + Loop Hardening (in progress)
Plan: 09-02 (next)
Status: 09-01 complete; ready for 09-02 (recv_timeout + WiFi loop hardening)
Last activity: 2026-03-07 — 09-01 executed: all unbounded channels bounded, UART TX errors logged

```
v1.3 Progress: [=         ] 0/5 phases complete (Phase 9 in progress: 1/2 plans done)
```

## Accumulated Context

### Decisions

All decisions from v1.0–v1.2 logged in PROJECT.md Key Decisions table.

Key v1.2 decisions carried forward:
- [Phase 07-rtcm-relay]: Box<[u8; 1029]> for RtcmBody buffer to avoid stack overflow even with 12288 stack
- [Phase 07-rtcm-relay]: Complete RTCM frame published (preamble+header+payload+CRC) for independent CRC verification by consumers
- [Phase 08-ota]: mark_running_slot_valid() non-fatal — factory partition has no OTA slot; warn and continue
- [Phase 08-ota]: espflash.toml [idf_format_args] partition_table required — cargo espflash flash silently uses default partition layout without it

Key v1.3 decisions (Phase 9):
- [Phase 09-01]: sync_channel(16) for cmd_tx: config batch typically <=16 commands; capacity 16 prevents blocking UART TX drain
- [Phase 09-01]: sync_channel(2/4/1) for subscribe/config/ota_tx: rationale per channel (reconnect burst / retained replay / OTA exclusivity)
- [Phase 09-01]: config_relay.apply_config() keeps blocking send() — not a hot-path thread; blocking on full 16-slot channel is acceptable
- [Phase 09-01]: uart_bridge uses try_send — interactive stdin path must not stall on full command channel
- [Phase 09-01]: UART_TX_ERRORS AtomicU32 counter accumulates write errors; will be read by Phase 13 health telemetry

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes (read `~/.cargo/registry/src/.../esp-idf-svc-0.51.0/Cargo.toml`)

### Blockers/Concerns

- [Future]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before BLE provisioning work (future milestone)
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-07
Stopped at: Completed 09-01 (channel bounding + UART TX error logging)
Resume file: None
Next action: `/gsd:execute-phase 9` — execute 09-02 (recv_timeout + WiFi loop hardening)
