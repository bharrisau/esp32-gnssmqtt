---
phase: 25-web-ui-remaining-gap-crate-skeletons
plan: 02
subsystem: ui
tags: [axum, websocket, broadcast, nmea, serde_json, tokio]

# Dependency graph
requires:
  - phase: 25-web-ui-remaining-gap-crate-skeletons-plan-01
    provides: GsvAccumulator, tag_heartbeat, run_web_server, AppState, broadcast::Sender<String> contract
provides:
  - run_decode_task with ws_tx parameter handling all three MqttMessage variants
  - web_server::run_web_server spawned in main alongside mqtt_supervisor
  - broadcast channel fan-out from MQTT messages to WebSocket clients
  - end-to-end data flow: MQTT -> decode task -> broadcast -> WebSocket clients
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - broadcast::channel<String>(16) created in main(), cloned for web_server and decode task
    - _ws_rx_discard holds one receiver so channel stays open when no WebSocket clients connected
    - Full match on MqttMessage variants replaces if-let (handles all three arms)

key-files:
  created: []
  modified:
    - gnss-server/src/main.rs

key-decisions:
  - "broadcast::channel capacity 16 sufficient for ~1 Hz MQTT message rate with small number of WebSocket clients"
  - "_ws_rx_discard held in main() scope prevents SendError when no WebSocket clients are subscribed"

patterns-established:
  - "main() creates broadcast channel, clones ws_tx for web_server spawn, moves original into run_decode_task"
  - "run_decode_task uses full match on MqttMessage — Rtcm writes RINEX, Nmea feeds GsvAccumulator, Heartbeat tags and broadcasts"

requirements-completed: [UI-01, UI-02, UI-03, UI-04]

# Metrics
duration: 5min
completed: 2026-03-12
---

# Phase 25 Plan 02: Web UI Wiring Summary

**broadcast::channel wired from main() into run_decode_task and web_server, completing MQTT->decode->WebSocket fan-out for GSV satellite state and heartbeat JSON**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-12T09:25:31Z
- **Completed:** 2026-03-12T09:30:00Z
- **Tasks:** 1 of 2 (Task 2 is a human-verify checkpoint)
- **Files modified:** 1

## Accomplishments
- `broadcast::channel::<String>(16)` created in main(); channel held open via `_ws_rx_discard`
- `web_server::run_web_server` spawned with `http_port` and cloned `ws_tx`
- `run_decode_task` updated with fourth `ws_tx` parameter and full match on all three `MqttMessage` variants
- NMEA path: `GsvAccumulator.feed()` -> `serde_json::to_string` -> `ws_tx.send`
- Heartbeat path: `nmea_parse::tag_heartbeat` -> `ws_tx.send`
- `cargo build -p gnss-server` clean; `cargo clippy -D warnings` clean; 38 tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire broadcast channel into main and run_decode_task** - `1ce814c` (feat)

**Task 2 (checkpoint:human-verify):** Awaiting human browser verification at http://localhost:8080

## Files Created/Modified
- `gnss-server/src/main.rs` — broadcast channel, web_server spawn, run_decode_task wired with ws_tx and full MqttMessage match

## Decisions Made
- `_ws_rx_discard` kept in main() scope: if all receivers drop, `broadcast::Sender::send` returns `SendError`; holding one dummy receiver prevents spurious errors when no WebSocket clients are connected.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- gnss-server binary compiles and starts the HTTP/WebSocket server on `http_port` (default 8080)
- Requires human browser verification at http://localhost:8080 (Task 2 checkpoint)
- After checkpoint approval, plan 25-02 is fully complete

---
*Phase: 25-web-ui-remaining-gap-crate-skeletons*
*Completed: 2026-03-12*
