# Stack Research

**Domain:** Embedded Rust firmware — ESP32-C6, GNSS/MQTT bridge
**Researched:** 2026-03-03
**Confidence:** MEDIUM (training data through Aug 2025; external verification tools unavailable during this session — all versions marked with verification notes)

---

## The Central Decision: esp-idf-hal vs esp-hal (bare metal)

This is the most important architectural choice. Get it wrong and you rewrite.

### Recommendation: Use esp-idf-hal (std, IDF-backed)

**Rationale:**

This project requires WiFi, BLE, and NVS — all of which depend on Espressif's IDF (IoT Development Framework) blobs. The ESP32-C6's WiFi and BLE stacks are closed-source firmware from Espressif. Both `esp-idf-hal` and `esp-hal` ultimately require these blobs, but the integration story differs dramatically:

- **`esp-idf-hal` (std):** Runs on top of ESP-IDF via the `esp-idf-sys` bindgen layer. You get std Rust (heap, threads, network stack via `esp-idf-svc`). WiFi, BLE, NVS, TCP/IP, MQTT are all first-party Espressif crates. This is the production-tested path.
- **`esp-hal` (no_std, bare metal):** Hardware abstraction with no OS. WiFi/BLE requires `esp-wifi` crate which is still maturing. NVS requires `esp-storage` + your own abstraction. More control, less ecosystem. As of mid-2025, `esp-wifi` BLE on C6 was functional but less battle-tested than IDF path.

**This project needs all three: WiFi + BLE + NVS simultaneously.** The `esp-idf-hal` path provides all three as stable, maintained crates from Espressif itself. The bare-metal path would require integrating three less-mature crates with no guarantee they coexist cleanly on C6.

**Choose `esp-hal` only if:** ultra-low latency requirements, no WiFi/BLE needed, or you need no_std for memory constraints so tight that IDF overhead is unacceptable. None of those apply here.

---

## Recommended Stack

### Core Framework

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `esp-idf-hal` | ~0.44 | HAL for ESP32 peripherals (UART, GPIO, SPI, etc.) over IDF | Official Espressif crate; exposes UART, GPIO via embedded-hal traits; stable API on C6; std Rust allowed |
| `esp-idf-svc` | ~0.49 | WiFi, BLE, NVS, MQTT, HTTP services over IDF | Official Espressif crate; wraps IDF service layer; provides `EspWifi`, `EspBle`, `EspNvs`, `EspMqttClient`; the only mature path for all three services simultaneously on C6 |
| `esp-idf-sys` | ~0.37 | Low-level bindgen bindings to ESP-IDF C API | Required transitive dependency; `esp-idf-hal` and `esp-idf-svc` both depend on it; do not call directly |
| Rust toolchain | nightly (with `esp` channel) | Rust compiler targeting RISC-V ESP32-C6 | ESP32-C6 is RISC-V (rv32imac); requires `riscv32imc-esp-espidf` target; use `espup` to install Espressif Rust toolchain |
| ESP-IDF | v5.2.x or v5.3.x | Underlying C framework providing WiFi/BLE blobs and OS primitives | IDF v5.x required for C6 support; IDF v5.2+ is the stable branch as of mid-2025; `esp-idf-sys` build script downloads and links it |

**Confidence:** MEDIUM — versions are from training data (Aug 2025); verify latest on crates.io before pinning.

### UART (GNSS Reading)

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp_idf_hal::uart` | (part of esp-idf-hal) | Blocking or async UART read/write | Built-in to esp-idf-hal; `UartDriver` provides read/write; no extra dependency |
| `nmea` | ~0.7 or ~0.6 | NMEA 0183 sentence parsing | Pure Rust, no_std-compatible parser; handles GGA, GLL, RMC, GSA, GSV, VTG; used for parsing only (we relay raw sentences, so parser is optional) |

**Recommendation on UART mode:** Use **blocking UART** for initial implementation. The UM980 sends sentences at 115200 baud; a blocking read loop on a dedicated FreeRTOS thread (via `std::thread`) is simple, predictable, and avoids the complexity of async executor integration with IDF. Async UART (`esp-idf-hal` async support) exists but adds `embassy` dependency and is less commonly tested on C6 as of mid-2025.

**Do not** use async unless the blocking read loop becomes a bottleneck — at 115200 baud with NMEA sentences, a blocking thread is perfectly adequate.

### WiFi

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp-idf-svc::wifi` | (part of esp-idf-svc) | Station mode WiFi connection | `EspWifi` wraps IDF WiFi driver; handles scan, connect, DHCP; most mature WiFi path for IDF Rust |

