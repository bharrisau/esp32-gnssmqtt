---
phase: 09-channel-loop-hardening
plan: "01"
subsystem: infra
tags: [mpsc, sync_channel, bounded-channels, uart, atomic, embedded]

# Dependency graph
requires:
  - phase: 08-ota
    provides: OTA task wired with ota_rx channel; full channel topology established

provides:
  - All four unbounded mpsc::channel() calls converted to sync_channel with documented capacities
  - UART TX error counter (UART_TX_ERRORS AtomicU32) with warn logging on every write failure
  - SyncSender<String> interface for GNSS command channel (gnss.rs, config_relay.rs, uart_bridge.rs)
  - try_send() non-blocking sends in pump_mqtt_events and uart_bridge to prevent hot-path blocking

affects:
  - 09-02 (recv_timeout conversion uses same channel handles)
  - 13-health-telemetry (UART_TX_ERRORS counter will be read for status payload)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Bounded mpsc::sync_channel with capacity rationale in comments (prevents OOM on embedded)
    - try_send() in hot-path threads (pump, bridge); blocking send() acceptable in config_relay (not hot-path)
    - AtomicU32 error counter with fetch_add for lock-free telemetry accumulation

key-files:
  created: []
  modified:
    - src/gnss.rs
    - src/config_relay.rs
    - src/uart_bridge.rs
    - src/main.rs
    - src/mqtt.rs

key-decisions:
  - "cmd_tx sync_channel(16): config batch is typically <= 16 commands at 100ms delay; capacity 16 prevents config_relay blocking UART TX drain"
  - "subscribe_tx sync_channel(2): at most one Connected event can queue while subscriber processes previous; beyond 2 is impossible"
  - "config_tx sync_channel(4): covers retained message on reconnect plus small burst; pump uses try_send so never blocks connection.next()"
  - "ota_tx sync_channel(1): at most one OTA operation queued; second trigger while OTA is running is dropped to prevent double-flash"
  - "config_relay.apply_config() keeps blocking send() into cmd_tx: not a hot-path thread, blocking on a full 16-slot channel during large batches is acceptable"
  - "uart_bridge uses try_send: user-interactive path, dropping a command when channel full is preferable to blocking the stdin read loop"

patterns-established:
  - "Bounded channel pattern: always use sync_channel with documented capacity rationale comment"
  - "Hot-path threads (pump, bridge) use try_send() and warn-log drops; non-hot-path threads (config_relay) may use blocking send()"
  - "UART write error accumulation via AtomicU32 + Relaxed fetch_add; warn log includes cumulative count for operator diagnosis"

requirements-completed: [HARD-01, HARD-02]

# Metrics
duration: 4min
completed: 2026-03-07
---

# Phase 9 Plan 01: Channel Bounding + UART TX Error Logging Summary

**All four unbounded mpsc::channel() calls replaced with sync_channel bounded variants (2/4/16/1 slots), UART TX write errors now logged with AtomicU32 cumulative counter**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-07T10:05:46Z
- **Completed:** 2026-03-07T10:10:36Z
- **Tasks:** 5 (gnss.rs, config_relay.rs, uart_bridge.rs, main.rs, mqtt.rs)
- **Files modified:** 5

## Accomplishments

- Converted all unbounded channels to bounded sync_channel with capacity rationale comments — eliminates OOM risk from slow consumers on embedded heap
- Added UART_TX_ERRORS AtomicU32 counter in gnss.rs TX thread; every uart_tx.write() error logged with incrementing count for operator visibility
- Changed pump_mqtt_events to use try_send() on all three channels — pump no longer risks blocking connection.next() if a consumer is slow
- Changed uart_bridge to use try_send() with warn-log drop — interactive stdin path never stalls on a full command channel

## Task Commits

All changes committed atomically in a single logical commit:

1. **All 5 files** — `73269e0` (feat: convert all unbounded channels to sync_channel; log UART TX errors)

## Files Created/Modified

- `src/gnss.rs` — cmd_tx/cmd_rx changed from channel() to sync_channel(16); return type updated to SyncSender<String>; UART_TX_ERRORS AtomicU32 static added; TX loop now logs write failures with counter
- `src/config_relay.rs` — gnss_cmd_tx parameter and apply_config() signature updated from Sender<String> to SyncSender<String>
- `src/uart_bridge.rs` — cmd_tx parameter updated to SyncSender<String>; blocking send() replaced with try_send() + match on Full/Disconnected
- `src/main.rs` — subscribe_tx sync_channel(2), config_tx sync_channel(4), ota_tx sync_channel(1); capacity comments explain rationale
- `src/mqtt.rs` — pump_mqtt_events signature updated to SyncSender variants; all send() calls changed to try_send() with TrySendError::Full and TrySendError::Disconnected handling

## Decisions Made

- sync_channel(16) for cmd_tx: config batch typically <=16 commands with 100ms inter-command delay; larger capacity unnecessary and wastes heap
- sync_channel(2) for subscribe_tx: only two states (idle subscriber, queued subscriber); capacity beyond 2 physically impossible
- sync_channel(4) for config_tx: rare operator-triggered config; 4 covers retained-on-reconnect scenario with small burst headroom
- sync_channel(1) for ota_tx: OTA is mutually exclusive — queuing more than 1 trigger causes double-flash hazard; drop is correct behavior
- config_relay.apply_config() retains blocking send(): config_relay is not a hot-path thread; blocking during a large 16-command batch is acceptable and simpler than try_send retry logic

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None — build succeeded on first attempt with only a pre-existing unused import warning in ota.rs (out of scope).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- All bounded channel infrastructure in place; 09-02 can convert Receiver loops to recv_timeout() without any channel type changes
- UART_TX_ERRORS counter ready for Phase 13 health telemetry reads
- No blockers

---
*Phase: 09-channel-loop-hardening*
*Completed: 2026-03-07*
