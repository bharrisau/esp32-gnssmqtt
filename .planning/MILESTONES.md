# Milestones

## v2.0 Field Deployment (Shipped: 2026-03-12)

**Phases completed:** 8 phases (14-21), 24 plans
**LOC:** 4,726 Rust (+2,477 from v1.3)
**Timeline:** 2026-03-08 → 2026-03-12 (4 days)
**Hardware verified:** device FFFEB5

**Key accomplishments:**
- Phase 14: SNTP wall-clock timestamps on WiFi connect; UM980 command relay via `gnss/{id}/command`; remote reboot trigger on OTA topic
- Phase 15: SoftAP web provisioning portal — WiFi (up to 3 SSIDs) and MQTT credentials configurable from browser; stored in NVS with multi-AP failover; GPIO9 button re-entry
- Phase 16: ESP-IDF log output forwarded to `gnss/{id}/log` via C vprintf hook; re-entrancy guard prevents MQTT feedback loops; runtime log-level configuration
- Phase 17: NTRIP v1 TCP client streams RTCM3 corrections to UM980 UART for RTK fix; captive portal DNS hijack for Android/iOS/Windows; NTRIP state in heartbeat
- Phase 18: GGA parsing into atomics (fix_type, satellites, HDOP); heartbeat extended with GNSS fix quality; OTA pipeline hardware-validated on device FFFEB5; project README
- Phase 19: DHCP DNS override fix unblocking Android captive detection; NVS TLS default corrected (post-OTA MQTT regression fixed); GPIO9 3-phase state machine (3s SoftAP / 10s factory reset)
- Phase 20: Windows/iOS captive portal probes fixed; NMEA channel raised 64→128 for 5 Hz headroom; UM980 GNSS config persisted to NVS and auto-reapplied on UM980 reboot; TLS NTRIP via EspTls for AUSCORS port 443
- Phase 21: Arc<Mutex<EspMqttClient>> eliminated — single publish thread owns client exclusively; all relay threads migrated to SyncSender<MqttMessage>; bytes crate for zero-copy RTCM; outbox observability counters + bench:N trigger

---

## v1.3 Reliability Hardening (Shipped: 2026-03-08)

**Phases completed:** 7 phases (07-13), 15 plans
**LOC:** 2,249 Rust (+1,161 from v1.1)
**Timeline:** 2026-03-07 → 2026-03-08 (2 days)
**Hardware verified:** device FFFEB5

**Key accomplishments:**
- Phase 7: Four-state UART RX state machine (NMEA+RTCM), CRC-24Q verification, RTCM3 frames published to `gnss/{id}/rtcm/{type}` at QoS 0
- Phase 8: Dual-slot OTA with rollback safety, HTTP streaming download + SHA-256 verification, MQTT-triggered via `/ota/trigger`, mark_valid on successful boot
- Phase 9: All mpsc channels converted to `sync_channel` with documented capacities; UART TX error logging with AtomicU32 counter; `recv_timeout` on all 6 blocking receives
- Phase 10: Pre-allocated RTCM buffer pool (4 × 1029 bytes, zero per-frame heap alloc in steady state); FreeRTOS HWM logged at entry of all 11 spawned threads
- Phase 11: Software watchdog with two AtomicU32 heartbeat counters; supervisor reboots via `esp_restart()` after 3 missed beats (15s); hardware TWDT backstop at 30s
- Phase 12: Auto-reboot after 10min WiFi disconnect (RESIL-01) or 5min MQTT disconnect while WiFi up (RESIL-02); reboot logged before triggering
- Phase 13: Drop-counter atomics in gnss.rs; JSON health snapshot (`uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`) to `/heartbeat` every 30s; retained `"online"` to `/status` on every MQTT reconnect

---

## v1.1 GNSS Relay (Shipped: 2026-03-07)

**Phases completed:** 3 phases (04-06), 6 plans, 11 tasks
**LOC:** 1,088 Rust (+390 from v1.0)
**Timeline:** 2026-03-05 → 2026-03-07 (3 days)
**Git range:** feat(04-01) → feat(06-02)
**Hardware verified:** device FFFEB5

**Key accomplishments:**
- Created gnss.rs: exclusive UartDriver owner with RX sentence-assembly thread and TX command thread, delivering (sentence_type, raw_sentence) tuples via sync_channel(64)
- Refactored uart_bridge.rs to TX-only (Sender<String>), wired main.rs Step 7 with gnss::spawn_gnss — full UART pipeline operational
- Created nmea_relay.rs with spawn_relay(): publishes raw NMEA sentences to gnss/{device_id}/nmea/{TYPE} at QoS 0 with bounded backpressure
- Wired NMEA relay into main.rs Step 14 — hardware-verified at 10 msg/sec on device FFFEB5
- Created config_relay.rs with spawn_config_relay(), djb2 hash deduplication, JSON/plain-text payload parser, 100ms per-command delay
- Wired config relay into main.rs Step 15 — CONF-01 through CONF-03 hardware-verified end-to-end on device FFFEB5

---

## v1.0 Foundation (Shipped: 2026-03-04)

**Phases completed:** 3 phases (01-03), 9 plans
**LOC:** 698 Rust
**Timeline:** 2026-03-03 → 2026-03-04 (2 days)
**Hardware verified:** device FFFEB5

**Key accomplishments:**
- ESP32-C6 project scaffold: Rust + esp-idf-svc/hal/sys, nightly toolchain, partitions.csv, sdkconfig.defaults
- Device ID from eFuse MAC last 3 bytes (FFFEB5) — stable across power cycles
- WiFi connect with exponential backoff supervisor
- MQTT connect with LWT, heartbeat, pump thread, re-subscribe on reconnect
- USB-serial to UM980 UART bridge (UART0, GPIO16/17)
- Status LED (GPIO15 active-low): connecting blink / connected steady / error burst

---
