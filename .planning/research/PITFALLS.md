# Pitfalls Research

**Domain:** Embedded Rust firmware — ESP32-C6 GNSS-to-MQTT bridge with BLE provisioning
**Researched:** 2026-03-03
**Confidence:** MEDIUM (web access unavailable; based on training knowledge of esp-rs ecosystem, FreeRTOS, UART/NMEA patterns, and MQTT client behavior — flagged where verification is needed)

---

## Critical Pitfalls

### Pitfall 1: esp-idf-hal vs esp-hal Framework Selection Lock-In

**What goes wrong:**
Developer starts with esp-hal (bare-metal, no_std, community-driven) assuming it is production-ready for ESP32-C6 WiFi and BLE, only to discover that WiFi and Bluetooth drivers on ESP32-C6 are only available through Espressif's `esp-idf` C SDK layer. Switching frameworks mid-project requires rewriting all peripheral drivers, task management, and network code.

**Why it happens:**
Both frameworks have active development. esp-hal looks appealing because it is pure Rust and avoids the C SDK binding overhead. The ESP32-C6 chip is also newer, and esp-hal's WiFi/BLE support for it lagged behind the more established ESP32-S series. Developers conflate "RISC-V support" with "full WiFi/BLE support."

**How to avoid:**
Use `esp-idf-hal` / `esp-idf-svc` stack from day one. This is the only path with production-ready WiFi (`EspWifi`), BLE (`esp-idf-svc` BLE GATT), MQTT (`esp-mqtt`), and NVS for the ESP32-C6. The Espressif `esp-rs` book explicitly recommends this path for connectivity-dependent projects. Pin the `esp-idf` version in `sdkconfig.defaults` (e.g., `v5.1` or `v5.2`) and commit `sdkconfig` to version control.

**Warning signs:**
- Project uses `esp-hal` crate without `esp-idf-svc` and attempts WiFi or BLE
- `Cargo.toml` references `esp-wifi` (the bare-metal WiFi crate) — this has limited stability for production use on ESP32-C6
- Build succeeds but linker fails with missing `esp_wifi_init` or `bt_controller_init` symbols

**Phase to address:** Project bootstrap / Phase 1 (toolchain and project scaffold)

---

### Pitfall 2: Toolchain Version Mismatch Between esp-idf-hal, esp-idf-sys, and esp-idf C SDK

**What goes wrong:**
`esp-idf-hal`, `esp-idf-sys`, and the actual `esp-idf` C SDK version pinned via `IDF_PATH` or downloaded by `embuild` fall out of sync. This produces cryptic linker errors ("undefined reference to `esp_...`"), panic at runtime due to ABI mismatches, or silent memory corruption from struct layout differences.

**Why it happens:**
The `esp-idf-sys` crate generates Rust FFI bindings from the actual esp-idf C headers at build time. If the crate version and the esp-idf SDK version are mismatched — even by a minor version — binding generation produces wrong types or missing symbols. The `embuild` crate that manages the SDK download is a third moving part. Developers often `cargo update` without realizing this pulls a new `esp-idf-sys` that expects a different SDK.

