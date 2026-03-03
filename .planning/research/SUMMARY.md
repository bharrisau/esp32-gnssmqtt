# Project Research Summary

**Project:** esp32-gnssmqtt
**Domain:** Embedded Rust firmware — ESP32-C6 GNSS-to-MQTT bridge with BLE provisioning
**Researched:** 2026-03-03
**Confidence:** MEDIUM

## Executive Summary

This project is a purpose-built embedded firmware for the ESP32-C6 that reads NMEA sentences from a UM980 RTK GNSS module over UART and relays them to an MQTT broker over WiFi, with BLE-based zero-touch provisioning on first boot. The correct implementation approach is `esp-idf-hal` + `esp-idf-svc` on the IDF std path — not bare-metal Embassy or esp-hal. This decision is non-negotiable: WiFi, BLE, NVS, and MQTT are all only mature in the IDF-backed stack for the ESP32-C6. The firmware uses FreeRTOS tasks via `std::thread`, channel-based decoupling between the UART reader and MQTT publisher, and an NVS-gated boot branch that switches between provisioning mode and operational mode. The architecture is well-understood and the patterns are established.

The recommended phase order follows hard component dependencies: project scaffold and toolchain first (the most dangerous phase if skipped), then BLE provisioning (highest complexity table-stakes feature), then WiFi and MQTT connectivity, then the UART-to-MQTT NMEA relay pipeline, and finally hardening and reconnect logic. Deferring anything from this ordering — especially BLE provisioning — creates rework. The architecture is purposefully simple: one binary crate, one thread per component, bounded channels for backpressure, and a single long-lived MQTT client instance.

The primary risks are all front-loaded: wrong framework selection causes a full rewrite; toolchain version mismatches produce opaque linker failures; BLE GATT server API stability on ESP32-C6 is lower confidence than the rest of the stack. None of these risks are blockers — they are well-understood failure modes with documented mitigations. The two areas that require explicit verification before writing code are (1) the current `esp-idf-hal`/`esp-idf-svc`/`esp-idf-sys` coordinated versions and (2) the BLE GATT server API surface in `esp-idf-svc`, which was the most volatile part of the Rust IDF ecosystem as of mid-2025.

---

## Key Findings

### Recommended Stack

The stack is the Espressif-official Rust IDF trio: `esp-idf-hal` (peripheral drivers), `esp-idf-svc` (WiFi, BLE, MQTT, NVS services), and `esp-idf-sys` (bindgen layer to IDF C). These three crates must be version-coordinated — use the `esp-idf-template` scaffolding to get a known-good pinned set. The Rust toolchain requires the `esp` channel (RISC-V target `riscv32imc-esp-espidf`) installed via `espup`. ESP-IDF v5.2.x or v5.3.x is required; IDF v4.x does not support the ESP32-C6. Concurrency is `std::thread` mapped to FreeRTOS tasks — no async executor, no tokio, no Embassy.

See `.planning/research/STACK.md` for full rationale, alternatives considered, version compatibility table, and project setup commands.

**Core technologies:**
- `esp-idf-hal` (~0.44): Peripheral drivers (UART, GPIO, timers) — the only mature HAL for ESP32-C6 with IDF backend
- `esp-idf-svc` (~0.49): All services (WiFi, BLE GATT, NVS, MQTT client, HTTP server) — official Espressif crate, covers all project requirements in one dependency
- `esp-idf-sys` (~0.37): Bindgen FFI layer; used transitively; do not call directly
- `EspMqttClient` (from esp-idf-svc): Native IDF MQTT 3.1.1 client — preferred over `rumqttc` (requires tokio) or `mqttrs` (requires manual transport)
- Custom GATT server via `esp-idf-svc::bt`: BLE provisioning — preferred over Espressif Unified Provisioning because MQTT credentials need provisioning alongside WiFi credentials and the standard protocol is WiFi-only
- `EspNvs` (from esp-idf-svc): Flash credential storage — wear-leveled, atomic, correct for this use case
- `espup` + `espflash` + `cargo-generate`: Development toolchain for project scaffold, flash, and serial monitor
- Blocking UART on dedicated thread: Simpler and adequate at 115200 baud; avoid async UART (adds Embassy dependency, less tested on C6)

### Expected Features

