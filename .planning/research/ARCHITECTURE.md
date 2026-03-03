# Architecture Research

**Domain:** Embedded Rust firmware — ESP32-C6 GNSS-to-MQTT bridge
**Researched:** 2026-03-03
**Confidence:** MEDIUM (WebSearch/WebFetch unavailable this session; based on esp-rs ecosystem knowledge through August 2025 training data, cross-referenced with official esp-rs book and crate documentation patterns. Flag for verification before implementation.)

---

## Standard Architecture

### System Overview

```
┌────────────────────────────────────────────────────────────────────┐
│                        EXTERNAL INTERFACES                          │
│  ┌──────────────┐  ┌──────────────────┐  ┌───────────────────────┐ │
│  │  UM980 GNSS  │  │   MQTT Broker    │  │  BLE Central (phone)  │ │
│  │  UART 115200 │  │  (Mosquitto etc) │  │  Provisioning client  │ │
│  └──────┬───────┘  └────────┬─────────┘  └──────────┬────────────┘ │
└─────────┼──────────────────┼────────────────────────┼──────────────┘
          │ UART RX/TX       │ TCP/WiFi               │ BLE GATT
┌─────────┼──────────────────┼────────────────────────┼──────────────┐
│                       ESP32-C6 FIRMWARE                             │
│                                                                     │
│  ┌───────────────┐   ┌──────────────┐   ┌───────────────────────┐  │
│  │ UART Reader   │   │ MQTT Client  │   │   BLE Provisioner     │  │
│  │ (FreeRTOS     │   │ (esp-mqtt /  │   │   (esp-idf BLE or     │  │
│  │  task, UART0) │   │  mqtt-client)│   │    esp32-nimble-ble)  │  │
│  └──────┬────────┘   └──────┬───────┘   └──────────┬────────────┘  │
│         │                  │                        │               │
│         ▼                  │                        │               │
│  ┌───────────────┐         │                        │               │
│  │ NMEA Router   │─────────▶  Channel/Queue         │               │
│  │ (parser +     │         │  (sentence → topic)    │               │
│  │  topic map)   │         │                        │               │
│  └───────────────┘         │                        │               │
│                            │                        │               │
│  ┌─────────────────────────┼────────────────────────┼─────────────┐ │
│  │              SHARED STATE / MESSAGE BUS           │             │ │
│  │   ┌───────────┐  ┌──────────────┐  ┌────────────┐│             │ │
│  │   │  NVS      │  │  LED State   │  │ Heartbeat  ││             │ │
│  │   │  Config   │  │  Machine     │  │  Timer     ││             │ │
│  │   │  Store    │  │  (RGB/GPIO)  │  │  (periodic)││             │ │
│  │   └───────────┘  └──────────────┘  └────────────┘│             │ │
│  └──────────────────────────────────────────────────┘             │ │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                     WiFi Manager                            │   │
│  │       (esp-idf WiFi + reconnect loop + event handler)       │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| UART Reader | DMA-buffered read of raw bytes from UM980; accumulate into line-terminated NMEA sentences; push complete sentences to a channel | `esp_idf_hal::uart::UartDriver` with `uart_read_bytes`, dedicated FreeRTOS task |
| NMEA Router | Parse each NMEA sentence to extract sentence type (e.g. `GNGLL`); route to per-type MQTT topic string; forward tuple `(topic, payload)` to MQTT publish channel | `nmea` or `nmea0183` crate, or custom parser for the `$...,*XX` sentence-type prefix |
| MQTT Client | Maintain persistent MQTT connection; subscribe to config topic on connect; publish outbound tuples from channel; handle reconnect | `esp_idf_svc::mqtt::client::EspMqttClient` — the official esp-idf-svc binding |
| MQTT Config Subscriber | Receive retained `gnss/{id}/config` payload; split into newline-delimited UM980 commands; write each over UART TX to UM980 | Part of MQTT Client event handler; writes back through `UartDriver` TX |
| BLE Provisioner | On first boot (no credentials in NVS): advertise BLE GATT service with characteristic for WiFi SSID, password, MQTT host/port/user/pass; write received values to NVS; reboot | `esp_idf_svc::bt` (Bluedroid) or `esp32-nimble` crate (NimBLE); typically a custom GATT profile |
| NVS Config Store | Persist and retrieve WiFi credentials, MQTT connection parameters, device flags (provisioned boolean) | `esp_idf_svc::nvs::EspNvs` with a named namespace |
| LED State Machine | Reflect system state (provisioning, connecting, connected, error) via GPIO LED; driven by state events | Simple enum state machine + `esp_idf_hal::gpio::PinDriver`; updated from any task via shared atomic or channel message |
| Heartbeat Timer | Publish periodic MQTT message to `gnss/{id}/heartbeat`; also serves as MQTT keepalive signal | `esp_idf_hal::timer` or FreeRTOS timer; enqueues to publish channel |
| WiFi Manager | Connect using NVS credentials; handle disconnect events; trigger reconnect with backoff; signal MQTT layer to reconnect | `esp_idf_svc::wifi::EspWifi` + `EspEventLoop` subscriptions |
| Device ID Provider | Derive unique device ID from ESP32 hardware MAC/efuse; expose as constant string | `esp_idf_hal::sys::esp_efuse_mac_get_default` or `esp_idf_svc::sys` binding |

---

## Recommended Project Structure

```
esp32-gnssmqtt/
├── Cargo.toml                     # workspace or single crate
├── build.rs                       # linker script, sdkconfig generation
├── sdkconfig.defaults             # ESP-IDF Kconfig: UART, BLE, MQTT buffer sizes
├── .cargo/
│   └── config.toml                # target = riscv32imac-esp-espidf; runner = espflash
└── src/
    ├── main.rs                    # app entrypoint: init NVS, branch to provisioning or main loop
    ├── config.rs                  # compile-time constants (UART baud, topic prefixes)
    ├── device_id.rs               # read hardware MAC, format as hex string
    ├── nvs_store.rs               # NVS read/write abstraction (credentials, flags)
    ├── wifi.rs                    # WiFi init, connect, reconnect loop
    ├── ble_provision.rs           # BLE GATT server for first-boot provisioning
    ├── uart_reader.rs             # UART init, DMA read loop, sentence assembly, channel send
    ├── nmea_router.rs             # sentence type extraction, topic string construction
    ├── mqtt_client.rs             # MQTT init, connect, subscribe, publish loop, event handler
    ├── led.rs                     # LED GPIO init, state machine, update function
    └── heartbeat.rs               # timer setup, periodic publish enqueue
