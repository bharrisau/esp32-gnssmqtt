---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: unknown
last_updated: "2026-03-03T11:23:00Z"
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 6
  completed_plans: 3
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-03)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.
**Current focus:** Phase 2 - Connectivity

## Current Position

Phase: 2 of 3 (Connectivity) — IN PROGRESS
Plan: 3 of 4 in phase 2 — COMPLETE (02-03)
Status: Phase 2 in progress — uart_bridge done, Plan 04 (human-verify) next
Last activity: 2026-03-03 — Plan 02-03 complete: uart_bridge.rs created with bidirectional UART1/USB-CDC bridge

Progress: [███░░░░░░░] 30%

## Performance Metrics

**Velocity:**
- Total plans completed: 3
- Average duration: ~35 min
- Total execution time: ~1h 34min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-scaffold | 2 | ~90min | ~45min |
| 02-connectivity | 1 (so far) | ~4min | ~4min |

**Recent Trend:**
- Last 5 plans: 01-01 (~60min), 01-02 (~30min), 02-03 (~4min)
- Trend: Phase 2 in progress

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Init]: Use `esp-idf-hal` + `esp-idf-svc` (IDF std path) — not bare-metal esp-hal; required for WiFi, BLE, NVS, MQTT on ESP32-C6
- [Init]: UM980 init commands delivered via retained MQTT topic — enables remote reconfiguration without reflash
- [Init]: Per-sentence MQTT topics (`nmea/{TYPE}`) — consumers subscribe selectively
- [Init]: Device ID from ESP32 hardware serial — unique per-device without manual configuration
- [01-01]: esp-idf-svc =0.51.0 / esp-idf-hal =0.45.2 / esp-idf-sys =0.36.1 with = pinning for build reproducibility
- [01-01]: embuild manages ESP-IDF v5.3.3 download — no manual SDK setup required
- [01-01]: Device ID from last 3 MAC bytes (first 3 are Espressif OUI, not unique)
- [01-01]: nightly Rust toolchain required for RISC-V esp-idf-sys build-std support
- [01-02]: Device ID FFFEB5 confirmed as permanent identifier for this hardware unit (eFuse-derived)
- [01-02]: Factory partition must extend to end of flash; 4MB XIAO ESP32-C6 needs 0x3E0000 factory size
- [01-02]: CONFIG_PARTITION_TABLE_CUSTOM=y and CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y required in sdkconfig.defaults
- [01-02]: Windows build.rs must copy partitions.csv (no symlinks without Developer Mode)
- [02-03]: Arc<UartDriver> used for thread-safe UART sharing — fallback is Arc<Mutex<UartDriver>> if UartDriver not Send
- [02-03]: stdin()/stdout() used for USB CDC side — unverified for XIAO ESP32-C6 USB JTAG; Plan 04 checkpoint will confirm
- [02-03]: NON_BLOCK + 10ms sleep in UM980->USB poll thread avoids FreeRTOS watchdog trips

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1 RESOLVED]: Pinned esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1 — build confirmed working
- [Phase 1 RESOLVED]: Hardware flash verified — device ID FFFEB5 stable, all SCAF requirements met
- [Phase 2]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before Phase 3 BLE provisioning work (v2 milestone)
- [01-01 NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-03
Stopped at: Completed 02-03-PLAN.md (uart_bridge.rs created, bidirectional UART1/USB-CDC bridge)
Resume file: .planning/phases/02-connectivity/02-04-PLAN.md (human-verify checkpoint — flash and test bridge)
