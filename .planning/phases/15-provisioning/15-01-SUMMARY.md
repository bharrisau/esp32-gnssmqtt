---
phase: 15-provisioning
plan: 01
subsystem: provisioning
tags: [esp32, rust, nvs, softap, http, wifi, mqtt, embedded]

# Dependency graph
requires: []
provides:
  - "src/provisioning.rs with 6 public functions: has_wifi_credentials, check_and_clear_force_softap, run_softap_portal, load_wifi_networks, load_mqtt_config, set_force_softap"
  - "SoftAP web portal serving HTML form at 192.168.71.1 for WiFi and MQTT credential entry"
  - "NVS credential storage with keys under 15 chars in namespace 'prov'"
  - "300-second no-client timeout loop with esp_restart()"
affects: [15-02, 15-03, wifi.rs, main.rs, ota.rs]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "EspNvsPartition<NvsDefault>.clone() for NVS access without consuming partition"
    - "Configuration::AccessPoint with start() + wait_netif_up() only (no connect()) for SoftAP"
    - "EspHttpServer stack_size: 10240 for POST body parsing headroom"
    - "Two-u8-key port storage (mqtt_port_hi, mqtt_port_lo) for u16 in NVS set_u8"
    - "Deferred esp_restart() via spawned thread after HTTP response sent"

key-files:
  created:
    - src/provisioning.rs
  modified: []

key-decisions:
  - "SoftAP uses open auth (AuthMethod::None) on channel 6, SSID 'GNSS-Setup' — easiest for first-time users without requiring app"
  - "HTTP server stack_size set to 10240 (not default 6144) to prevent stack overflow during POST body parsing"
  - "Port stored as two u8 keys (mqtt_port_hi, mqtt_port_lo) because EspNvs has no set_u16"
  - "esp_restart() deferred 1 second via spawned thread so browser receives HTTP 200 response before reboot"
  - "parse_form_field does not handle percent-encoding — passwords with &, %, +, =, non-ASCII unsupported (documented limitation)"
  - "300s no-client timeout restarts WITHOUT setting force_softap so next boot tries STA with stored credentials"

patterns-established:
  - "Pattern: NVS partition clone before use — EspNvsPartition<NvsDefault> is Clone; always pass &nvs_partition and clone inside helpers"
  - "Pattern: SoftAP exit — always via esp_restart(), never via in-place mode switch"

requirements-completed: [PROV-01, PROV-02, PROV-03, PROV-04]

# Metrics
duration: 15min
completed: 2026-03-08
---

# Phase 15 Plan 01: Provisioning Module Summary

**SoftAP captive portal with NVS credential storage — 6 public functions for WiFi+MQTT setup via browser form at 192.168.71.1**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-08T00:34:11Z
- **Completed:** 2026-03-08T00:49:00Z
- **Tasks:** 2
- **Files modified:** 1 (created)

## Accomplishments
- `src/provisioning.rs` created with all 6 public functions and 3 private helpers
- SoftAP portal starts on channel 6 (open SSID "GNSS-Setup"), serves HTML form, handles POST /save
- NVS credential store uses 12 keys all within 15-char ESP-IDF limit
- 300-second no-client timeout loop triggers esp_restart() to return to STA mode

## Task Commits

Each task was committed atomically:

1. **Task 1: NVS credential functions** - `d168539` (feat) — also includes Task 2 content since both were written together in a single file creation
2. **Task 2: SoftAP portal and save handler** - `d168539` (feat) — same commit as Task 1

**Plan metadata:** (see final docs commit)

## Files Created/Modified
- `src/provisioning.rs` — All provisioning logic: NVS read/write, SoftAP init, HTTP server, form parsing, credential save, 300s timeout loop

## Decisions Made
- Used `req.read()` directly on the request (not `split()`) since the embedded_svc `Request<C>` exposes `read()` without needing split for reading the body
- Wrote both tasks in a single file creation rather than two separate edits since Task 2 logically extends Task 1 without conflict
- Verified compilation by temporarily adding `mod provisioning;` to main.rs (then removed per plan spec — wiring is Plan 15-02's job)

## Deviations from Plan

None - plan executed exactly as written. The plan's example POST handler used `request.split()` to get a connection reference for `read()`; the actual embedded_svc API also provides `req.read()` directly on the Request, which was used instead (cleaner, same result).

## Issues Encountered
None — all APIs confirmed as described in the research document. Build succeeded first attempt.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `src/provisioning.rs` is ready for Plan 15-02 to wire into `main.rs` and `wifi.rs`
- Plan 15-02 will add `mod provisioning;` to main.rs, check NVS at boot, and call `run_softap_portal()` on first boot or force_softap flag
- All NVS helper functions (`load_wifi_networks`, `load_mqtt_config`) are ready for Plan 15-02's multi-network wifi_connect_any()

---
*Phase: 15-provisioning*
*Completed: 2026-03-08*
