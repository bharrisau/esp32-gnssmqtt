# Phase 2: Connectivity - Research

**Researched:** 2026-03-03
**Domain:** ESP32-C6 WiFi + MQTT connectivity in Rust (esp-idf-svc 0.51.0 / esp-idf-hal 0.45.2), UART serial bridging
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CONN-01 | Device connects to WiFi using compile-time hardcoded SSID and password | `BlockingWifi` + `EspWifi` pattern verified; credentials via `const` in `config.rs` |
| CONN-02 | Device connects to MQTT broker using compile-time hardcoded host, port, username, password | `EspMqttClient::new()` with `MqttClientConfiguration` (username/password fields confirmed) |
| CONN-03 | Device automatically reconnects to WiFi after a connection drop, with exponential backoff | No built-in auto-reconnect in `BlockingWifi`; must implement poll loop with `is_connected()` + sleep backoff |
| CONN-04 | Device automatically reconnects to MQTT broker after a connection drop; re-subscribes inside `Connected` event handler | ESP-IDF MQTT client auto-reconnects; re-subscription via `EventPayload::Connected` in connection pump thread |
| CONN-05 | Device publishes periodic heartbeat to `gnss/{device_id}/heartbeat` with retain flag | `client.publish(topic, QoS::AtMostOnce, true, payload)` — retain=true confirmed |
| CONN-06 | Device registers LWT to `gnss/{device_id}/status` with payload `offline` and retain flag at connect time | `LwtConfiguration { topic, payload: b"offline", qos, retain: true }` in `MqttClientConfiguration` |
| CONN-07 | Device bridges USB debug serial (UART0/USB CDC) to UM980 UART — lines from USB forwarded to UM980, replies echoed back | `UartDriver` on UART1 (GPIO20/21 on XIAO C6); read/write threads bridge UART0 stdin to UART1 |
</phase_requirements>

---

## Summary

Phase 2 implements WiFi and MQTT connectivity on an ESP32-C6 (XIAO form factor) using the already-pinned `esp-idf-svc =0.51.0` and `esp-idf-hal =0.45.2` crates. The project uses the ESP-IDF std path (not bare-metal `esp-hal`), so the full IDF WiFi and MQTT stack is available through safe Rust wrappers.

WiFi is handled by `BlockingWifi<EspWifi>` from `esp_idf_svc::wifi`. There is no built-in auto-reconnect in the wrapper — the application must implement its own reconnect loop using `wifi.is_connected()` polling with sleep-based exponential backoff. MQTT is handled by `EspMqttClient` from `esp_idf_svc::mqtt::client`, which wraps the ESP-IDF MQTT client. The underlying MQTT client does auto-reconnect to the broker, but re-subscription to topics must be triggered by the application by watching for `EventPayload::Connected` events in the connection pump thread. LWT is configured at connect time via `LwtConfiguration` in `MqttClientConfiguration`.

The USB-to-UM980 serial bridge (CONN-07) uses `UartDriver` on UART1 (GPIO20=RX, GPIO21=TX on XIAO ESP32-C6) for UM980 communication. UART0 is occupied by the USB Serial/JTAG controller for logging/debug output. The bridge reads from UART0 stdin and writes to UART1, and reads from UART1 to write back to UART0. This requires two FreeRTOS threads (one per direction) since `UartDriver::read()` is blocking.

