---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Field Deployment
status: executing
stopped_at: Phase 19 Plan 02 complete — BUG-3/BUG-4 NVS TLS versioning fixed
last_updated: "2026-03-09T14:44:37.545Z"
last_activity: "2026-03-09 — Phase 19 plan 01 complete; SoftAP DHCP DNS fix via EspNetif::new_with_conf"
progress:
  total_phases: 6
  completed_phases: 5
  total_plans: 17
  completed_plans: 16
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-08)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** v2.0 Field Deployment — Phase 14: Quick Additions

## Current Position

Phase: 19 of 19 (pre-2.0-bugfix)
Plan: 1 of 3 in current phase (19-01 complete — BUG-1 DHCP DNS fix; BUG-2 unblocked)
Status: Phase 19 in progress — BUG-1 fixed (WifiDriver+wrap_all DNS pre-config); BUG-3/BUG-4 NVS versioning pending (19-02); FEAT-1 boot button pending (19-03)
Last activity: 2026-03-09 — Phase 19 plan 01 complete; SoftAP DHCP DNS fix via EspNetif::new_with_conf

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
| Phase 16 P01 | 6 | 2 tasks | 5 files |
| Phase 16-remote-logging P02 | 3 | 2 tasks | 2 files |
| Phase 17-ntrip-client P01 | 4 | 2 tasks | 2 files |
| Phase 17 P02 | 8 | 2 tasks | 2 files |
| Phase 17-ntrip-client P03 | 3 | 2 tasks | 4 files |
| Phase 18-telemetry-and-ota-validation P01 | 3 | 3 tasks | 4 files |
| Phase 18 P03 | 2 | 1 tasks | 1 files |
| Phase 18-telemetry-and-ota-validation P02 | 5 | 1 tasks | 2 files |

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
- [Phase 16]: cc::Build include paths parsed from embuild cincl_args shell tokens — strip outer quotes, classify by -isystem/-I/-D prefix
- [Phase 16]: mod log_relay added to main.rs in Plan 01 (not Plan 02) to allow cargo build verification; spawn_log_relay not called yet
- [Phase 16]: spawn_log_relay returns anyhow::Result<()> — SyncSender stored in LOG_TX OnceLock, main.rs does not hold the sender
- [Phase 16-remote-logging]: esp_idf_svc::log::set_target_level() free function used instead of EspLogger instance — EspLogger has cache field, not zero-sized; free function delegates to global LOGGER
- [Phase 16-remote-logging]: Phase 16 LOG-01/02/03 pipeline complete: vprintf hook at Step 2b, spawn_log_relay at 9.5, log_level_relay_task at 9.6
- [Phase 17-01]: spawn_gnss returns Arc<UartDriver<'static>> as 5th tuple element; main.rs update deferred to Plan 02
- [Phase 17-01]: RTCM correction bytes written directly to Arc<UartDriver> (not gnss_cmd_tx String channel) to avoid binary data corruption
- [Phase 17-01]: Custom inline base64 encoder avoids adding base64 crate dependency; NTRIP config no deduplication (reconnect on repeat payload)
- [Phase 17]: ntrip/config dispatch placed BEFORE /config branch to prevent routing collision (both end with /config)
- [Phase 17]: NTRIP_BACKOFF_STEPS kept in ntrip_client.rs as module-local const — no config.rs addition needed
- [Phase 17-03]: strip_ansi uses byte scan (no regex crate) matching ESC-bracket pattern for ANSI SGR sequences from C vprintf path
- [Phase 17-03]: UM980 reboot monitor uses warning fallback — NVS-backed gnss config re-apply deferred (config_relay reads MQTT channel not NVS)
- [Phase 17-03]: um980_reboot channel bounded to 1 to coalesce rapid reboot signals; sentence_type cloned before nmea_tx move for reboot check
- [Phase 17-ntrip-client]: DNS thread intentionally not stopped before 300s timeout: esp_restart() terminates all threads
- [Phase 17-ntrip-client]: Captive portal probe URLs use meta-refresh HTML (200 OK) not HTTP 302 — matches existing into_ok_response() handler style
- [Phase 17-04]: Hardware verification of captive portal detection deferred to end of milestone — will be validated alongside Phase 18 hardware sign-off
- [Phase 18]: Sentinel values 0xFF/0xFFFF in gnss_state atomics indicate no GGA received; heartbeat emits JSON null — unambiguous vs 0 which means no-fix
- [Phase 18]: HDOP stored as x10 integer in AtomicU32 (e.g. 1.2 -> 12); no AtomicF32 in std Rust; formatted back to 1-decimal in heartbeat JSON
- [Phase 18]: ends_with('GGA') match in nmea_relay.rs handles GNGGA, GPGGA, GLGGA uniformly without exhaustive list
- [Phase 18]: README authored from source inspection (led.rs timing, heartbeat null sentinel semantics) to ensure accuracy over plan approximations
- [Phase 18-telemetry-and-ota-validation]: Hardware validation (OTA + captive portal) deferred to end-of-milestone sign-off session; testing.md checklist written with SHA-256 of canary binary
- [Phase 19-01]: EspNetif::new_with_conf with RouterConfiguration.dns is the only lifecycle point that survives wifi.start() — post-start DHCP override via unsafe sys calls does not survive ESP-IDF reinit
- [Phase 19-01]: WifiDriver::new + EspWifi::wrap_all pattern used to inject pre-configured ap_netif for SoftAP; STA netif uses default EspNetif::new(NetifStack::Sta) since STA not used in AP mode
- [Phase 19]: TLS defaults false on key absence — old firmware never wrote mqtt_tls; absence == plain MQTT
- [Phase 19]: config_ver=1 written on every credential save — idempotent NVS schema versioning convention
- [Phase 19]: broker_url scheme switches mqtt:// vs mqtts:// based on tls bool from load_mqtt_config

### Roadmap Evolution

- Phase 19 added: pre-2.0-bugfix

### Pending Todos

- Verify `esp-idf-svc` SoftAP/captive-portal API availability before Phase 15
- Verify `esp-idf-svc::sntp` API before Phase 14

### Blockers/Concerns

(none at phase 14 start)

## Session Continuity

Last session: 2026-03-09T14:44:17.787Z
Stopped at: Phase 19 Plan 02 complete — BUG-3/BUG-4 NVS TLS versioning fixed
Resume file: None
Next action: Phase 19 Plan 02 — NVS versioning (BUG-3/BUG-4 fix).