Configuration pattern: `EspWifi::new()` → configure with `ClientConfiguration` → `wifi.connect()` → wait for IP via event loop. The IDF event loop (`EspSystemEventLoop`) is required.

### MQTT

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp-idf-svc::mqtt::client` | (part of esp-idf-svc) | MQTT 3.1.1 client over TCP | `EspMqttClient` wraps IDF MQTT client; native C implementation in IDF; no extra Rust MQTT crate needed; supports QoS 0/1/2, retained messages, username/password auth; battle-tested on IDF |

**Recommendation:** Use `EspMqttClient` from `esp-idf-svc` rather than a pure-Rust MQTT crate (like `rumqttc`). Reason: `rumqttc` requires a full async runtime (`tokio`) or careful integration; `EspMqttClient` is built on the IDF's own MQTT implementation which is already running inside the firmware, no extra code size overhead. The IDF MQTT client handles reconnect logic via `MqttClientConfiguration::reconnect_timeout` natively.

**Confidence for EspMqttClient:** MEDIUM — documented in esp-idf-svc 0.48+; verify API hasn't changed.

### BLE Provisioning

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp-idf-svc::bt` | (part of esp-idf-svc) | Bluetooth/BLE via IDF BT stack | `EspBluetooth` + GATT server APIs; or use `esp_idf_svc::ble::gap` for advertising |
| Espressif `wifi_provisioning` component | via IDF component | Wi-Fi provisioning over BLE using standard Espressif provisioning protocol | IDF has a built-in `wifi_provisioning` component that implements the Espressif Unified Provisioning Protocol; works with the official ESP SoftAP/BLE Provisioning mobile app (Android/iOS); can be called via `esp-idf-sys` bindings |

**Important nuance on BLE provisioning approach:**

Two paths exist:

1. **Espressif Unified Provisioning (`wifi_provisioning` IDF component):** Uses the standard Espressif provisioning protocol over BLE GATT. Pairs with the official "ESP BLE Prov" mobile apps. Protobuf-based. Requires calling IDF C API through `esp-idf-sys`. As of mid-2025, no dedicated Rust wrapper crate exists — you write `unsafe` FFI to `wifi_provisioning_mgr_init()` and friends, or wrap it yourself.

2. **Custom GATT server via `esp-idf-svc::bt`:** Write a custom BLE GATT service that accepts WiFi/MQTT credentials as characteristic writes. Pure Rust with `esp-idf-svc`'s BLE APIs. More code, but full control over the protocol. Can work with any BLE client (nRF Connect, custom app, etc.).

**Recommendation:** Use path 2 (custom GATT server) for this project. Reason: MQTT credentials (host, port, username, password) need provisioning alongside WiFi credentials. The Espressif Unified Provisioning protocol is WiFi-only — you'd need to extend it for MQTT config, which requires the same custom GATT work anyway. Build one simple GATT service that accepts a JSON blob with all credentials.

**Confidence:** LOW — BLE GATT server in `esp-idf-svc` Rust API is documented but had rough edges as of mid-2025; verify current API surface and look for examples in the `esp-idf-svc` repo.

