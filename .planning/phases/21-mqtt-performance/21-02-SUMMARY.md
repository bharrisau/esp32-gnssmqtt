---
phase: 21-mqtt-performance
plan: 02
subsystem: mqtt
tags: [mqtt, rust, esp-idf, channel, atomics, nmea, log, heartbeat]

# Dependency graph
requires:
  - phase: 21-mqtt-performance
    plan: 01
    provides: "MqttMessage enum, SyncSender channel, MQTT_ENQUEUE_ERRORS and MQTT_OUTBOX_DROPS atomics"

provides:
  - "nmea_relay::spawn_relay migrated to SyncSender<MqttMessage> + Arc<str> consolidated topic"
  - "All NMEA published to single gnss/{id}/nmea topic (topic consolidation)"
  - "log_relay::spawn_log_relay migrated to SyncSender<MqttMessage> + Arc<str> topic"
  - "Log relay thread no longer sets LOG_REENTERING — publish_thread owns the guard"
  - "mqtt::heartbeat_loop migrated to SyncSender<MqttMessage> + two Arc<str> topics"
  - "Heartbeat JSON includes mqtt_enqueue_errors and mqtt_outbox_drops fields"
  - "EventPayload::Deleted arm increments MQTT_OUTBOX_DROPS on outbox expiry"

affects: [21-mqtt-performance]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pre-built Arc<str> topics passed into relay threads — zero allocation on hot path (no per-message format!())"
    - "try_send on channel full: increment drop counter and continue (non-blocking relay semantics)"
    - "Log relay thread is channel-only — no MQTT calls, no re-entrancy management"

key-files:
  created: []
  modified:
    - src/nmea_relay.rs
    - src/log_relay.rs
    - src/mqtt.rs

key-decisions:
  - "NMEA topic consolidated: all sentence types to single gnss/{id}/nmea — sentence type visible from payload prefix, zero information loss"
  - "Log relay silent drop on TrySendError::Full preserved (LOG-03 non-blocking contract)"
  - "LOG_REENTERING.store() removed from log relay loop — publish_thread is sole owner of guard (prevents double-toggle if relay and publish thread both set it)"
  - "EventPayload::Deleted arm added before catch-all warn arm — increments MQTT_OUTBOX_DROPS for CONFIG_MQTT_REPORT_DELETED_MESSAGES telemetry"

patterns-established:
  - "TrySendError::Full in relay threads → increment domain drop counter (NMEA_DROPS) and continue"
  - "TrySendError::Disconnected in relay threads → log error and break loop (publish thread exited)"

requirements-completed: [PERF-21-1, PERF-21-2, PERF-21-4, OBS-21-1]

# Metrics
duration: 10min
completed: 2026-03-11
---

# Phase 21 Plan 02: Relay Migration Summary

**NMEA/log/heartbeat relay threads migrated from Arc<Mutex<EspMqttClient>> to SyncSender<MqttMessage> with NMEA topic consolidation and heartbeat MQTT observability counters**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-03-11T19:15:00Z
- **Completed:** 2026-03-11T19:20:33Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments

- Migrated `nmea_relay::spawn_relay` to `SyncSender<MqttMessage>` + `Arc<str>` consolidated topic; all sentence types now publish to `gnss/{id}/nmea` (single topic, zero-alloc on hot path at 40 msg/s)
- Migrated `log_relay::spawn_log_relay` to `SyncSender<MqttMessage>`; removed `LOG_REENTERING` management from relay loop (publish_thread is sole guard owner per Plan 01 architecture)
- Migrated `mqtt::heartbeat_loop` to `SyncSender<MqttMessage>` with two pre-built `Arc<str>` topics; extended heartbeat JSON with `mqtt_enqueue_errors` and `mqtt_outbox_drops` fields; added `EventPayload::Deleted` arm to `mqtt_connect`

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate nmea_relay.rs to SyncSender and consolidate NMEA topic** - `ded3f4f` (feat)
2. **Task 2: Migrate log_relay.rs — make LOG_REENTERING pub, update spawn_log_relay to SyncSender** - `49f7fde` (feat)
3. **Task 3: Migrate mqtt.rs heartbeat_loop to SyncSender and add MQTT counter fields** - `7eba32f` (feat)

## Files Created/Modified

- `src/nmea_relay.rs` - spawn_relay now takes SyncSender<MqttMessage> + Arc<str> nmea_topic; topic consolidated to gnss/{id}/nmea; TrySendError::Full increments NMEA_DROPS
- `src/log_relay.rs` - spawn_log_relay now takes SyncSender<MqttMessage> + Arc<str> log_topic; relay loop no longer sets LOG_REENTERING; silent drop on channel full (LOG-03)
- `src/mqtt.rs` - heartbeat_loop takes SyncSender + two Arc<str> topics; heartbeat JSON gains mqtt_enqueue_errors/mqtt_outbox_drops fields; EventPayload::Deleted increments MQTT_OUTBOX_DROPS

## Decisions Made

- NMEA topic consolidation: eliminated per-sentence `format!("gnss/{}/nmea/{}", device_id, sentence_type)`. Sentence type remains visible from payload prefix (`$GNGGA`, `$GNRMC`, etc.). Breaking change for per-type subscribers; noted in plan as expected.
- `LOG_REENTERING` management removed from log relay loop. The relay now only sends to the channel; the publish_thread sets/clears the guard when dispatching `MqttMessage::Log`. This avoids a double-toggle edge case if both threads were managing the guard.
- `TrySendError::Full` in nmea_relay increments `crate::gnss::NMEA_DROPS` (consistent with existing drop counter semantics for the NMEA pipeline).
- `EventPayload::Deleted` arm placed before the `m => log::warn!(...)` catch-all so expired outbox messages increment `MQTT_OUTBOX_DROPS` without generating a warn log.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused TrySendError import from log_relay.rs**
- **Found during:** Task 2 verification (clippy run)
- **Issue:** Imported `TrySendError` in the use statement but the relay loop uses `let _ = try_send(...)` which discards the error silently — `TrySendError` was not referenced in the match arms. Clippy flagged `unused import: TrySendError` as a `-D warnings` error.
- **Fix:** Removed `TrySendError` from the import line.
- **Files modified:** `src/log_relay.rs`
- **Verification:** `cargo clippy -- -D warnings` passes with no warnings in the modified files.
- **Committed in:** `49f7fde` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — unused import caught by clippy -D warnings)
**Impact on plan:** Minor cleanup. No scope creep.

## Issues Encountered

None beyond the auto-fixed item above.

## Next Phase Readiness

- All three hot-path relay threads now use `SyncSender<MqttMessage>` — mutex contention on the 40 msg/s NMEA path is eliminated
- main.rs call sites for `spawn_relay`, `spawn_log_relay`, and `heartbeat_loop` still pass `Arc<Mutex<EspMqttClient>>` (3 compile errors in main.rs) — these are expected and will be fixed in Plan 03
- Plan 03 will wire `publish_thread` into main.rs, fix all call sites, and remove the remaining `Arc<Mutex<EspMqttClient>>` references (RTCM relay + subscriber_loop)

## Self-Check: PASSED

- src/nmea_relay.rs: FOUND
- src/log_relay.rs: FOUND
- src/mqtt.rs: FOUND
- 21-02-SUMMARY.md: FOUND
- Commit ded3f4f (nmea_relay migration): FOUND
- Commit 49f7fde (log_relay migration): FOUND
- Commit 7eba32f (heartbeat_loop migration): FOUND

---
*Phase: 21-mqtt-performance*
*Completed: 2026-03-11*
