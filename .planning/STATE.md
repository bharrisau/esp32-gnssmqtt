---
gsd_state_version: 1.0
milestone: v2.1
milestone_name: Server and nostd Foundation
status: planning
stopped_at: Completed 25-01-PLAN.md
last_updated: "2026-03-12T09:18:26.814Z"
last_activity: 2026-03-12 — v2.1 roadmap revised to 4 phases (22-25); gap crate work interleaved with server feature phases; 20/20 requirements mapped (NOSTD-04 split into NOSTD-04a + NOSTD-04b)
progress:
  total_phases: 4
  completed_phases: 3
  total_plans: 11
  completed_plans: 9
  percent: 84
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-12)

**Core value:** GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.
**Current focus:** Phase 22 — Workspace + Nostd Audit (v2.1 start)

## Current Position

Phase: 22 of 25 (Workspace + Nostd Audit)
Plan: 0 of TBD in current phase
Status: Ready to plan
Last activity: 2026-03-12 — v2.1 roadmap revised to 4 phases (22-25); gap crate work interleaved with server feature phases; 20/20 requirements mapped (NOSTD-04 split into NOSTD-04a + NOSTD-04b)

Progress: [████████████████████░░░░░] 84% (21/25 phases complete across all milestones)

## Execution Path

Phase dependencies for v2.1:

```
22 (Workspace + Audit) → 23 (MQTT + RTCM3 + gnss-nvs) → 24 (RINEX + gnss-ota)
                                                        → 25 (Web UI + gap skeletons)
```

Phase 24 and Phase 25 both depend on Phase 23 and can run in parallel with each other once Phase 23 completes.

## Performance Metrics

**Velocity (v2.0 reference):**
- Total plans completed: 48 (v1.0-v2.0 combined)
- v2.0: 24 plans across 8 phases
- Trend: Stable

**By Milestone:**

| Milestone | Phases | Plans | Status |
|-----------|--------|-------|--------|
| v1.0-v2.0 | 21 | 48 | Complete |
| v2.1 | 4 | TBD | Not started |
| Phase 22-workspace-nostd-audit P01 | 10 | 3 tasks | 10 files |
| Phase 22-workspace-nostd-audit P02 | 2 | 2 tasks | 1 files |
| Phase 23-mqtt-rtcm3-gnss-nvs-crate P02 | 5 | 2 tasks | 4 files |
| Phase 23-mqtt-rtcm3-gnss-nvs-crate P01 | 14 | 3 tasks | 6 files |
| Phase 23-mqtt-rtcm3-gnss-nvs-crate P03 | 7 | 1 tasks | 6 files |
| Phase 24-rinex-files-gnss-ota-gap-crate P03 | 2 | 1 tasks | 3 files |
| Phase 24-rinex-files-gnss-ota-gap-crate P01 | 11 | 1 tasks | 5 files |
| Phase 24-rinex-files-gnss-ota-gap-crate P02 | 7 | 2 tasks | 3 files |
| Phase 25-web-ui-remaining-gap-crate-skeletons P01 | 10 | 2 tasks | 6 files |

## Accumulated Context

### Decisions

