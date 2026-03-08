---
phase: 16-remote-logging
plan: 02
subsystem: logging
tags: [esp-idf, mqtt, log-level, ffi, sync-channel, runtime-config]

# Dependency graph
requires:
  - phase: 16-01
    provides: spawn_log_relay, install_mqtt_log_hook, log_relay module
  - phase: 14-quick-additions
    provides: SLOW_RECV_TIMEOUT constant, command_relay_task pattern
provides:
  - Full LOG-01/02/03 pipeline wired and active in firmware
  - Runtime log level control via gnss/{device_id}/log/level MQTT topic
  - log_level_relay_task thread applying EspLogger level changes
  - install_mqtt_log_hook called at boot (Step 2b) — captures all ESP-IDF log output
affects: [firmware-operations, observability, debugging]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - mqtt_connect extended with SyncSender parameter following existing cmd_relay_tx pattern
    - subscriber_loop extended with additional subscription following existing pattern
    - log_level_relay_task mirrors command_relay_task (recv_timeout loop, HWM at entry, park on close)
    - esp_idf_svc::log::set_target_level("*", filter) for global runtime log level change

key-files:
  created: []
  modified:
    - src/mqtt.rs
    - src/main.rs

key-decisions:
  - "Used esp_idf_svc::log::set_target_level() (free function) instead of EspLogger instance — EspLogger has a cache field and is not a zero-sized Copy type as the plan interface comment implied"
  - "Task 1 (mqtt.rs) and Task 2 (main.rs) committed separately even though build only passes after both — mqtt.rs is structurally correct in isolation; main.rs argument error is a wiring issue resolved in Task 2"

patterns-established:
  - "SyncSender channel extension pattern: add parameter to mqtt_connect after cmd_relay_tx, capture by move in callback, route in Received branch with try_send + silent drop on Full"
  - "subscriber_loop subscription extension: add inside Ok(mut c) block after existing subscriptions"

requirements-completed: [LOG-01, LOG-02, LOG-03]

# Metrics
duration: 2min
completed: 2026-03-08
---

# Phase 16 Plan 02: Remote Logging Wiring Summary

**mqtt_connect extended with log_level_tx, install_mqtt_log_hook wired at Step 2b, spawn_log_relay and log_level_relay_task spawned after MQTT connects — full LOG-01/02/03 pipeline live**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-08T03:01:43Z
- **Completed:** 2026-03-08T03:04:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `install_mqtt_log_hook()` called immediately after `EspLogger::initialize_default()` — all ESP-IDF log output captured from boot
- `log_level_tx` channel wired through mqtt_connect; `/log/level` subscribed at QoS::AtLeastOnce in subscriber_loop
- `apply_log_level` + `log_level_relay_task` parse payload strings ("error"/"warn"/"info"/"debug"/"verbose") and apply via `esp_idf_svc::log::set_target_level`
- `spawn_log_relay` and `log_level_relay_task` thread spawned in correct order (Steps 9.5, 9.6 — after mqtt_connect, before subscriber thread)
- `cargo build --release` passes with zero errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend mqtt.rs with log/level channel, subscription, and apply_log_level** - `628cb65` (feat)
2. **Task 2: Wire log relay into main.rs startup sequence** - `b2ff364` (feat)

## Files Created/Modified
- `src/mqtt.rs` - Added log_level_tx parameter, /log/level callback routing, /log/level subscription, apply_log_level, log_level_relay_task
- `src/main.rs` - Step 2b hook install, log_level channel creation, mqtt_connect call updated, Steps 9.5/9.6 spawn calls, startup order comments

## Decisions Made
- Used `esp_idf_svc::log::set_target_level()` free function rather than `EspLogger` struct instance. The plan's interface comment said "EspLogger is a zero-sized Copy struct" but the actual type has a `Mutex<BTreeMap>` cache field and cannot be constructed as a bare value expression. The module-level free function `esp_idf_svc::log::set_target_level` achieves the same effect cleanly.
- Tasks committed separately: mqtt.rs Task 1 is structurally complete and correct; the build error from Task 1's state is a missing argument in main.rs that Task 2 resolves. Separate commits preserve the atomic-task property.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Used set_target_level free function instead of EspLogger instance**
- **Found during:** Task 1 (apply_log_level implementation)
- **Issue:** Plan interface comment states "EspLogger is a zero-sized Copy struct; instantiate locally" but `EspLogger { cache: Mutex<BTreeMap<...>> }` is not zero-sized and `EspLogger;` is not a valid value expression (E0423)
- **Fix:** Used `esp_idf_svc::log::set_target_level("*", filter)` module-level free function that delegates to the global `LOGGER` instance
- **Files modified:** src/mqtt.rs
- **Verification:** `cargo build --release` passes; `use esp_idf_svc::log::EspLogger` import removed as unused
- **Committed in:** 628cb65 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — incorrect API instantiation pattern in plan interface reference)
**Impact on plan:** Fix is equivalent in behavior — same target level change applied to all log targets. No scope creep.

## Issues Encountered
- The plan interface comment for EspLogger was incorrect ("zero-sized Copy struct"). Discovered during compilation. The `esp_idf_svc::log` module exports a `set_target_level` free function that wraps the global logger instance, which is the idiomatic approach for this use case.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Full remote logging pipeline (LOG-01/02/03) is live
- Hardware smoke test recommended: subscribe to `gnss/+/log`, boot device, verify log lines appear within 1s
- Runtime level control: `mosquitto_pub -t 'gnss/{id}/log/level' -m 'warn' -r` should reduce log volume immediately
- Phase 16 is complete — all LOG requirements satisfied

---
*Phase: 16-remote-logging*
*Completed: 2026-03-08*
