# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 (XIAO Seeed) that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences and RTCM3 correction frames from the UM980 over UART, publishes each to dedicated MQTT topics in real time, accepts remote UM980 reconfiguration via retained MQTT config, supports MQTT-triggered OTA firmware updates with rollback safety, and publishes periodic health telemetry. Designed for unattended long-running operation with automatic recovery from connectivity loss.

v1.3 shipped: reliability hardening — bounded channels, zero-alloc RTCM hot path, FreeRTOS stack HWM diagnostics, thread watchdog with auto-reboot, resilience timeouts (WiFi + MQTT), and MQTT health heartbeat.

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

### Active

- [ ] UM980 `#`-prefixed query responses routed to `gnss/{device_id}/nmea/response` (shipped post-v1.3, candidate for v1.4)
- [ ] Free-text UM980 output mirrored to stdout for espflash monitor visibility (shipped post-v1.3)

### Out of Scope

- BLE provisioning — deferred; hardcoded credentials sufficient for development
- Web portal (SoftAP) provisioning — depends on BLE provisioning
- TLS/mTLS for MQTT — separate milestone
- Full NMEA field parsing — firmware relays, consumers parse downstream
- Local NMEA buffering across power cycles — real-time relay only
- JSON-wrapped NMEA publish — raw NMEA preferred
- Multi-broker publishing — single broker only
- Remote log streaming — high complexity, deferred to v2

## Context

- **Hardware**: Seeed XIAO ESP32-C6 — RISC-V, WiFi 6, single yellow LED GPIO15 active-low
- **GNSS**: UM980 multi-band RTK receiver, UART0 at 115200 baud (GPIO16 TX, GPIO17 RX)
- **Language**: Rust with std via esp-idf-svc/hal/sys (ESP-IDF v5.3.3)
- **MQTT broker**: External (Mosquitto/HiveMQ); username/password auth, no TLS in v1
- **Shipped v1.3**: 2,249 lines of Rust, 13 phases, 24 plans total; device FFFEB5
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

---
*Last updated: 2026-03-08 after v1.3 milestone*