Key carry-forward decisions affecting v2.1:
- [v2.0]: Single publish thread owns EspMqttClient; SyncSender<MqttMessage> pattern — server follows similar message-passing design
- [v2.0]: bytes crate for zero-copy RTCM on publish path — server receives these Bytes payloads from MQTT
- [v2.1 planning]: resolver="2" mandatory in workspace root — prevents std feature unification into no_std gap crates (Cargo pitfall)
- [v2.1 planning]: rtcm-rs 0.11 for server decode — avoids hand-rolled MSM cell mask and pseudorange bugs
- [v2.1 planning]: GLONASS carrier phase without FCN is Option::None, written as 16 spaces in RINEX, never 0.0
- [v2.1 revised]: Workspace restructure and nostd audit merged into Phase 22 — both produce no user-facing features and are tightly coupled groundwork; single phase avoids artificial split of interdependent setup work
- [v2.1 revised 2]: Gap crate work interleaved with server feature phases — gnss-nvs in Phase 23, gnss-ota in Phase 24, remaining skeletons in Phase 25; avoids one large gap crate phase blocking delivery feedback; NOSTD-04 split into NOSTD-04a (gnss-ota, Phase 24) and NOSTD-04b (gnss-softap/dns/log, Phase 25)
- [Phase 22-workspace-nostd-audit]: panic=immediate-abort replaced with -C panic=abort rustflag in firmware/.cargo/config.toml — Cargo workspace builds ignore member profiles; panic cannot be scoped per-package in workspace profile overrides
- [Phase 22-workspace-nostd-audit]: Firmware builds from firmware/ directory using .cargo/config.toml with [unstable] build-std; workspace root config has no build.target
- [Phase 22-workspace-nostd-audit]: rust-toolchain.toml kept at workspace root (nightly) and copied to firmware/; both locations ensure nightly is used from any invocation context
- [Phase 22-workspace-nostd-audit]: NVS: log-based KV store with sequential-storage; crates must be ecosystem-reusable (not project-specific)
- [Phase 22-workspace-nostd-audit]: OTA target is esp-hal (not pure no_std); willing to contribute to esp-hal-ota for ESP32-C6
- [Phase 22-workspace-nostd-audit]: NTRIP TLS preferred: rustls unbuffered API with cert-hash pinning in config payload; alternative is RTCM-over-MQTT
- [Phase 22-workspace-nostd-audit]: HTTP server candidates: picoserve (primary) and nanofish (smaller, client+server); evaluate size tradeoff
- [Phase 22-workspace-nostd-audit]: MQTT client: benchmark Phase 23, implement Phase 24; SoftAP SSID: GNSS-[ID] with same value as WPA2 PSK
- [Phase 22-workspace-nostd-audit]: SoftAP portal: WiFi station scan for SSID dropdown; UM980 reset:true field in /config plus first-boot trigger
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: Fresh AsyncClient+EventLoop per reconnect cycle in mqtt_supervisor — avoids rumqttc connection state pollution; subscribe() before poll loop is idiomatic rumqttc (enqueued, not blocking)
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: figment TOML+env config: GNSS_ prefix with __ nesting separator; MqttConfig and MqttMessage marked #[allow(dead_code)] for Phase 23-03 forward-compat
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: sequential-storage 7.1.0 (not 0.5): MapStorage is a typed struct with async methods; PostcardValue does not exist; plan research was based on older version
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: SeqNvsStore uses RefCell<MapStorage> for interior mutability to satisfy NvsStore &self requirement on get/get_blob
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: cargo check --features esp-idf requires riscv32imac-esp-espidf target (run from firmware/ directory)
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: BeiDou ephemeris is RTCM msg1042 (Msg1042T) not 1044 (QZSS) — plan had incorrect type; corrected in rtcm_decode.rs and observation.rs
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: Signal extraction inline in match arms avoids naming private msg1074_sig::DataType — rtcm-rs module subpaths not directly accessible
- [Phase 23-mqtt-rtcm3-gnss-nvs-crate]: MSM4 cnr_dbhz is Option<u8> (df403 inv:0); MSM7 is Option<f64> (df408 inv:0) — MSM4 converted with .map(|v| v as f64) for uniform Observation struct
- [Phase 24-rinex-files-gnss-ota-gap-crate]: gnss-ota gap crate: trait-only with no external deps; OtaSlot + OtaManager via core::fmt::Debug; BLOCKER.md cites esp-rs/esp-hal#3259 and three esp-hal-ota C6 issues
- [Phase 24-rinex-files-gnss-ota-gap-crate]: write_obs LLI/SSI are I1 format (1 char each); plan test expectation was wrong with extra space; RINEX 2.11 obs field = F14.3 + LLI(I1) + SSI(I1) = 16 chars
- [Phase 24-rinex-files-gnss-ota-gap-crate]: pseudorange_ms stores full reconstructed range (rough_int + rough_mod + fine) in Observation; rough_range_ms kept as documentation field; GLONASS carrier_phase_cycles returns None (no FCN in MSM signal data)
- [Phase 24-rinex-files-gnss-ota-gap-crate]: to_d19_12 returns ' 0.000000000000D+00' (19 chars, 1 leading space) — plan example had 2 leading spaces (20 chars typo); RINEX D19.12 is 19 chars total
- [Phase 24-rinex-files-gnss-ota-gap-crate]: write_nav_header uses 'NAVIGATION DATA' label (RINEX 2.11 sec 5.3); plan draft 'N: GPS NAV DATA' was incorrect
- [Phase 25-web-ui-remaining-gap-crate-skeletons]: GP talker (not GN) used in nmea_parse tests — nmea 0.7 parse_gsv rejects GN talker (UnknownGnssType error); in production, GN sentences return None via .ok()?
- [Phase 25-web-ui-remaining-gap-crate-skeletons]: index.html embedded via include_str! (single binary, no runtime file dependency); broadcast::Sender<String> is the fan-out primitive for WebSocket clients

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 24]: rinex 0.21 OBS output format (2.x vs 3.x) unverified without running code — evaluate at Phase 24 start; DIY fallback is ~200-300 lines
- [Phase 24]: rinex 0.21 NAV writer marked under construction — may need DIY fixed-width writer
- [Phase 23]: esp-hal ecosystem moved fast in 2025; re-check esp-radio SoftAP password-protection and embedded-tls TLS 1.2 status before finalising gap table (Phase 22 audit will surface this)
- [Phase 23]: sequential-storage + esp-hal flash driver on ESP32-C6 unverified — include minimal build test in phase

## Session Continuity

Last session: 2026-03-12T09:18:26.810Z
Stopped at: Completed 25-01-PLAN.md
Resume file: None
Next action: /gsd:plan-phase 22