```

### Structure Rationale

- **One crate, not a workspace:** The firmware is a single binary target. A workspace adds no benefit until unit-testable pure logic warrants a separate lib crate.
- **`sdkconfig.defaults`:** Critical for controlling MQTT RX/TX buffer sizes, BLE stack enable, UART FIFO depth — must be checked in.
- **`build.rs`:** Required by `esp-idf-sys` embuild to find and link the ESP-IDF C components.
- **Per-file modules:** Each component above maps to one file. This makes phase-by-phase construction clean — add a file, wire it into `main.rs`.

---

## Architectural Patterns

### Pattern 1: FreeRTOS Thread-Per-Component (Recommended for this project)

**What:** Each long-running component (UART reader, MQTT client, heartbeat) runs in its own `std::thread::spawn` which maps 1:1 to a FreeRTOS task. Communication between tasks uses `std::sync::mpsc` channels or `crossbeam-channel` equivalents — both work on ESP-IDF because `std` is available.

**When to use:** When using `esp-idf-hal` / `esp-idf-svc` (the `std`-capable ESP-IDF Rust bindings). This is the correct model for the ESP32-C6 with esp-idf. FreeRTOS tasks are the native concurrency primitive under the hood; `std::thread` is a thin wrapper.

**Trade-offs:** Simpler than async; each thread needs its own stack (configure with `thread::Builder::stack_size`); FreeRTOS tasks on ESP32 default to 4KB stack which is too small for most Rust code (use 8-16KB per task). Context switching overhead is minimal at the scale of this firmware.

**Example:**
```rust
// src/uart_reader.rs
use std::sync::mpsc::SyncSender;
use esp_idf_hal::uart::UartDriver;

