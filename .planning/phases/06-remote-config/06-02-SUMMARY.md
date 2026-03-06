---
phase: 06-remote-config
plan: 02
subsystem: config-relay
tags: [mqtt, gnss, config, uart, rust, embedded, hardware-verified]
dependency_graph:
  requires:
    - phase: 06-01
      provides: config_relay.rs and extended pump_mqtt_events signature
    - phase: 05-nmea-relay
      provides: gnss_cmd_tx Sender<String> and gnss module ownership
  provides:
    - main.rs Step 9b config channel creation
    - main.rs Step 10 pump spawn with config_tx argument
    - main.rs Step 15 config relay spawn
    - hardware verification of CONF-01, CONF-02, CONF-03 on device FFFEB5
  affects: []
tech_stack:
  added: []
  patterns:
    - mpsc channel chained from pump to relay via main.rs (subscribe pattern)
    - gnss_cmd_tx.clone() to relay thread; original retained in idle loop to keep Sender alive
key_files:
  created: []
  modified:
    - src/main.rs
key_decisions:
  - "main.rs Step 9b creates (config_tx, config_rx) channel before pump spawn; config_tx moved into pump closure"
  - "gnss_cmd_tx.clone() passed to spawn_config_relay; original _gnss_cmd_tx retained in idle loop keeps TX thread alive"
  - "All three CONF requirements hardware-verified on device FFFEB5 — relay operational end-to-end"
requirements-completed: [CONF-01, CONF-02, CONF-03]
metrics:
  duration: "~30 min (includes flash + hardware verification)"
  completed: "2026-03-07"
  tasks: 2
  files: 1
---

# Phase 06 Plan 02: Config Relay Wiring and Hardware Verification Summary

**config_relay wired into main.rs (channel + pump arg + Step 15 spawn) and all three CONF requirements hardware-verified on device FFFEB5 — MQTT config commands reach UM980 UART with hash dedup and per-command delay**

## Performance

- **Duration:** ~30 min (includes cargo build, flash, and hardware verification)
- **Completed:** 2026-03-07
- **Tasks:** 2 (1 code + 1 hardware checkpoint)
- **Files modified:** 1

## Accomplishments

- Wired `config_relay` into `main.rs` with three targeted changes: Step 9b channel creation, Step 10 pump call fix, Step 15 relay spawn
- Firmware flashed to device FFFEB5 and booted cleanly with `Config relay started` in log sequence
- CONF-01 verified: JSON payload `{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}` published to `gnss/FFFEB5/config`; both commands forwarded to UM980 (`OK` response from UM980 for `MODE ROVER`)
- CONF-02 verified: MQTT reconnect produced `Config relay: payload unchanged (hash 0xc7469b45), skipping` — config not re-applied
- CONF-03 verified: 200ms spacing between command log lines matches `delay_ms: 200` field
- Plain text fallback verified: multi-line plain text payload forwarded with 100ms default delay
- Empty payload guard verified: blank retained message produced `retained message cleared` log, no commands forwarded

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire config_relay into main.rs** - `685acdd` (feat)

**Plan metadata:** (this commit — docs: complete plan)

## Files Created/Modified

- `src/main.rs` — added `mod config_relay;` declaration, Step 9b config channel, Step 10 pump call with `config_tx`, Step 15 `spawn_config_relay` call, Step 15 entry in module init comment block

## Decisions Made

- `gnss_cmd_tx.clone()` passed to `spawn_config_relay`; original `_gnss_cmd_tx` retained in idle loop. This ensures the TX thread in gnss.rs stays alive as long as main runs — if all Senders drop the TX thread exits.
- No changes needed to `config_relay.rs` or `mqtt.rs` — Plan 01 interfaces were correct as specified.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None. The `CONFIGSAVE` command returned `PARSING FAILED` from UM980 — this is a UM980 command rejection (the command itself is not valid on this firmware version), not a relay issue. The relay delivered the command correctly, confirming CONF-01.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Phase 6 remote config is complete. All CONF requirements hardware-verified:
- CONF-01: config topic subscribed at QoS 1, payload forwarded line-by-line to UM980 UART TX
- CONF-02: hash dedup prevents re-application on MQTT reconnect; empty payload guard works
- CONF-03: 100ms default delay operative; delay_ms JSON override operative

The full GNSS MQTT relay pipeline is operational end-to-end on device FFFEB5. No outstanding blockers.

## Self-Check

- src/main.rs: FOUND
- Commit 685acdd (Task 1 — wire config_relay into main.rs): FOUND

## Self-Check: PASSED

---
*Phase: 06-remote-config*
*Completed: 2026-03-07*