**How to avoid:**
- Pin exact versions in `Cargo.toml` with `=` version specifiers for `esp-idf-hal`, `esp-idf-sys`, `esp-idf-svc`, and `embuild`
- Use `esp_idf_sys::esp_idf_version_major!()` / the `ESP_IDF_VERSION` env var in `build.rs` to assert the expected SDK version at compile time
- Commit the `.embuild/` directory contents (or document the exact `ESP_IDF_VERSION` in README) so CI and other developers use the same SDK
- Follow the [esp-idf-template](https://github.com/esp-rs/esp-idf-template) as the canonical starting point — it wires up `embuild` correctly

**Warning signs:**
- Linker errors mentioning `esp_` prefixed symbols after `cargo update`
- Build log shows "Downloading IDF" pulling a different version than previously
- Runtime crash immediately at `app_main()` startup (before any user code runs)
- `bindgen` errors during build mentioning type conflicts

**Phase to address:** Phase 1 (toolchain scaffold) — pin all versions before writing any application code

---

### Pitfall 3: UART Receive Buffer Overflow with High-Frequency NMEA Output

**What goes wrong:**
The UM980 outputs NMEA sentences continuously at 115200 baud. At full output rate (multiple sentence types per second), the UART hardware FIFO (128 bytes on ESP32-C6) overflows if the application task is not reading fast enough. Overflow causes byte loss, producing truncated or corrupt NMEA sentences. The corruption is often silent — the parser does not error, it just processes garbage data.

**Why it happens:**
`esp-idf-hal` UART driver has a software ring buffer (configurable at init time) that is separate from the hardware FIFO. The default software buffer is 256 bytes. If the NMEA processing task blocks on WiFi/MQTT operations, the ring buffer fills and newer bytes are dropped. Because UART overflow in `esp-idf` is silently discarded by default (no error propagated to Rust), the application continues processing partial data.

**How to avoid:**
- Configure the UART driver with a large software RX ring buffer at init: `UartDriver::new(..., rx_buffer_size: 4096)` — minimum 2KB for UM980 at full output rate
- Dedicate a high-priority FreeRTOS task to UART reading that feeds a separate channel/queue — never read UART from the same task that does MQTT
- Enable UART event queue in esp-idf (`uart_driver_install` with event queue size > 0) and monitor for `UART_FIFO_OVF` and `UART_BUFFER_FULL` events during development
- Log UART overflow events as errors (not silently ignore them) for at least the first firmware revision
- Consider calling `uart_get_buffered_data_len()` periodically to track high-water mark

**Warning signs:**
- NMEA sentences with incorrect checksums arriving at parser
- Sentences appearing truncated mid-line (missing `\r\n` terminator in the right place)
- Parser error rate increases when WiFi reconnect is in progress
- MQTT publish latency spikes correlate with UART parse failures

**Phase to address:** Phase 2 (UART + NMEA parsing) — set buffer size and overflow monitoring from the start; do not tune later

---

### Pitfall 4: NMEA Sentence Fragmentation Across UART Reads

**What goes wrong:**
The application calls `uart.read()` in a loop, but UART reads do not respect NMEA sentence boundaries. A single read may return bytes that span two sentences, or a sentence may be split across multiple reads. If the parser assumes each `read()` call returns exactly one complete sentence, it silently drops the fragment at the end of one sentence and the fragment at the start of the next.

**Why it happens:**
UART is a byte stream. There is no framing at the hardware level. The `UartDriver::read()` in esp-idf-hal returns however many bytes are available up to the requested count — it does not stop at `\n`. Developers often write `read_to_string()` style code that does not accumulate a line buffer.

**How to avoid:**
- Implement a line buffer accumulator: read bytes into a `Vec<u8>` or fixed-size `[u8; 256]` ring buffer, scanning for `\r\n` to extract complete sentences
- Use `BufReader`-style logic: when `\n` is found, extract everything up to and including it as one sentence, leave the remainder in the buffer
- Alternatively, use `esp-idf-hal`'s `UartDriver::read_exact()` in combination with scanning for `$` (NMEA start-of-sentence marker) to re-sync after errors
- Test with a UM980 simulator or recorded NMEA stream before connecting to real hardware — inject fragmented reads deliberately

**Warning signs:**
- Sentence type extraction returns empty strings or garbage (sentence starts mid-way through a type)
- Parser logs show sentences starting with characters other than `$`
- MQTT topic names contain garbage from partial sentence type parsing

**Phase to address:** Phase 2 (UART + NMEA parsing) — part of the line accumulator implementation, not an afterthought

---

### Pitfall 5: BLE + WiFi Coexistence Causing WiFi Brownouts or BLE Disconnects

**What goes wrong:**
ESP32-C6 shares the 2.4GHz radio between BLE and WiFi using a time-division coexistence scheme. Running BLE provisioning (advertising + GATT connection) while simultaneously attempting WiFi scan/connect causes either BLE disconnections (provisioning fails mid-way) or WiFi association failures. In the worst case, both stacks appear to work in testing but fail intermittently in production environments with RF congestion.

**Why it happens:**
ESP-IDF's coexistence controller (`esp_coex`) coordinates radio time-sharing. The default coexistence mode gives WiFi higher priority than BLE during connection establishment. If provisioning firmware starts WiFi before BLE provisioning is complete, the WiFi stack aggressively uses the radio during BLE connection, starving BLE GATT notifications and causing iOS/Android BLE clients to time out. The sequence order of `esp_wifi_start()` vs. `bt_controller_enable()` matters critically.

**How to avoid:**
- Use a strict state machine: BLE advertising state → BLE connected + provisioning complete → BLE disconnect + WiFi start → WiFi connected → MQTT connect. Do not start WiFi until BLE provisioning delivers credentials and the BLE connection is cleanly closed.
- Call `esp_bluedroid_disable()` and `esp_bt_controller_disable()` before calling `esp_wifi_connect()` — this frees radio resources for WiFi
- In `esp-idf-svc`, ensure the `BtDriver` is dropped before the `EspWifi` connection attempt begins
- Test on a real mobile device (not emulator) — coexistence issues rarely manifest on bench with no RF contention

**Warning signs:**
- BLE provisioning succeeds 80-90% of the time but fails occasionally — suggests coexistence race condition
- WiFi connect time increases from ~2s to 15s+ when BLE is active
- BLE MTU negotiation fails (Android log shows "connection parameter update rejected")
- `esp_wifi_connect()` returns `ESP_ERR_WIFI_CONN` when BLE is running

**Phase to address:** Phase 3 (BLE provisioning) — state machine sequence must be validated on real hardware before moving to WiFi/MQTT phase

---

### Pitfall 6: NVS Partition Misconfiguration Causing Credential Loss or Boot Loops

**What goes wrong:**
NVS (Non-Volatile Storage) credentials (WiFi SSID/password, MQTT broker URL/credentials) are written but lost on OTA update, factory reset, or flash erase. Alternatively, NVS partition is too small for all stored keys, causing `ESP_ERR_NVS_NOT_ENOUGH_SPACE` at runtime. In the worst case, a corrupted NVS entry causes a panic loop on boot because the firmware tries to read a malformed credential string.

**Why it happens:**
The default `partitions.csv` in many esp-idf templates allocates only 24KB for NVS, which is the minimum. Developers add more NVS keys (device ID, MQTT topic prefix, config version) without checking available space. NVS does not compact automatically until it runs out of space — at that point, writes fail. Also, the esp-idf NVS library uses a page-based format where a corrupted page causes the entire namespace to be unreadable.

**How to avoid:**
- Define a custom `partitions.csv` with NVS at least 64KB (two NVS pages, more reliable with wear leveling)
- Use a dedicated NVS namespace per subsystem (e.g., `"wifi_creds"`, `"mqtt_creds"`, `"device_cfg"`) — isolates corruption scope
- On boot, wrap NVS reads in explicit error handling: if `NvsDefault::get()` returns `EspError`, branch to provisioning mode rather than panicking
- Test NVS behavior after `esptool.py erase_flash` — this is a common field operation that should gracefully return device to provisioning mode
- Do NOT store raw password strings — store them as blobs with a version byte to allow future schema migration

**Warning signs:**
- Device enters provisioning on every boot after an OTA update (NVS erased by OTA)
- `ESP_ERR_NVS_NOT_ENOUGH_SPACE` in logs after adding new stored keys
- Boot panic with `Guru Meditation` error pointing to NVS read code
- Intermittent WiFi connect failures that resolve after power cycle (corrupted SSID string)

**Phase to address:** Phase 1 (project scaffold) — define partition table before first NVS write; Phase 3 (provisioning) — validate full erase/re-provision cycle

---

### Pitfall 7: MQTT Reconnection Loop Creating Memory Leak or Heap Exhaustion

**What goes wrong:**
When the MQTT broker is unreachable (WiFi connected but broker down), the reconnect loop allocates a new MQTT client connection attempt on each retry without properly releasing the previous attempt's resources. Over hours, heap fragments until allocations fail, causing `panic!` or silent corruption. Related: retained message delivery on reconnect causes a burst of incoming messages that the application processes slower than they arrive, exhausting the MQTT client's receive queue.

**Why it happens:**
`esp-idf-svc`'s `EspMqttClient` manages reconnection internally when `MqttClientConfiguration::reconnect_timeout_ms` is set. However, if application code creates a new `EspMqttClient` on each reconnect (as seen in naive retry loops), the previous client is not dropped before the new one is created, causing double-registration of the internal mqtt task and handler memory. The reconnect-on-retain burst happens because QoS 1 retained messages are re-delivered on every clean reconnect, and the GNSS config topic retained message arrives before the UART initialization task has registered its handler.

**How to avoid:**
- Use `EspMqttClient`'s built-in reconnect mechanism (set `reconnect_timeout_ms` in config) — do NOT manually recreate the client on disconnect
- Implement a single `EspMqttClient` instance that lives for the firmware's lifetime; use the event callback to track connection state
- On `MQTT_EVENT_CONNECTED`, re-subscribe to all topics (subscriptions are lost on reconnect)
- Process the config topic retained message only after UART is initialized and the UM980 is ready to receive commands — use a flag/channel to sequence this
- Monitor heap with `esp_get_free_heap_size()` logged periodically; alert if drops below 20KB

**Warning signs:**
- Free heap reported by `esp_get_free_heap_size()` decreasing 1-4KB per disconnect/reconnect cycle
- WiFi reconnect succeeds but MQTT publish fails with allocation error
- UM980 receives garbled init commands on reconnect (retained message processed before UART ready)
- `esp_mqtt` task stack overflow logged (default MQTT task stack is 6KB — may need increase)

**Phase to address:** Phase 4 (MQTT integration) — reconnect loop must be implemented correctly from the start, not refactored later

---

### Pitfall 8: FreeRTOS Task Stack Overflow Causing Silent Corruption or Panic

**What goes wrong:**
Embedded Rust on ESP32 with esp-idf uses FreeRTOS tasks under the hood. Every `std::thread::spawn()` or `esp_idf_hal::task::thread::ThreadSpawnConfiguration` creates a FreeRTOS task with a fixed stack. Stack overflows in FreeRTOS do not necessarily cause an immediate panic — they corrupt adjacent heap or other task stacks, producing bizarre behavior like MQTT publishing garbage payloads or BLE GATT attributes returning wrong values.

**Why it happens:**
Default thread stack in the esp-idf Rust integration is 8KB. NMEA parsing with format strings, MQTT publish with topic string formatting, and JSON serialization (if added later) all use stack. A task that formats a topic string like `gnss/{device_id}/nmea/{sentence_type}` allocates several hundred bytes of stack per call. When tasks are nested (UART read → parse → channel send → MQTT task formats topic → publishes), stack consumption compounds. ESP32-C6 stack canary detection is not enabled by default in release builds.

**How to avoid:**
- Enable stack overflow detection: set `CONFIG_FREERTOS_TASK_STACK_OVEFLOW_CHECK=2` in `sdkconfig.defaults` for development builds (canary + full stack checking)
- Size each task generously: UART/NMEA task: 8KB minimum, MQTT task: 12KB minimum, BLE provisioning task: 16KB minimum
- Use `uxTaskGetStackHighWaterMark()` during development to measure actual peak usage, then set stack 50% larger than measured peak
- Prefer fixed-size buffers over `String`/`Vec` for NMEA line accumulation to make stack usage predictable
- Never allocate large arrays on stack inside tasks — use heap (`Box`, `Vec`) for buffers > 256 bytes

**Warning signs:**
- `Guru Meditation Error: Core 0 panic'ed (Unhandled debug exception)` with no clear cause
- Task that was working starts failing after adding a new feature in a different task
- MQTT payloads contain garbage bytes appended to valid NMEA data
- Stack overflow detector fires (`E (1234) FreeRTOS: Task X stack overflow`)

**Phase to address:** Phase 2 onwards — configure stack overflow detection in Phase 1; measure and tune in Phase 4 (MQTT integration) when all tasks are running together

---

### Pitfall 9: Retained MQTT Config Message Processed Before UM980 UART Is Ready

**What goes wrong:**
On reconnect to the MQTT broker, the broker immediately delivers the retained message on `gnss/{device_id}/config`. If this message is processed (UART command sent to UM980) before the UART driver is initialized, the write silently fails or the UM980 is in an indeterminate state. The UM980 misses its initialization commands, resulting in default NMEA output (wrong sentences, wrong rate) that looks valid but is not what the operator configured.

**Why it happens:**
MQTT subscription and message delivery happen asynchronously in the esp-idf MQTT client. The `MQTT_EVENT_DATA` callback fires as soon as the broker delivers the retained message, which may be within milliseconds of `MQTT_EVENT_CONNECTED`. UART initialization (baud rate config, DMA setup) takes a deterministic but non-zero time. If the MQTT task starts before UART initialization completes — which is the natural order if MQTT connects first — the race condition occurs.

**How to avoid:**
- Use a `AtomicBool` or channel flag `uart_ready` that is set only after UART driver initialization completes and the UM980 has responded to a test command
- In the MQTT event callback, queue received config messages to a channel; only dequeue and send to UART after `uart_ready` is set
- Send a known test command to UM980 on startup (e.g., a `VERSION` query) and wait for a response before signaling `uart_ready`
- Sequence in `main()`: initialize UART → test UM980 communication → initialize WiFi → connect MQTT → subscribe and begin processing

**Warning signs:**
- UM980 outputs default NMEA sentences (e.g., only `$GNGGA` at 1Hz) instead of operator-configured types/rates on first boot
- Behavior is correct when manually power-cycling the ESP32 after MQTT broker is already running, but incorrect on first boot from cold
- Issue resolves after adding a `thread::sleep(Duration::from_secs(5))` hack — confirms it is a race condition

**Phase to address:** Phase 4 (MQTT integration) — sequencing must be explicitly documented and tested

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Hardcode UART buffer size as 256 bytes | Compiles and works in testing | Silent data loss at full UM980 output rate in production | Never — set to 4096 from day one |
| Use `unwrap()` on all NVS reads | Reduces boilerplate in early dev | Boot panic loop if NVS is corrupt or empty | Only in Phase 1 smoke tests; replace with error handling in Phase 3 |
| Spin up all tasks in `main()` without sizing | Fast initial code | Stack overflows in integration testing | Only if stack canary is enabled; always measure before shipping |
| Single monolithic MQTT event handler | Easier to write initially | Impossible to test, grows to 500+ lines, ordering bugs | Never — split into per-concern handlers from day one |
| Recreate EspMqttClient on every reconnect | Avoids dealing with reconnect state | Memory leak over 24+ hours of operation | Never — use built-in reconnect |
| Store WiFi password in plain string in NVS | Simple | Password readable via flash dump | Acceptable for v1 per project constraints; note in README |
| Process NMEA inline in UART ISR | Reduces latency | Deadlocks, priority inversion, unpredictable timing | Never — always hand off to task via channel |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| UM980 UART init commands | Send all commands in rapid succession | Wait for `OK` response after each command before sending the next; UM980 has a 50ms processing window |
| UM980 UART | Assume 115200 baud is the default | UM980 ships at 115200 but can be changed by a prior config; always send a baud rate sync command first or use autobaud |
| MQTT retained config | Subscribe at QoS 0 | Use QoS 1 for the config topic so the retained message is acknowledged and broker knows the device received it |
| MQTT heartbeat | Publish at fixed wall-clock interval | Use a monotonic timer (`Instant::now()`); wall clock can skip if NTP sync happens mid-interval |
| BLE provisioning | Assume BLE GATT write is atomic | BLE writes > MTU size (typically 23 bytes) are fragmented; WiFi passwords > 22 bytes must be handled via multiple writes or larger MTU negotiation |
| NVS key length | Use descriptive long key names | NVS key names are limited to 15 characters; truncation causes key collisions silently |
| Device ID from MAC | Use full 6-byte MAC as string | ESP32-C6 exposes MAC via `esp_efuse_mac_get_default()`; format as 8-char hex for topic-safe string (no colons) |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Blocking UART read in MQTT task | NMEA sentences queue up; publish latency spikes during reconnect | Dedicate a separate high-priority task to UART reading | Immediately when WiFi drops and reconnect takes >1s |
| String allocation per MQTT publish | `alloc::string::String` allocation on every topic format | Pre-format topic strings at boot, store in `static` or `Arc<str>` | After ~10K publishes if heap is fragmented |
| Logging every NMEA sentence at `info!` level | UART logging steals CPU time, slows NMEA processing | Use `debug!` level for per-sentence logs; `info!` only for state changes | Immediately on UM980 at 10Hz output rate |
| Parsing all NMEA sentence types regardless of content | CPU cycles wasted on unwanted types | Filter by sentence type early (before full parse); discard unsupported types immediately | Not a CPU issue at 115200 baud, but wastes channel buffer space |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| MQTT credentials stored as plain NVS strings | Flash dump reveals credentials | Acceptable per v1 project scope (no TLS); document the risk; encrypt in v2 |
| BLE provisioning with no authentication | Any nearby BLE device can provision/re-provision the device | Add a 6-digit PIN displayed on device (LED blink pattern or serial output) that must be entered in the provisioning app; or use ESP-IDF's BLE Provisioning library which supports proof-of-possession |
| No MQTT topic ACL | Any client knowing the device ID can send commands to `gnss/{id}/config` | Acceptable for v1 internal use; note that broker-side ACL is the correct mitigation in production |
| Device ID (MAC) exposed in MQTT topic | MAC address is PII in some jurisdictions | Acceptable for internal deployments; hash or truncate MAC for public-facing deployments |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No visual feedback during BLE provisioning | User does not know if device received credentials | Use status LED: slow blink = BLE advertising, fast blink = BLE connected, solid = provisioned+connected, off = fault |
| BLE provisioning app shows "connected" but credentials not yet saved to NVS | User walks away thinking device is configured; it is not | Only return BLE GATT write success after NVS write confirms success |
| Reconnect retries with no backoff | Floods broker logs; ban-worthy on shared brokers | Use exponential backoff: 1s, 2s, 4s, 8s... capped at 60s |
| Silent GNSS fix status | Operator cannot tell if UM980 has a fix | Publish a `status` MQTT topic including GNSS fix type from `$GNGGA` field 6 |

---

## "Looks Done But Isn't" Checklist

- [ ] **UART initialization:** Often missing large enough RX buffer — verify `rx_buffer_size >= 4096` in UartDriver init
- [ ] **NMEA parser:** Often missing line accumulator — verify parser handles sentences split across multiple `read()` calls
- [ ] **BLE provisioning:** Often missing BLE shutdown before WiFi start — verify `BtDriver` is dropped before `EspWifi::connect()` is called
- [ ] **MQTT reconnect:** Often missing re-subscription on reconnect — verify all topic subscriptions are re-issued in `MQTT_EVENT_CONNECTED` handler
- [ ] **NVS error handling:** Often missing graceful fallback — verify `NvsDefault::get()` errors route to provisioning mode, not panic
- [ ] **Config message sequencing:** Often missing UART-ready guard — verify retained config message is not sent to UM980 before UART initialized
- [ ] **Task stack sizes:** Often set to default — verify `uxTaskGetStackHighWaterMark()` shows >25% headroom on each task
- [ ] **Device ID formatting:** Often includes MAC colons — verify device ID in MQTT topics uses only alphanumeric characters

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Wrong framework (esp-hal instead of esp-idf-hal) | HIGH | Rewrite all peripheral drivers; keep business logic (NMEA parser, MQTT topic formatter) as pure Rust crates that compile for both targets |
| Toolchain version mismatch | MEDIUM | Pin versions in Cargo.toml; run `cargo clean` + full rebuild; verify `esp-idf-sys` build log shows correct SDK version |
| UART buffer overflow data loss | LOW | Increase `rx_buffer_size` in UartDriver init; add overflow event monitoring; no data model changes required |
| NMEA fragmentation bugs | LOW-MEDIUM | Replace ad-hoc parser with line accumulator; existing NMEA parsing logic reusable |
| BLE+WiFi coexistence failures | MEDIUM | Refactor state machine to enforce strict BLE-then-WiFi sequencing; may require restructuring `main()` significantly |
| NVS corruption causing boot loop | LOW | Add NVS error handler that erases namespace and reboots to provisioning; ship recovery from day one |
| MQTT reconnect memory leak | MEDIUM | Replace manual reconnect loop with `EspMqttClient` built-in reconnect; audit all MQTT client creation sites |
| Task stack overflow | MEDIUM | Enable stack canary, run integration test, measure high watermarks, resize affected tasks |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Framework selection lock-in (esp-hal vs esp-idf-hal) | Phase 1: Project scaffold | Build compiles with `esp-idf-svc` WiFi example |
| Toolchain version mismatch | Phase 1: Project scaffold | Commit `sdkconfig`, pinned Cargo.toml, CI passes clean build |
| UART buffer overflow | Phase 2: UART + NMEA | Run at full UM980 output rate for 10 min; zero overflow events logged |
| NMEA sentence fragmentation | Phase 2: UART + NMEA | Parser correctly handles artificially fragmented reads in unit tests |
| BLE + WiFi coexistence | Phase 3: BLE provisioning | Provisioning succeeds 10/10 times on real mobile device; WiFi connects within 5s after BLE disconnect |
| NVS misconfiguration | Phase 1 (partition table) + Phase 3 (provisioning) | Erase flash, provision, reboot — credentials restored correctly |
| MQTT reconnect memory leak | Phase 4: MQTT integration | Run 50 broker disconnect/reconnect cycles; heap size stable (< 1KB variance) |
| FreeRTOS stack overflow | Phase 4: MQTT integration (all tasks running) | Stack high watermark > 25% on all tasks; stack canary never fires |
| Retained config before UART ready | Phase 4: MQTT integration | Power cycle with broker live; UM980 receives correct init commands every time |
| BLE GATT write fragmentation (password > 22 bytes) | Phase 3: BLE provisioning | Test with 64-char WiFi password and 64-char MQTT password |

---

## Sources

- Training knowledge of esp-rs ecosystem (esp-idf-hal, esp-idf-svc, embuild crates) — MEDIUM confidence; verify against https://github.com/esp-rs/esp-idf-hal and https://esp-rs.github.io/book/
- ESP-IDF UART driver documentation (FreeRTOS ring buffer, overflow behavior) — MEDIUM confidence; verify against https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-reference/peripherals/uart.html
- ESP-IDF coexistence documentation for BLE + WiFi — MEDIUM confidence; verify against https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-guides/coexist.html
- NVS partition behavior and key length limits — MEDIUM confidence; verify against https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-reference/storage/nvs_flash.html
- FreeRTOS task stack behavior on ESP32 — HIGH confidence (well-documented, stable across ESP32 variants)
- MQTT retained message delivery behavior (MQTT 3.1.1 spec, Section 3.3.1) — HIGH confidence
- UM980 UART characteristics — LOW confidence for specific timing values (50ms window); verify against Unicore UM980 Integration Manual

NOTE: Web access was unavailable during this research session. All findings are based on training knowledge. Priority verification targets before starting Phase 1:
1. Confirm esp-idf-hal crate version compatibility matrix with ESP-IDF v5.x for ESP32-C6
2. Confirm esp-idf-svc BLE GATT provisioning API is stable for ESP32-C6
3. Confirm UM980 default baud rate and response timing from hardware datasheet

---
*Pitfalls research for: Embedded Rust ESP32-C6 GNSS-to-MQTT bridge firmware*
*Researched: 2026-03-03*
