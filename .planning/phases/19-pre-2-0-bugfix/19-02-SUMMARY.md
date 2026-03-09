---
phase: 19-pre-2-0-bugfix
plan: "02"
subsystem: provisioning, mqtt
tags: [bugfix, nvs, tls, ota, mqtt]
requirements: [BUG-3, BUG-4]

dependency_graph:
  requires: [19-01]
  provides: [NVS schema v1, TLS-aware mqtt_connect]
  affects: [src/provisioning.rs, src/mqtt.rs, src/main.rs]

tech_stack:
  added: []
  patterns:
    - "NVS schema versioning via config_ver u8 key — written on every save for future detection"
    - "tls: bool threaded from NVS through provisioning -> main -> mqtt_connect -> broker_url scheme"

key_files:
  created: []
  modified:
    - src/provisioning.rs
    - src/mqtt.rs
    - src/main.rs

decisions:
  - "TLS defaults false on key absence — old firmware never wrote mqtt_tls; absence == plain MQTT"
  - "config_ver=1 written on every save (not just first) — idempotent and robust"
  - "broker_url scheme switches mqtt:// vs mqtts:// based on tls bool — no MqttClientConfiguration field needed"
  - "Fallback tuple in main.rs also provides false for tls — compile-time config always uses plain MQTT"

metrics:
  duration_minutes: 2
  completed_date: "2026-03-09"
  tasks_completed: 2
  files_modified: 3
---

# Phase 19 Plan 02: NVS TLS Versioning (BUG-3/BUG-4) Summary

**One-liner:** Fixed post-OTA MQTT failure by defaulting mqtt_tls to false on key absence and threading tls: bool from NVS through provisioning to mqtt_connect broker_url scheme selection.

## What Was Built

Two tightly coupled changes that together resolve BUG-3 (NVS TLS default) and BUG-4 (MQTT failure post-OTA):

**provisioning.rs — Task 1:**
- `load_mqtt_config` return type extended from `Option<(String, u16, String, String)>` to `Option<(String, u16, String, String, bool)>`. The fifth element is `tls: bool`.
- TLS read uses `nvs.get_u8("mqtt_tls").unwrap_or(None).unwrap_or(0) != 0` — key absence produces `false` (plain MQTT). Old firmware never wrote this key, so devices upgraded via OTA will correctly default to plain MQTT.
- `save_credentials` now writes `mqtt_tls=0` and `config_ver=1` before `Ok(())` on every credential save. `config_ver=1` is a forward-looking NVS schema version for future firmware to detect schema compatibility.

**mqtt.rs + main.rs — Task 2:**
- `mqtt_connect` gains `tls: bool` parameter after `pass: &str`. The `#[allow(clippy::too_many_arguments)]` attribute was already present — no change needed.
- `broker_url` construction now switches scheme: `mqtt://` when `tls == false`, `mqtts://` when `tls == true`.
- `main.rs` destructures the new 5-tuple from `load_mqtt_config`, passes `mqtt_tls` to `mqtt_connect`. The fallback `unwrap_or_else` tuple also supplies `false` for tls.

## Verification

- `cargo clippy -- -D warnings`: clean (0 warnings, 0 errors)
- `cargo build --release`: succeeded

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check: PASSED

- src/provisioning.rs: FOUND
- src/mqtt.rs: FOUND
- src/main.rs: FOUND
- Commit d3ec49d: FOUND
- Commit 61bbf6b: FOUND
