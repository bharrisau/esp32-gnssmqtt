# Milestones

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