**Primary recommendation:** Use `BlockingWifi` for WiFi, `EspMqttClient` with a dedicated connection-pump thread for MQTT, implement WiFi reconnect as a polling loop in a supervisor thread, and bridge UART0/UART1 with two dedicated threads.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp-idf-svc` | `=0.51.0` (pinned) | WiFi (`EspWifi`, `BlockingWifi`), MQTT (`EspMqttClient`), event loop, NVS | Already pinned in Phase 1; the authoritative Espressif-blessed Rust IDF wrapper |
| `esp-idf-hal` | `=0.45.2` (pinned) | UART (`UartDriver`), peripherals | Already pinned in Phase 1; HAL layer for hardware peripherals |
| `esp-idf-sys` | `=0.36.1` (pinned) | Low-level sys bindings | Already pinned; do not change version |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `embedded-svc` | Re-exported by `esp-idf-svc` | `EventPayload` enum, MQTT `QoS` type | Used indirectly through `esp_idf_svc::mqtt::client` |
| `log` | `0.4` (already in Cargo.toml) | Structured logging via `EspLogger` | All log output |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `BlockingWifi` | `AsyncWifi` | Async would need an executor (e.g., `embassy`); overkill for this project; blocking is simpler |
| `EspMqttClient` (connection pump thread) | `EspMqttClient::new_cb` (callback variant) | Callback variant is simpler but harder to re-subscribe on reconnect; pump-thread pattern gives full event access |
| Manual reconnect loop | ESP-IDF auto-reconnect | ESP-IDF WiFi auto-reconnect (`disable_auto_reconnect=false`) exists at C layer; Rust wrapper doesn't expose clean hook for backoff |

**Installation:** No new dependencies needed. All required functionality is already in the pinned `esp-idf-svc =0.51.0` and `esp-idf-hal =0.45.2`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── main.rs           # Initialization, spawns tasks, owns WiFi/MQTT/UART handles
├── config.rs         # Compile-time constants: WIFI_SSID, MQTT_HOST, etc. (already exists)
├── device_id.rs      # Device ID from eFuse (already exists)
├── wifi.rs           # WiFi connect + reconnect supervisor
├── mqtt.rs           # MQTT client creation, LWT config, publish helpers
└── uart_bridge.rs    # UART0 <-> UART1 bidirectional bridge
```

### Pattern 1: WiFi Initialization with BlockingWifi

**What:** Create `EspWifi` wrapped in `BlockingWifi`, configure station mode, call connect sequence.
**When to use:** Initial connection on boot; also re-called inside reconnect loop.

```rust
// Source: https://github.com/esp-rs/esp-idf-svc/blob/master/examples/wifi.rs
use esp_idf_svc::hal::prelude::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use embedded_svc::wifi::AuthMethod;

fn wifi_connect(
    modem: impl esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: crate::config::WIFI_SSID.try_into().unwrap(),
        password: crate::config::WIFI_PASS.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;
    Ok(wifi)
}
```

### Pattern 2: WiFi Reconnect Supervisor Loop

**What:** A loop that checks `wifi.is_connected()` periodically and calls `wifi.connect()` + `wifi.wait_netif_up()` after a drop. Runs in a dedicated thread.
**When to use:** CONN-03 — must reconnect after drops with exponential backoff.

```rust
// Source: Derived from esp-idf-svc BlockingWifi API docs + community pattern
// No built-in auto-reconnect in BlockingWifi — must implement manually.
fn wifi_supervisor(mut wifi: BlockingWifi<EspWifi<'static>>) -> ! {
    let mut backoff_secs = 1u64;
    loop {
        if !wifi.is_connected().unwrap_or(false) {
            log::warn!("WiFi disconnected — reconnecting in {}s", backoff_secs);
            std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
            match wifi.connect() {
                Ok(_) => {
                    let _ = wifi.wait_netif_up();
                    log::info!("WiFi reconnected");
                    backoff_secs = 1; // reset
                }
                Err(e) => {
                    log::error!("WiFi reconnect failed: {:?}", e);
                    backoff_secs = (backoff_secs * 2).min(60); // cap at 60s
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
```

### Pattern 3: MQTT Client with LWT, Credentials, and Connection Pump Thread

**What:** Create `EspMqttClient` with `MqttClientConfiguration` including LWT and credentials. Spawn a thread to pump `EspMqttConnection::next()` events. Watch for `EventPayload::Connected` to re-subscribe.
**When to use:** CONN-02, CONN-04, CONN-05, CONN-06.

