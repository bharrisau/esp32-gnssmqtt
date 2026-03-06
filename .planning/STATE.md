---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: GNSS Relay
status: verifying
stopped_at: Completed 06-02-PLAN.md
last_updated: "2026-03-06T23:55:49.568Z"
last_activity: "2026-03-07 — Plan 04-02 complete: uart_bridge refactored TX-only, main.rs wired to gnss::spawn_gnss, hardware-verified"
progress:
  total_phases: 6
  completed_phases: 6
  total_plans: 15
  completed_plans: 15
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-03)

**Core value:** NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with zero-touch provisioning and remote reconfiguration of the GNSS module.
**Current focus:** Phase 3 - GNSS

## Current Position

Phase: 4 of 4 (04-uart-pipeline) — COMPLETE
Plan: 2 of 2 in phase 04 — COMPLETE
Status: Phase 04 COMPLETE — all UART-01 through UART-03 verified on hardware. Full GNSS UART pipeline operational on device FFFEB5.
Last activity: 2026-03-07 — Plan 04-02 complete: uart_bridge refactored TX-only, main.rs wired to gnss::spawn_gnss, hardware-verified

Progress: [██████████] 100% (Phase 2) — Phase 3 not yet planned

## Performance Metrics

**Velocity:**
- Total plans completed: 4
- Average duration: ~26 min
- Total execution time: ~1h 41min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-scaffold | 2 | ~90min | ~45min |
| 02-connectivity | 4 | ~18min | ~5min |

**Recent Trend:**
- Last 5 plans: 01-02 (~30min), 02-01 (~3min), 02-02 (~7min), 02-03 (~4min), 02-04 (~4min)
- Trend: Phase 2 complete — 4 of 4 plans done, all CONN requirements hardware-verified

*Updated after each plan completion*
| Phase 03-status-led P01 | 2 | 2 tasks | 3 files |
| Phase 03-status-led P03-02 | 10 | 2 tasks | 2 files |
| Phase 03-status-led P03-03 | 15 | 2 tasks | 0 files |
| Phase 04-uart-pipeline P01 | 2 | 1 tasks | 1 files |
| Phase 04-uart-pipeline P02 | 5 | 2 tasks | 2 files |
| Phase 05-nmea-relay P01 | 2 | 2 tasks | 2 files |
| Phase 05-nmea-relay P02 | 10 | 2 tasks | 1 files |
| Phase 06-remote-config P01 | 2 | 2 tasks | 2 files |
| Phase 06-remote-config P02 | 30 | 2 tasks | 1 files |

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
- [02-03]: stdin()/stdout() used for USB CDC side — VERIFIED working on XIAO ESP32-C6 USB JTAG in Plan 04 hardware test (CONN-07)
- [02-03]: NON_BLOCK + 10ms sleep in UM980->USB poll thread avoids FreeRTOS watchdog trips
- [02-02]: lwt_topic String declared before MqttClientConfiguration in same scope to satisfy LwtConfiguration<'a> lifetime
- [02-02]: EspMqttConnection moved into pump thread; EspMqttClient in Arc<Mutex<>> — reconnect-aware pattern from esp-idf-svc
- [02-02]: pump_mqtt_events returns ! (diverging); permanent sleep loop after connection.next() exits to keep thread alive
- [02-02]: heartbeat uses client.publish() (blocking) from dedicated thread — acceptable because pump keeps outbox moving
- [02-04]: UART bridge wired to uart0 GPIO16 (TX) / GPIO17 (RX) — plan showed uart1/gpio20/gpio21 but real XIAO ESP32-C6 hardware uses uart0/gpio16/gpio17
- [02-04]: 3-thread MQTT (pump + subscriber_loop + heartbeat) preserved in main.rs — pump NEVER holds client reference, signals subscriber via mpsc channel
- [02-01]: wifi.start() called only once in wifi_connect — never in supervisor loop (re-init would corrupt driver state)
- [02-01]: 5-second poll sleep is outer loop sleep; backoff sleep placed before wifi.connect() — ensures wait before every reconnect attempt
- [02-01]: wait_netif_up() called on reconnect success before resetting backoff — ensures IP assigned before declaring success
- [Phase 03-status-led]: Arc<AtomicU8> chosen for LED state — single u8, no lock contention on 50ms LED poll path
- [Phase 03-status-led]: wifi_supervisor never writes Connected — MQTT pump owns that transition to prevent false green before MQTT is ready
- [Phase 03-status-led]: elapsed_ms counter over sleep-per-blink — state changes apply within 50ms not at end of blink cycle
- [Phase 03-status-led]: pump_mqtt_events uses Ordering::Relaxed for LED atomic stores — visual-only, no happens-before required
- [Phase 03-status-led]: LED thread spawned at Step 3e before WiFi/MQTT init — observer ready before writers
- [Phase 03-status-led]: LED-03 error burst accepted via code inspection + WiFi reconnect test — triggering 3x max-backoff on hardware requires sustained AP disable (~3 min) which was not performed
- [Phase 04-uart-pipeline]: Arc<UartDriver> chosen over Arc<Mutex<UartDriver>> for GNSS thread sharing — read/write take &self, no Mutex needed
- [Phase 04-uart-pipeline]: 512-byte line_buf in RX thread covers UM980 proprietary sentences exceeding standard 82-byte NMEA limit
- [Phase 04-uart-pipeline]: uart_bridge refactored to Sender<String> parameter — UART ownership exclusively in gnss.rs
- [Phase 04-uart-pipeline]: gnss_cmd_tx.clone() to uart_bridge, original retained in main.rs for Phase 6
- [Phase 04-uart-pipeline]: Explicit _gnss_cmd_tx and _nmea_rx bindings in idle loop document Phase 5/6 handoff points
- [Phase 05-nmea-relay]: sync_channel(64) chosen over unbounded channel — RX thread must not block on UART reads when relay is slow
- [Phase 05-nmea-relay]: QoS::AtMostOnce (QoS 0) / retain=false for NMEA relay — real-time sentences, retransmission of stale positions is harmful
- [Phase 05-nmea-relay]: Mutex acquired per-sentence in nmea_relay — released each iteration to prevent heartbeat/subscriber thread starvation at 10+ Hz
- [Phase 05-nmea-relay]: nmea_rx moved into spawn_relay at Step 14 — placeholder _nmea_rx removed, compiler enforces single consumer
- [Phase 05-nmea-relay]: Hardware tested at 10 msg/sec — sync_channel(64) sufficient, no relay channel full warnings at normal UM980 NMEA output rate
- [Phase 06-remote-config]: djb2 hash chosen for payload deduplication — non-cryptographic, adequate for retained MQTT messages; 100ms default per-command delay overridable via delay_ms JSON field; gnss_cmd_tx send failure logs + abandons (no panic)
- [Phase 06-remote-config]: All three CONF requirements hardware-verified on device FFFEB5 — config_relay wired into main.rs, relay operational end-to-end with djb2 hash dedup and per-command delay

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 1 RESOLVED]: Pinned esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1 — build confirmed working
- [Phase 1 RESOLVED]: Hardware flash verified — device ID FFFEB5 stable, all SCAF requirements met
- [Phase 2]: BLE GATT server API (`esp-idf-svc::bt`) was volatile as of mid-2025 — verify before Phase 3 BLE provisioning work (v2 milestone)
- [01-01 NOTE]: Fresh clone needs `cargo install ldproxy` and first build needs git submodule init in ESP-IDF dir (embuild auto-handles submodules on subsequent builds)

## Session Continuity

Last session: 2026-03-06T23:55:49.565Z
Stopped at: Completed 06-02-PLAN.md
Resume file: None