pub fn start(uart: UartDriver<'static>, tx: SyncSender<String>) {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let mut buf = [0u8; 256];
            let mut line = String::new();
            loop {
                let n = uart.read(&mut buf, 100).unwrap_or(0);
                for &b in &buf[..n] {
                    if b == b'\n' {
                        let sentence = core::mem::take(&mut line);
                        let _ = tx.send(sentence);
                    } else if b != b'\r' {
                        line.push(b as char);
                    }
                }
            }
        })
        .expect("UART reader thread failed to spawn");
}
```

### Pattern 2: Channel-Based Decoupling Between UART and MQTT

**What:** The UART reader and MQTT publisher are decoupled via a bounded `mpsc` channel. The UART task pushes complete NMEA sentences; the MQTT task pops them, routes to the correct topic, and publishes. The channel provides natural backpressure: if MQTT is slow (reconnecting), the bounded channel fills and UART drops sentences rather than causing memory exhaustion.

**When to use:** Always — this is the correct model for a streaming source feeding a potentially-disconnected sink.

**Trade-offs:** Bounded queue = data loss during broker unavailability. Acceptable per the requirements ("real-time relay only, no buffering across power cycles"). An unbounded channel risks OOM on extended broker outage.

**Example:**
```rust
// src/main.rs (wiring)
use std::sync::mpsc::sync_channel;

let (nmea_tx, nmea_rx) = sync_channel::<String>(64); // 64-sentence buffer
uart_reader::start(uart_driver, nmea_tx);
mqtt_client::start(mqtt_client, nmea_rx, device_id);
```

### Pattern 3: Event-Loop MQTT with EspMqttClient

**What:** `EspMqttClient` from `esp_idf_svc::mqtt::client` uses a callback-based event model. The MQTT client is created with a closure that handles `Connected`, `Received` (for config topic), and `Disconnected` events. Publishing is done by calling `client.publish(...)` from the MQTT task thread — the callback fires on the ESP-IDF internal MQTT task thread.

**When to use:** This is the only supported model for `EspMqttClient`. Do not attempt to use it in a purely blocking poll loop.

**Trade-offs:** The callback fires on an ESP-IDF internal thread, so any shared state accessed in the callback must use `Arc<Mutex<...>>` or `Arc<AtomicBool>`. Keep the callback fast — do not block inside it. Route received config payloads back to the UART TX via a channel or shared buffer rather than writing UART inside the callback.

**Example:**
```rust
// src/mqtt_client.rs (sketch)
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS, EventPayload};

let config = MqttClientConfiguration {
    client_id: Some(&device_id),
    username: Some(&mqtt_user),
    password: Some(&mqtt_pass),
    ..Default::default()
};

let config_tx_clone = config_uart_tx.clone(); // to forward config to UART
let (client, _conn) = EspMqttClient::new_cb(
    &mqtt_url,
    &config,
    move |event| match event.payload() {
        EventPayload::Connected(_) => {
            log::info!("MQTT connected");
            // re-subscribe to config topic on reconnect
        }
        EventPayload::Received { topic: Some(t), data, .. } if t.ends_with("/config") => {
            // forward UM980 commands to UART TX channel
            let _ = config_tx_clone.try_send(data.to_vec());
        }
        EventPayload::Disconnected => {
            log::warn!("MQTT disconnected");
        }
        _ => {}
    },
)?;
```

### Pattern 4: NVS-Gated Boot Branching

**What:** On startup, check a boolean flag in NVS (e.g., `"provisioned"` key). If absent or false, enter BLE provisioning mode. If true, load credentials and proceed to WiFi+MQTT. After successful provisioning, write the flag and reboot.

**When to use:** This is the standard pattern for zero-touch provisioning on ESP32. It avoids maintaining two parallel code paths at runtime.

**Trade-offs:** Reboot-to-switch-modes means a ~2s delay after provisioning. Acceptable; avoids complex state management between provisioning and operational modes.

**Example:**
```rust
// src/main.rs
let nvs = EspDefaultNvsPartition::take()?;
let store = nvs_store::NvsStore::new(nvs)?;

