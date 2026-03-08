---
phase: 17-ntrip-client
plan: 02
subsystem: ntrip-integration
tags: [ntrip, mqtt, main-rs, wiring, heartbeat]
dependency_graph:
  requires: [17-01]
  provides: [NTRIP-02, NTRIP-04]
  affects: [src/main.rs, src/mqtt.rs]
tech_stack:
  added: []
  patterns: [SyncSender channel dispatch, AtomicU8 state read, mpsc channel wiring]
key_files:
  created: []
  modified:
    - src/mqtt.rs
    - src/main.rs
decisions:
  - ntrip/config dispatch placed BEFORE /config branch to prevent routing collision (both end with /config)
  - NTRIP_BACKOFF_STEPS kept in ntrip_client.rs as module-local const — no config.rs addition needed
  - uart_arc captured from spawn_gnss 5th return value without any gnss.rs changes (already done in Plan 01)
metrics:
  duration_min: 8
  completed: "2026-03-08"
  tasks_completed: 2
  files_modified: 2
---

# Phase 17 Plan 02: NTRIP Client Wiring Summary

**One-liner:** NTRIP client wired into firmware via ntrip_config channel, /ntrip/config MQTT subscription, and heartbeat ntrip field.

## What Was Built

Plan 02 completed the NTRIP integration by connecting the `ntrip_client.rs` module (built in Plan 01) to the rest of the firmware. Two files were modified: `mqtt.rs` received the NTRIP config dispatch, subscription, and heartbeat extension; `main.rs` received the module declaration, channel creation, mqtt_connect update, and spawn call.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Extend mqtt.rs with ntrip_config dispatch, subscription, heartbeat | a64a130 | src/mqtt.rs |
| 2 | Wire ntrip_client into main.rs; declare mod and update channel + call sites | b688248 | src/main.rs |

## Key Changes

**src/mqtt.rs:**
- `mqtt_connect` gains `ntrip_config_tx: SyncSender<Vec<u8>>` parameter (before `led_state`)
- `EventPayload::Received` dispatch: `/ntrip/config` branch added BEFORE `/config` branch (prevents routing collision since `/ntrip/config` ends with `/config`)
- `subscriber_loop`: subscribes to `gnss/{device_id}/ntrip/config` at `QoS::AtLeastOnce` on every connection (ensures retained config re-delivery on broker reconnect)
- `heartbeat_loop`: reads `crate::ntrip_client::NTRIP_STATE` atomic and adds `"ntrip":"connected"/"disconnected"` to JSON

**src/main.rs:**
- `mod ntrip_client;` added after `mod log_relay;`
- `spawn_gnss` destructure updated to capture `uart_arc` as 5th element
- `ntrip_config` channel created (`sync_channel::<Vec<u8>>(4)`) after `log_level` channel
- `mqtt_connect` call updated with `ntrip_config_tx` before `led_state_mqtt`
- `ntrip_client::spawn_ntrip_client(Arc::clone(&uart_arc), ntrip_config_rx, nvs.clone())` called at Step 17b

## Build Verification

`cargo build --release` exits 0 with no errors. Only pre-existing warnings remain (unused constants in config.rs, unused `wifi_connect` fn, uart_bridge comparison warning).

## Requirements Satisfied

- **NTRIP-02**: Runtime config via MQTT retained topic — dispatcher routes `/ntrip/config` payloads to ntrip_client thread; subscription at AtLeastOnce ensures re-delivery on reconnect
- **NTRIP-04**: Heartbeat field — `"ntrip":"connected"` or `"ntrip":"disconnected"` included in every heartbeat JSON

All 4 NTRIP requirements (NTRIP-01..04) are now implemented end-to-end in the compiled firmware.

## Deviations from Plan

**1. [Rule 2 - Missing Critical Functionality] ntrip/config dispatch placed before /config**
- **Found during:** Task 1 analysis
- **Issue:** `t.ends_with("/config")` would match `/ntrip/config` too — NTRIP config payloads would be routed to the device config channel instead of the NTRIP client
- **Fix:** Added `/ntrip/config` branch BEFORE the `/config` branch in the `EventPayload::Received` match
- **Files modified:** src/mqtt.rs
- **Commit:** a64a130

Note: This was implicit in the plan (which said "Place before the final // All other topics comment") but the plan didn't call out the routing collision explicitly. The fix ensures correctness.

## Self-Check: PASSED

- [x] src/main.rs exists and contains `mod ntrip_client`, channel, mqtt call, spawn
- [x] src/mqtt.rs exists and contains dispatch, subscription, NTRIP_STATE heartbeat read
- [x] Commits a64a130 and b688248 exist
- [x] `cargo build --release` exits 0