```rust
// Source: https://github.com/esp-rs/esp-idf-svc/blob/master/src/mqtt/client.rs
// Source: https://github.com/esp-rs/esp-idf-svc/issues/90 (re-subscribe pattern)
use esp_idf_svc::mqtt::client::{EspMqttClient, EspMqttConnection, LwtConfiguration, MqttClientConfiguration};
use embedded_svc::mqtt::client::{EventPayload, QoS};
use std::sync::{Arc, Mutex};

fn mqtt_connect(device_id: &str) -> anyhow::Result<(EspMqttClient<'static>, EspMqttConnection)> {
    let lwt_topic = format!("gnss/{}/status", device_id);
    let broker_url = format!("mqtt://{}:{}", crate::config::MQTT_HOST, crate::config::MQTT_PORT);

    let conf = MqttClientConfiguration {
        client_id: Some(device_id),
        username: Some(crate::config::MQTT_USER),
        password: Some(crate::config::MQTT_PASS),
        lwt: Some(LwtConfiguration {
            topic: &lwt_topic,
            payload: b"offline",
            qos: QoS::AtLeastOnce,
            retain: true,
        }),
        keep_alive_interval: Some(std::time::Duration::from_secs(60)),
        reconnect_timeout: Some(std::time::Duration::from_secs(5)),
        ..Default::default()
    };

    let (client, connection) = EspMqttClient::new(&broker_url, &conf)?;
    Ok((client, connection))
}

// Pump thread — MUST be running or publish/subscribe calls will block indefinitely
fn pump_mqtt_events(
    mut connection: EspMqttConnection,
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
) {
    while let Ok(event) = connection.next() {
        match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("MQTT connected — re-subscribing");
                if let Ok(mut c) = client.lock() {
                    let topic = format!("gnss/{}/config", device_id);
                    let _ = c.subscribe(&topic, QoS::AtLeastOnce);
                }
            }
            EventPayload::Disconnected => log::warn!("MQTT disconnected"),
            EventPayload::Error(e) => log::error!("MQTT error: {:?}", e),
            _ => {}
        }
    }
}
```

### Pattern 4: Heartbeat Publisher

**What:** A loop that publishes a retained heartbeat message every N seconds.
**When to use:** CONN-05.

```rust
// Source: EspMqttClient::publish signature from esp-idf-svc/src/mqtt/client.rs
fn heartbeat_loop(client: Arc<Mutex<EspMqttClient<'static>>>, device_id: &str) -> ! {
    let topic = format!("gnss/{}/heartbeat", device_id);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if let Ok(mut c) = client.lock() {
            let payload = b"online";
            // retain=true ensures broker persists last heartbeat for new subscribers
            if let Err(e) = c.publish(&topic, QoS::AtMostOnce, true, payload) {
                log::warn!("Heartbeat publish failed: {:?}", e);
            }
        }
    }
}
```

### Pattern 5: UART Bridge (UART0 USB <-> UART1 UM980)

**What:** Two threads — one forwards USB CDC (UART0 stdin) bytes to UART1 (UM980), the other forwards UART1 bytes back to UART0 stdout.
**When to use:** CONN-07.

```rust
// Source: https://docs.esp-rs.org/esp-idf-hal/esp_idf_hal/uart/struct.UartDriver.html
// Source: https://github.com/esp-rs/esp-idf-hal (UartDriver API)
use esp_idf_svc::hal::uart::{UartDriver, config::Config};
use esp_idf_svc::hal::units::Hertz;

// XIAO ESP32-C6: UART1 on GPIO20 (RX) and GPIO21 (TX)
// Source: https://forum.seeedstudio.com/t/xiao-esp32c6-uarts/292856
let um980_uart = UartDriver::new(
    peripherals.uart1,
    peripherals.pins.gpio21,   // TX to UM980
    peripherals.pins.gpio20,   // RX from UM980
    Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None, // no CTS
    Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None, // no RTS
    &Config::new().baudrate(Hertz(115_200)),
)?;

// UM980 -> USB direction (thread 1)
let um980_uart_rx = Arc::new(um980_uart); // share via Arc after splitting or cloning handle
std::thread::spawn(move || {
    let mut buf = [0u8; 256];
    loop {
        match um980_uart_rx.read(&mut buf, NON_BLOCK) {
            Ok(n) if n > 0 => {
                // write to stdout (UART0/USB CDC)
                use std::io::Write;
                let _ = std::io::stdout().write_all(&buf[..n]);
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
});
```