if !store.is_provisioned()? {
    log::info!("Not provisioned — starting BLE provisioner");
    ble_provision::run_and_save(&mut store)?; // blocks until done, saves creds
    esp_idf_hal::reset::restart();            // reboot into normal mode
}

let creds = store.load_credentials()?;
wifi::connect(&creds)?;
mqtt_client::start(creds, nmea_rx, device_id)?;
```

---

## Data Flow

### Primary Flow: NMEA Sentences UART → MQTT

```
UM980 GNSS Module
    │
    │ (115200 baud UART bytes, continuous stream)
    ▼
UART Reader Task
    │ assemble bytes into '\n'-terminated sentences
    │ discard malformed / incomplete lines
    ▼
mpsc::SyncSender<String>  [bounded: 64 sentences]
    │
    ▼
MQTT Client Task  (pops from channel in a loop)
    │ nmea_router: extract sentence type from "$GNGLL,..."
    │ build topic:  "gnss/{device_id}/nmea/GNGLL"
    ▼
EspMqttClient::publish(topic, payload, QoS::AtMostOnce, retain=false)
    │
    ▼
MQTT Broker (external)
```

### Config Flow: Broker → UM980 Initialization

```
MQTT Broker (retained message on "gnss/{device_id}/config")
    │
    │ (delivered once on subscribe)
    ▼
EspMqttClient event callback  (EventPayload::Received)
    │ payload: newline-delimited UM980 commands
    │ e.g. "GPGLL ON\r\nGPGGA ON\r\nGPRMC ON\r\n"
    ▼
mpsc::SyncSender<Vec<u8>>  [config_uart_tx channel]
    │
    ▼
UART Writer (in UART Reader task or separate config-apply task)
    │ iterate lines, write each with "\r\n" terminator
    │ optional: small delay between commands (UM980 may need it)
    ▼
UM980 GNSS Module  (applies configuration, begins streaming)
```

### Provisioning Flow (First Boot)

```
Power On → NVS check: not provisioned
    │
    ▼
BLE Provisioner: start GATT server, advertise
    │
    │  (user writes credentials via BLE client app)
    ▼
NVS Store: save WiFi SSID/pass + MQTT host/port/user/pass
    │
    ▼
Set NVS provisioned=true → reboot
    │
    ▼
Normal boot: load credentials → WiFi connect → MQTT connect
```

### LED State Machine Flow

```
System events → LED State enum → GPIO output

States:
  Provisioning     → rapid blink (BLE advertising)
  WiFi Connecting  → slow blink
  MQTT Connecting  → double blink pattern
  Operational      → solid on
  Error            → fast blink (3x) then off
  Disconnected     → slow blink (same as WiFi Connecting)

State transitions pushed via:
  mpsc channel (preferred) OR
  Arc<AtomicU8> (lighter weight)

