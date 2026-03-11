# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 (XIAO Seeed) that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences and RTCM3 correction frames from the UM980 over UART, publishes each to dedicated MQTT topics in real time, accepts remote UM980 reconfiguration via retained MQTT config, supports MQTT-triggered OTA firmware updates with rollback safety, and publishes periodic health telemetry including GNSS fix quality.

v2.0 shipped: field deployment — SoftAP web provisioning (no recompile), NTRIP corrections client with TLS, remote log streaming, captive portal with DNS hijack, GNSS fix telemetry, post-field bug fixes (captive portal probes, MQTT throughput, UM980 config persistence), and MQTT publish thread refactor eliminating Arc<Mutex> contention.

## Core Value

GNSS data (NMEA + RTCM3) from the UM980 is reliably delivered to the MQTT broker in real time, with remote reconfiguration, OTA updates, and automatic recovery — safe for unattended operation.

## Requirements

### Validated

- ✓ Project compiles for ESP32-C6 (riscv32imac-esp-espidf), flashes via espflash — v1.0
- ✓ ESP-IDF crate versions pinned with `=` specifiers — v1.0
- ✓ sdkconfig.defaults: UART RX ring buffer 4096+ bytes, FreeRTOS stack overflow detection — v1.0
- ✓ partitions.csv: NVS partition 64KB+ — v1.0
- ✓ Device ID from hardware eFuse MAC, stable across power cycles — v1.0
- ✓ WiFi connect with compile-time credentials — v1.0
- ✓ MQTT connect with LWT, heartbeat, pump thread, re-subscribe on reconnect — v1.0
- ✓ WiFi exponential backoff reconnect supervisor — v1.0
- ✓ USB-serial ↔ UM980 UART bridge (UART0, GPIO16/17) — v1.0
- ✓ Status LED (GPIO15 active-low): connecting blink / connected steady / error burst — v1.0
- ✓ Device reads raw NMEA bytes from UM980 UART at 115200 baud 8N1 — v1.1
- ✓ Each valid NMEA sentence published to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` at QoS 0 — v1.1
- ✓ Device subscribes to `gnss/{device_id}/config` and forwards payload to UM980 over UART TX with djb2 hash deduplication — v1.1
- ✓ RTCM3 frames published to `gnss/{device_id}/rtcm/{msg_type}` with CRC-24Q verification — v1.2
- ✓ OTA firmware update: MQTT-triggered, HTTP download, SHA-256 verify, dual-slot rollback — v1.2
- ✓ All mpsc channels bounded (`sync_channel`) with documented capacities; `recv_timeout` on all blocking receives — v1.3
- ✓ UART TX write failures logged and counted (AtomicU32 UART_TX_ERRORS) — v1.3
- ✓ RTCM hot path: pre-allocated 4-buffer pool, zero per-frame heap allocation in steady state — v1.3
- ✓ FreeRTOS stack HWM logged at entry of all 11 spawned threads — v1.3
- ✓ Thread watchdog: GNSS RX + MQTT pump feed heartbeat atomics; supervisor reboots after 3 missed beats (15s) — v1.3
- ✓ Auto-reboot after 10min WiFi disconnect or 5min MQTT disconnect while WiFi up — v1.3
- ✓ JSON health heartbeat (`uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`) to `/heartbeat` every 30s — v1.3
- ✓ Retained `"online"` published to `/status` on every MQTT reconnect (clears LWT) — v1.3
- ✓ UM980 `#`-prefixed query responses routed to `gnss/{device_id}/nmea/response` — post-v1.3
- ✓ Free-text UM980 output mirrored to stdout for espflash monitor visibility — post-v1.3
- ✓ SNTP wall-clock time sync on WiFi connect — v2.0
- ✓ UM980 command relay via `gnss/{id}/command` MQTT topic — v2.0
- ✓ Remote reboot trigger via OTA topic — v2.0
- ✓ SoftAP web provisioning portal — WiFi (3 SSIDs) + MQTT + NTRIP credentials without recompile — v2.0
- ✓ Multi-AP failover — device retries stored networks indefinitely with backoff — v2.0
- ✓ GPIO9 button: 3s SoftAP re-entry / 10s factory reset; same 300s idle timeout — v2.0
- ✓ ESP-IDF remote log streaming to `gnss/{id}/log` with re-entrancy guard; runtime level config — v2.0
- ✓ NTRIP v1 client: TCP + TLS (EspTls/mbedTLS) streams RTCM3 to UM980 UART for RTK fix; auto-reconnect — v2.0
- ✓ Captive portal DNS hijack (port 53 UDP) — Android/iOS/Windows probe handlers for seamless SoftAP onboarding — v2.0
- ✓ GNSS fix telemetry: heartbeat includes fix_type, satellites, HDOP from GGA atomics — v2.0
- ✓ UM980 GNSS config persisted to NVS and auto-reapplied after UM980 hardware reboot — v2.0
- ✓ MQTT publish thread refactor: single publish thread owns EspMqttClient exclusively; SyncSender<MqttMessage> across all relay threads; bytes crate for zero-copy RTCM — v2.0
- ✓ MQTT outbox observability: MQTT_ENQUEUE_ERRORS + MQTT_OUTBOX_DROPS atomics; bench:N trigger for field diagnostics — v2.0

