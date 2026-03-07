---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Observations + OTA
status: completed
stopped_at: Completed 08-03 OTA wiring — full Phase 8 OTA complete, v1.2 milestone complete
last_updated: "2026-03-07T05:56:40.504Z"
last_activity: 2026-03-07 — OTA prerequisites complete, hardware reflash verified
progress:
  total_phases: 2
  completed_phases: 2
  total_plans: 6
  completed_plans: 6
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.
**Current focus:** v1.2 Observations + OTA — COMPLETE

## Current Position

Phase: 8 — OTA (complete)
Plan: 08-03 (complete — all plans done)
Status: Phase 8 complete; milestone v1.2 complete
Last activity: 2026-03-07 — OTA wiring complete, hardware Test 1 verified, v1.2 shipped

```
v1.2 progress: [██████████] 100% (15/15 plans complete)
```

## Accumulated Context

### Decisions

All decisions logged in PROJECT.md Key Decisions table.
- [Phase 07-rtcm-relay]: Box<[u8; 1029]> for RtcmBody buffer to avoid stack overflow even with 12288 stack
- [Phase 07-rtcm-relay]: Complete RTCM frame published (preamble+header+payload+CRC) not just payload for independent CRC verification by consumers
- [Phase 07-rtcm-relay]: Silent drop for non-/config topics in pump_mqtt_events to avoid log spam during Phase 8 OTA retain playback
- [Phase 07-rtcm-relay]: out_buffer_size: 2048 in MqttClientConfiguration to support 1029-byte RTCM MSM7 frames
- [Phase 07-rtcm-relay]: Plan 07-03 wiring was pre-completed as auto-fix in 07-01 — all main.rs changes (mod rtcm_relay, 3-value destructure, spawn_relay call) already in place; firmware compiles cleanly
- [Phase 08-ota]: Removed phy_init partition — ESP-IDF v5 embeds phy calibration in NVS; not required on ESP32-C6
- [Phase 08-ota]: sha2 = 0.10 with default-features = false + oid feature for no_std SHA-256 streaming OTA verification
- [Phase 08-ota]: CONFIG_ESP_TASK_WDT_TIMEOUT_S=30 chosen over runtime TWDT feed — simpler, sufficient for single OTA thread erase window
- [Phase 08-ota]: sha256 field required in OTA trigger payload — reject missing sha256 with failed state
- [Phase 08-ota]: spawn_ota() thread stack 16384 bytes — HTTP + SHA + OTA handle exceed 8192 default
- [Phase 08-ota]: espflash.toml [idf_format_args] partition_table required — cargo espflash flash silently uses default partition layout without it; OTA slots absent
- [Phase 08-ota]: mark_running_slot_valid() non-fatal — factory partition has no OTA slot so expect() panics; warn and continue keeps device operational

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before Phase 8 implementation (read `~/.cargo/registry/src/.../esp-idf-svc-0.51.0/Cargo.toml`; takes 2 minutes; resolves whether `features = ["ota"]` is needed)

### Blockers/Concerns

- [Phase 8 PREREQUISITE RESOLVED]: `partitions.csv` redesign + USB reflash complete — device boots cleanly from ota_0. OTA code is now testable.
- [Phase 8 PITFALL]: `mark_running_slot_valid()` must be called early in `main()` after UART init succeeds on every boot — omitting it causes rollback on every reboot when `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`.
- [Phase 8 PITFALL]: OTA download must run in independent `ota.rs` thread — running inside MQTT pump blocks `connection.next()`, causes keep-alive timeout and broker disconnect.
- [Phase 8 PITFALL]: Watchdog fires during OTA partition erase (4-8 seconds); use sequential erase mode from the start.
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)
- [Future]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before BLE provisioning work (future milestone)

## Session Continuity

Last session: 2026-03-07T05:56:40.501Z
Stopped at: Completed 08-03 OTA wiring — full Phase 8 OTA complete, v1.2 milestone complete
Resume file: None
Next action: Plan next milestone (v1.3) or run `/gsd:plan` for next phase
