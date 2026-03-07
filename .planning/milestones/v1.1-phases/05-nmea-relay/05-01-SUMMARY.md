---
phase: 05-nmea-relay
plan: 01
subsystem: gnss
tags: [rust, mqtt, nmea, mpsc, sync_channel, embedded, esp32]

# Dependency graph
requires:
  - phase: 04-uart-pipeline
    provides: gnss::spawn_gnss returning Receiver<(String,String)> and Sender<String>
provides:
  - Bounded NMEA relay channel (sync_channel 64) in gnss.rs with try_send drop semantics
  - New nmea_relay.rs module with pub spawn_relay() consuming Receiver to MQTT publish
affects:
  - 05-02 (main.rs wiring — adds mod nmea_relay and calls spawn_relay)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - mpsc::sync_channel(64) for bounded drop-on-full NMEA relay backpressure
    - TrySendError::Full/Disconnected pattern for non-blocking NMEA forwarding
    - Arc<Mutex<EspMqttClient>> per-sentence lock acquisition — released each iteration

key-files:
  created:
    - src/nmea_relay.rs
  modified:
    - src/gnss.rs

key-decisions:
  - "sync_channel(64) chosen over unbounded channel — RX thread must not block on UART reads when relay is slow"
  - "try_send with TrySendError::Full WARN chosen over silent drop — visibility into backpressure events"
  - "SyncSender not added to gnss.rs public API — Receiver<T> type unchanged from channel() to sync_channel()"
  - "QoS::AtMostOnce (QoS 0) for NMEA relay — real-time sentences, retransmission is harmful"
  - "retain=false for NMEA relay — consumers want current sentences, not cached stale positions"

patterns-established:
  - "NMEA relay: for (type, raw) in &nmea_rx — blocking iteration, exits when all SyncSenders dropped"
  - "Mutex acquired and released per sentence — prevents heartbeat/subscriber thread starvation"

requirements-completed:
  - NMEA-01
  - NMEA-02

# Metrics
duration: 2min
completed: 2026-03-07
---

# Phase 5 Plan 01: NMEA Relay Foundation Summary

**Bounded sync_channel(64) in gnss.rs with try_send drop semantics, plus nmea_relay.rs spawn_relay() that drains Receiver to MQTT enqueue() at QoS 0**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-06T22:40:24Z
- **Completed:** 2026-03-06T22:42:37Z
- **Tasks:** 2
- **Files modified:** 2 (1 modified, 1 created)

## Accomplishments

- Switched gnss.rs from unbounded `mpsc::channel()` to `mpsc::sync_channel(64)` — RX thread can now drop NMEA sentences without blocking UART reads when the relay is slow
- Replaced silent `let _ = nmea_tx.send()` with `try_send` match that warns on Full and errors on Disconnected — backpressure events are now visible in logs
- Created src/nmea_relay.rs with `pub fn spawn_relay()` implementing the NMEA-01 relay: drains `Receiver<(String,String)>`, publishes each sentence to `gnss/{device_id}/nmea/{sentence_type}` via `enqueue()` at QoS 0

## Task Commits

Each task was committed atomically:

1. **Task 1: Switch gnss.rs to sync_channel(64) with try_send drop semantics** - `72e3e00` (feat)
2. **Task 2: Create src/nmea_relay.rs — spawn_relay() consuming Receiver to MQTT publish** - `beb279d` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/gnss.rs` - Changed channel creation to sync_channel(64), added TrySendError import, updated send site to try_send match
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/nmea_relay.rs` - New module: spawn_relay() thread draining Receiver to MQTT enqueue

## Decisions Made

- `sync_channel(64)` over unbounded: UART RX thread must never block waiting for the relay consumer; 64 slots provides ~6 seconds of buffer at 10 Hz NMEA rate before dropping
- `try_send` with logged errors: silent drops would hide backpressure; Full warns, Disconnected errors for severity distinction
- `SyncSender` not exposed in gnss.rs return type: `Receiver<T>` is identical from both `channel()` and `sync_channel()` — public API unchanged, Plan 02 wiring unaffected
- QoS 0 / retain=false for relay: NMEA is real-time telemetry; retransmission of stale positions is worse than loss
- Mutex acquired per-sentence in relay thread: holding across iterations would block heartbeat_loop for the full duration of sentence processing at 10+ Hz

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed unused SyncSender import**
- **Found during:** Task 1 (gnss.rs sync_channel change)
- **Issue:** Plan spec included `SyncSender` in the import but the variable `nmea_tx` is inferred as `SyncSender` without naming the type — explicit import would trigger unused import warning
- **Fix:** Omitted `SyncSender` from the import; kept `TrySendError` which is explicitly named in the match arms
- **Files modified:** src/gnss.rs
- **Verification:** cargo build produces zero error lines
- **Committed in:** 72e3e00 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - unused import cleanup)
**Impact on plan:** Trivial — import spec in plan was overly precise; actual Rust semantics don't require naming inferred types in imports. No behavior change.

## Issues Encountered

- Plan verification command used incorrect build target `riscv32imc-esp-espidf` (missing 'a'). Correct target per `.cargo/config.toml` is `riscv32imac-esp-espidf`. Build succeeded with correct target — zero Rust compiler errors.

## Next Phase Readiness

- Both artifacts ready for Plan 02 wiring: `mod nmea_relay` declaration and `spawn_relay()` call in main.rs
- `_nmea_rx` binding in main.rs idle loop is the handoff point (documented in Phase 04 decisions)
- No blockers

---
*Phase: 05-nmea-relay*
*Completed: 2026-03-07*
