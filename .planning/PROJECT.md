# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 (XIAO Seeed) that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences from the UM980 over UART, publishes each sentence type to a dedicated MQTT topic in real time, and accepts remote UM980 reconfiguration via a retained MQTT config topic.

v1.1 shipped: full GNSS relay pipeline — UART sentence assembly, per-type NMEA publishing, and remote config forwarding with hash deduplication. All requirements hardware-verified on device FFFEB5.

## Core Value

NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.

## Current Milestone: v1.2 Observations + OTA

**Goal:** Add RTCM3 binary message relay for RTK/calibration use and MQTT-triggered OTA firmware update with rollback.

**Target features:**
- Mixed NMEA+RTCM byte-stream parsing in gnss.rs (RxState state machine)
- RTCM frames published raw to `gnss/{device_id}/rtcm/{message_type}`
- OTA: dual partition table, HTTP pull with SHA256 verify, rollback safety

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

### Active

- [ ] RTCM3 binary relay: mixed NMEA+RTCM stream, publish to `gnss/{device_id}/rtcm/{message_type}` (RTCM-01 through RTCM-05)
- [ ] OTA firmware update: MQTT trigger → HTTP pull → SHA256 → dual partition write → rollback (OTA-01 through OTA-06)

### Out of Scope

- BLE provisioning — deferred; hardcoded credentials sufficient for development
- Web portal (SoftAP) provisioning — depends on BLE provisioning
- TLS/mTLS for MQTT — separate milestone
- OTA firmware update — requires dual-partition design; separate milestone
- Full NMEA field parsing — firmware relays, consumers parse downstream
- Local NMEA buffering across power cycles — real-time relay only
- JSON-wrapped NMEA publish — raw NMEA preferred
- Multi-broker publishing — single broker only

## Context

- **Hardware**: Seeed XIAO ESP32-C6 — RISC-V, WiFi 6, single yellow LED GPIO15 active-low
- **GNSS**: UM980 multi-band RTK receiver, UART0 at 115200 baud (GPIO16 TX, GPIO17 RX)
- **Language**: Rust with std via esp-idf-svc/hal/sys (ESP-IDF v5.3.3)
- **MQTT broker**: External (Mosquitto/HiveMQ); username/password auth, no TLS in v1
- **Shipped v1.1**: 1,088 lines of Rust, 6 phases, 15 plans, device FFFEB5 hardware-verified
- **UM980 current state**: Configured via retained MQTT config topic at boot; RESET causes reboot (wait required), UNLOG cleans NMEA outputs without reboot; avoid CONFIGSAVE (NVM wear)

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

---
*Last updated: 2026-03-07 after v1.2 milestone start*
