---
phase: 14-quick-additions
plan: "02"
subsystem: mqtt
tags: [mqtt, command-relay, reboot, ota, um980, qos, esp32, rust]

# Dependency graph
requires:
  - phase: 14-01
    provides: SNTP wall-clock time; sdkconfig.defaults baseline for phase 14

provides:
  - MQTT /command topic subscription (QoS 0) forwarding payloads to UM980 via gnss_cmd_tx
  - command_relay_task fn in mqtt.rs with recv_timeout loop and HWM logging
  - cmd_relay_tx dispatch arm in mqtt_connect callback
  - Reboot early-exit in ota_task before JSON parse ("reboot" payload on /ota/trigger)

affects: [phase-15, phase-16, any phase touching mqtt.rs or ota.rs]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "QoS 0 subscription for /command prevents retain replay on reconnect (CMD-02)"
    - "Reboot payload check before JSON parse avoids cascading parse errors"
    - "SyncSender<String> for gnss_cmd_tx (matches config_relay.rs channel type)"
    - "recv_timeout loop with explicit Disconnected arm parks thread rather than crashing"

key-files:
  created: []
  modified:
    - src/ota.rs
    - src/mqtt.rs
    - src/main.rs

key-decisions:
  - "QoS 0 (AtMostOnce) for /command subscription — no retain replay; old commands must not re-execute on reconnect (CMD-02)"
  - "Reboot check uses json.trim() == 'reboot' before extract_json_str — graceful short-circuit avoids noise logs for the common reboot case"
  - "command_relay_task uses send() (blocking) not try_send() — ensures no silent drops to UM980; channel capacity 4 provides backpressure buffer"
  - "200ms sleep before restart() in reboot handler gives log output time to flush to UART serial buffer"

patterns-established:
  - "New MQTT→UM980 relay: channel declared in main.rs, try_send in callback, dedicated recv_timeout task, gnss_cmd_tx.clone() for task"

requirements-completed: [MAINT-01, CMD-01, CMD-02]

# Metrics
duration: 12min
completed: 2026-03-08
---

# Phase 14 Plan 02: Command Relay and Reboot Trigger Summary

**MQTT /command topic (QoS 0) wired to UM980 via command_relay_task, and "reboot" payload handled in ota_task before OTA JSON parse**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-08T00:00:00Z
- **Completed:** 2026-03-08T00:12:00Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Operators can now send arbitrary UM980 commands remotely by publishing to `gnss/{device_id}/command`; each payload is forwarded exactly once as a raw string via gnss_cmd_tx
- Remote reboot via `gnss/{device_id}/ota/trigger` works for "reboot" payload without triggering OTA JSON parsing errors
- QoS 0 subscription on /command ensures retained messages from previous sessions are not re-executed after reconnect (CMD-02 satisfied)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add reboot early-exit to ota_task in ota.rs** - `bab8eff` (feat)
2. **Task 2: Add command relay to mqtt.rs and wire channel in main.rs** - `94fe156` (feat)

**Plan metadata:** (created in final docs commit)

## Files Created/Modified

- `src/ota.rs` - Added reboot check (json.trim() == "reboot") before extract_json_str call for "url"; uses existing restart() import
- `src/mqtt.rs` - Added command_relay_task fn; added /command dispatch arm in callback; added QoS 0 subscription in subscriber_loop; mqtt_connect gained cmd_relay_tx parameter
- `src/main.rs` - Declared cmd_relay channel (capacity 4); passed cmd_relay_tx to mqtt_connect; spawned command_relay_task thread (8192-byte stack) after config relay

## Decisions Made

- QoS 0 (AtMostOnce) for /command subscription — prevents retain replay; old commands must not re-execute on reconnect (CMD-02 requirement)
- Reboot check uses `json.trim() == "reboot"` before `extract_json_str` — short-circuits gracefully, avoids misleading "missing url" log entries for the common reboot case
- command_relay_task uses blocking `send()` rather than `try_send()` — ensures no silent drops to the UM980; channel capacity 4 provides adequate backpressure buffer
- 200ms sleep before `restart()` in the reboot handler to allow the log line to flush to UART serial buffer before the chip resets

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Command relay and reboot trigger are fully operational
- All three MAINT-01, CMD-01, CMD-02 requirements satisfied
- Phase 15 (SoftAP provisioning) can proceed; mqtt.rs interface is stable
- Note: verify `esp-idf-svc` SoftAP/captive-portal API availability before Phase 15 (tracked in STATE.md pending todos)

## Self-Check: PASSED

- FOUND: .planning/phases/14-quick-additions/14-02-SUMMARY.md
- FOUND: commit bab8eff (feat: add reboot early-exit to ota_task)
- FOUND: commit 94fe156 (feat: add command relay task and wire channel in main)

---
*Phase: 14-quick-additions*
*Completed: 2026-03-08*