The MVP (v1) must deliver: reliable NMEA relay with per-sentence-type MQTT topic routing, zero-touch BLE provisioning on first boot, persistent NVS credential storage, WiFi and MQTT auto-reconnect, status LED, heartbeat publish, MQTT LWT for offline detection, and remote UM980 configuration via retained MQTT topic. These 17 features are all P1 — the device is operationally useless without any of them.

See `.planning/research/FEATURES.md` for the full feature prioritization matrix, feature dependency graph, and anti-feature analysis.

**Must have (table stakes):**
- UART RX from UM980 at 115200 baud 8N1 — without this, there is no product
- NMEA sentence framing, line extraction, type parsing, and checksum validation — prerequisites for all downstream processing
- Device ID from ESP32 hardware MAC/eFuse — required for per-device MQTT topic namespacing
- NVS credential storage (WiFi SSID/pass, MQTT host/port/user/pass) — required for persistence across reboots
- BLE provisioning on first boot — highest-complexity table-stakes feature; enables zero-touch field deployment
- WiFi station mode connect on boot — core connectivity
- MQTT client connect with username/password auth — core connectivity
- MQTT publish NMEA to `gnss/{device_id}/nmea/{TYPE}` topics — primary product function
- MQTT subscribe to `gnss/{device_id}/config` (QoS 1) and UART TX passthrough — required for remote UM980 initialization without reflash
- Auto-reconnect for WiFi and MQTT with exponential backoff — required; network drops are inevitable
- Status LED (provisioning / connecting / connected / error) — required for field diagnosis
- Heartbeat publish to `gnss/{device_id}/heartbeat` — required for consumers to detect device presence
- MQTT LWT (`gnss/{device_id}/status` retained, payload `offline`) — zero-cost capability; include at connect time
- Retain flag on heartbeat/status publishes — zero-cost flag; include from day one

**Should have (add post-validation, v1.x):**
- Web portal fallback provisioning (SoftAP + HTTP form) — for environments lacking BLE support; already in requirements but lower priority than BLE path
- NVS wipe and re-provisioning trigger (GPIO button or MQTT reset command) — for field re-provisioning without reflash
- Structured JSON heartbeat payload (uptime, fix status, satellite count) — richer monitoring data
- Sentence-type allow/deny filter via retained config topic — reduces broker load for high-rate UM980 output

**Defer (v2+):**
- TLS/mTLS for MQTT — requires certificate provisioning design, mbedTLS tuning, NVS space planning; separate milestone
- OTA firmware update — requires dual-partition layout, rollback, image signing; wrong implementation bricks devices
- Panic/error reporting to MQTT — requires careful panic handler design to avoid boot loops; defer until v1 is stable

**Anti-features to explicitly avoid in v1:**
- Full NMEA field parsing (lat/lon decode) — firmware job is relay, not parse; consumers parse in any language
- Local NMEA buffering across power cycles — stale positions are misleading; flash wear from 10Hz writes is unacceptable
- Multi-broker publishing — multiplies state management complexity on a constrained MCU
- GNSS config stored in device NVS (not from broker) — defeats the remote reconfiguration design goal

### Architecture Approach

The firmware is a single binary crate structured as one file per component, each mapped to its own FreeRTOS task via `std::thread::spawn`. Components communicate via bounded `mpsc::sync_channel` queues (64-sentence bound for NMEA, 8-message bound for config payloads to UART). The boot sequence is NVS-gated: on first boot (no credentials), enter BLE provisioning mode; after successful provisioning, write credentials, set a `provisioned` flag, and reboot into normal operational mode. This avoids maintaining two parallel runtime code paths. BLE is explicitly shut down before WiFi starts to avoid 2.4GHz coexistence conflicts.

See `.planning/research/ARCHITECTURE.md` for full component diagram, data flow diagrams, concurrency model comparison, build order, and anti-patterns.

