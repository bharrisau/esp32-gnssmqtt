---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: unknown
last_updated: "2026-03-03T11:44:00Z"
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 6
  completed_plans: 5
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-03)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.
**Current focus:** Phase 2 - Connectivity

## Current Position

Phase: 2 of 3 (Connectivity) — IN PROGRESS
Plan: 4 of 4 in phase 2 — IN PROGRESS (02-04 next)
Status: Phase 2 in progress — wifi.rs done (02-01), mqtt.rs done (02-02), uart_bridge done (02-03), Plan 04 (human-verify) next
Last activity: 2026-03-03 — Plan 02-01 complete: wifi.rs created with wifi_connect and wifi_supervisor (exponential backoff reconnect)

Progress: [█████░░░░░] 50%

## Performance Metrics

**Velocity:**
- Total plans completed: 4
- Average duration: ~26 min
- Total execution time: ~1h 41min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-scaffold | 2 | ~90min | ~45min |
| 02-connectivity | 3 (so far) | ~14min | ~5min |

**Recent Trend:**
- Last 5 plans: 01-01 (~60min), 01-02 (~30min), 02-03 (~4min), 02-02 (~7min), 02-01 (~3min)
- Trend: Phase 2 in progress — 3 of 4 plans done

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
- [02-02]: lwt_topic String declared before MqttClientConfiguration in same scope to satisfy LwtConfiguration<'a> lifetime
- [02-02]: EspMqttConnection moved into pump thread; EspMqttClient in Arc<Mutex<>> — reconnect-aware pattern from esp-idf-svc
- [02-02]: pump_mqtt_events returns ! (diverging); permanent sleep loop after connection.next() exits to keep thread alive
- [02-02]: heartbeat uses client.publish() (blocking) from dedicated thread — acceptable because pump keeps outbox moving
- [02-01]: wifi.start() called only once in wifi_connect — never in supervisor loop (re-init would corrupt driver state)
- [02-01]: 5-second poll sleep is outer loop sleep; backoff sleep placed before wifi.connect() — ensures wait before every reconnect attempt
- [02-01]: wait_netif_up() called on reconnect success before resetting backoff — ensures IP assigned before declaring success

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1 RESOLVED]: Pinned esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1 — build confirmed working
- [Phase 1 RESOLVED]: Hardware flash verified — device ID FFFEB5 stable, all SCAF requirements met
- [Phase 2]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before Phase 3 BLE provisioning work (v2 milestone)
- [01-01 NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-03
Stopped at: Completed 02-01-PLAN.md (wifi.rs created: wifi_connect + wifi_supervisor with exponential backoff reconnect)
Resume file: .planning/phases/02-connectivity/02-04-PLAN.md (human-verify checkpoint — wire main.rs, cargo build, flash + test full connectivity stack)