LED task: polls channel/flag at ~100ms interval, drives GPIO
```

---

## Concurrency Model

### Decision: FreeRTOS Tasks via std::thread (NOT embassy async)

**Recommendation:** Use `esp-idf-hal` + `esp-idf-svc` with `std::thread`. Do not use Embassy for this project.

**Rationale:**

| Factor | std::thread (esp-idf) | Embassy (esp-hal bare-metal) |
|--------|-----------------------|------------------------------|
| WiFi support | Full, mature via esp-idf-svc | Experimental, incomplete on ESP32-C6 as of 2025 |
| BLE support | Full via esp-idf Bluedroid/NimBLE | Not supported in embassy-esp |
| MQTT | esp-idf-svc EspMqttClient, stable | Must use external async MQTT crate, less tested |
| std availability | Yes (`std` feature on esp-idf-hal) | No (no_std only) |
| Complexity | Lower: familiar Rust threading model | Higher: async executor, lifetime constraints |
| NVS | esp-idf-svc EspNvs, stable | Manual flash access |
| Maturity | Production-ready | Actively developed, breaking changes expected |

Embassy's main advantage — power efficiency via async/await with WFI — is not a requirement here (WiFi/BLE keep the radio powered; the device is likely mains or USB powered for a GNSS station).

**Thread allocation (approximate):**

| Thread | Stack Size | Priority | Role |
|--------|-----------|----------|------|
| main (app_main) | 8 KB | 5 (normal) | Init, boot branching, credential loading |
| uart_reader | 8 KB | 10 (above normal) | Time-sensitive: must keep up with 115200 baud stream |
| mqtt_task | 12 KB | 7 | Dequeue sentences, publish, manage reconnect |
| heartbeat | 4 KB | 5 | Timer callback or loop; low priority is fine |
| led_task | 4 KB | 3 (low) | GPIO toggle; never blocks |
| esp-idf internal (WiFi/BT) | managed by IDF | varies | Not directly controlled |

Note: Total FreeRTOS heap on ESP32-C6 is ~320 KB. With MQTT buffers, UART FIFO, BLE stack (~60 KB), WiFi stack (~80 KB), Rust stacks and heap, budget carefully. Set `CONFIG_ESP_MAIN_TASK_STACK_SIZE` and per-thread sizes explicitly.

---

## Build Order (Phase Dependencies)

The firmware has hard dependencies between components. Build in this order:

```
Phase 1: Foundation
  ├── Cargo.toml + build.rs + sdkconfig.defaults
  ├── .cargo/config.toml (target, runner, linker)
  ├── device_id.rs (pure, no deps)
  └── nvs_store.rs (needed by provisioning AND main boot)

Phase 2: BLE Provisioning
  ├── ble_provision.rs  [depends on: nvs_store]
  └── main.rs boot branch: if !provisioned → ble, else continue

Phase 3: WiFi + MQTT Skeleton
  ├── wifi.rs  [depends on: nvs_store for credentials]
  ├── mqtt_client.rs skeleton  [depends on: wifi]
  └── Verify: device connects to broker, heartbeat publishes

Phase 4: UART + NMEA Pipeline
  ├── uart_reader.rs  [depends on: channel infrastructure]
  ├── nmea_router.rs  [depends on: uart_reader output]
  └── mqtt_client.rs publish loop  [depends on: nmea_router output]

Phase 5: MQTT Config → UM980 Init
  ├── config subscriber in mqtt_client.rs event handler
  └── UART TX write path in uart_reader.rs (or separate config_writer.rs)

Phase 6: LED State Machine + Reconnect Logic
  ├── led.rs state machine
  ├── wifi.rs reconnect with backoff
  └── mqtt_client.rs reconnect on disconnect event

Phase 7: Integration + Hardening
  ├── Stack size tuning
  ├── Memory profiling (heap_caps_get_free_size)
  └── Field test with live UM980
