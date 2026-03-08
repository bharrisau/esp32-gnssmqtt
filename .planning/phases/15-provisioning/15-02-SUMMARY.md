---
phase: 15-provisioning
plan: 02
subsystem: wifi
tags: [esp32, esp-idf-svc, wifi, mqtt, nvs, provisioning, softap]

# Dependency graph
requires:
  - phase: 15-01
    provides: provisioning module with has_wifi_credentials, check_and_clear_force_softap, run_softap_portal, load_wifi_networks, load_mqtt_config

provides:
  - wifi_connect_any() in wifi.rs: multi-network STA cycling (up to 3x per network) without SoftAP fallback
  - SOFTAP_SSID and SOFTAP_TIMEOUT_SECS constants in config.rs
  - Boot-path decision in main.rs: SoftAP when force_softap or no NVS credentials; STA otherwise
  - mqtt_connect() with runtime host/port/user/pass parameters (NVS with compile-time fallback)
  - mod provisioning declared in main.rs

affects: [15-03, any phase modifying wifi.rs or mqtt.rs, any phase using mqtt_connect call site]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - nvs.clone() before any function that might consume the partition — EspNvsPartition<NvsDefault> is Clone
    - SoftAP path calls unreachable!() after run_softap_portal — function never returns (uses esp_restart)
    - MQTT credentials passed as runtime &str parameters with empty-string None guard

key-files:
  created: []
  modified:
    - src/wifi.rs
    - src/mqtt.rs
    - src/main.rs
    - src/config.rs

key-decisions:
  - "wifi_connect_any does NOT enter SoftAP on failure — RESIL-01 reboot timer handles sustained WiFi failure"
  - "run_softap_portal is unreachable after return — always calls esp_restart(); code after it marked unreachable!()"
  - "config.rs is gitignored (contains credentials) — SOFTAP constants added locally only; wifi.rs change committed"
  - "mqtt_connect username/password use None for empty strings — matches MqttClientConfiguration Option<&str> pattern"

patterns-established:
  - "Provisioning boot decision: check force_softap and has_credentials before EspWifi::new"
  - "NVS clone pattern: nvs.clone() for every call that takes ownership; &nvs for read-only checks"

requirements-completed: [PROV-01, PROV-05]

# Metrics
duration: 2min
completed: 2026-03-08
---

# Phase 15 Plan 02: Provisioning Integration Summary

**Multi-network wifi_connect_any + NVS-driven boot-path decision in main.rs: SoftAP on first boot, STA with stored credentials thereafter; MQTT reads host/port/user/pass from NVS with compile-time fallback**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-08T00:58:54Z
- **Completed:** 2026-03-08T01:00:48Z
- **Tasks:** 2
- **Files modified:** 4 (wifi.rs, mqtt.rs, main.rs, config.rs)

## Accomplishments
- wifi_connect_any() added to wifi.rs: cycles up to 3x through stored networks with stop/start between attempts, no SoftAP fallback (PROV-05)
- Boot-path decision integrated in main.rs: SoftAP when force_softap flag or no NVS credentials, STA otherwise (PROV-01)
- mqtt_connect() signature updated to accept runtime host/port/user/pass parameters; NVS loaded with compile-time constant fallback
- SOFTAP_SSID and SOFTAP_TIMEOUT_SECS constants added to config.rs (gitignored file)

## Task Commits

Each task was committed atomically:

1. **Task 1: wifi_connect_any and config constants** - `5a5e62b` (feat)
2. **Task 2: Update mqtt_connect signature and wire boot-path in main.rs** - `d50e339` (feat)

**Plan metadata:** (final docs commit — see below)

## Files Created/Modified
- `src/wifi.rs` - Added wifi_connect_any() multi-network STA function
- `src/mqtt.rs` - Updated mqtt_connect signature with host/port/user/pass parameters
- `src/main.rs` - Added mod provisioning, boot-path decision block, MQTT config NVS load
- `src/config.rs` - Added SOFTAP_SSID and SOFTAP_TIMEOUT_SECS constants (gitignored, local only)

## Decisions Made
- `config.rs` is gitignored (contains WiFi/MQTT credentials) — constants added locally but only wifi.rs committed to git
- `run_softap_portal` never returns (calls esp_restart()), so the SoftAP branch ends with `unreachable!()` — the compiler requires this since the if/else must produce a typed value
- `mqtt_connect` uses `None` for empty user/pass strings — matches the MqttClientConfiguration `Option<&str>` pattern already used
- wifi_connect_any does NOT enter SoftAP on failure — RESIL-01 reboot timer in wifi_supervisor handles sustained failure per PROV-05

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- config.rs is in .gitignore (contains credentials). The SOFTAP constants were added locally but only wifi.rs was committed. This is expected behavior — config.rs changes persist on the device but are not version-controlled.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Provisioning integration complete — boot-path, SoftAP portal, multi-network STA, and NVS MQTT config all wired
- Ready for Phase 15-03: GPIO9 trigger for force_softap and MQTT "softap" command trigger
- cargo build --release passes with no errors; warnings are pre-existing (wifi_connect, set_force_softap unused — reserved for future use)

---
*Phase: 15-provisioning*
*Completed: 2026-03-08*
