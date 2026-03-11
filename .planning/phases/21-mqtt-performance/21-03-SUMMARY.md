---
phase: 21-mqtt-performance
plan: 03
subsystem: mqtt
tags: [mqtt, rust, esp-idf, channel, bytes, ota, rtcm, arch-refactor]

# Dependency graph
requires:
  - phase: 21-mqtt-performance
    plan: 01
    provides: "MqttMessage enum, publish_thread, SyncSender channel, MQTT_ENQUEUE_ERRORS/MQTT_OUTBOX_DROPS atomics"
  - phase: 21-mqtt-performance
    plan: 02
    provides: "nmea_relay/log_relay/heartbeat_loop migrated to SyncSender; mqtt_connect with EventPayload::Deleted"

provides:
  - "rtcm_relay::spawn_relay uses bytes::BytesMut + split().freeze() for zero-copy RTCM handoff to publish channel"
  - "All RTCM published to single gnss/{id}/rtcm topic (consolidated from per-message-type subtopics)"
  - "ota::spawn_ota uses SyncSender<MqttMessage> — Arc<Mutex<EspMqttClient>> fully removed from ota.rs"
  - "bench:N OTA trigger sends N messages to gnss/{id}/bench and logs sent/dropped/elapsed summary"
  - "mqtt_connect returns (EspMqttClient, SyncSender<MqttMessage>, Receiver<MqttMessage>) — no Arc/Mutex in return"
  - "subscriber_loop uses SyncSender<MqttMessage> with Subscribe variant; publish_thread calls client.subscribe()"
  - "main.rs wires all relay threads with SyncSender<MqttMessage> and spawns dedicated publish_thread"
  - "main.rs builds all Arc<str> topics at startup — zero allocation on hot path"
  - "Arc<Mutex<EspMqttClient>> absent from src/ (only in doc comments)"
  - "CONFIG_MQTT_REPORT_DELETED_MESSAGES=y in sdkconfig.defaults"

affects: [21-mqtt-performance]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "bytes::BytesMut + split().freeze() for zero-copy buffer handoff — pool buffer returned before try_send prevents leak"
    - "Subscribe variant in MqttMessage enum delegates client.subscribe() to publish_thread (exclusive client owner)"
    - "Pre-built Arc<str> topics at main() startup — all relay threads receive cloned Arc at thread creation"
    - "OTA publish_status uses Heartbeat variant (Vec<u8>) for dynamic JSON strings (Status variant is &'static [u8] only)"

key-files:
  created: []
  modified:
    - src/rtcm_relay.rs
    - src/ota.rs
    - src/mqtt.rs
    - src/mqtt_publish.rs
    - src/main.rs
    - sdkconfig.defaults

key-decisions:
  - "RTCM topic consolidated: all message types to single gnss/{id}/rtcm — message type in binary frame header, downstream consumers parse it"
  - "Subscribe variant added to MqttMessage: subscriber_loop sends Subscribe{topic,qos,signal} to publish_thread which calls client.subscribe() — cleaner than Arc/Mutex for the infrequent subscribe operation"
  - "OTA publish_status uses Heartbeat variant not Status: Status uses &'static [u8] (unsuitable for dynamically formatted JSON); Heartbeat takes Vec<u8> and shares QoS 0/retain=false semantics"
  - "SyncSender unused import removed from main.rs (clippy -D warnings auto-fix): type inference handles mqtt_tx binding without explicit import"
  - "client.subscribe() returns Result<u32, _> not Result<(), _>: Subscribe signal type is Result<(), String>, fixed with .map(|_| ())"

patterns-established:
  - "Publish thread as exclusive EspMqttClient owner: all relay threads use SyncSender; subscribe routed via Subscribe variant"
  - "Pool buffer returned before try_send in RTCM relay: no leak possible even if channel full or closed"

requirements-completed: [PERF-21-1, PERF-21-2, PERF-21-3, PERF-21-4, OBS-21-2, DIAG-21-1]

# Metrics
duration: 30min
completed: 2026-03-12
---

# Phase 21 Plan 03: Final Wiring Summary

**Arc<Mutex<EspMqttClient>> fully eliminated from src/ — publish_thread exclusively owns EspMqttClient; RTCM relay uses bytes zero-copy; OTA and subscriber_loop migrated; all relay threads wired via SyncSender in main.rs**

## Performance

- **Duration:** ~30 min
- **Started:** 2026-03-12T03:25:00Z
- **Completed:** 2026-03-12T04:00:00Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- Migrated `rtcm_relay::spawn_relay` to `SyncSender<MqttMessage>` + `Arc<str>` topic + `bytes::BytesMut` zero-copy buffer; pool buffer returned before try_send (no leak on channel full); RTCM topic consolidated to single `gnss/{id}/rtcm`
- Migrated `ota::spawn_ota` to `SyncSender<MqttMessage>` with pre-built `Arc<str>` topics; added `bench:N` trigger that sends N messages to bench topic and logs sent/dropped/elapsed; OTA status uses Heartbeat variant for dynamic JSON strings
- Updated `mqtt_connect` to return `(EspMqttClient, SyncSender, Receiver)` 3-tuple; added `Subscribe` variant to `MqttMessage`; `subscriber_loop` now routes subscribe calls via publish_thread (no direct EspMqttClient access); `main.rs` pre-builds all Arc<str> topics at startup and wires all relay threads with `mqtt_tx.clone()`; `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` added to sdkconfig.defaults

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate rtcm_relay.rs to bytes crate and SyncSender** - `479a7bf` (feat)
2. **Task 2: Migrate ota.rs to SyncSender and add bench:N trigger** - `dd43b53` (feat)
3. **Task 3: Update mqtt_connect, subscriber_loop, main.rs wiring, sdkconfig** - `27740d0` (feat)