### NVS (Flash Config Storage)

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp-idf-svc::nvs` | (part of esp-idf-svc) | Non-volatile storage for key-value config | `EspNvs` wraps IDF NVS partition; stores strings (WiFi SSID/password, MQTT host/port/credentials, device ID); survives power cycles; wear-leveled by IDF; direct Rust API |

Usage pattern: Open a namespace (`EspNvs::new(nvs_partition, "config", true)`), use `get_str`/`set_str` for each credential. Max value size for NVS strings is 4000 bytes by default (configurable). Suitable for all config values in this project.

**Do not use** `embassy-nvm` or external flash crates — NVS via IDF is the correct solution for ESP32 config storage with wear leveling and atomic updates.

### Device ID

| Approach | Why |
|----------|-----|
| `esp_idf_sys::esp_efuse_mac_get_default()` | Returns the 6-byte factory-burned MAC address; unique per chip; use as device ID base; format as hex string for MQTT topic construction |

### Web Portal Fallback (Provisioning)

| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `esp-idf-svc::http::server` | (part of esp-idf-svc) | HTTP server for web provisioning portal | `EspHttpServer` wraps IDF HTTP server; serves a simple HTML form for WiFi/MQTT credential entry; runs alongside WiFi AP mode |

For web portal: set ESP32 to AP mode (soft AP), run HTTP server on 192.168.4.1, serve form, accept POST with credentials, save to NVS, reboot into station mode. Standard IDF pattern.

---

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `espup` | Installs ESP Rust toolchain + RISC-V target | Run `espup install` to get the `esp` Rust channel, RISC-V target, and `ldproxy`; replaces the old manual `espflash` + custom toolchain setup |
| `espflash` | Flashes firmware to ESP32 over USB/UART | `cargo espflash flash --monitor`; also handles serial monitor; required in `Cargo.toml` as `[alias]` |
| `cargo-generate` | Project scaffolding | Use `cargo generate esp-rs/esp-idf-template` to get a working std/IDF project skeleton |
| `ldproxy` | Linker proxy for IDF builds | Required by `esp-idf-sys` build system; installed by `espup` |
| `probe-rs` | Debugging via JTAG/USB-JTAG | ESP32-C6 has built-in USB-JTAG; `probe-rs run` + `defmt` for RTT logging during development |

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| `esp-idf-hal` + `esp-idf-svc` (std) | `esp-hal` + `esp-wifi` (no_std) | When: no WiFi/BLE needed, need deterministic timing, ultra-low power, or no_std required. Not this project. |
| `EspMqttClient` (from esp-idf-svc) | `rumqttc` (pure Rust async MQTT) | When: targeting a std Linux/embedded-linux target with tokio. On ESP32/IDF, native MQTT is better. |
| `EspMqttClient` (from esp-idf-svc) | `mqttrs` (sync MQTT encoder/decoder) | When: building your own transport layer. Don't do this; EspMqttClient handles the full stack. |
| Custom GATT provisioning | Espressif Unified Provisioning (`wifi_provisioning` IDF component) | When: WiFi-only provisioning and you want compatibility with the official ESP BLE Prov app. |
| Blocking UART on dedicated thread | Async UART with Embassy | When: many concurrent peripherals and you need cooperative scheduling across them all. Overkill for one UART + MQTT publish. |
| `EspNvs` | `sequential-storage` + raw flash | When: targeting no_std bare metal without IDF. `EspNvs` is simpler and correct for this project. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `tokio` | No tokio support on ESP32-C6 IDF; IDF uses FreeRTOS threads, not async Rust runtime | `std::thread` + IDF blocking APIs, or Embassy if async truly needed |
| `embassy` + `esp-hal` (bare metal async) | BLE provisioning + NVS + WiFi simultaneously on bare metal is immature on C6 as of 2025; Embassy ESP32 support exists but is missing stable BLE GATT server | `esp-idf-svc` on IDF |
| `rumqttc` | Requires tokio or async-std; doesn't integrate cleanly with IDF event loop; adds 50-100KB to binary vs native IDF MQTT | `EspMqttClient` from `esp-idf-svc` |
| IDF v4.x | Does not support ESP32-C6 (C6 requires IDF v5.0+) | IDF v5.2.x or v5.3.x |
| `esp32-hal` (older unmaintained) | Superseded by `esp-hal` (the new unified bare metal HAL from esp-rs); name collision risk | `esp-idf-hal` (for IDF path) or `esp-hal` (for bare metal) |
| Direct `esp-idf-sys` calls for UART/WiFi | Unsafe FFI when safe wrappers exist in `esp-idf-hal`/`esp-idf-svc`; brittle | Use the HAL/svc safe APIs |

---

## Stack Patterns by Variant

**If std + IDF (this project):**
- Use `esp-idf-hal` + `esp-idf-svc`
- Use `std::thread` for concurrency (one thread per: UART reader, MQTT publisher, heartbeat, BLE provisioning)
- Use IDF event loop (`EspSystemEventLoop`) for WiFi/IP events
- Use `EspMqttClient` with callback-based message handler

**If bare metal was chosen (not this project):**
- Use `esp-hal` for peripherals
- Use `esp-wifi` for WiFi (note: BLE GATT server maturity unclear on C6)
- Use `embassy` for async runtime
- Use `sequential-storage` for flash KV store
- Use `mqttrs` + manual TCP for MQTT

**If NMEA parsing needed (optional):**
- Use `nmea` crate (pure Rust, no_std compatible)
- This project relays raw NMEA strings, so parsing is optional (only needed if you want to filter or transform sentences before publishing)

---

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| `esp-idf-hal` ~0.44 | `esp-idf-sys` ~0.37, `esp-idf-svc` ~0.49 | These three crates must be version-coordinated; check the `Cargo.toml` in the `esp-idf-template` generated project for the current pinned set |
| ESP-IDF v5.2.x | `esp-idf-sys` ~0.37 | `esp-idf-sys` build downloads IDF; pin via `ESP_IDF_VERSION` env var or `[package.metadata.esp-idf-sys]` in Cargo.toml |
| Rust nightly (`esp` channel) | `riscv32imc-esp-espidf` target | Standard Rust stable does NOT support ESP32 targets; must use `espup`-installed toolchain |
| `esp-idf-svc::mqtt` | MQTT 3.1.1 | IDF MQTT client is MQTT 3.1.1; MQTT 5.0 support in IDF is partial/experimental — do not rely on MQTT 5.0 features |

**Confidence note:** All version numbers above are from training data (Aug 2025). Versions WILL have incremented. The relationship between the three Espressif crates is stable, but specific version numbers must be verified at project creation time via the official `esp-idf-template` or the `esp-rs` GitHub org.

---

## Project Setup Commands

```bash
# 1. Install Espressif Rust toolchain
cargo install espup
espup install  # installs esp Rust channel, RISC-V target, ldproxy

