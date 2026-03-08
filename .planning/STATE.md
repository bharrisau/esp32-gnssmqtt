---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Field Deployment
status: executing
stopped_at: Completed 15-03-PLAN.md
last_updated: "2026-03-08T00:46:42.030Z"
last_activity: 2026-03-08 — Phase 15 plan 02 executed; provisioning wired into main.rs boot-path, wifi_connect_any added, mqtt_connect runtime credentials
progress:
  total_phases: 5
  completed_phases: 2
  total_plans: 5
  completed_plans: 5
  percent: 20
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-08)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** v2.0 Field Deployment — Phase 14: Quick Additions

## Current Position

Phase: 15 of 18 (Provisioning)
Plan: 3 of 3 in current phase (all plans 01-03 complete — Phase 15 complete)
Status: Phase 15 complete — all 8 PROV requirements implemented
Last activity: 2026-03-08 — Phase 15 plan 03 executed; GPIO9 monitor, MQTT "softap" trigger, LedState::SoftAP pattern added

Progress: [██████████] 100%

## Performance Metrics

**Velocity:**
- Total plans completed: 0 (v2.0)
- Prior milestone (v1.3): 9 plans, ~30 min avg/plan
- Total v1.x execution time: ~8 hours across 24 plans

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| v2.0 — not started | - | - | - |

**Recent Trend:**
- v1.3 last 5 plans: stable
- Trend: Stable

| Phase 14 P01 | 5 | 2 tasks | 2 files |
| Phase 14 P02 | 12 | 2 tasks | 3 files |
| Phase 15 P03 | 3 | 2 tasks | 3 files |

## Accumulated Context

### Decisions

All decisions from v1.0–v1.3 logged in PROJECT.md Key Decisions table.

Key carry-forward notes:
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir
- [OTA NOTE]: Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes
- [BLE NOTE]: BLE provisioning deferred — SoftAP chosen for v2.0; covers WiFi + MQTT in one web UI without custom app
- [Phase 14]: EspSntp handle in main() scope prevents sntp_stop() on drop — mirrors _gnss_cmd_tx keep-alive pattern
- [Phase 14]: CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y in sdkconfig.defaults switches ESP-IDF log from ms-since-boot to HH:MM:SS.mmm wall-clock
- [Phase 14 P02]: QoS 0 (AtMostOnce) for /command subscription — prevents retain replay; old commands must not re-execute (CMD-02)
- [Phase 14 P02]: Reboot check uses json.trim() == "reboot" before extract_json_str — graceful short-circuit for MAINT-01 without parse error noise
- [Phase 14 P02]: command_relay_task uses blocking send() for gnss_cmd_tx to ensure no silent drops to UM980
- [Phase 15 P01]: SoftAP uses open auth (AuthMethod::None), channel 6, SSID 'GNSS-Setup' — no password required from user
- [Phase 15 P01]: HTTP server stack_size 10240 (not default 6144) to prevent stack overflow in POST handler
- [Phase 15 P01]: MQTT port stored as two u8 NVS keys (mqtt_port_hi, mqtt_port_lo) — no set_u16 in EspNvs
- [Phase 15 P01]: esp_restart() after credential save deferred 1s via spawned thread so browser receives HTTP 200
- [Phase 15 P01]: 300s no-client timeout restarts WITHOUT force_softap so next boot tries STA with stored credentials
- [Phase 15 P02]: wifi_connect_any does NOT enter SoftAP on failure — RESIL-01 reboot timer handles sustained WiFi failure (PROV-05)
- [Phase 15 P02]: run_softap_portal is unreachable after return; SoftAP if-else branch ends with unreachable!() macro
- [Phase 15 P02]: mqtt_connect username/password use None for empty strings — matches MqttClientConfiguration Option<&str> pattern
- [Phase 15 P02]: config.rs is gitignored (credentials); SOFTAP constants added locally but not committed
- [Phase 15]: GPIO9 polled every 100ms with 3s hold threshold; timer resets on release preventing accidental SoftAP re-entry
- [Phase 15]: nvs passed by clone to spawn_ota since EspNvsPartition<NvsDefault> implements Clone cheaply

### Pending Todos

- Verify `esp-idf-svc` SoftAP/captive-portal API availability before Phase 15
- Verify `esp-idf-svc::sntp` API before Phase 14

### Blockers/Concerns

(none at phase 14 start)

## Session Continuity

Last session: 2026-03-08T00:46:42.026Z
Stopped at: Completed 15-03-PLAN.md
Resume file: None
Next action: Phase 15 complete — all 8 PROV requirements done. Proceed to next phase.
