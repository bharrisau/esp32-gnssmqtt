# Phase 1: Scaffold - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

A correctly structured, version-pinned Rust project that compiles for ESP32-C6 (`riscv32imc-esp-espidf`), flashes successfully via `espflash`, and exposes a stable unique device ID derived from hardware MAC/eFuse. No connectivity, no GNSS, no LED logic — just a buildable foundation with the right project structure, sdkconfig, partition table, and device ID.

</domain>

<decisions>
## Implementation Decisions

### Device ID format
- Derived from the base MAC address via `esp_efuse_mac_get_default` (hardware-burned at factory, stable forever)
- Format: last 3 bytes, uppercase hex — 6 characters (e.g., `"A1B2C3"`)
- Exposed as a `&'static str` or owned `String` via `device_id::get()`
- Printed to serial on every boot so the operator can identify the device without a label

### Module structure
- `src/device_id.rs` — device ID module (SCAF-05 calls this out explicitly)
- `src/config.rs` — stub for compile-time constants (SSID, MQTT host, etc.); filled in by Phase 2; placeholders with `todo!()` or empty strings now
- `main.rs` stays thin: init logging, log device ID, loop
- `rust-toolchain.toml` — pin the esp Rust toolchain channel for reproducible builds

### Logging
- `log` crate + `EspLogger` from `esp-idf-svc` initialized in `main` before any other code
- Default log level: INFO in `sdkconfig.defaults`
- Use `log::info!()`, `log::warn!()`, `log::error!()` throughout — not `println!`

### esp-idf SDK version
- Target esp-idf v5.x (current stable; esp-idf-template uses v5.2+)
- `esp-idf-hal`, `esp-idf-svc`, `esp-idf-sys` versions pinned with `=` specifiers from a known-good `esp-idf-template` snapshot

### sdkconfig and partitions
- `sdkconfig.defaults`: UART RX ring buffer ≥ 4096 bytes; FreeRTOS stack overflow detection enabled; LOG level INFO
- `partitions.csv`: NVS partition ≥ 64KB; standard factory app partition

### Claude's Discretion
- Exact crate version numbers (researcher must verify current coordinated versions from esp-idf-template; training data may be stale)
- Exact Rust toolchain channel string (verify from current esp-idf-template)
- Whether to use `esp_efuse_mac_get_default` directly or `EspWifi::get_mac` (both return the same bytes; pick whichever compiles cleanly without needing wifi initialized)
- FreeRTOS task stack sizes in sdkconfig
- `.cargo/config.toml` runner configuration for espflash

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — clean slate project; no existing code

### Established Patterns
- None yet — this phase establishes the patterns all subsequent phases will follow
- Key pattern to establish: module-per-concern (`device_id.rs`, future `wifi.rs`, `mqtt.rs`)

### Integration Points
- `device_id::get()` → used by Phase 2 for MQTT topic construction (`gnss/{device_id}/heartbeat`, etc.)
- `src/config.rs` stub → Phase 2 fills in WiFi SSID/password, MQTT host/port/credentials
- Logging init in `main` → all future phases inherit and extend

</code_context>

<specifics>
## Specific Ideas

- The device ID string appears in every MQTT topic path in the project — getting the format right now avoids topic migration later
- STATE.md blocker: "Verify current coordinated versions of esp-idf-hal/esp-idf-svc/esp-idf-sys from latest esp-idf-template before pinning" — researcher must resolve this before planner can pin versions
- Hardware: Seeed XIAO ESP32-C6, single yellow LED on GPIO15 active-low (not needed in Phase 1 but good to document early)

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-scaffold*
*Context gathered: 2026-03-03*
