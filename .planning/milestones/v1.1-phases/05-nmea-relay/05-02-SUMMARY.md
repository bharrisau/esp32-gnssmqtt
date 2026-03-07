---
phase: 05-nmea-relay
plan: "02"
subsystem: gnss
tags: [nmea, mqtt, rust, esp32, uart, relay, channel]

# Dependency graph
requires:
  - phase: 05-01
    provides: nmea_relay.rs spawn_relay() and gnss.rs sync_channel(64) with try_send
  - phase: 04-uart-pipeline
    provides: gnss::spawn_gnss() returning nmea_rx Receiver<(String, String)>
provides:
  - Full NMEA relay pipeline wired end-to-end in main.rs
  - Hardware-verified NMEA sentences published to gnss/FFFEB5/nmea/{TYPE} MQTT topics
affects: [06-gnss-config, future-consumers-of-nmea-mqtt]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - mod nmea_relay declared in main.rs mod block, called via full path nmea_relay::spawn_relay
    - nmea_rx Receiver moved into relay thread — main.rs no longer holds a reference after Step 14

key-files:
  created: []
  modified:
    - src/main.rs

key-decisions:
  - "nmea_rx is moved (not cloned) into spawn_relay — compiler enforces single consumer on the Receiver"
  - "spawn_relay() called before the idle loop at Step 14 — main.rs owns gnss_cmd_tx for Phase 6 forwarding"
  - "Hardware tested at 10 msg/sec with no relay channel full warnings — sync_channel(64) bound is sufficient for UM980 output rate"

patterns-established:
  - "Phase handoff pattern: placeholder _nmea_rx binding removed, Receiver moved into spawned thread"
  - "Module wired with full-path call (nmea_relay::spawn_relay) — no use statement needed"

requirements-completed: [NMEA-01, NMEA-02]

# Metrics
duration: ~10min
completed: 2026-03-07
---

# Phase 5 Plan 02: NMEA Relay Integration Summary

**NMEA relay pipeline wired into main.rs and hardware-verified: sentences published to gnss/FFFEB5/nmea/{TYPE} at up to 10 msg/sec with no channel backpressure warnings**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-03-07
- **Completed:** 2026-03-07
- **Tasks:** 2 (1 auto + 1 hardware checkpoint)
- **Files modified:** 1

## Accomplishments

- Added `mod nmea_relay;` to main.rs mod block (alphabetically between mqtt and uart_bridge)
- Replaced placeholder `_nmea_rx` binding at Step 14 with `nmea_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), nmea_rx)` — Receiver moved into relay thread
- Hardware-verified on device FFFEB5: NMEA sentences arrived on `gnss/FFFEB5/nmea/#` topics with `$`-prefixed payloads
- Throughput tested at 10 msg/sec — no "relay channel full" WARN lines, no UART RX stall observed

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire nmea_relay into main.rs — module declaration and spawn call** - `6686ca8` (feat)
2. **Task 2: Hardware verification — NMEA sentences on MQTT broker** - hardware-only, no code commit

**Plan metadata:** (this summary commit)

## Files Created/Modified

- `src/main.rs` — added mod nmea_relay; and replaced Step 14 _nmea_rx placeholder with spawn_relay() call

## Decisions Made

- `nmea_rx` is moved into `spawn_relay` — the placeholder `let _nmea_rx = nmea_rx;` line was removed as required; the compiler enforces single consumer
- Hardware test at 10 msg/sec confirms the sync_channel(64) bound from Plan 01 is well-sized for normal UM980 NMEA output rates
- `gnss_cmd_tx` retained in main.rs idle loop as `_gnss_cmd_tx` — Phase 6 will clone it for MQTT config command forwarding

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- NMEA-01 and NMEA-02 requirements hardware-verified on device FFFEB5
- Phase 5 complete — all NMEA relay requirements satisfied
- Phase 6 (gnss-config): `gnss_cmd_tx` Sender is alive in main.rs idle loop, ready to be cloned for MQTT config command forwarding to UM980

---
*Phase: 05-nmea-relay*
*Completed: 2026-03-07*
