---
phase: 09-channel-loop-hardening
plan: "02"
subsystem: reliability
tags: [rust, mpsc, recv_timeout, wifi, embedded, esp32]

# Dependency graph
requires:
  - phase: 09-01-channel-loop-hardening
    provides: sync_channel bounded channels; SyncSender types in gnss/mqtt/ota

provides:
  - recv_timeout loops on all 6 producer-consumer channel pairs (no unbounded recv() calls remain)
  - RELAY_RECV_TIMEOUT (5s) and SLOW_RECV_TIMEOUT (30s) named duration constants
  - MAX_WIFI_RECONNECT_ATTEMPTS (20) named constant for WiFi supervisor
  - consecutive_failures counter in wifi_supervisor that resets on success
  - Dead-end park loops after channel disconnect on all affected threads

affects: [phase-11-watchdog, phase-12-restart-resilience]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "recv_timeout loop: match Ok(item) / Timeout (continue) / Disconnected (break + park)"
    - "Non-credential compile-time constants in config.rs (and documented in config.example.rs)"

key-files:
  created: []
  modified:
    - src/config.example.rs
    - src/gnss.rs
    - src/nmea_relay.rs
    - src/rtcm_relay.rs
    - src/config_relay.rs
    - src/mqtt.rs
    - src/ota.rs
    - src/wifi.rs

key-decisions:
  - "config.example.rs updated with non-credential constants (RELAY_RECV_TIMEOUT, SLOW_RECV_TIMEOUT, MAX_WIFI_RECONNECT_ATTEMPTS) — real values live in gitignored config.rs per project convention"
  - "consecutive_failures (not max_backoff_failures) counts every failure, resets on success — gives accurate at-limit logging"
  - "Dead-end park loop added after break in all converted threads — satisfies -> ! return types and prevents thread exit"
  - "Timeout arm is no-op (continue) in all threads — Phase 11 watchdog will feed heartbeat counters here"

patterns-established:
  - "recv_timeout pattern: loop { match rx.recv_timeout(CONST) { Ok => process, Timeout => continue, Disconnected => { log; break; } } } followed by park loop"

requirements-completed: [HARD-05, HARD-06]

# Metrics
duration: 5min
completed: 2026-03-07
---

# Phase 9 Plan 02: Loop Hardening + recv_timeout Conversion Summary

**All 6 unbounded channel recv() calls replaced with recv_timeout() loops using named duration constants; WiFi supervisor hardened with consecutive_failures counter and MAX_WIFI_RECONNECT_ATTEMPTS = 20**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-07T10:13:59Z
- **Completed:** 2026-03-07T10:19:11Z
- **Tasks:** 8 (all changes in single atomic commit)
- **Files modified:** 8

## Accomplishments

- Replaced every `for x in &receiver` (unbounded blocking recv) with `recv_timeout()` loops in all 6 affected threads: gnss-tx, nmea-relay, rtcm-relay, config-relay, subscriber, ota
- Added RELAY_RECV_TIMEOUT (5s), SLOW_RECV_TIMEOUT (30s), MAX_WIFI_RECONNECT_ATTEMPTS (20) as named constants in config.rs / config.example.rs
- Replaced implicit `max_backoff_failures` (only counted at max backoff) with `consecutive_failures` (counts every failure, resets on success) in wifi_supervisor
- All threads now break out of recv loop on Disconnected and enter a dead-end park loop — preserves `-> !` return type semantics

## Task Commits

Each task was committed atomically:

1. **All tasks (config constants + 6 recv_timeout conversions + WiFi hardening)** - `d048ca8` (feat)

**Plan metadata:** (pending)

## Files Created/Modified

- `src/config.example.rs` — Added RELAY_RECV_TIMEOUT, SLOW_RECV_TIMEOUT, MAX_WIFI_RECONNECT_ATTEMPTS constants
- `src/gnss.rs` — TX thread: for..iter() → recv_timeout(RELAY_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/nmea_relay.rs` — for..in &nmea_rx → recv_timeout(RELAY_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/rtcm_relay.rs` — for..in &rtcm_rx → recv_timeout(RELAY_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/config_relay.rs` — for..in &config_rx → recv_timeout(SLOW_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/mqtt.rs` — subscriber_loop: for()..in &subscribe_rx → recv_timeout(SLOW_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/ota.rs` — ota_task: for..in &ota_rx → recv_timeout(SLOW_RECV_TIMEOUT) loop; added RecvTimeoutError import
- `src/wifi.rs` — consecutive_failures counter replaces max_backoff_failures; MAX_WIFI_RECONNECT_ATTEMPTS used; attempt/limit logged

## Decisions Made

- `config.example.rs` updated with non-credential constants alongside credentials placeholder — this is the project's established pattern for keeping config.rs gitignored while documenting required fields
- `consecutive_failures` replaces `max_backoff_failures` — old counter only incremented when already at max backoff, giving misleading threshold behavior. New counter increments on every failure and resets cleanly on success.
- Dead-end park added after break in all threads — necessary for `-> !` return types; the infinite park after channel close is intentional (thread has no more work to do, but OS thread exit could trigger unexpected behavior on embedded targets)
- Timeout arm is no-op (`continue`) everywhere — Phase 11 will feed watchdog heartbeat counters in this arm without any further changes to the loop structure

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated config.example.rs with new constants**
- **Found during:** Task 1 (adding constants to config.rs)
- **Issue:** config.rs is gitignored (contains WiFi/MQTT credentials per project convention); new non-credential constants added to config.rs would not be committed or visible to future contributors
- **Fix:** Added same new constants (RELAY_RECV_TIMEOUT, SLOW_RECV_TIMEOUT, MAX_WIFI_RECONNECT_ATTEMPTS) to config.example.rs — the committed template file
- **Files modified:** src/config.example.rs
- **Verification:** config.example.rs committed; constants in gitignored config.rs work at build time
- **Committed in:** d048ca8 (task commit)

---

**Total deviations:** 1 auto-fixed (1 blocking — config.example.rs needed update)
**Impact on plan:** Non-credential constants documented in committed template. No scope creep.

## Issues Encountered

None — all recv_timeout conversions were straightforward. Build succeeded on first attempt with only a pre-existing unused-import warning in ota.rs (unrelated to this plan's changes).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 9 complete: channels bounded (09-01) and recv_timeout loops in place (09-02)
- Phase 10 (if any) or Phase 11 (Thread Watchdog) can feed heartbeat counters in the Timeout arms without further structural changes
- Phase 12 (RESIL-01) inserts `esp_restart()` at the `consecutive_failures >= MAX_WIFI_RECONNECT_ATTEMPTS` point in wifi.rs — the comment placeholder is already in place

## Self-Check: PASSED

All referenced files exist and commit d048ca8 is verified in git history.

---
*Phase: 09-channel-loop-hardening*
*Completed: 2026-03-07*