```

**Critical dependency:** MQTT subscribe to config topic MUST happen inside the `Connected` event handler, not at startup. The client will reconnect, and subscriptions must be re-established on every reconnect — this is a common bug if wired at init time only.

---

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| UM980 GNSS | UART full-duplex, 115200 8N1, no flow control | RX: continuous read; TX: send init commands from config topic. UM980 outputs NMEA + optional proprietary sentences. |
| MQTT Broker | `esp_idf_svc::mqtt::client::EspMqttClient` over TCP | MQTT 3.1.1; username/password; QoS 0 for NMEA (fire-and-forget); QoS 1 for heartbeat (optional). No TLS in v1. |
| BLE Central (phone) | Custom GATT profile; one service, characteristics for each credential field | Alternatively: use Espressif's `wifi_provisioning` component via esp-idf which uses BLE Protocol Buffers over GATT — but requires protobuf dependency and a companion app. Custom GATT with plain strings is simpler for v1. |
| NVS Flash | `esp_idf_svc::nvs::EspNvs<NvsReadWrite>` | Namespace: `"gnssmqtt"`. Keys: `"ssid"`, `"wifi_pass"`, `"mqtt_host"`, `"mqtt_port"`, `"mqtt_user"`, `"mqtt_pass"`, `"provisioned"`. Max key length: 15 chars. Max value: namespace-dependent. |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| UART Reader → NMEA Router → MQTT Client | `mpsc::SyncSender<String>` (bounded, 64) | Router logic can be inlined into UART reader or MQTT consumer; separate function/module is fine |
| MQTT Event Callback → UART TX | `mpsc::SyncSender<Vec<u8>>` (bounded, 8) | Config payloads are infrequent; small bound is fine |
| Any component → LED | `mpsc::SyncSender<LedState>` or `Arc<AtomicU8>` | AtomicU8 is simpler; channel gives ordering guarantees |
| WiFi reconnect → MQTT reconnect | `Arc<AtomicBool>` (`wifi_connected` flag) or event channel | MQTT task polls or blocks on flag before attempting connect |

---

## Anti-Patterns

### Anti-Pattern 1: Subscribing to MQTT Config Topic Only at Startup

**What people do:** Subscribe once in the initialization function, then assume the subscription persists.

**Why it's wrong:** If the MQTT broker disconnects and reconnects, all subscriptions are lost. The retained config message will not be re-delivered. The UM980 will never receive its initialization commands after a reconnect.

**Do this instead:** Subscribe to `gnss/{device_id}/config` inside the `Connected` event handler. Every reconnection automatically re-subscribes and the broker re-delivers the retained message.

### Anti-Pattern 2: Writing UART from the MQTT Event Callback

**What people do:** Call `uart.write(data)` directly inside the `EventPayload::Received` handler.

**Why it's wrong:** The MQTT event callback fires on an ESP-IDF internal MQTT task thread. That thread has a limited stack (~4 KB by default) and holding UART write locks can cause priority inversion or deadlock with the UART reader task.

**Do this instead:** Send the received config payload to a dedicated channel. The UART reader task (or a config-writer task) dequeues and writes to UART on its own thread.

### Anti-Pattern 3: Unbounded Channel Between UART and MQTT

**What people do:** Use `mpsc::channel()` (unbounded) for NMEA sentences.

**Why it's wrong:** If the MQTT broker is unreachable for minutes, NMEA sentences accumulate in memory. The ESP32-C6 has ~320 KB free heap. At ~100 bytes/sentence and 10 sentences/second, this exhausts in under a minute.

**Do this instead:** Use `mpsc::sync_channel(64)` (bounded). The sender blocks or uses `try_send` (preferred: drop the sentence with a log warning). Real-time relay does not need buffering.

### Anti-Pattern 4: Blocking Inside FreeRTOS Task with Wrong Stack Size

**What people do:** Spawn a `std::thread` without setting stack size, hit stack overflow, get opaque panic or reset.

**Why it's wrong:** Default thread stack size on ESP-IDF is configured by `CONFIG_ESP_SYSTEM_EVENT_TASK_STACK_SIZE` but `std::thread` spawns at `CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT` (default 3 KB in some SDK versions). Rust frames are large.

**Do this instead:** Always use `std::thread::Builder::new().stack_size(N).spawn(...)`. Set N to at least 8192 bytes for any task that uses formatting, parsing, or heap allocation.

### Anti-Pattern 5: Using Embassy with esp-hal for This Feature Set

**What people do:** Choose Embassy (bare-metal async) for its modern Rust ergonomics.

**Why it's wrong:** As of 2025, Embassy on ESP32-C6 does not have stable WiFi or BLE support. The network stack (esp-wifi) is experimental. Provisioning via BLE is not available. This project requires both WiFi and BLE, which are only mature in the esp-idf-based stack.

**Do this instead:** Use `esp-idf-hal` + `esp-idf-svc`. Accept std threading. Revisit Embassy when esp-wifi matures (track https://github.com/esp-rs/esp-wifi).

### Anti-Pattern 6: Hardcoding UM980 Init Commands in Firmware

**What people do:** Embed UM980 NMEA output configuration as constants in Rust source.

**Why it's wrong:** Every configuration change (enable a new sentence type, change rate) requires reflashing. The project requirement explicitly avoids this.

**Do this instead:** Subscribe to the retained MQTT config topic. The retained message persists in the broker; updating the broker message reconfigures the device on next reboot or reconnect without firmware changes.

---

## Scaling Considerations

This is single-device embedded firmware — "scaling" means handling edge cases and resource constraints, not user load.

| Concern | Approach |
|---------|---------|
| NMEA throughput | UM980 at 10 Hz all sentence types ≈ ~20-50 sentences/sec. At 100 bytes avg = ~5 KB/s UART. ESP32-C6 UART with DMA handles this easily. MQTT at QoS 0 over WiFi 6 is sufficient. |
| Memory pressure | Monitor with `unsafe { esp_idf_sys::heap_caps_get_free_size(MALLOC_CAP_DEFAULT) }` in debug builds. Budget: WiFi ~80 KB, BLE ~60 KB during provisioning (can be deinited after), MQTT buffers (set `CONFIG_MQTT_BUFFER_SIZE`), Rust heap. |
| WiFi reconnect storms | Use exponential backoff (1s, 2s, 4s, up to 60s). Do not hammer reconnect on every disconnect event. |
| MQTT reconnect | Same backoff as WiFi. Check WiFi is connected before attempting MQTT reconnect. |
| NVS wear | NVS credentials written once at provisioning. Heartbeat and runtime state are not written to NVS. No wear concern. |
| UM980 UART buffer | UM980 outputs continuously. If the UART Reader task is preempted for too long, the hardware FIFO overflows. Give the UART reader a higher FreeRTOS priority than the MQTT task. |

---

## Sources

- esp-rs book (authoritative): https://docs.esp-rs.org/book/ — std vs no_std, project setup, thread model (MEDIUM confidence — verified against training knowledge through Aug 2025)
- esp-idf-hal crate: https://github.com/esp-rs/esp-idf-hal — UART, GPIO, timer drivers (MEDIUM confidence)
- esp-idf-svc crate: https://github.com/esp-rs/esp-idf-svc — WiFi, MQTT, NVS, BT bindings (MEDIUM confidence)
- esp-idf-sys crate: https://github.com/esp-rs/esp-idf-sys — embuild, sdkconfig (MEDIUM confidence)
- esp-wifi (Embassy WiFi): https://github.com/esp-rs/esp-wifi — experimental, not recommended for this project (MEDIUM confidence)
- UM980 UART interface: Unicore Communications UM980 datasheet — NMEA output, configuration commands via UART (LOW confidence — based on general RTK receiver knowledge; verify command syntax with vendor docs)
- FreeRTOS task model on ESP32: https://docs.espressif.com/projects/esp-idf/en/latest/esp32c6/api-guides/freertos-smp.html (MEDIUM confidence)

**Note:** WebSearch and WebFetch were unavailable during this research session. All findings are based on training data through August 2025. Before implementation, verify:
1. Current esp-idf-svc version and `EspMqttClient` API (callback vs connection-based API may have changed)
2. esp32-nimble crate status for NimBLE on ESP32-C6 specifically
3. Whether `esp_idf_svc::bt` provides a usable GATT server API in current versions, or whether esp-nimble-coex is needed

---

*Architecture research for: ESP32-C6 GNSS-to-MQTT bridge (embedded Rust)*
*Researched: 2026-03-03*