**NOTE on UART0 / USB CDC:** On XIAO ESP32-C6, UART0 is connected through the USB Serial/JTAG controller. The firmware uses `EspLogger` which writes to UART0. For the USB-to-UM980 bridge direction, reading from USB input requires reading from `std::io::stdin()` (which maps to UART0 under the ESP-IDF std environment). Writing UM980 replies back to USB uses `std::io::stdout()`. Both work without conflicting with the log output because logs go to the same UART0 channel — log lines and UM980 echo share the same USB CDC stream, which is acceptable for development debugging purposes.

### Anti-Patterns to Avoid

- **Calling `wifi.connect()` immediately after `wifi.disconnect()`:** The ESP-IDF documentation warns this causes a reconnect loop race condition. Always add a delay before reconnecting.
- **Subscribing inside the MQTT event callback passed to `EspMqttClient::new_cb()`:** The `EspMqttClient` is not available inside that callback. Use the pump-thread pattern with `EspMqttConnection::next()` instead, and hold the client in an `Arc<Mutex<>>`.
- **Holding the `Mutex<EspMqttClient>` lock while the pump thread needs to process:** The pump thread calls `connection.next()` which can block. Keep publish/subscribe calls brief.
- **Not pumping the `EspMqttConnection`:** If nothing calls `connection.next()`, all `client.publish()` and `client.subscribe()` calls will block indefinitely. The pump thread MUST start before any publish/subscribe.
- **Using `client.publish()` (blocking) vs `client.enqueue()` (non-blocking) incorrectly:** `publish()` blocks until the message is sent. `enqueue()` queues it to the outbox. For heartbeats from a dedicated thread, `publish()` is fine. For time-sensitive callers, use `enqueue()`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WiFi connection management | Custom WiFi state machine | `BlockingWifi<EspWifi>` | Handles STA mode, DHCP, event loop wiring |
| MQTT protocol framing | Custom TCP/MQTT parser | `EspMqttClient` | ESP-IDF MQTT handles QoS, keepalive, LWT, reconnect, outbox |
| LWT registration | Manual CONNECT packet | `LwtConfiguration` in `MqttClientConfiguration` | LWT must be sent in MQTT CONNECT; can't add it post-connect |
| MQTT retain flag | Manual broker config | `retain: true` in `publish()` / `LwtConfiguration` | Broker-managed; just set the flag |
| UART framing | Custom byte accumulator | `UartDriver` with `rx_buffer_size=4096` | Ring buffer handles fragmented reads; Phase 3 UART pipeline adds accumulator |
| Exponential backoff timer | Tokio/embassy timers | `std::thread::sleep()` with doubling counter | std sleep is sufficient; no async runtime needed |

**Key insight:** The ESP-IDF MQTT client (and its Rust wrapper) handles nearly all protocol complexity. The application layer only needs to: (1) configure credentials and LWT at connect time, (2) pump the event connection, and (3) re-subscribe on `Connected` events.

---

## Common Pitfalls

### Pitfall 1: LWT topic string lifetime

**What goes wrong:** `LwtConfiguration.topic` is `&'a str`. If you build the topic string inside `mqtt_connect()` as a local `String`, the reference goes out of scope before `EspMqttClient::new()` uses it, causing a compile error or dangling reference.
**Why it happens:** The LWT configuration struct is lifetime-parameterized (`LwtConfiguration<'a>`). The topic string must live at least as long as the `MqttClientConfiguration`.
**How to avoid:** Build the LWT topic string before the function, or hold it in a `let lwt_topic = format!(...)` that is declared before `conf` in the same scope so it outlives `conf`.
**Warning signs:** Compiler error about lifetime mismatch or "does not live long enough" on `conf`.

### Pitfall 2: MQTT publish/subscribe blocks without pump thread

**What goes wrong:** `client.publish()` or `client.subscribe()` hangs forever.
**Why it happens:** The underlying ESP-IDF MQTT client requires its event loop to be pumped. In the `EspMqttConnection` model, `connection.next()` must be called continuously in a thread. If no thread is pumping, all client operations stall.
**How to avoid:** Spawn the pump thread BEFORE calling any `client.subscribe()` or `client.publish()`.
**Warning signs:** Application hangs at the first publish or subscribe call with no log output.