# 2. Activate toolchain (add to shell profile)
. $HOME/export-esp.sh   # Linux/macOS
# Windows: run %USERPROFILE%\export-esp.ps1

# 3. Install flash tool
cargo install espflash

# 4. Generate project from template
cargo install cargo-generate
cargo generate esp-rs/esp-idf-template

# Answer prompts:
#   Target: ESP32-C6
#   std (not no_std)  ← CRITICAL CHOICE
#   IDF version: v5.2 (or v5.3 if available and tested)

# 5. Verify build
cd <project-name>
cargo build
```

## Cargo.toml Key Entries

```toml
[package]
name = "esp32-gnssmqtt"
edition = "2021"

[dependencies]
# Core ESP32 IDF crates (verify latest versions on crates.io)
esp-idf-sys = { version = "0.37", features = ["binstart"] }
esp-idf-hal = "0.44"
esp-idf-svc = { version = "0.49", features = ["std", "alloc"] }

# Embedded-hal traits (for HAL compatibility)
embedded-hal = "1.0"

# NMEA parsing (optional, only if sentence filtering needed)
# nmea = "0.7"

# Logging
log = "0.4"
esp-idf-svc = { version = "0.49", features = ["log"] }  # enables IDF log backend

[build-dependencies]
embuild = "0.32"  # required by esp-idf-sys for build coordination

[[bin]]
name = "esp32-gnssmqtt"
harness = false  # required for no_std-compat test builds on ESP
```

**WARNING:** The exact version numbers above are from training data. Run the `esp-idf-template` generator for a known-good set of coordinated versions.

---

## Sources

- Training data through August 2025 — core framework recommendation (MEDIUM confidence)
- `esp-rs` GitHub organization: https://github.com/esp-rs — official source for all ESP Rust crates
- Espressif ESP-IDF Programming Guide: https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/ — C6-specific IDF docs
- `esp-idf-svc` repo: https://github.com/esp-rs/esp-idf-svc — examples for WiFi, MQTT, BLE, NVS
- `esp-idf-hal` repo: https://github.com/esp-rs/esp-idf-hal — UART, GPIO examples
- The Rust on ESP Book: https://esp-rs.github.io/book/ — official guide; covers std vs no_std choice authoritatively
- `esp-idf-template`: https://github.com/esp-rs/esp-idf-template — canonical project scaffold

**Verification required before coding:**
1. Check current `esp-idf-hal`, `esp-idf-svc`, `esp-idf-sys` versions on crates.io
2. Check "The Rust on ESP Book" for updated esp-hal vs esp-idf-hal guidance
3. Verify BLE GATT server API in `esp-idf-svc` examples — this was the most volatile API as of mid-2025
4. Confirm ESP-IDF version compatibility with C6 in the esp-idf-sys build notes

---

*Stack research for: Embedded Rust firmware, ESP32-C6, GNSS/MQTT bridge*
*Researched: 2026-03-03*
*Confidence: MEDIUM — training data Aug 2025; external verification tools unavailable; verify all versions before use*