## Current Milestone: v2.1 Server and nostd Foundation

**Goal:** Build a companion Rust server that converts MQTT telemetry into RINEX files and a live web UI, while auditing and scaffolding the embassy/nostd crate ecosystem needed to eventually port the firmware off ESP-IDF.

**Target features:**
- MQTT-subscribed server binary: RTCM3 MSM decode → RINEX 2.x .26O/.26P files with hourly rotation
- HTTP + WebSocket server: live skyplot (polar SVG), SNR bar chart, device health panel
- Complete ESP-IDF dependency audit against embassy/nostd equivalents
- Gap crate skeletons with trait definitions for every missing nostd capability (NVS, OTA, SoftAP, NTRIP TLS…)
- Begin implementation of priority gap crates

### Active

- [ ] Server subscribes to MQTT RTCM3 and NMEA topics for a configured device ID
- [ ] Server decodes RTCM3 MSM messages to extract pseudorange, carrier phase, and SNR observations
- [ ] Server decodes RTCM3 ephemeris messages (1019/1020/1044/1045) for GPS/GLONASS/BeiDou/Galileo
- [ ] Server writes RINEX 2.x observation files (.26O) with hourly rotation
- [ ] Server writes RINEX 2.x mixed navigation files (.26P) with hourly rotation
- [ ] HTTP server with WebSocket pushes live satellite state to browser
- [ ] Browser renders polar skyplot SVG (elevation/azimuth per satellite from NMEA GSV)
- [ ] Browser renders SNR bar chart per satellite
- [ ] Browser shows device health panel from MQTT heartbeat
- [ ] Complete audit of all ESP-IDF crate dependencies mapped to nostd/embassy equivalents
- [ ] Gap crates created with trait definitions for each capability lacking a nostd solution
- [ ] Priority gap crates (NVS at minimum) begin implementation

### Out of Scope

- TLS/mTLS for MQTT — separate milestone
- Full NMEA field parsing — firmware relays, consumers parse downstream
- Local NMEA buffering across power cycles — real-time relay only
- JSON-wrapped NMEA publish — raw NMEA preferred
- Multi-broker publishing — single broker only

## Context

- **Hardware**: Seeed XIAO ESP32-C6 — RISC-V, WiFi 6, single yellow LED GPIO15 active-low
- **GNSS**: UM980 multi-band RTK receiver, UART0 at 115200 baud (GPIO16 TX, GPIO17 RX)
- **Language**: Rust with std via esp-idf-svc/hal/sys (ESP-IDF v5.3.3)
- **MQTT broker**: External (Mosquitto/HiveMQ); username/password auth, no TLS in v1
- **Shipped v2.0**: 4,726 lines of Rust, 21 phases, 48 plans total; device FFFEB5
- **UM980 UART protocol**: NMEA sentences (`$`-prefix), RTCM3 frames (`0xD3`-prefix), `#`-prefix query responses (checksum-terminated); free-text banners otherwise
- **UM980 config**: Configured via retained MQTT config topic at boot; RESET causes reboot (wait required), UNLOG cleans NMEA outputs without reboot; avoid CONFIGSAVE (NVM wear)

## Constraints