### Pitfall 3: WiFi reconnect loop after intentional disconnect

**What goes wrong:** Supervisor thread tries to reconnect immediately after an intentional `wifi.disconnect()` call, creating a storm of reconnect attempts.
**Why it happens:** The `WIFI_EVENT_STA_DISCONNECTED` event fires for both intentional and unexpected disconnects. The ESP-IDF documentation explicitly warns: "If the event is raised because `esp_wifi_disconnect()` is called, the application should not call `esp_wifi_connect()` to reconnect."
**How to avoid:** Use a flag (`intentional_disconnect: bool`) that the supervisor checks before reconnecting.
**Warning signs:** Log shows rapid repeated connect/disconnect cycles.

### Pitfall 4: Re-subscribing after broker restart

**What goes wrong:** Device reconnects to MQTT broker after a broker restart but no longer receives messages on previously subscribed topics.
**Why it happens:** MQTT subscriptions are session-state on the broker. When `clean_session=true` (the default), the broker drops all session state on disconnect. Even when the client reconnects, subscriptions are gone unless re-sent.
**How to avoid:** Watch for `EventPayload::Connected` in the pump thread and re-call `client.subscribe()` for all required topics every time it fires.
**Warning signs:** Device shows "MQTT connected" in logs but config commands never arrive.

### Pitfall 5: UART1 pin assignment on XIAO ESP32-C6

**What goes wrong:** Using wrong GPIO pins for UART1, getting no data from UM980 or a compile error.
**Why it happens:** XIAO ESP32-C6 board exposes UART1 on GPIO20 (RX) and GPIO21 (TX) — physical pins D8/D9 on the Seeed Studio pinout. Confusing these with UART0 (which goes to USB CDC) will cause silent failures.
**How to avoid:** Use `peripherals.pins.gpio20` for RX and `peripherals.pins.gpio21` for TX when constructing `UartDriver` for UART1.
**Warning signs:** No bytes read from UM980, or UART driver init returns an error.

### Pitfall 6: Sharing EspMqttClient across threads

**What goes wrong:** Rust rejects the code because `EspMqttClient` is not `Send` or requires ownership in multiple threads.
**Why it happens:** The pump thread takes ownership of `EspMqttConnection`. The heartbeat thread and main thread need the `EspMqttClient`. These are different types — `EspMqttClient` for publishing/subscribing, `EspMqttConnection` for receiving events.
**How to avoid:** Move `EspMqttConnection` into the pump thread. Wrap `EspMqttClient` in `Arc<Mutex<EspMqttClient>>` and clone the `Arc` for each thread that needs to publish.
**Warning signs:** Compiler error about `Send` bound or value moved after use.

### Pitfall 7: Stack overflow on new threads

**What goes wrong:** FreeRTOS task panics or hard-faults with stack overflow detection triggering.
**Why it happens:** The default stack size for Rust std threads under ESP-IDF is typically 4096 bytes. WiFi event processing and MQTT event processing may need more. The main task stack is set to 8000 in sdkconfig.defaults but spawned threads get the FreeRTOS default.
**How to avoid:** Use `std::thread::Builder::new().stack_size(8192).spawn(...)` for MQTT pump thread and UART bridge threads.
**Warning signs:** Random crashes or stack canary detection fires during WiFi/MQTT events.

---

## Code Examples

Verified patterns from official sources:

### WiFi Credentials as compile-time constants

```rust
// Source: config.rs (already exists in project)
// Fill in Phase 2:
pub const WIFI_SSID: &str = "your_ssid";   // replace with real value
pub const WIFI_PASS: &str = "your_pass";   // replace with real value
pub const MQTT_HOST: &str = "192.168.1.x"; // replace with broker IP
pub const MQTT_PORT: u16 = 1883;
pub const MQTT_USER: &str = "";
pub const MQTT_PASS: &str = "";
```

### MqttClientConfiguration with LWT