**Major components:**
1. `uart_reader.rs` — DMA-buffered UART read loop; accumulates bytes into `\n`-terminated NMEA sentences; pushes complete sentences to bounded channel; runs at elevated FreeRTOS priority to prevent FIFO overflow
2. `nmea_router.rs` — Extracts sentence type from `$TYPE,` prefix; constructs `gnss/{device_id}/nmea/{TYPE}` topic string; passes (topic, payload) tuple to MQTT task; validates NMEA checksum and drops corrupt sentences
3. `mqtt_client.rs` — Manages single long-lived `EspMqttClient` instance; subscribes to config topic inside `Connected` event handler (not at init, to survive reconnects); publishes from NMEA channel; forwards received config payloads to UART TX channel via a separate bounded queue
4. `ble_provision.rs` — Custom GATT server accepting WiFi and MQTT credentials as characteristic writes; runs only on first boot; shuts down cleanly before WiFi starts
5. `nvs_store.rs` — `EspNvs` wrapper with custom namespace; key-value abstraction for all credentials and the `provisioned` flag; all NVS reads wrapped in explicit error handling that routes to provisioning mode on failure
6. `wifi.rs` — `EspWifi` station mode connection with reconnect loop and exponential backoff; signals MQTT task via `Arc<AtomicBool>` when WiFi is up
7. `led.rs` — State machine driven by `mpsc` channel messages or `Arc<AtomicU8>` from any task; polls at ~100ms interval; drives GPIO LED through defined states (provisioning, connecting, connected, error)
8. `heartbeat.rs` — Periodic MQTT publish to `gnss/{device_id}/heartbeat` with retained flag; uses monotonic timer to avoid NTP-induced skips

### Critical Pitfalls

1. **Framework selection lock-in (esp-hal vs esp-idf-hal)** — Choosing esp-hal (bare-metal) for an ESP32-C6 project requiring WiFi, BLE, and NVS simultaneously is a rewrite trap. Use `esp-idf-hal` + `esp-idf-svc` from day one. Verify at project scaffold by successfully building the WiFi example.

2. **Toolchain version mismatch (esp-idf-hal / esp-idf-sys / ESP-IDF C SDK)** — These three components must be version-coordinated. A `cargo update` can silently pull incompatible versions, producing cryptic linker failures or ABI corruption. Pin all three with `=` version specifiers in `Cargo.toml`; start from `esp-idf-template` which provides a known-good coordinated set.

3. **UART receive buffer overflow with high-frequency NMEA output** — UM980 at full output rate overwhelms the default 256-byte software ring buffer if the UART reader task is preempted by WiFi reconnect. Set `rx_buffer_size: 4096` at UART init and dedicate a high-priority FreeRTOS task exclusively to UART reading.

4. **MQTT reconnect creating memory leak or heap exhaustion** — Creating a new `EspMqttClient` on each disconnect event leaks memory. Use the client's built-in `reconnect_timeout_ms` configuration and a single long-lived client instance. Re-subscribe to all topics in the `Connected` event handler, not at initialization.

5. **BLE and WiFi 2.4GHz coexistence conflicts** — Starting WiFi connect while BLE provisioning is active causes intermittent provisioning failures (80-90% success rate that degrades under RF congestion). The BLE driver (`BtDriver`) must be fully dropped before `EspWifi::connect()` is called. Enforce this with a strict state machine; test on real mobile hardware, not a bench emulator.

6. **MQTT config message processed before UART is ready** — The broker delivers the retained config topic message within milliseconds of MQTT connect. If UART initialization has not completed, config commands are silently lost and the UM980 runs in default state. Use an `AtomicBool` `uart_ready` flag; queue config messages and only apply them after the flag is set.

---

## Implications for Roadmap

The feature dependency graph and architecture build order both point to the same seven-phase structure. Each phase has hard prerequisites from the previous one. Do not reorder.

### Phase 1: Project Scaffold and Toolchain

**Rationale:** All other phases depend on a correct, version-pinned build environment. The most common catastrophic failure mode (wrong framework, toolchain mismatch) occurs here. Get it right once; never revisit.

**Delivers:** Compiling project from `esp-idf-template`; correct Cargo.toml with pinned Espressif crate versions; `sdkconfig.defaults` with BLE stack enabled, correct UART buffer sizes, FreeRTOS stack overflow detection; `partitions.csv` with 64KB+ NVS partition; `device_id.rs` module reading hardware MAC; verified `cargo build` and `espflash` flash cycle.

**Addresses features:** Device ID from MAC/eFuse; foundation for all other features.

**Avoids pitfalls:** Framework selection lock-in (commit to esp-idf-hal); toolchain version mismatch (pin all versions from template); NVS misconfiguration (define partition table now, not later).