- **Tech stack**: Rust (esp-idf-hal, esp-idf-svc, esp-idf-sys)
- **Hardware**: ESP32-C6 only
- **UART**: UM980 fixed at 115200 baud, 8N1
- **MQTT**: No TLS in v1; standard MQTT 3.1.1

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Device ID from ESP32 hardware eFuse | Unique per-device without manual config | ✓ Good — last 3 bytes of MAC as 6-char hex |
| Hardcoded credentials for v1 | BLE provisioning deferred; gets firmware running fast | ✓ Good — delivered v1.0 on schedule |
| MQTT pump thread never calls client methods | Avoids internal mutex deadlock in ESP-IDF MQTT task | ✓ Good — mpsc channel to subscriber_loop solves it cleanly |
| `Arc<AtomicU8>` for LED state | Lock-free; LED ownership stays in led_task | ✓ Good — no contention, Relaxed ordering sufficient |
| Connected state written only by MQTT pump | Single writer for Connected; WiFi supervisor writes Connecting/Error | ✓ Good — clear ownership, no races |
| UART bridge on UART0/GPIO16-17 (not UART1/GPIO20-21) | Hardware reality — UM980 physically wired to GPIO16/17 | ✓ Corrected during Phase 2 execution |
| `disable_clean_session: true` | Broker remembers subscriptions across reconnects (not broker restarts) | ✓ Good |
| LWT lifetime: String declared before MqttClientConfiguration | &str in LwtConfiguration borrows the String; drop order matters | ✓ Required — would cause lifetime compile error otherwise |
| `Arc<UartDriver>` for GNSS thread sharing | read/write take &self — no Mutex needed | ✓ Good — simpler than Arc<Mutex<UartDriver>> |
| `sync_channel(64)` for NMEA relay | RX thread must not block on UART reads when relay is slow; bounded drop-on-full | ✓ Good — no relay full warnings at UM980 normal output rate |
| QoS 0 / retain=false for NMEA relay | Real-time sentences; retransmission of stale positions is harmful | ✓ Good |
| djb2 hash for config deduplication | Non-cryptographic, adequate for retained MQTT messages | ✓ Good — prevents re-applying identical configs on reconnect |
| No-serde JSON parsing for config payload | Fixed schema, no special characters in UM980 commands; avoids dependency | ✓ Good |
| UNLOG over CONFIGSAVE for UM980 init | CONFIGSAVE writes NVM; prefer configuring at boot via MQTT retained message | ✓ Good — NVM wear avoided |
| Pre-allocated RTCM buffer pool (4 × 1029 bytes) | Eliminates per-frame Vec allocation; pool exhaustion drops frame with warn log | ✓ Good — zero heap churn in steady state |
| Software watchdog via AtomicU32 + supervisor thread | Detects silent hangs without FreeRTOS task handles; hardware TWDT backstop | ✓ Good — 15s software + 30s hardware layered detection |
| `recv_timeout` on all blocking receives | Prevents threads from hanging indefinitely if producer dies | ✓ Good — all threads now have finite liveness guarantees |
| Separate `status_tx` channel for heartbeat "online" publish | MQTT callback signals both subscriber and heartbeat on Connected; heartbeat re-publishes retained online on every reconnect | ✓ Good — LWT cleared correctly on all reconnects |
| `RxState` four-state machine with `FreeLine`/`HashLine` | UM980 sends `#`-prefixed query responses and free-text; state machine cleanly routes each to appropriate sink | ✓ Good — no bytes silently discarded |
| SoftAP provisioning over BLE | BLE requires custom app to configure both WiFi and MQTT; SoftAP web UI covers both with zero client install | ✓ Good — simpler UX, no app dependency |
| EspNetif::new_with_conf for DNS in SoftAP | Post-start DHCP DNS injection failed; configuring at netif creation is the correct approach | ✓ Good — Android captive detection unblocked |
| NVS TLS default `false` + config_ver field | OTA resets NVS schema; version check ensures new schema fields have correct defaults after flash | ✓ Good — post-OTA MQTT regression eliminated |
| Single publish thread owning EspMqttClient exclusively | Arc<Mutex<EspMqttClient>> caused contention; SyncSender<MqttMessage> decouples callers cleanly | ✓ Good — simpler ownership, measurably lower latency |
| bytes crate for zero-copy RTCM on publish path | RTCM frames were cloned into Vec<u8> per message; Bytes avoids allocation after initial receive | ✓ Good — no per-frame heap allocation on publish path |
| NTRIP TLS via EspTls (mbedTLS crt_bundle_attach) | AUSCORS requires port 443/TLS; ESP-IDF's bundled CA certs cover the certificate chain | ✓ Good — no manual cert management required |

---
*Last updated: 2026-03-12 after v2.1 milestone start*
