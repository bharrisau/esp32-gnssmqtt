---
phase: 07-rtcm-relay
plan: 01
subsystem: mqtt
tags: [mqtt, rtcm, esp-idf-svc, embedded-svc, rust, firmware]

# Dependency graph
requires:
  - phase: 06-remote-config
    provides: config_tx channel wired through pump_mqtt_events; mqtt_connect baseline configuration
provides:
  - Topic-discriminated routing in pump_mqtt_events (ends_with("/config") guard)
  - MQTT out_buffer_size bumped to 2048 bytes in mqtt_connect
  - mod rtcm_relay declared and wired in main.rs with rtcm_rx from spawn_gnss
affects:
  - 07-02
  - 07-03
  - 08-ota

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Topic suffix matching via ends_with() for MQTT message routing"
    - "out_buffer_size field in MqttClientConfiguration for large binary payloads"

key-files:
  created: []
  modified:
    - src/mqtt.rs
    - src/main.rs

key-decisions:
  - "Silent drop for non-/config topics in pump_mqtt_events (no else log) to avoid log spam during Phase 8 OTA retain playback"
  - "out_buffer_size set to 2048 (not larger) — covers 1029-byte RTCM MSM7 + MQTT overhead with comfortable margin"
  - "Auto-fixed main.rs 3-tuple destructure for gnss::spawn_gnss (pre-existing blocking compile error from 07-02 commit)"

patterns-established:
  - "Topic-based routing: topic.unwrap_or(\"\").ends_with(\"/suffix\") pattern for MQTT message dispatch"

requirements-completed:
  - RTCM-05
  - RTCM-04

# Metrics
duration: 8min
completed: 2026-03-07
---

# Phase 7 Plan 01: RTCM Relay MQTT Bug Fix Summary

**Topic-discriminated pump_mqtt_events routing and 2048-byte MQTT output buffer for RTCM MSM7 frame support**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-07T03:10:58Z
- **Completed:** 2026-03-07T03:18:58Z
- **Tasks:** 2
- **Files modified:** 2 (src/mqtt.rs, src/main.rs)

## Accomplishments
- Fixed latent bug where all MQTT Received events were forwarded to config_tx regardless of topic; now only /config-suffixed topics route to the UM980 UART
- Bumped MQTT output buffer from default 1024 to 2048 bytes, enabling RTCM MSM7 frames up to 1029 bytes to publish without truncation
- Auto-fixed pre-existing compile error in main.rs (spawn_gnss 3-tuple destructure + mod rtcm_relay wiring) that blocked build verification

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix topic discrimination in pump_mqtt_events (RTCM-05)** - `54e02b8` (fix)
2. **Task 2: Bump MQTT output buffer to 2048 bytes (RTCM-04)** - `23ead2b` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified
- `src/mqtt.rs` - Added ends_with("/config") guard in Received arm; added out_buffer_size: 2048 to MqttClientConfiguration; updated doc comments for both functions
- `src/main.rs` - Added mod rtcm_relay; updated spawn_gnss destructure to 3-tuple (gnss_cmd_tx, nmea_rx, rtcm_rx); added rtcm_relay::spawn_relay call (Step 16)

## Decisions Made
- Silent drop (no log) for non-/config topics in pump_mqtt_events — an else branch logging unrecognised topics would generate log noise during Phase 8 OTA retain playback; silence is intentional
- out_buffer_size: 2048 chosen to cover 1029-byte max RTCM MSM7 frame + MQTT fixed header (~5 bytes) + topic string overhead (~25 bytes); leaves ~989 bytes headroom

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed main.rs 3-tuple destructure for gnss::spawn_gnss**
- **Found during:** Task 1 build verification
- **Issue:** Commit `7c01622` updated gnss.rs to return `(cmd_tx, nmea_rx, rtcm_rx)` (3-tuple) but main.rs still destructured `(gnss_cmd_tx, nmea_rx)` (2-tuple), causing a mismatched types compile error. rtcm_relay.rs already existed from a separate commit but main.rs was never updated.
- **Fix:** Added `mod rtcm_relay` declaration, updated spawn_gnss call to destructure 3 values, added `rtcm_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), rtcm_rx)` call at Step 16
- **Files modified:** src/main.rs
- **Verification:** `cargo build --release` exits 0 with no errors after fix
- **Committed in:** `54e02b8` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 3 - blocking)
**Impact on plan:** Auto-fix necessary to restore a compilable state broken by an incomplete prior commit. No scope creep — implements exactly what plan 07-03 specified for main.rs wiring.

## Issues Encountered
- The 07-02 plan commit (`7c01622`) only updated gnss.rs and not main.rs, leaving the build broken. rtcm_relay.rs was committed separately (`f102c93`) but main.rs still had the old 2-tuple destructure. This was auto-fixed under Rule 3 as part of Task 1 to enable build verification.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- RTCM-04 and RTCM-05 requirements satisfied
- main.rs now wires rtcm_rx through to rtcm_relay::spawn_relay (plan 07-03 work completed as auto-fix)
- Plan 07-02 (gnss.rs RxState state machine + rtcm_relay.rs) was already committed; plan 07-03 (main.rs wiring) is also done as part of this auto-fix
- Firmware compiles cleanly with all Phase 7 RTCM relay changes integrated
- No blockers for Phase 8 (OTA); pump_mqtt_events now silently ignores /ota/trigger — Phase 8 will add ota_tx channel

---
*Phase: 07-rtcm-relay*
*Completed: 2026-03-07*
