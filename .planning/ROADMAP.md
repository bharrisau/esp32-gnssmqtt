# Roadmap: esp32-gnssmqtt

## Overview

Milestone 1 (Foundation) builds the full working skeleton of the ESP32-C6 firmware: a correctly scaffolded, version-pinned Rust project that can flash to hardware, connect to WiFi and an MQTT broker using hardcoded credentials, publish a heartbeat, and provide visual connectivity status via the status LED. This milestone delivers a device that is observable over MQTT and visually diagnosable in the field — the foundation every subsequent GNSS relay feature depends on.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Scaffold** - Version-pinned project that compiles for ESP32-C6, flashes via espflash, and provides a stable device ID
- [x] **Phase 2: Connectivity** - Device connects to WiFi and MQTT with hardcoded credentials, publishes heartbeat, reconnects automatically, and bridges USB debug to UM980
- [ ] **Phase 3: Status LED** - LED reflects connectivity state with distinct blink patterns for connecting, connected, and error states

## Phase Details

### Phase 1: Scaffold
**Goal**: A correctly structured, version-pinned Rust project that compiles for ESP32-C6, flashes successfully, and provides a stable unique device ID from hardware
**Depends on**: Nothing (first phase)
**Requirements**: SCAF-01, SCAF-02, SCAF-03, SCAF-04, SCAF-05
**Success Criteria** (what must be TRUE):
  1. `cargo build` completes without error targeting `riscv32imac-esp-espidf` and `espflash` successfully flashes the binary to the ESP32-C6
  2. The device prints a stable, unique device ID string derived from the hardware MAC/eFuse on every boot — the same string across power cycles
  3. The NVS partition is present in `partitions.csv` (64KB+) and the sdkconfig sets the UART RX ring buffer to 4096+ bytes with FreeRTOS stack overflow detection enabled
  4. All three Espressif crates (`esp-idf-hal`, `esp-idf-svc`, `esp-idf-sys`) are pinned with `=` version specifiers and the project builds from a clean `cargo clean`
**Plans**: 2 plans

Plans:
- [x] 01-01-PLAN.md — Project scaffold: all config + source files, cargo build verification (SCAF-01, SCAF-02, SCAF-03, SCAF-04)
- [x] 01-02-PLAN.md — Flash to hardware, verify stable device ID across power cycles (SCAF-05)

### Phase 2: Connectivity
**Goal**: Device connects to WiFi and MQTT broker using compile-time credentials, publishes a periodic retained heartbeat, registers an LWT for offline detection, reconnects automatically after drops, and bridges USB serial to the UM980 for development debugging
**Depends on**: Phase 1
**Requirements**: CONN-01, CONN-02, CONN-03, CONN-04, CONN-05, CONN-06, CONN-07
**Success Criteria** (what must be TRUE):
  1. Device connects to the configured WiFi network and MQTT broker on boot; the heartbeat message appears on `gnss/{device_id}/heartbeat` with the retain flag set within 30 seconds of power-on
  2. When the broker's retained topic list is checked, `gnss/{device_id}/status` shows `offline` after the device's TCP connection is severed (LWT delivered correctly)
  3. After a deliberate WiFi disconnect, the device reconnects and resumes publishing heartbeats without a manual reboot
  4. After a deliberate MQTT broker restart, the device reconnects and re-subscribes to all topics without a manual reboot
  5. Lines typed into the USB serial console are forwarded to the UM980 UART, and UM980 responses appear in the USB serial console
**Plans**: 4 plans

Plans:
- [x] 02-01-PLAN.md — Populate config.rs credentials + create src/wifi.rs (connect + reconnect supervisor) (CONN-01, CONN-03)
- [x] 02-02-PLAN.md — Create src/mqtt.rs (LWT, pump thread, heartbeat) (CONN-02, CONN-04, CONN-05, CONN-06)
- [x] 02-03-PLAN.md — Create src/uart_bridge.rs (UART0/USB CDC <-> UART1/UM980 bridge) (CONN-07)
- [x] 02-04-PLAN.md — Wire main.rs, cargo build, flash + hardware verification checkpoint (all CONN)

### Phase 3: Status LED
**Goal**: The status LED communicates connectivity state through distinct blink patterns, giving an operator standing next to the device clear visual feedback without needing a serial monitor
**Hardware**: XIAO ESP32-C6 single yellow user LED on GPIO15, active-low (3.3V → 1.5kΩ → LED → GPIO15; drive GPIO low to illuminate)
**Depends on**: Phase 2
**Requirements**: LED-01, LED-02, LED-03
**Success Criteria** (what must be TRUE):
  1. While the device is attempting to connect to WiFi or MQTT, the LED blinks at a clearly distinct rate (e.g., rapid pulse) that differs from all other states
  2. When both WiFi and MQTT are connected, the LED shows a steady-on or slow-blink pattern that is visually distinct from the connecting pattern
  3. After repeated failed connection attempts (WiFi or MQTT unreachable), the LED shows a recognizable error pattern (e.g., fast blink or off) that differs from both connecting and connected states
**Plans**: 3 plans

Plans:
- [ ] 03-01-PLAN.md — Create src/led.rs (LedState enum + led_task) + update src/wifi.rs (supervisor error threshold) (LED-01, LED-02, LED-03)
- [ ] 03-02-PLAN.md — Update src/mqtt.rs (pump Connected/Connecting writes) + wire src/main.rs + cargo build (LED-01, LED-02)
- [ ] 03-03-PLAN.md — Flash to hardware, visual verification of all three LED patterns checkpoint (LED-01, LED-02, LED-03)

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Scaffold | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | 2/3 | In Progress|  |
