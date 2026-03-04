# esp32-gnssmqtt

## What This Is

Rust firmware for the ESP32-C6 (XIAO Seeed) that bridges a UM980 GNSS module to an MQTT broker over WiFi. The device reads NMEA sentences from the UM980 over UART and publishes each sentence type to a dedicated MQTT topic. Remote UM980 reconfiguration is delivered via a retained MQTT config topic.

v1.0 shipped: scaffold + WiFi/MQTT connectivity + status LED. Device connects, publishes heartbeats, supervises reconnection, and visually reflects state via GPIO15 LED.

## Core Value

NMEA sentences from the UM980 are reliably delivered to the MQTT broker in real time, with remote reconfiguration of the GNSS module via MQTT.

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

### Active

- [ ] Device reads raw NMEA bytes from UM980 UART at 115200 baud 8N1 (UART-01 through UART-04)
- [ ] Each valid NMEA sentence published to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` (NMEA-01, NMEA-02)
- [ ] Device subscribes to `gnss/{device_id}/config` and forwards payload to UM980 over UART TX (CONF-01 through CONF-03)

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
- **Shipped v1.0**: 698 lines of Rust, 3 phases, 9 plans, device FFFEB5 hardware-verified
- **UM980 current state**: BASE TIME mode — needs `MODE ROVER` before GNSS relay phase

## Constraints

- **Tech stack**: Rust (esp-idf-hal, esp-idf-sys, esp-idf-svc)
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

---
*Last updated: 2026-03-04 after v1.0 milestone*
