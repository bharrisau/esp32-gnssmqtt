# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 (XIAO Seeed) that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences and RTCM3 correction frames from the UM980 over UART, publishes each to dedicated MQTT topics in real time, accepts remote UM980 reconfiguration via retained MQTT config, supports MQTT-triggered OTA firmware updates with rollback safety, and publishes periodic health telemetry including GNSS fix quality.

A companion Rust server (`gnss-server`) subscribes to the same MQTT topics, decodes RTCM3 frames into RINEX 2.11 observation and navigation files with hourly rotation, and serves a live browser UI with polar satellite skyplot, SNR bar chart, and device health panel via HTTP + WebSocket.

v2.1 shipped: server + nostd foundation — Cargo workspace restructure, complete ESP-IDF nostd audit, gnss-nvs crate (NvsStore trait + ESP-IDF + sequential-storage impls), RTCM3 MSM/ephemeris decode pipeline, RINEX 2.11 .26O/.26P writers, axum HTTP+WebSocket server with live browser UI, and 5 gap crate skeletons (gnss-ota, gnss-softap, gnss-dns, gnss-log) with trait definitions and BLOCKER.md each.

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
- ✓ Cargo workspace with resolver=2; firmware/ + gnss-server/ + crates/* members without target conflicts — v2.1
- ✓ Complete audit of all ESP-IDF dependency usages mapped to embassy/nostd equivalents or flagged as gaps — v2.1
- ✓ gnss-nvs crate: NvsStore trait (namespaced, typed getters/setters, blob support) + ESP-IDF impl + sequential-storage skeleton — v2.1
- ✓ Server subscribes to MQTT gnss/{id}/rtcm, nmea, heartbeat; reconnects with exponential backoff — v2.1
- ✓ RTCM3 MSM4/MSM7 decode for GPS, GLONASS, Galileo, BeiDou + ephemeris 1019/1020/1046/1042; EpochBuffer flush-on-change — v2.1
- ✓ RINEX 2.11 observation files (.26O) with hourly rotation, mandatory headers, column-exact format — v2.1
- ✓ RINEX 2.11 navigation files (.26P) from decoded GPS/GLONASS ephemeris with hourly rotation — v2.1
- ✓ HTTP + WebSocket server: live satellite skyplot SVG, SNR bar chart, device health panel at ~1 Hz — v2.1
- ✓ Gap crate skeletons: gnss-ota, gnss-softap, gnss-dns, gnss-log — trait definitions + BLOCKER.md each — v2.1

### Active

*(Next milestone requirements to be defined via `/gsd:new-milestone`)*

### Out of Scope

- TLS/mTLS for MQTT — separate milestone
- Full NMEA field parsing — firmware relays, consumers parse downstream
- Local NMEA buffering across power cycles — real-time relay only
- JSON-wrapped NMEA publish — raw NMEA preferred
- Multi-broker publishing — single broker only
- Full embassy firmware port — blocked by SoftAP password, DNS hijack, and log hook gaps
- RINEX 3.x format — RINEX 2.11 sufficient for RTKLIB/PPP workflows
- BLE provisioning — SoftAP covers WiFi+MQTT+NTRIP without custom app

## Context

- **Hardware**: Seeed XIAO ESP32-C6 — RISC-V, WiFi 6, single yellow LED GPIO15 active-low
- **GNSS**: UM980 multi-band RTK receiver, UART0 at 115200 baud (GPIO16 TX, GPIO17 RX)
- **Language**: Rust with std via esp-idf-svc/hal/sys (ESP-IDF v5.3.3); server uses tokio + axum
- **MQTT broker**: External (Mosquitto/HiveMQ); username/password auth, no TLS in v1
- **Shipped v2.1**: Cargo workspace with firmware/ + gnss-server/ + 6 gap crates; 25 phases, 59 plans total; device FFFEB5
- **UM980 UART protocol**: NMEA sentences (`$`-prefix), RTCM3 frames (`0xD3`-prefix), `#`-prefix query responses (checksum-terminated); free-text banners otherwise
- **UM980 config**: Configured via retained MQTT config topic at boot; RESET causes reboot (wait required), UNLOG cleans NMEA outputs without reboot; avoid CONFIGSAVE (NVM wear)
- **Known tech debt**: GN-talker gap (nmea 0.7 crate ignores $GN combined-constellation sentences — skyplot/SNR chart will not update with live UM980 data until UM980 emits per-constellation talkers or crate is upgraded); gnss-nvs not yet wired into firmware (intentionally deferred)

## Constraints

- **Tech stack**: Rust (esp-idf-hal, esp-idf-svc, esp-idf-sys for firmware; tokio/axum for server)
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
| Cargo workspace resolver=2 mandatory | Prevents std feature unification leaking into no_std gap crates; member profiles ignored by workspace builds | ✓ Good — resolver=2 confirmed via dependency graph |
| panic=abort via -C rustflag in firmware/.cargo/config.toml | cargo-features panic-immediate-abort cannot be scoped per-package in workspace profile overrides | ✓ Good — abort-on-panic preserved for embedded target only |
| rtcm-rs 0.11 for server RTCM3 decode | Avoids hand-rolled MSM cell mask and pseudorange bugs; covers all 8 MSM variants and 4 ephemeris types | ✓ Good — 8 unit tests pass with real fixture data |
| BeiDou ephemeris is RTCM msg 1042 (not 1044) | Plan had incorrect type; msg 1044 is QZSS | ✓ Corrected — rtcm_decode.rs matches Msg1042T |
| figment TOML+env for server config | GNSS_ prefix with __ nesting separator; consistent with 12-factor app config | ✓ Good — TOML file + env override works cleanly |
| axum 0.7 + broadcast::Sender<String> for WebSocket fan-out | Tokio-native; broadcast channel decouples MQTT decode task from N WebSocket clients | ✓ Good — Lagged errors skipped gracefully |
| include_str! for embedding index.html | Single binary, no runtime file dependency; simplest path for deployment | ✓ Good — no ServeDir complexity |
| Gap crates as trait-only skeletons with BLOCKER.md | Captures exactly what is missing for a nostd implementation without blocking v2.1 delivery | ✓ Good — 5 gap crates document the embassy port path |

---
*Last updated: 2026-03-12 after v2.1 milestone — Server and nostd Foundation shipped*
