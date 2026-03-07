---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Reliability Hardening
status: completed
stopped_at: Completed 13-01-PLAN.md
last_updated: "2026-03-07T14:54:42.096Z"
last_activity: "2026-03-07 — 09-02 executed: recv_timeout loops on all 6 channels, WiFi consecutive_failures counter"
progress:
  total_phases: 7
  completed_phases: 7
  total_plans: 15
  completed_plans: 15
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.
**Current focus:** v1.3 Reliability Hardening — Phase 9 complete; next phase TBD

## Current Position

Phase: 9 — Channel + Loop Hardening (COMPLETE)
Plan: 09-02 (complete)
Status: Both plans complete; Phase 9 done
Last activity: 2026-03-07 — 09-02 executed: recv_timeout loops on all 6 channels, WiFi consecutive_failures counter

```
v1.3 Progress: [==        ] 1/5 phases complete (Phase 9 complete: 2/2 plans done)
```

## Accumulated Context

### Decisions

All decisions from v1.0–v1.2 logged in PROJECT.md Key Decisions table.

Key v1.2 decisions carried forward:
- [Phase 07-rtcm-relay]: Box<[u8; 1029]> for RtcmBody buffer to avoid stack overflow even with 12288 stack
- [Phase 07-rtcm-relay]: Complete RTCM frame published (preamble+header+payload+CRC) for independent CRC verification by consumers
- [Phase 08-ota]: mark_running_slot_valid() non-fatal — factory partition has no OTA slot; warn and continue
- [Phase 08-ota]: espflash.toml [idf_format_args] partition_table required — cargo espflash flash silently uses default partition layout without it

Key v1.3 decisions (Phase 9):
- [Phase 09-01]: sync_channel(16) for cmd_tx: config batch typically <=16 commands; capacity 16 prevents blocking UART TX drain
- [Phase 09-01]: sync_channel(2/4/1) for subscribe/config/ota_tx: rationale per channel (reconnect burst / retained replay / OTA exclusivity)
- [Phase 09-01]: config_relay.apply_config() keeps blocking send() — not a hot-path thread; blocking on full 16-slot channel is acceptable
- [Phase 09-01]: uart_bridge uses try_send — interactive stdin path must not stall on full command channel
- [Phase 09-01]: UART_TX_ERRORS AtomicU32 counter accumulates write errors; will be read by Phase 13 health telemetry
- [Phase 09-02]: config.example.rs updated with non-credential constants — project convention: config.rs gitignored, example.rs is committed template
- [Phase 09-02]: consecutive_failures replaces max_backoff_failures in wifi_supervisor — counts every failure, resets on success, gives accurate at-limit logging
- [Phase 09-02]: Timeout arm is no-op (continue) in all recv_timeout loops — Phase 11 will feed watchdog heartbeat counters here without structural changes
- [Phase 09-02]: Dead-end park loop after break preserves -> ! semantics on all affected threads
- [Phase 10-memory-diagnostics]: esp_idf_svc::sys full path for HWM calls — no new use imports needed, direct Cargo.toml dep accessible via full path in Rust 2021
- [Phase 10-memory-diagnostics]: RTCM_POOL_SIZE=4 buffers (4116 bytes) allocated once at spawn_gnss init; pool exhaustion drops frame with warn log; buffer returned on all error paths
- [Phase 11-01]: spawn_supervisor() call deferred to Plan 02 — module declaration only in Plan 01 so Plan 02 compiler errors are isolated to wiring
- [Phase 11-01]: 4096-byte stack for watchdog supervisor: no I/O or buffers, only loop + arithmetic + log
- [Phase 11-thread-watchdog]: Heartbeat in GNSS RX at top of loop{} not inside match arm — UART stall returning Ok(0) would freeze counter if inside Ok(n) arm
- [Phase 11-thread-watchdog]: spawn_supervisor() as Step 18 (last spawn) — supervisor first check occurs after all monitored threads are live
- [Phase 11-thread-watchdog]: CONFIG_ESP_TASK_WDT_PANIC=y in sdkconfig.defaults — hardware TWDT reboots if supervisor itself hangs (WDT-02 criterion 3)
- [Phase 12-resilience]: AtomicU32 not AtomicU64 for MQTT_DISCONNECTED_AT — ESP32 Xtensa target lacks AtomicU64; u32 epoch seconds safe for 5-min RESIL-02 window
- [Phase 12-resilience]: RESIL-01 uses Option<Instant> local to wifi_supervisor — no cross-thread sharing needed for WiFi disconnect duration tracking
- [Phase 12-resilience]: MQTT timer cleared in !connected arm of wifi_supervisor — prevents RESIL-02 false-trigger during combined WiFi+MQTT outage
- [Phase 12-resilience]: compare_exchange(0, now_secs()) in Disconnected arm — only first disconnect stamps the timer; subsequent events no-op via .ok()
- [Phase 12-resilience]: store(0, Relaxed) in Connected arm before subscribe_tx.try_send() — timer cleared as early as possible on reconnect
- [Phase 13]: HEARTBEAT_INTERVAL_SECS constant added to config.example.rs for operator-visible interval setting; drop counters cumulative since boot (no reset)

### Pending Todos

- Verify `esp-idf-svc-0.51.0` OTA Cargo feature name before any OTA changes (read `~/.cargo/registry/src/.../esp-idf-svc-0.51.0/Cargo.toml`)

### Blockers/Concerns

- [Future]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before BLE provisioning work (future milestone)
- [Build NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-07T14:54:42.092Z
Stopped at: Completed 13-01-PLAN.md
Resume file: None
Next action: `/gsd:execute-phase <next-phase>` — Phase 10 or as per ROADMAP.md
