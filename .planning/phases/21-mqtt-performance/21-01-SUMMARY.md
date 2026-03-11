---
phase: 21-mqtt-performance
plan: 01
subsystem: mqtt
tags: [mqtt, bytes, esp-idf, rust, atomics, thread, channel]

# Dependency graph
requires:
  - phase: 20-field-testing-fixes
    provides: stable firmware base with MQTT Arc<Mutex<EspMqttClient>> pattern to replace

provides:
  - "pub enum MqttMessage with 6 variants (Nmea, Rtcm, Log, Heartbeat, Status, Bench)"
  - "pub fn publish_thread owning EspMqttClient<'static> exclusively"
  - "pub MQTT_ENQUEUE_ERRORS and MQTT_OUTBOX_DROPS AtomicU32 observability counters"
  - "bytes = 1 crate dependency for zero-copy RTCM3 payload transfer"
  - "LOG_REENTERING made pub in log_relay for re-entrancy guard access"

affects: [21-mqtt-performance]

# Tech tracking
tech-stack:
  added: [bytes = "1"]
  patterns:
    - "Typed channel enum (MqttMessage) for single-owner MQTT publish thread"
    - "Re-entrancy guard applied only to Log variant — not NMEA/RTCM to avoid 40 msg/s suppression"

key-files:
  created: [src/mqtt_publish.rs]
  modified: [Cargo.toml, src/main.rs, src/log_relay.rs]

key-decisions:
  - "LOG_REENTERING made pub in Plan 21-01 (not deferred to Plan 02) — required for mqtt_publish.rs to compile and clippy to pass"
  - "MqttMessage::Rtcm uses bytes::Bytes for zero-copy RTCM3 payload (avoids Vec<u8> clone from pool buffer)"
  - "MQTT_OUTBOX_DROPS incremented alongside MQTT_ENQUEUE_ERRORS on any enqueue error — conservative over-count until finer error classification available in later plans"
  - "dispatch() extracted as private fn to keep publish_thread loop readable and avoid deep match nesting"

patterns-established:
  - "if result.is_err() pattern for enqueue error detection (not if let Err(_) = — clippy redundant_pattern_matching)"

requirements-completed: [PERF-21-1, PERF-21-2, OBS-21-1]

# Metrics
duration: 3min
completed: 2026-03-11
---

# Phase 21 Plan 01: MQTT Publish Foundation Summary

**Typed MqttMessage enum and exclusive-ownership publish_thread with observability atomics, plus bytes crate for zero-copy RTCM3 payloads**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-11T19:10:00Z
- **Completed:** 2026-03-11T19:13:00Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Created `src/mqtt_publish.rs` with `MqttMessage` enum (6 variants), `publish_thread` fn, and `MQTT_ENQUEUE_ERRORS`/`MQTT_OUTBOX_DROPS` AtomicU32 counters
- Added `bytes = "1"` crate dependency for zero-copy RTCM3 buffer transfer via `bytes::Bytes`
- Made `LOG_REENTERING` pub in `log_relay.rs` so `publish_thread` can set the re-entrancy guard only on `Log` variant enqueues
- Module wired to `main.rs` via `mod mqtt_publish` for compile-time verification; not yet active (Plan 03 wires it in)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add bytes crate to Cargo.toml** - `7793cbf` (chore)
2. **Task 2: Create src/mqtt_publish.rs** - `52b5788` (feat)

## Files Created/Modified

- `src/mqtt_publish.rs` - New module: MqttMessage enum, publish_thread, MQTT_ENQUEUE_ERRORS, MQTT_OUTBOX_DROPS
- `Cargo.toml` - Added `bytes = "1"` dependency
- `src/main.rs` - Added `mod mqtt_publish` declaration for compile-time checking
- `src/log_relay.rs` - Made `LOG_REENTERING` pub (required by re-entrancy guard in mqtt_publish)

## Decisions Made

- LOG_REENTERING made pub in Plan 21-01 rather than waiting for Plan 02 — the module needed it to compile and clippy clean. The TODO comment originally planned for Plan 02 was removed and replaced with a note that it was done here.
- `dispatch()` extracted as a private fn to avoid deep match nesting inside the publish loop.
- `MQTT_OUTBOX_DROPS` incremented conservatively alongside every enqueue error; finer classification (outbox-full vs connection-down) deferred to a later plan once error codes are available.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Made LOG_REENTERING pub ahead of Plan 02**
- **Found during:** Task 2 (Create mqtt_publish.rs)
- **Issue:** `mqtt_publish.rs` references `crate::log_relay::LOG_REENTERING` for the Log variant re-entrancy guard. The static was `static` (private), so the module failed to compile.
- **Fix:** Changed `static LOG_REENTERING` to `pub static LOG_REENTERING` in `log_relay.rs`. Plan 02 was described as the place to do this, but the compile dependency was immediate.
- **Files modified:** `src/log_relay.rs`
- **Verification:** `cargo clippy -- -D warnings` passes clean.
- **Committed in:** `52b5788` (Task 2 commit)

**2. [Rule 1 - Bug] Fixed clippy redundant_pattern_matching in dispatch()**
- **Found during:** Task 2 verification (clippy run)
- **Issue:** Clippy flagged 6 occurrences of `if let Err(_) = expr` — should use `.is_err()`.
- **Fix:** Rewrote all enqueue error checks as `if expr.is_err()`.
- **Files modified:** `src/mqtt_publish.rs`
- **Verification:** `cargo clippy -- -D warnings` passes with zero warnings.
- **Committed in:** `52b5788` (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 1 — correctness/compile bugs)
**Impact on plan:** Both fixes necessary for clean compilation and `-D warnings` compliance. No scope creep.

## Issues Encountered

None beyond the auto-fixed items above.

## Next Phase Readiness

- Foundation complete: `MqttMessage` type and `publish_thread` exist and compile
- Plan 02 can proceed immediately: migrate NMEA/RTCM/Log relay threads to send `MqttMessage` over the channel
- Plan 03 can wire `publish_thread` into `main.rs` and remove the existing `Arc<Mutex<EspMqttClient>>` pattern
- `LOG_REENTERING` pub visibility is already done — Plan 02 may skip that step

## Self-Check: PASSED

- src/mqtt_publish.rs: FOUND
- 21-01-SUMMARY.md: FOUND
- Commit 7793cbf (bytes dependency): FOUND
- Commit 52b5788 (mqtt_publish module): FOUND

---
*Phase: 21-mqtt-performance*
*Completed: 2026-03-11*