```rust
// Source: https://github.com/esp-rs/esp-idf-svc/blob/master/src/mqtt/client.rs
use esp_idf_svc::mqtt::client::{LwtConfiguration, MqttClientConfiguration};
use embedded_svc::mqtt::client::QoS;

let lwt_topic = format!("gnss/{}/status", device_id);
let conf = MqttClientConfiguration {
    client_id: Some(&device_id),
    username: if config::MQTT_USER.is_empty() { None } else { Some(config::MQTT_USER) },
    password: if config::MQTT_PASS.is_empty() { None } else { Some(config::MQTT_PASS) },
    lwt: Some(LwtConfiguration {
        topic: &lwt_topic,
        payload: b"offline",
        qos: QoS::AtLeastOnce,
        retain: true,
    }),
    keep_alive_interval: Some(std::time::Duration::from_secs(60)),
    reconnect_timeout: Some(std::time::Duration::from_secs(5)),
    ..Default::default()
};
```

### Publish with retain flag

```rust
// Source: https://github.com/esp-rs/esp-idf-svc/blob/master/src/mqtt/client.rs
// publish(topic, qos, retain, payload)
client.publish(
    &format!("gnss/{}/heartbeat", device_id),
    QoS::AtMostOnce,
    true,  // retain = true
    b"online",
)?;
```

### MQTT event pump thread

```rust
// Source: https://github.com/esp-rs/esp-idf-svc/blob/master/examples/mqtt_client.rs
// MUST start before any subscribe() or publish() call
std::thread::Builder::new()
    .stack_size(8192)
    .spawn(move || {
        while let Ok(event) = connection.next() {
            match event.payload() {
                EventPayload::Connected(_) => {
                    // Re-subscribe here (client accessed via Arc<Mutex<>>)
                }
                EventPayload::Disconnected => log::warn!("MQTT disconnected"),
                _ => {}
            }
        }
        log::error!("MQTT connection closed");
    })
    .unwrap();
```

### UartDriver for UART1