**Research flag:** Verify current `esp-idf-hal`/`esp-idf-svc`/`esp-idf-sys` coordinated versions on crates.io and from the latest `esp-idf-template` before pinning. Training data versions may have incremented.

---

### Phase 2: NVS Credential Store and Boot Branch

**Rationale:** NVS is required by every subsequent component (BLE provisioning writes to it; WiFi and MQTT read from it; boot logic gates on it). Building and validating it standalone avoids debugging NVS issues while simultaneously debugging BLE or WiFi.

**Delivers:** `nvs_store.rs` module with typed read/write for all credential keys; `main.rs` boot branch that reads `provisioned` flag and routes accordingly; validated erase-flash → provisioning mode behavior; all NVS errors route to provisioning mode rather than panic.

**Addresses features:** NVS persistent credential storage; graceful factory reset behavior.

**Avoids pitfalls:** NVS partition misconfiguration; boot loop on NVS corruption; accidental panic on empty NVS.

---

### Phase 3: BLE Provisioning

**Rationale:** BLE provisioning is the highest-complexity P1 feature and the one with the lowest-confidence API surface. Building it early, while the codebase is small, means any API surprises are cheap to handle. It also validates the critical BLE-then-WiFi sequencing before WiFi code exists.

**Delivers:** `ble_provision.rs` custom GATT server accepting WiFi SSID/password and MQTT host/port/user/password as characteristic writes; credentials written to NVS on successful provisioning; `provisioned` flag set; BLE driver fully shut down before returning; `main.rs` reboot after provisioning complete; status LED shows provisioning state.

**Addresses features:** BLE provisioning on first boot; status LED (provisioning blink pattern).

**Avoids pitfalls:** BLE + WiFi coexistence (BtDriver dropped before WiFi starts); BLE GATT write fragmentation for long passwords (>22 bytes requires MTU negotiation).

**Research flag:** Verify current `esp-idf-svc::bt` GATT server API and look for working examples in the esp-idf-svc repository. This was the most volatile API as of mid-2025. If the Rust GATT API is insufficient, fall back to `esp32-nimble` crate or `unsafe` FFI to the `wifi_provisioning` IDF component.

---

### Phase 4: WiFi and MQTT Connectivity Skeleton

**Rationale:** With credentials in NVS and the boot branch working, WiFi and MQTT can be built as a validated connectivity layer before adding NMEA data. Testing MQTT publish with a hardcoded heartbeat message confirms broker reachability and credential correctness before introducing UART complexity.

**Delivers:** `wifi.rs` with station mode connect, event loop integration, and exponential backoff reconnect; `mqtt_client.rs` with single long-lived `EspMqttClient`, LWT registration at connect time, config topic subscription inside `Connected` handler, heartbeat publish; `heartbeat.rs` with periodic timer; MQTT LWT and retain flags set correctly from the start.

**Addresses features:** WiFi connection; MQTT client connect; MQTT LWT for offline status; retain flag on heartbeat/status; heartbeat publish; auto-reconnect for WiFi and MQTT.

**Avoids pitfalls:** MQTT reconnect memory leak (single client, built-in reconnect); MQTT config topic subscription only at startup (re-subscribe in Connected handler).

---

### Phase 5: UART-to-MQTT NMEA Pipeline

**Rationale:** This is the core product function. With connectivity validated, the UART reader and NMEA router can be added and the end-to-end relay pipeline confirmed on live hardware with a real UM980. The bounded channel between UART and MQTT provides natural backpressure.

**Delivers:** `uart_reader.rs` with high-priority FreeRTOS task, 4096-byte RX ring buffer, line accumulator handling fragmented reads, NMEA checksum validation, and bounded channel send; `nmea_router.rs` with sentence type extraction and topic string construction; `mqtt_client.rs` NMEA publish loop dequeuing from channel; full end-to-end relay validated with live UM980 connected.

**Addresses features:** UART RX from UM980; NMEA sentence framing and line extraction; NMEA sentence type parsing; NMEA checksum validation; NMEA publish to per-type topics; per-sentence topic routing.

**Avoids pitfalls:** UART receive buffer overflow (4096-byte buffer, dedicated high-priority task); NMEA sentence fragmentation (line accumulator from the start, not added later); unbounded channel memory exhaustion (bounded sync_channel(64)).

