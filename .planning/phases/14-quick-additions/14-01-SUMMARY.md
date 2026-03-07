---
phase: 14-quick-additions
plan: "01"
subsystem: infra
tags: [sntp, ntp, logging, timestamps, esp-idf, kconfig]

# Dependency graph
requires: []
provides:
  - Wall-clock timestamps in ESP-IDF log output (HH:MM:SS.mmm) after WiFi connects
  - EspSntp initialized in main() scope after wifi_connect (Step 6.5)
  - CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM Kconfig in sdkconfig.defaults
affects: [all future phases — log output timestamps visible in serial monitor and MQTT]

# Tech tracking
tech-stack:
  added: [esp_idf_svc::sntp::EspSntp]
  patterns: [SNTP handle kept in main() scope to prevent sntp_stop() on drop]

key-files:
  created: []
  modified:
    - sdkconfig.defaults
    - src/main.rs

key-decisions:
  - "EspSntp handle stored as _sntp in main() scope (not a sub-block) — dropping it calls sntp_stop() which reverts timestamps to boot-relative ms"
  - "sntp::EspSntp::new_default() inserted at Step 6.5 — after WiFi (IP required for NTP) and before GNSS/MQTT (benefits all subsequent log output)"
  - "CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y placed in sdkconfig.defaults — build-time Kconfig that switches EspLogger from esp_log_timestamp() to esp_log_system_timestamp()"

patterns-established:
  - "SNTP-after-WiFi: EspSntp::new_default() must follow wifi_connect and must live in main() scope"

requirements-completed: [MAINT-02]

# Metrics
duration: 5min
completed: "2026-03-08"
---

# Phase 14 Plan 01: SNTP Wall-Clock Timestamps Summary

**EspSntp initialized after WiFi connects with CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM Kconfig — log output shows HH:MM:SS.mmm wall-clock time instead of ms-since-boot after first NTP sync**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-03-07T23:44:13Z
- **Completed:** 2026-03-07T23:49:09Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` to sdkconfig.defaults, switching ESP-IDF log formatter from RTOS ticks to system time
- Added `use esp_idf_svc::sntp` import and `let _sntp = sntp::EspSntp::new_default()` at Step 6.5 in main.rs (after wifi_connect, handle kept alive in main() scope)
- Confirmed clean build after `cargo clean && cargo build --release` (forces CMake regeneration of sdkconfig)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM to sdkconfig.defaults** - `3cb6cf7` (feat)
2. **Task 2: Initialise EspSntp in main.rs after wifi_connect** - `1b57292` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `sdkconfig.defaults` - Added CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y block after CONFIG_ESP_TASK_WDT_PANIC=y
- `src/main.rs` - Added `use esp_idf_svc::sntp` import and Step 6.5 SNTP init after WiFi connect

## Decisions Made

- EspSntp handle must remain in main() scope — dropping it calls sntp_stop() which reverts log timestamps to boot-relative ms. Pattern mirrors `let _gnss_cmd_tx = gnss_cmd_tx` already used in idle loop for same reason.
- Placement at Step 6.5 (after wifi_connect, before gnss spawn) is correct: NTP requires IP connectivity, and placing it early means all subsequent log output from GNSS, MQTT, and relay threads benefits from wall-clock timestamps once NTP syncs (~1-5s after WiFi connects).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required. Hardware verification (flash device, observe serial monitor timestamps switch from "NNNN ms" format to "HH:MM:SS.mmm" format within ~5 seconds of WiFi connecting) is a manual step.

## Next Phase Readiness

- Phase 14-01 complete — SNTP timestamps active for all firmware log output
- Ready for Phase 14-02 (command relay or next quick addition)
- No blockers

---
*Phase: 14-quick-additions*
*Completed: 2026-03-08*