## Files Created/Modified

- `src/rtcm_relay.rs` - spawn_relay takes SyncSender<MqttMessage> + Arc<str>; BytesMut zero-copy; single rtcm topic; pool buffer returned before try_send
- `src/ota.rs` - spawn_ota takes SyncSender + 3x Arc<str> topics + device_id; publish_status via Heartbeat variant; bench:N trigger; retained trigger clear via Status variant
- `src/mqtt.rs` - mqtt_connect returns (EspMqttClient, SyncSender, Receiver) 3-tuple; subscriber_loop takes SyncSender; removed Arc<Mutex> imports
- `src/mqtt_publish.rs` - Subscribe variant added to MqttMessage enum; dispatch() handles Subscribe by calling client.subscribe() and signaling result; .map(|_| ()) for u32->() conversion
- `src/main.rs` - pre-builds 8 Arc<str> topics at startup; spawns publish_thread with exclusive EspMqttClient; all relay threads receive mqtt_tx.clone(); removed SyncSender unused import
- `sdkconfig.defaults` - CONFIG_MQTT_REPORT_DELETED_MESSAGES=y for EventPayload::Deleted telemetry

## Decisions Made

- **Subscribe variant pattern**: subscriber_loop sends `MqttMessage::Subscribe{topic, qos, signal}` to publish_thread which calls `client.subscribe()` and sends the result back via a one-shot signal channel. Subscriber blocks with 5s timeout. This keeps publish_thread as the sole EspMqttClient accessor without needing Arc/Mutex for the infrequent subscribe operation.
- **OTA publish_status via Heartbeat variant**: `MqttMessage::Status` uses `&'static [u8]` (only works for compile-time constants like `b"online"`). OTA status strings are dynamically formatted (`{"state":"downloading","progress":65536}`), so `Heartbeat` variant (takes `Vec<u8>`) is used. Same QoS 0/retain=false semantics.
- **RTCM pool buffer returned before try_send**: Buffer is returned to the GNSS RX pool immediately after copying into BytesMut, before calling try_send. If the channel is full or closed, the frame is dropped but the pool buffer is safely returned — no memory leak possible.
- **client.subscribe() returns u32**: The ESP-IDF MQTT subscribe returns `Result<u32, _>` (message ID), not `Result<(), _>`. Fixed by adding `.map(|_| ())` before sending through the signal channel.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused SyncSender import from main.rs**
- **Found during:** Task 3 verification (clippy -D warnings run)
- **Issue:** Added `use std::sync::mpsc::SyncSender;` import to main.rs but it was not referenced explicitly in the code (type inference handles `mqtt_tx` binding without it). Clippy flagged `unused import` as `-D warnings` error.
- **Fix:** Removed the import line. All usages are inferred from context.
- **Files modified:** `src/main.rs`
- **Verification:** `cargo clippy -- -D warnings` passes with no warnings.
- **Committed in:** `27740d0` (Task 3 commit)

**2. [Rule 1 - Bug] Fixed Subscribe signal channel type mismatch**
- **Found during:** Task 3 verification (clippy -D warnings run)
- **Issue:** `client.subscribe()` returns `Result<u32, _>` (MQTT message ID) but the `Subscribe` signal channel was typed as `SyncSender<Result<(), String>>`. Attempting to send the raw subscribe result produced a type mismatch error.
- **Fix:** Added `.map(|_| ())` to convert `Result<u32, E>` to `Result<(), E>` before sending through the signal channel.
- **Files modified:** `src/mqtt_publish.rs`
- **Verification:** `cargo clippy -- -D warnings` and `cargo build` both pass.
- **Committed in:** `27740d0` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (Rule 1 — unused import + type mismatch caught by clippy -D warnings)
**Impact on plan:** Minor fixes. No scope creep. Both fixed within Task 3 commit.

## Issues Encountered

None beyond the auto-fixed items above. `cargo build` (full cross-compile for riscv32imac-esp-espidf) succeeded on first attempt after auto-fixes.

## Next Phase Readiness

- Phase 21 refactor complete: `Arc<Mutex<EspMqttClient>>` is absent from all of `src/` (confirmed via grep returning only doc comment matches)
- `publish_thread` is the sole caller of `EspMqttClient::enqueue()` and `EspMqttClient::subscribe()`
- All relay threads (NMEA, RTCM, log, heartbeat, subscriber, OTA) use `SyncSender<MqttMessage>`
- `bench:N` benchmark tool available via MQTT OTA trigger topic for performance measurement
- `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` enables outbox deletion events in the MQTT callback
- Hardware validation (device FFFEB5) remains pending per `testing.md`

## Self-Check: PASSED

- src/rtcm_relay.rs: FOUND
- src/ota.rs: FOUND
- src/mqtt.rs: FOUND
- src/mqtt_publish.rs: FOUND
- src/main.rs: FOUND
- sdkconfig.defaults: FOUND
- 21-03-SUMMARY.md: FOUND (this file)
- Commit 479a7bf (rtcm_relay migration): FOUND
- Commit dd43b53 (ota migration): FOUND
- Commit 27740d0 (mqtt_connect/subscriber/main wiring): FOUND

---
*Phase: 21-mqtt-performance*
*Completed: 2026-03-12*