---

### Phase 6: MQTT Config to UM980 Init

**Rationale:** With the NMEA relay pipeline working, the config pathway completes the bidirectional control loop. This phase introduces sequencing requirements (UART must be ready before config is applied) and the UART TX write path.

**Delivers:** Config channel from MQTT event callback to UART TX; `uart_ready` `AtomicBool` flag set after UM980 responds to test command; config messages queued and held until `uart_ready` is set; UART TX write path with per-command delay for UM980 processing window; QoS 1 subscription to config topic confirmed.

**Addresses features:** MQTT subscribe to config topic (QoS 1); UART TX to UM980 (send received config payload); remote UM980 initialization without reflash.

**Avoids pitfalls:** Retained MQTT config message processed before UART ready; writing UART from MQTT event callback directly (use channel; never write UART inside the callback).

---

### Phase 7: LED State Machine, Reconnect Hardening, and Integration

**Rationale:** Final integration phase. All components are running together for the first time, exposing stack size and memory pressure issues that only appear under combined load. Reconnect logic is validated under deliberate fault injection. LED state machine completes the operator-visible status feedback.

**Delivers:** `led.rs` full state machine (provisioning, connecting, connected, error states); WiFi and MQTT reconnect validated through 50+ disconnect/reconnect cycles with stable heap; FreeRTOS stack high-water marks measured on all tasks with canary enabled; `uxTaskGetStackHighWaterMark()` confirms >25% headroom on all tasks; status LED wired to all state transitions; field test with live UM980 and real MQTT broker.

**Addresses features:** Status LED full state machine; auto-reconnect hardening; all "looks done but isn't" checklist items.

**Avoids pitfalls:** FreeRTOS task stack overflow (canary enabled in Phase 1, measured and tuned here); string allocation per MQTT publish (pre-format topic strings at boot); logging every NMEA sentence at info level (use debug level).

---

### Phase Ordering Rationale

- **Foundation before provisioning:** NVS must exist before BLE can write to it; the partition table must be defined before NVS is first used; toolchain must be pinned before any code is written that relies on specific API surfaces.
- **Provisioning before connectivity:** BLE provisioning delivers the credentials that WiFi and MQTT depend on. Testing provisioning standalone, while the codebase is small, is dramatically cheaper than debugging it after WiFi and MQTT are also in play.
- **Connectivity before pipeline:** Validating MQTT publish with a heartbeat message before adding UART confirms that broker address, credentials, and network path are correct. Introducing UART complexity simultaneously would obscure connectivity issues.
- **Pipeline before config passthrough:** The UART TX config path depends on the UART driver being initialized and the MQTT subscription being active; both are established in phases 4 and 5.
- **Integration last:** Stack sizing, heap profiling, and reconnect stress testing require all components to be running simultaneously; they cannot be done meaningfully in partial builds.

### Research Flags

Phases needing deeper research or API verification during planning:

- **Phase 1 (Scaffold):** Verify the current coordinated versions of `esp-idf-hal`, `esp-idf-svc`, and `esp-idf-sys` from the latest `esp-idf-template` on crates.io and the esp-rs GitHub org. Training data versions (0.44 / 0.49 / 0.37) are from August 2025 and will have incremented.
- **Phase 3 (BLE Provisioning):** Verify the `esp-idf-svc::bt` GATT server API with working examples from the esp-idf-svc repository. BLE GATT server was the most volatile API surface in the Rust IDF ecosystem. If the Rust API is insufficient, evaluate `esp32-nimble` as an alternative before starting implementation.
- **Phase 5 (UART Pipeline):** Verify the UM980 default baud rate and response timing (50ms command processing window is unverified) against the Unicore UM980 Integration Manual before writing the UART init sequence and config passthrough logic.

Phases with standard, well-documented patterns (research-phase not needed):

