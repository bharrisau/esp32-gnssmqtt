---
phase: 02-connectivity
verified: 2026-03-04T00:00:00Z
status: passed
score: 7/7 must-haves verified
re_verification: false
---

# Phase 2: Connectivity Verification Report

**Phase Goal:** Device connects to WiFi and MQTT broker using compile-time credentials, publishes a periodic retained heartbeat, registers an LWT for offline detection, reconnects automatically after drops, and bridges USB serial to the UM980 for development debugging.
**Verified:** 2026-03-04
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths (from ROADMAP.md Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|---------|
| 1 | Device connects to configured WiFi and MQTT on boot; heartbeat on `gnss/{device_id}/heartbeat` with retain flag within 30 seconds | VERIFIED | `wifi_connect` calls start→connect→wait_netif_up; `mqtt_connect` builds LWT + client; `heartbeat_loop` publishes `b"online"` with `retain=true` via `enqueue()`; initial 5s delay ensures MQTT is up before first publish |
| 2 | `gnss/{device_id}/status` shows `offline` after TCP severance (LWT delivered) | VERIFIED | `LwtConfiguration` in `mqtt_connect`: topic=`gnss/{id}/status`, payload=`b"offline"`, qos=AtLeastOnce, retain=true; declared correctly with `lwt_topic` before `conf` in same scope |
| 3 | Device reconnects after WiFi disconnect without manual reboot | VERIFIED | `wifi_supervisor` polls every 5s, reconnects via `wifi.connect()` (NOT `start()`), exponential backoff 1s→2s→…→60s cap, resets to 1s on success |
| 4 | Device reconnects and re-subscribes after MQTT broker restart without manual reboot | VERIFIED | `pump_mqtt_events` sends `()` on every `EventPayload::Connected`; `subscriber_loop` receives signal and calls `client.subscribe()` — handles both initial connect and broker restarts |
| 5 | USB console input forwarded to UM980; UM980 responses echoed back over USB | VERIFIED | `spawn_bridge` creates UartDriver on UART0 (GPIO16 TX, GPIO17 RX, 115200 baud); Thread A: `uart_rx.read()` → `stdout().write_all()`; Thread B: `stdin().read()` → `um980.write()` with line-edit support |

**Score:** 5/5 ROADMAP success criteria verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/config.rs` | Compile-time WiFi and MQTT constants | VERIFIED | Non-empty WIFI_SSID (`"AFP013"`), MQTT_HOST (`"10.86.32.41"`), MQTT_PORT, MQTT_USER, MQTT_PASS, UART_RX_BUF_SIZE=4096 — all populated |
| `src/wifi.rs` | WiFi connect + reconnect supervisor | VERIFIED | 85 lines; exports `wifi_connect` and `wifi_supervisor`; substantive implementation with BlockingWifi, backoff logic, log messages |
| `src/mqtt.rs` | MQTT client factory, LWT, pump, heartbeat | VERIFIED | 151 lines; exports `mqtt_connect`, `pump_mqtt_events`, `subscriber_loop`, `heartbeat_loop`; substantive (LWT wired, deadlock-safe channel pattern) |
| `src/uart_bridge.rs` | Bidirectional UART0/USB bridge | VERIFIED | 130 lines; exports `spawn_bridge`; both threads substantive (Thread A polls NON_BLOCK, Thread B line-edits with backspace/echo) |
| `src/main.rs` | Firmware entry point wiring all modules | VERIFIED | 110 lines; all 5 modules used (`config`, `device_id`, `wifi`, `mqtt`, `uart_bridge`); 14-step init order enforced in comments |
| `Cargo.toml` | `anyhow = "1"` dependency | VERIFIED | `anyhow = "1"` present at line 11 |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/wifi.rs` | `BlockingWifi<EspWifi>` | `BlockingWifi::wrap(EspWifi::new(...))` | WIRED | Line 20-23: `BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?` |
| `src/main.rs` | `src/wifi.rs` | `wifi::wifi_connect(peripherals.modem, sysloop.clone(), nvs)` | WIRED | Line 55: `let wifi = wifi::wifi_connect(peripherals.modem, sysloop.clone(), nvs)` |
| `src/main.rs` | `src/mqtt.rs` | `mqtt::mqtt_connect(&device_id)` | WIRED | Line 70: `let (mqtt_client, mqtt_connection) = mqtt::mqtt_connect(&device_id)` |
| `src/main.rs` | `src/uart_bridge.rs` | `uart_bridge::spawn_bridge(uart0, gpio16, gpio17)` | WIRED | Lines 60-65: `uart_bridge::spawn_bridge(peripherals.uart0, peripherals.pins.gpio16, peripherals.pins.gpio17)` |
| `pump thread` | `mqtt::pump_mqtt_events` | `std::thread::Builder::new().stack_size(8192).spawn` | WIRED | Line 78-81: spawned with `subscribe_tx` channel; returns `!` |
| `subscriber thread` | `mqtt::subscriber_loop` | `std::thread::Builder::new().stack_size(8192).spawn` | WIRED | Lines 84-89: spawned with `subscribe_rx` channel; handles Connected signals |
| `heartbeat thread` | `mqtt::heartbeat_loop` | `std::thread::Builder::new().stack_size(8192).spawn` | WIRED | Lines 92-97: spawned with `Arc<Mutex<client>>` and `device_id` |
| `wifi supervisor thread` | `wifi::wifi_supervisor` | `std::thread::Builder::new().stack_size(8192).spawn` | WIRED | Lines 100-103: spawned with `wifi` handle |
| `pump_mqtt_events` | `EventPayload::Connected` | `connection.next()` loop | WIRED | Line 69: `EventPayload::Connected(_)` → `subscribe_tx.send(())` |
| `heartbeat_loop` | `client.enqueue` | `Arc<Mutex<client>>` lock, enqueue retain=true | WIRED | Line 142: `c.enqueue(&topic, QoS::AtMostOnce, true, b"online")` |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|---------|
| CONN-01 | 02-01, 02-04 | Device connects to WiFi using compile-time hardcoded SSID and password | SATISFIED | `wifi_connect` uses `crate::config::WIFI_SSID` / `WIFI_PASS`; config.rs has non-empty values; wired in main.rs step 6 |
| CONN-02 | 02-02, 02-04 | Device connects to MQTT broker using compile-time hardcoded host, port, username, and password | SATISFIED | `mqtt_connect` uses `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`; broker URL constructed from constants; client created with EspMqttClient::new |
| CONN-03 | 02-01, 02-04 | Device automatically reconnects to WiFi after a connection drop, with exponential backoff | SATISFIED | `wifi_supervisor` polls every 5s, retries with `wifi.connect()`, backoff doubles on failure (lines 73, 79: `(backoff_secs * 2).min(60)`), resets to 1 on success |
| CONN-04 | 02-02, 02-04 | Device automatically reconnects to MQTT broker after a drop; re-subscribes inside Connected event handler | SATISFIED | Deadlock-safe pattern: pump sends `()` on every `EventPayload::Connected`; `subscriber_loop` subscribes on each signal; handles broker restarts |
| CONN-05 | 02-02, 02-04 | Device publishes periodic heartbeat to `gnss/{device_id}/heartbeat` with MQTT retain flag set | SATISFIED | `heartbeat_loop`: topic = `gnss/{id}/heartbeat`, `c.enqueue(&topic, QoS::AtMostOnce, true, b"online")` every 30s; retain=true (3rd arg) |
| CONN-06 | 02-02, 02-04 | Device registers LWT to `gnss/{device_id}/status` with payload `offline` and retain flag set | SATISFIED | `mqtt_connect`: `LwtConfiguration { topic: &lwt_topic, payload: b"offline", qos: AtLeastOnce, retain: true }`; `disable_clean_session: true` |
| CONN-07 | 02-03, 02-04 | Device bridges USB debug serial to UM980 UART — lines from USB forwarded, replies echoed | SATISFIED | `spawn_bridge` on UART0, GPIO16 TX/GPIO17 RX, 115200 baud, 4096 rx_fifo_size; Thread A echoes UM980 output; Thread B forwards USB input with line-edit |

**All 7 CONN requirements: SATISFIED. No orphaned requirements.**

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| None | — | — | — | — |

No TODO/FIXME/PLACEHOLDER comments found in `src/`. No empty implementations. No stub returns. No console-log-only handlers.

---

### Notable Deviations from Plan (Not Defects)

The following deviations from the PLAN documents are documented improvements, not failures:

1. **UART peripheral corrected:** 02-04-PLAN specified `uart1`/GPIO20/GPIO21; implementation correctly uses `uart0`/GPIO16/GPIO17 matching actual XIAO ESP32-C6 hardware wiring. CONN-07 verified on hardware.

2. **3-thread MQTT vs 2-thread:** 02-02-PLAN specified `pump_mqtt_events(connection, client, device_id)`. Final implementation uses `pump_mqtt_events(connection, subscribe_tx)` + `subscriber_loop(client, device_id, subscribe_rx)`. This deadlock-safe pattern is correct and was verified on hardware.

3. **heartbeat uses `enqueue` not `publish`:** 02-02-PLAN specified `client.publish()`; implementation uses `c.enqueue()`. Both are valid for QoS::AtMostOnce from a dedicated thread — enqueue is non-blocking when the outbox has space, which is safe given the pump thread keeps the outbox moving.

4. **`subscriber_loop` is an additional public export** not listed in 02-02 must_haves artifacts. It is substantive and properly wired into main.rs.

---

### Human Verification Required

Hardware has already been verified in a prior session per the 02-04-SUMMARY.md and project MEMORY.md. All requirements were confirmed on the physical device:

- CONN-01/02: Boot log showed WiFi + MQTT connected within 30s
- CONN-05: `gnss/FFFEB5/heartbeat` received with retain=true on broker
- CONN-06: `gnss/FFFEB5/status` delivered `offline` after TCP severance
- CONN-03: WiFi reconnected after AP drop without reboot
- CONN-04: MQTT re-subscribed after broker restart without reboot
- CONN-07: UART bridge forwarded USB input to UM980 and echoed responses

No additional human verification required for this phase.

---

### Gaps Summary

None. All artifacts exist and are substantive, all key links are wired, all 7 CONN requirements are satisfied. No anti-patterns found. Hardware verification documented in 02-04-SUMMARY.md.

---

_Verified: 2026-03-04T00:00:00Z_
_Verifier: Claude (gsd-verifier)_