```rust
// Source: https://docs.esp-rs.org/esp-idf-hal/esp_idf_hal/uart/struct.UartDriver.html
// XIAO ESP32-C6: GPIO20=RX, GPIO21=TX for UART1
use esp_idf_svc::hal::uart::{UartDriver, config::Config};
use esp_idf_svc::hal::units::Hertz;

let um980 = UartDriver::new(
    peripherals.uart1,
    peripherals.pins.gpio21,  // TX
    peripherals.pins.gpio20,  // RX
    Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None,
    Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None,
    &Config::new()
        .baudrate(Hertz(115_200))
        .rx_buffer_size(4096),  // matches UART_RX_BUF_SIZE in config.rs
)?;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `embedded-svc` WiFi trait directly | `esp_idf_svc::wifi::BlockingWifi` wrapper | esp-idf-svc ~0.46+ | Cleaner API; `BlockingWifi::wrap()` + `wait_netif_up()` |
| `EspMqttClient::new_cb()` with closure | `EspMqttClient::new()` returning `(client, connection)` | ~0.48+ | Pump-thread pattern decouples events from client; allows re-subscribe |
| `esp-idf-svc` re-exporting `esp-idf-hal` types | Crates separate but `esp_idf_svc::hal::*` re-exports | ~0.49+ | Can use either `esp_idf_hal::uart` or `esp_idf_svc::hal::uart` — same types |

**Deprecated/outdated:**
- `rust-esp32-std-demo` repo: Now superseded by examples in `esp-idf-svc` itself (per official docs). Do not use as reference.
- `EspMqttClient::new_cb()`: The callback-variant constructor — use `EspMqttClient::new()` + pump thread instead for reconnect-aware code.

---

## Open Questions

1. **Can `UartDriver` be shared across threads without `Arc<Mutex<>>`?**
   - What we know: `UartDriver` wraps a `*mut esp_idf_sys::uart_port_t`. TX and RX are separate operations.
   - What's unclear: Whether `UartDriver` implements `Send`. If not, the bridge threads need a different approach (e.g., split into tx/rx handles, or use raw unsafe UART calls).
   - Recommendation: Check compiler output when attempting to move `UartDriver` into a thread. If it fails, use `unsafe` raw IDF calls for the simpler bridge, or restructure with `Arc<Mutex<UartDriver>>` accepting the mutex overhead.

2. **Does the XIAO ESP32-C6's UART0 conflict with USB Serial/JTAG for stdin reads?**
   - What we know: XIAO ESP32-C6 uses the built-in USB Serial/JTAG controller (not a separate CH340/CP2102 chip). UART0 (GPIO16/17) may not be the USB path.
   - What's unclear: Whether `std::io::stdin()` on XIAO ESP32-C6 maps to UART0 or to the USB Serial/JTAG CDC port. The ESP32-C6 USB Serial/JTAG controller is separate from UART0 hardware.
   - Recommendation: Verify by printing to `log::info!` which console the output appears on. If stdin/stdout map to the USB JTAG CDC port (not UART0), then UART0 hardware is free — and the "USB debug serial" in CONN-07 refers to the USB JTAG CDC virtual COM port, not physical UART0 pins. This is likely the correct interpretation for XIAO ESP32-C6 since it lacks a physical USB-UART chip.

3. **`reconnect_timeout: Some(Duration)` in `MqttClientConfiguration` — does it control reconnect delay or connection timeout?**
   - What we know: Field is present in `MqttClientConfiguration`. The underlying ESP-IDF field is likely `reconnect_timeout_ms`.
   - What's unclear: In ESP-IDF, this may be the delay between reconnect attempts (not how long to wait for a connection). Setting it to `None` may disable automatic reconnection entirely.
   - Recommendation: Set `reconnect_timeout: Some(Duration::from_secs(5))` for 5-second reconnect intervals. Verify with broker restart test during Phase 2 verification.

---

## Sources

### Primary (HIGH confidence)

- `https://github.com/esp-rs/esp-idf-svc/blob/master/src/mqtt/client.rs` — `MqttClientConfiguration`, `LwtConfiguration`, `EspMqttClient::new()`, `publish()` signatures
- `https://github.com/esp-rs/esp-idf-svc/blob/master/examples/mqtt_client.rs` — pump-thread pattern, subscribe/publish flow
- `https://github.com/esp-rs/esp-idf-svc/blob/master/examples/wifi.rs` — `BlockingWifi` setup, `wait_netif_up()` sequence
- `https://github.com/esp-rs/esp-idf-svc/blob/master/src/wifi.rs` — `is_connected()`, no built-in auto-reconnect confirmed
- `https://docs.esp-rs.org/esp-idf-hal/esp_idf_hal/uart/struct.UartDriver.html` — `UartDriver::new()` signature, `read()`, `write()` methods
- `https://www.espboards.dev/esp32/xiao-esp32c6/` — XIAO ESP32-C6 pinout: UART1 on GPIO20 (RX) / GPIO21 (TX)

### Secondary (MEDIUM confidence)

- `https://github.com/esp-rs/esp-idf-svc/issues/90` — Re-subscribe on `Connected` event confirmed as the community-endorsed pattern
- `https://forum.seeedstudio.com/t/xiao-esp32c6-uarts/292856` — XIAO ESP32-C6 UART pin assignments corroborated
- `https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/wifi.html` — WiFi reconnect event caveats (don't call `esp_wifi_connect()` after intentional `esp_wifi_disconnect()`)
- `https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/protocols/mqtt.html` — ESP-IDF MQTT `disable_auto_reconnect`, LWT, outbox behavior

### Tertiary (LOW confidence)

- WebSearch results for UART timeout behavior — read timeout not directly exposed in `esp-idf-hal` 0.45.2 (unverified, would need source inspection)
- WebSearch results for USB Serial/JTAG vs UART0 on ESP32-C6 — open question, needs on-device verification

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries are pinned from Phase 1; API confirmed via source inspection
- Architecture: HIGH — patterns confirmed via official example code and source
- UART1 GPIO pins: HIGH — confirmed via Seeed Studio XIAO ESP32-C6 pinout documentation
- WiFi reconnect: HIGH — no built-in auto-reconnect confirmed via source inspection
- MQTT re-subscribe: HIGH — community-confirmed via issue #90 discussion
- UART bridge USB question: LOW — open question requiring on-device test

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (30 days — esp-idf-svc and esp-idf-hal are version-pinned; low churn risk)