- **Phase 2 (NVS):** `EspNvs` API and NVS behavior are well-documented and stable. Standard pattern.
- **Phase 4 (WiFi + MQTT):** `EspWifi` and `EspMqttClient` are the primary examples in the esp-idf-svc repository. Callback model and reconnect pattern are documented.
- **Phase 6 (Config passthrough):** Pure channel plumbing; no novel APIs. The MQTT → channel → UART TX pattern is straightforward given the architecture established in earlier phases.
- **Phase 7 (Integration):** Profiling and tuning; no new APIs. FreeRTOS stack measurement APIs are stable and well-documented.

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Framework choice (esp-idf-hal vs esp-hal) is HIGH confidence and well-documented. Specific crate versions are training data from Aug 2025 — verify before pinning. BLE GATT API stability is LOW confidence within the otherwise MEDIUM stack picture. |
| Features | MEDIUM | Feature list is grounded in project requirements (PROJECT.md) and well-established NMEA/MQTT/IoT patterns. NMEA 0183 spec and MQTT 3.1.1 spec are HIGH confidence. UM980-specific timing and command details are LOW confidence — verify against Unicore datasheet. |
| Architecture | MEDIUM | FreeRTOS thread model, bounded channel pattern, NVS-gated boot branch, and EspMqttClient event model are all established patterns with examples in the esp-idf-svc repository. No web access during research; verify EspMqttClient callback API against current crate version before implementing. |
| Pitfalls | MEDIUM-HIGH | Core embedded pitfalls (stack overflow, UART overflow, MQTT reconnect leak, BLE+WiFi coexistence) are well-documented in ESP-IDF literature and FreeRTOS documentation. UM980-specific timing values are unverified. |

**Overall confidence:** MEDIUM

### Gaps to Address

- **BLE GATT server API surface:** This was the most volatile part of the esp-rs ecosystem as of mid-2025. Before writing Phase 3 code, check the `esp-idf-svc` repository for current GATT server examples and confirm whether `esp32-nimble` is a better alternative. If neither has a working example, consider falling back to `unsafe` FFI to the `wifi_provisioning` IDF component (WiFi-only) and extending it with a separate GATT characteristic for MQTT credentials.
- **Crate versions:** All version numbers in STACK.md are training data. Run `cargo generate esp-rs/esp-idf-template` at project start and accept the versions it provides; do not manually transcribe versions from this document.
- **UM980 UART specifics:** Default baud rate confirmed as 115200 in the project brief, but command acknowledgment timing (the 50ms window cited in PITFALLS.md) requires verification against the Unicore UM980 Integration Manual before implementing the config passthrough UART TX sequence.
- **ESP32-C6 memory budget:** The heap budget estimate (WiFi ~80KB, BLE ~60KB during provisioning, MQTT buffers, Rust stacks) needs validation against measured values on the actual hardware during Phase 7. The 320KB figure is a training data estimate; actual available heap depends on sdkconfig options.

---

## Sources

### Primary (HIGH confidence)
- MQTT 3.1.1 specification (OASIS, Section 3.1.3.4 LWT, 3.3.1.3 retained) — LWT behavior, retained messages, QoS levels
- NMEA 0183 / IEC 61162-1 — sentence format, max length (82 chars), checksum algorithm (XOR between `$` and `*`)
- FreeRTOS task stack behavior on ESP32 — well-documented, stable across ESP32 variants

### Secondary (MEDIUM confidence)
- The Rust on ESP Book (https://esp-rs.github.io/book/) — std vs no_std choice, project setup, thread model; training data Aug 2025
- esp-idf-hal crate (https://github.com/esp-rs/esp-idf-hal) — UART, GPIO, timer drivers; training data Aug 2025
- esp-idf-svc crate (https://github.com/esp-rs/esp-idf-svc) — WiFi, MQTT, NVS, BT bindings; training data Aug 2025
- ESP-IDF Programming Guide for ESP32-C6 (https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/) — UART overflow behavior, NVS key limits, coexistence; training data Aug 2025
- esp-idf-template (https://github.com/esp-rs/esp-idf-template) — canonical project scaffold; training data Aug 2025
- ESP-IDF coexistence guide (https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-guides/coexist.html) — BLE+WiFi coexistence; training data Aug 2025

### Tertiary (LOW confidence — verify before use)
- UM980 UART characteristics — command acknowledgment timing (50ms window) and baud rate confirmation; based on general RTK receiver patterns; verify against Unicore UM980 Integration Manual
- esp-idf-svc BLE GATT server API — functional but had rough edges as of mid-2025; verify current API surface and examples before implementing Phase 3

---

*Research completed: 2026-03-03*
*Ready for roadmap: yes*
