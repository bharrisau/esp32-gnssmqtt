---
phase: 20-field-testing-fixes
plan: "02"
subsystem: gnss
tags: [nmea, mqtt, throughput, channel-capacity, sdkconfig]

# Dependency graph
requires:
  - phase: 18-telemetry-and-ota-validation
    provides: nmea_relay.rs with GGA parsing and enqueue pattern
provides:
  - NMEA channel capacity raised to 128 for 5 Hz GNSS support
  - Per-100-sentences throughput diagnostic log in NMEA relay
  - MQTT outbox timeout tuned to 5s (prevents heap growth during disconnects)
affects: [field-testing, mqtt-throughput, gnss-relay]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Throughput diagnostic: sentence_count % 100 log with Instant timer reset; no hot-path allocations"
    - "sdkconfig.defaults for MQTT outbox expiry tuning (CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS)"

key-files:
  created: []
  modified:
    - src/gnss.rs
    - src/nmea_relay.rs
    - sdkconfig.defaults

key-decisions:
  - "NMEA channel raised 64->128: at 5 Hz x 8 sentence types = 40 msg/s; 128 provides ~3s headroom before drops"
  - "Throughput log per 100 sentences (~2.5s at 5 Hz): sufficient for field diagnosis without log spam"
  - "No timing around client.lock() — log overhead would perturb the measurement; throughput count is sufficient"
  - "MQTT outbox expiry 5s (not default 30s): QoS 0 GNSS data older than 5s is irrelevant; 30s x 40 msg/s = 1200 messages heap growth during disconnect"

patterns-established:
  - "Throughput diagnostic using sentence_count modulo check with Instant reset — zero allocation in hot path"

requirements-completed: [PERF-1]

# Metrics
duration: 15min
completed: 2026-03-11
---

# Phase 20 Plan 02: MQTT Throughput Tuning Summary

**NMEA channel capacity raised 64->128, per-100-sentences throughput logging added, and MQTT outbox timeout reduced from 30s to 5s to sustain 5 Hz GNSS output without drops**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-11T00:00:00Z
- **Completed:** 2026-03-11T00:15:00Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- NMEA sync_channel capacity raised from 64 to 128 with 5 Hz rationale comment
- Throughput diagnostic logging added: per-100-sentences log at INFO level shows actual msg/s rate
- MQTT outbox timeout set to 5000ms — prevents heap growth from stale QoS 0 messages during disconnects
- cargo clippy -- -D warnings passes clean; cargo build --release succeeds

## Task Commits

Each task was committed atomically:

1. **Task 1: Increase NMEA channel capacity and add throughput diagnostic logging** - `03d751e` (feat)
2. **Task 2: Tune MQTT outbox timeout in sdkconfig.defaults** - `d377173` (chore)

## Files Created/Modified
- `src/gnss.rs` - NMEA sync_channel capacity 64 -> 128; comment updated with 5 Hz rationale
- `src/nmea_relay.rs` - Added sentence_count (u64) and throughput_tick (Instant); per-100-sentences INFO log
- `sdkconfig.defaults` - Added CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000 with explanatory comment block

## Decisions Made
- Throughput log placed after enqueue (not around client.lock()) — avoids perturbing the measurement being diagnosed
- Kept mutex-per-sentence pattern unchanged — research concluded contention is benign at 5 Hz; architectural change not warranted
- No CONFIG_MQTT_TASK_PRIORITY change — could destabilize other subsystems; research priority order put channel size first

## Deviations from Plan

None - plan executed exactly as written. Both source files already had changes applied when execution began; committed as planned.

## Issues Encountered

None - files were pre-modified (gnss.rs and nmea_relay.rs had the planned changes already applied as unstaged edits). Committed and then added the sdkconfig.defaults change as planned.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- 5 Hz GNSS throughput fixes complete; field test at 5 Hz can now be performed
- Configure UM980 with `GPGGA 0.2` (5 Hz), flash firmware, monitor `/log` topic
- Throughput log should show ~40 msg/s; heartbeat `nmea_drops` should remain 0 after 60s
- Phase 20 Plan 03 (captive portal probe fix) ready to proceed

---
*Phase: 20-field-testing-fixes*
*Completed: 2026-03-11*
