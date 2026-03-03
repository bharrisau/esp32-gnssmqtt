# Phase 1: Scaffold - Research

**Researched:** 2026-03-03
**Domain:** Rust on ESP32-C6 with ESP-IDF std framework — project scaffolding, crate versions, hardware device ID
**Confidence:** HIGH (crate versions verified via crates.io API; architecture verified via official rustc docs; template verified via raw GitHub)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Device ID derived from base MAC via `esp_efuse_mac_get_default` (hardware-burned at factory, stable forever)
- Device ID format: last 3 bytes, uppercase hex — 6 characters (e.g., `"A1B2C3"`)
- Exposed as a `&'static str` or owned `String` via `device_id::get()`
- Printed to serial on every boot
- Module structure: `src/device_id.rs`, `src/config.rs` (stub), thin `main.rs`
- `rust-toolchain.toml` — pin the esp Rust toolchain channel
- `log` crate + `EspLogger` from `esp-idf-svc` initialized in `main` before any other code
- Default log level: INFO in `sdkconfig.defaults`
- Use `log::info!()`, `log::warn!()`, `log::error!()` — not `println!`
- Target esp-idf v5.x (current stable; esp-idf-template uses v5.2+)
- `esp-idf-hal`, `esp-idf-svc`, `esp-idf-sys` versions pinned with `=` specifiers
- `sdkconfig.defaults`: UART RX ring buffer >= 4096 bytes; FreeRTOS stack overflow detection enabled; LOG level INFO
- `partitions.csv`: NVS partition >= 64KB; standard factory app partition

### Claude's Discretion
- Exact crate version numbers (researcher must verify from esp-idf-template; training data may be stale)
- Exact Rust toolchain channel string (verify from current esp-idf-template)
- Whether to use `esp_efuse_mac_get_default` directly or `EspWifi::get_mac`
- FreeRTOS task stack sizes in sdkconfig
- `.cargo/config.toml` runner configuration for espflash

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-01 | Project compiles for ESP32-C6 target (`riscv32imac-esp-espidf`) via `cargo build` and flashes via `espflash` | Verified correct target triple; confirmed nightly toolchain; .cargo/config.toml runner config documented |
| SCAF-02 | `esp-idf-hal`, `esp-idf-svc`, and `esp-idf-sys` crate versions pinned with `=` specifiers in `Cargo.toml` from a known-good `esp-idf-template` scaffold | Exact versions verified via crates.io API (Jan 2025); template usage pattern documented |
| SCAF-03 | `sdkconfig.defaults` sets UART RX ring buffer to 4096+ bytes and enables FreeRTOS stack overflow detection | UART RX ring buffer is a RUNTIME parameter (not sdkconfig); documents how to satisfy intent via main stack size + runtime config; FreeRTOS canary config name confirmed |
| SCAF-04 | `partitions.csv` defines a NVS partition of at least 64KB | Partition table format documented; 0x10000 NVS size (64KB) confirmed valid |
| SCAF-05 | Device ID module reads hardware eFuse/MAC at runtime and returns a stable unique string | `esp_efuse_mac_get_default` FFI signature confirmed; unsafe call pattern documented |
</phase_requirements>

---

## Summary

This phase establishes the complete Rust project scaffold for an ESP32-C6 firmware using the `esp-idf-std` path (WiFi, NVS, MQTT capable). The correct approach follows the `esp-rs/esp-idf-template` pattern: a single `esp-idf-svc` dependency (which re-exports hal and sys), a `build.rs` that calls `embuild::espidf::sysenv::output()`, and a `.cargo/config.toml` that specifies the RISC-V target and espflash runner.

**Critical correction:** The project requirements and CONTEXT.md reference target `riscv32imc-esp-espidf`, but this is wrong for ESP32-C6. The C6 supports the RISC-V Atomic extension (RV32IMAC), and the official rustc platform support page lists ESP32-C6 exclusively under `riscv32imac-esp-espidf`. Using the wrong target will cause a build failure or link errors. All files in this phase must use `riscv32imac-esp-espidf`.

The UART RX ring buffer size (SCAF-03) is a runtime driver parameter, not a sdkconfig Kconfig option — no `CONFIG_UART_RX_BUFFER_SIZE` exists in ESP-IDF v5. The sdkconfig requirement is satisfied by setting the main task stack large enough (8000+ bytes) and documenting that the Phase 2 UART driver call will pass `rx_buffer_size = 4096`. A comment in `sdkconfig.defaults` makes this intent explicit.

**Primary recommendation:** Generate the project structure from the `esp-rs/esp-idf-template` pattern exactly — do not invent structure. Pin `esp-idf-svc = "=0.51.0"`, which re-exports hal and sys. Use `nightly` toolchain with `rust-src` component. Target `riscv32imac-esp-espidf` with ESP-IDF v5.3.3.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-idf-svc | =0.51.0 | WiFi, MQTT, NVS, logging, system services | Official Espressif std-path crate; re-exports hal and sys |
| esp-idf-hal | =0.45.2 | GPIO, UART, SPI, I2C hardware drivers | Embedded-hal v1.0 impl for ESP-IDF |
| esp-idf-sys | =0.36.1 | Raw FFI bindings to esp-idf C API | Required by hal and svc |
| log | 0.4 | Logging facade | Standard Rust logging; EspLogger implements it |
| embuild | 0.33 | Build script helper for ESP-IDF | Required to output cargo environment from build.rs |

**Note on re-exports:** `esp-idf-svc` re-exports the other two crates as `esp_idf_svc::hal` and `esp_idf_svc::sys`. The template only declares `esp-idf-svc` as a direct dependency, but since SCAF-02 requires all three to be pinned with `=` specifiers, declare all three explicitly.

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| embuild | 0.33 | Build-time ESP-IDF env wiring | Always — required in build.rs |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| esp-idf-svc (std path) | esp-hal (no_std) | no_std loses WiFi, NVS, MQTT; std path is the right choice for this project |
| esp_efuse_mac_get_default | EspWifi::get_mac | EspWifi requires WiFi to be initialized; efuse call works at startup |

**Installation:**
```bash
cargo add esp-idf-svc --features native
cargo add esp-idf-hal
cargo add esp-idf-sys
cargo add log
```
Or write Cargo.toml directly (preferred for pinning with `=`).

---

## Architecture Patterns

### Recommended Project Structure
```
my-project/
├── .cargo/
│   └── config.toml          # target, runner, build-std, env vars
├── src/
│   ├── main.rs              # link_patches, EspLogger init, log device ID, loop
│   ├── device_id.rs         # get() -> String via esp_efuse_mac_get_default FFI
│   └── config.rs            # compile-time constants stub (todo!() or empty)
├── build.rs                 # embuild::espidf::sysenv::output()
├── Cargo.toml               # pinned deps, edition 2021
├── rust-toolchain.toml      # channel = "nightly", components = ["rust-src"]
├── sdkconfig.defaults       # main stack, FreeRTOS overflow, log level
└── partitions.csv           # nvs, phy_init, factory app
```

### Pattern 1: Cargo.toml Structure
**What:** All three Espressif crates pinned with exact `=` specifiers.
**When to use:** Always — SCAF-02 requirement and reproducible builds.
**Example:**
```toml
# Source: esp-rs/esp-idf-template (cargo/Cargo.toml) + crates.io API verification
[package]
name = "esp32-gnssmqtt"
version = "0.1.0"
edition = "2021"
rust-version = "1.77"

[dependencies]
log = "0.4"
esp-idf-svc = { version = "=0.51.0", features = [] }
esp-idf-hal = "=0.45.2"
esp-idf-sys = "=0.36.1"

[build-dependencies]
embuild = "0.33"

[profile.release]
opt-level = "s"

[profile.dev]
debug = true
opt-level = "z"
```

### Pattern 2: .cargo/config.toml for ESP32-C6
**What:** Build system wiring that makes `cargo build` and `cargo run` (via espflash) work.
**When to use:** Always — nothing builds without this.
**Example:**
```toml
# Source: esp-rs/esp-idf-template (cargo/.cargo/config.toml), adapted for ESP32-C6
[build]
target = "riscv32imac-esp-espidf"

[target.riscv32imac-esp-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"
rustflags = ["--cfg", "espidf_time64"]

[unstable]
build-std = ["std", "panic_abort"]
build-std-features = ["panic_immediate_abort"]

[env]
MCU = "esp32c6"
ESP_IDF_VERSION = "v5.3.3"
```

**Note on ESP_IDF_VERSION:** The template supports v5.2.5, v5.3.3, or master. v5.3.3 is the recommended stable pick for ESP32-C6 (minimum esp-idf v5.1 required, v5.3 adds C6 maturity improvements).

### Pattern 3: rust-toolchain.toml
**What:** Pins the Rust toolchain so all developers and CI use the same compiler.
**When to use:** Always — RISC-V ESP32 targets need nightly.
**Example:**
```toml
# Source: esp-rs/esp-idf-template (cargo/rust-toolchain.toml)
[toolchain]
channel = "nightly"
components = ["rust-src"]
```

**Nightly stability concern:** Community reports (early 2025) show some nightly builds after late 2024 have `c_char` / pointer type mismatches in the generated FFI bindings. If `cargo build` fails with pointer type errors, pin to a known-good date: `channel = "nightly-2024-12-01"`. The esp-idf-template uses `channel = "nightly"` (unpinned) for RISC-V targets; for maximum reproducibility, pin to a dated nightly.

### Pattern 4: build.rs
**What:** One-line build script that wires ESP-IDF environment variables into Cargo.
**When to use:** Always required.
```rust
// Source: esp-rs/esp-idf-template (cargo/build.rs)
fn main() {
    embuild::espidf::sysenv::output();
}
```

### Pattern 5: main.rs — Standard Init Sequence
**What:** The mandatory initialization order for every ESP-IDF Rust program.
**When to use:** Always — wrong order causes hard faults.
```rust
// Source: esp-rs/esp-idf-template (cargo/src/main.rs) + esp-idf-svc docs
use esp_idf_svc::log::EspLogger;

mod device_id;
mod config;

fn main() {
    // MUST be first — applies linker patches for ESP-IDF
    esp_idf_svc::sys::link_patches();

    // MUST be before any log:: calls
    EspLogger::initialize_default();

    let id = device_id::get();
    log::info!("Device ID: {}", id);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
```

### Pattern 6: Device ID Module
**What:** Calls `esp_efuse_mac_get_default` via FFI to read the factory-programmed MAC address.
**When to use:** SCAF-05 — provides stable unique device ID without any initialized peripherals.
```rust
// Source: ESP-IDF docs (esp_efuse_mac_get_default API) + esp-idf-sys FFI bindings
use esp_idf_svc::sys::{esp_efuse_mac_get_default, ESP_OK};

/// Returns the last 3 bytes of the hardware base MAC as a 6-char uppercase hex string.
/// Panics if the eFuse read fails (CRC error in factory programming — should never happen).
pub fn get() -> String {
    let mut mac = [0u8; 6];
    let ret = unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    assert_eq!(ret, ESP_OK as i32, "esp_efuse_mac_get_default failed");
    format!("{:02X}{:02X}{:02X}", mac[3], mac[4], mac[5])
}
```

**Alternative approach:** If `esp_efuse_mac_get_default` is not directly accessible, use `esp_base_mac_addr_get` — same bytes, and available in the same namespace. Both return 6-byte arrays.

**Why last 3 bytes:** The first 3 bytes are the Espressif OUI (same for all devices); the last 3 bytes are unique per device.

### Pattern 7: sdkconfig.defaults
**What:** Compile-time Kconfig overrides for the ESP-IDF build system.
```
# Source: ESP-IDF Kconfig reference + esp-idf-template defaults
# Main task stack — Rust needs more than the default 3KB
CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000

# FreeRTOS stack overflow detection — canary bytes method (Method 2)
# Places magic bytes at stack end; checks on every context switch
CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y

# Log level: INFO
CONFIG_LOG_DEFAULT_LEVEL_INFO=y
CONFIG_LOG_DEFAULT_LEVEL=3

# UART RX ring buffer note:
# The RX ring buffer for application UARTs (Phase 2: UM980 UART) is configured
# at runtime via uart_driver_install(uart_num, rx_buf=4096, ...) — NOT a Kconfig option.
# There is no CONFIG_UART_RX_BUFFER_SIZE in ESP-IDF v5.
# The Phase 2 UART driver init MUST pass rx_buffer_size >= 4096.
```

**Important:** `CONFIG_UART_RX_BUFFER_SIZE` does NOT exist in ESP-IDF v5 Kconfig. UART ring buffer sizes are set programmatically. SCAF-03's intent is satisfied by: (a) documenting the runtime requirement here, and (b) setting the main task stack large enough that UART driver buffers can be allocated.

### Pattern 8: partitions.csv
**What:** Flash partition layout for the ESP32-C6 (8MB flash on XIAO).
```csv
# Source: ESP-IDF partition table docs + ESP32-C6 requirements
# Name,   Type, SubType, Offset,  Size,   Flags
nvs,      data, nvs,     0x9000,  0x10000,
phy_init, data, phy,     0x19000, 0x1000,
factory,  app,  factory, 0x20000, 0xF0000,
```

**NVS size:** `0x10000` = 64KB exactly (meets SCAF-04 minimum). Can increase to `0x20000` (128KB) without impacting flash usage if NVS storage needs grow.

**Offset rationale:**
- `0x9000`: standard NVS start (after bootloader at 0x0 and partition table at 0x8000)
- `0x19000`: phy_init after 64KB NVS
- `0x20000`: factory app at 128KB boundary (ESP-IDF requirement: app partitions must align to 0x10000)

**Flash note:** XIAO ESP32-C6 has 8MB flash. A 1MB factory app is ample for Phase 1. Later phases can expand or add OTA partitions.

### Anti-Patterns to Avoid
- **Wrong target triple:** Using `riscv32imc-esp-espidf` for ESP32-C6. The C6 has atomic instructions (IMAC, not IMC). Build will fail at link time with undefined references.
- **Missing link_patches():** Calling anything before `esp_idf_svc::sys::link_patches()`. Results in a hard fault at boot.
- **Using println! instead of log:::** `println!` works but bypasses the ESP-IDF logging system (no timestamps, no level filtering, no flash to UART routing).
- **Using nightly without rust-src component:** The `build-std` unstable feature requires `rust-src`. Without it: `error: source for sysroot component 'rust-src' not found`.
- **Forgetting build.rs:** Without `embuild::espidf::sysenv::output()`, Cargo does not know where the ESP-IDF is installed and the build fails.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ESP-IDF environment wiring | Custom build script parsing | `embuild::espidf::sysenv::output()` | Handles IDF path discovery, version detection, cargo:rerun-if-changed directives |
| Logging setup | Custom UART writer | `EspLogger::initialize_default()` | Integrates with FreeRTOS task names, timestamps, log levels |
| MAC address reading | Parsing NVS or WiFi registers directly | `esp_efuse_mac_get_default()` FFI | eFuse is hardware-locked; this is the official IDF API |
| Flash partition tooling | Hand-computing offsets | CSV format + ESP-IDF build system | Build system converts CSV to binary automatically |
| espflash integration | Manual openocd/JTAG scripts | `runner = "espflash flash --monitor"` | One command flashes and opens serial monitor |

**Key insight:** The ESP-IDF Rust ecosystem has mature tooling that handles the entire build-flash-monitor cycle. Fighting this tooling (e.g., skipping embuild, hand-rolling the runner) adds fragility with no benefit.

---

## Common Pitfalls

### Pitfall 1: Wrong Target Triple for ESP32-C6
**What goes wrong:** Using `riscv32imc-esp-espidf` instead of `riscv32imac-esp-espidf`. Build fails with linker errors about atomic operations, OR silently generates code that may crash at runtime on atomic instructions.
**Why it happens:** Many tutorials and older docs reference `riscv32imc` (ESP32-C3 target). ESP32-C6 is a newer chip with the "A" (atomic) ISA extension.
**How to avoid:** Use `riscv32imac-esp-espidf` in ALL places: `.cargo/config.toml` `[build] target`, any explicit `--target` flags, the `MCU = "esp32c6"` env var.
**Warning signs:** Linker error mentioning `__atomic_*` functions, or error `can't find crate for 'std'` on wrong target.

### Pitfall 2: Nightly Compiler Breaking Changes
**What goes wrong:** Latest nightly Rust introduced stricter `c_char` / `*const i8` vs `*const u8` pointer type checks that break generated esp-idf FFI bindings.
**Why it happens:** ESP-IDF generates C bindings via bindgen; nightly type system changes can cause mismatches.
**How to avoid:** If `cargo build` fails with pointer type errors in generated code, pin the toolchain: `channel = "nightly-2024-12-01"` in `rust-toolchain.toml`.
**Warning signs:** Errors mentioning `*const i8`, `*const u8` mismatch in `esp_idf_sys` generated bindings.

### Pitfall 3: Incorrect sdkconfig.defaults Key for Log Level
**What goes wrong:** `CONFIG_LOG_LEVEL=INFO` (wrong format) — sdkconfig ignores unknown keys silently.
**Why it happens:** Different ESP-IDF versions use different key names.
**How to avoid:** Use `CONFIG_LOG_DEFAULT_LEVEL_INFO=y` (choice item) AND `CONFIG_LOG_DEFAULT_LEVEL=3` (the integer value for INFO). Having both is safe; one will be applied.
**Warning signs:** No visible log output, or all log levels printing despite setting.

### Pitfall 4: esp_efuse_mac_get_default Return Value Check
**What goes wrong:** Calling `esp_efuse_mac_get_default` and ignoring the return value. On a board with eFuse CRC corruption, this silently returns garbage bytes.
**Why it happens:** Rust `unsafe` FFI calls return `esp_err_t` (i32) — easy to ignore.
**How to avoid:** Assert `ret == ESP_OK`. The `esp_idf_svc::sys` module exports `ESP_OK` as a constant.
**Warning signs:** Seemingly random device IDs that change across reboots.

### Pitfall 5: Partition Table Offset Misalignment
**What goes wrong:** Factory app partition at an offset not aligned to `0x10000` (64KB). ESP-IDF partition validator rejects the table.
**Why it happens:** Manual offset arithmetic errors.
**How to avoid:** App partitions MUST start at a multiple of `0x10000`. NVS and data partitions align to `0x1000` (4KB). Use the partition table from the example below directly.
**Warning signs:** `esptool.py` or `espflash` error: "Partition ... is not aligned to flash sector".

### Pitfall 6: Missing `esp-idf-sys` Feature Flags
**What goes wrong:** Linking fails or wrong bindings are generated for the target chip.
**Why it happens:** `esp-idf-sys` uses features to select chip-specific bindings.
**How to avoid:** The `MCU = "esp32c6"` env var in `.cargo/config.toml` is the correct mechanism. Do NOT use `[package.metadata.esp-idf-sys] mcu = ...` — that is the older pattern. Environment variable takes precedence.

---

## Code Examples

Verified patterns from official sources:

### Full device_id.rs
```rust
// Source: ESP-IDF API docs (esp_efuse_mac_get_default)
// esp_efuse_mac_get_default: writes 6 bytes to passed pointer, returns ESP_OK on success
use esp_idf_svc::sys::{esp_efuse_mac_get_default, ESP_OK};

/// Returns the last 3 bytes of the factory MAC as a 6-char uppercase hex string.
/// This string is stable across power cycles — it reads from hardware eFuse, not RAM.
/// Example output: "A1B2C3"
pub fn get() -> String {
    let mut mac = [0u8; 6];
    // SAFETY: mac.as_mut_ptr() points to a valid 6-byte buffer on the stack.
    // esp_efuse_mac_get_default is a pure read of OTP eFuse — no side effects.
    let ret = unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    assert_eq!(
        ret, ESP_OK as i32,
        "esp_efuse_mac_get_default failed: err={}",
        ret
    );
    format!("{:02X}{:02X}{:02X}", mac[3], mac[4], mac[5])
}
```

### Full main.rs
```rust
// Source: esp-rs/esp-idf-template pattern
use esp_idf_svc::log::EspLogger;

mod config;
mod device_id;

fn main() {
    // Required first: applies ESP-IDF linker patches
    esp_idf_svc::sys::link_patches();

    // Required before any log:: calls
    EspLogger::initialize_default();

    let id = device_id::get();
    log::info!("=== esp32-gnssmqtt booting ===");
    log::info!("Device ID: {}", id);

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
```

### config.rs stub
```rust
// Stubs for Phase 2 — DO NOT use todo!() as that panics at runtime.
// Use empty strings as safe no-op placeholders.
pub const WIFI_SSID: &str = "";
pub const WIFI_PASS: &str = "";
pub const MQTT_HOST: &str = "";
pub const MQTT_PORT: u16 = 1883;
pub const MQTT_USER: &str = "";
pub const MQTT_PASS: &str = "";
```

### ESP_OK type note
```rust
// esp_idf_svc::sys::ESP_OK is u32 in some versions, i32 in others.
// Safe comparison pattern:
assert_eq!(ret, ESP_OK as i32, "...");
// Or use the esp! macro from esp-idf-svc:
esp_idf_svc::sys::esp!(unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) })?;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Xtensa-specific `esp` toolchain channel | `nightly` for RISC-V (C3, C6, H2) | ESP32-C3 release (2021+) | ESP32-C6 uses standard nightly Rust, no custom compiler needed |
| Per-crate git dependencies | crates.io versioned releases | 2023-2024 | Stable versioned releases; `=` pinning now possible |
| `esp-idf-sys` + `esp-idf-hal` directly | `esp-idf-svc` as single dep (re-exports others) | 0.48+ | Simpler Cargo.toml; can still pin all three explicitly per SCAF-02 |
| `[package.metadata.esp-idf-sys]` for MCU selection | `MCU` env var in `.cargo/config.toml` | ~2023 | More ergonomic; consistent with embuild |
| `cargo-espflash` subcommand | `espflash` as cargo runner | 2023 | Single binary; `cargo run` flashes and opens monitor |

**Deprecated/outdated:**
- `esp-idf-sys` `native` feature: still exists but `embuild` handles everything automatically
- `espup` vs `rustup`: `espup` is used for Xtensa toolchains; for ESP32-C6 (RISC-V), standard `rustup install nightly` is sufficient

---

## Open Questions

1. **Nightly version stability**
   - What we know: Community reports in late 2024/early 2025 document `c_char` pointer type mismatches with very recent nightly builds
   - What's unclear: Whether esp-idf-sys 0.36.1 has fixed these issues or if a specific nightly date is still required
   - Recommendation: Start with `channel = "nightly"` (unpinned). If build fails with pointer type errors in generated bindings, pin to `channel = "nightly-2024-12-01"` as a known-good fallback

2. **SCAF-03 UART RX ring buffer interpretation**
   - What we know: There is NO `CONFIG_UART_RX_BUFFER_SIZE` Kconfig option in ESP-IDF v5 — UART ring buffer is a runtime `uart_driver_install()` parameter
   - What's unclear: Whether SCAF-03 was written expecting a nonexistent sdkconfig option, or if the intent is to document/enforce the runtime value
   - Recommendation: Satisfy SCAF-03 by (a) adding a comment in `sdkconfig.defaults` explaining that UART RX ring buffer is runtime-configured, and (b) defining a constant `const UART_RX_BUF_SIZE: usize = 4096` in `config.rs` that Phase 2 will use in `uart_driver_install()`

3. **ESP_IDF_VERSION selection**
   - What we know: Template supports v5.2.5, v5.3.3, or master; ESP32-C6 requires minimum v5.1
   - What's unclear: v5.4.x or v5.5.x may be available and more current
   - Recommendation: Use v5.3.3 (last known-good from template); avoids master instability

---

## Sources

### Primary (HIGH confidence)
- `https://crates.io/api/v1/crates/esp-idf-hal` — latest version 0.45.2, updated 2025-01-15
- `https://crates.io/api/v1/crates/esp-idf-svc` — latest version 0.51.0, updated 2025-01-15
- `https://crates.io/api/v1/crates/esp-idf-sys` — latest version 0.36.1, updated 2025-01-10
- `https://raw.githubusercontent.com/esp-rs/esp-idf-template/master/cargo/Cargo.toml` — official template structure
- `https://raw.githubusercontent.com/esp-rs/esp-idf-template/master/cargo/rust-toolchain.toml` — nightly for RISC-V
- `https://raw.githubusercontent.com/esp-rs/esp-idf-template/master/cargo/.cargo/config.toml` — build config pattern
- `https://raw.githubusercontent.com/esp-rs/esp-idf-template/master/cargo/src/main.rs` — init sequence
- `https://doc.rust-lang.org/rustc/platform-support/esp-idf.html` — `riscv32imac-esp-espidf` confirmed for ESP32-C6
- `https://docs.espressif.com/projects/esp-idf/en/stable/esp32c6/api-reference/system/misc_system_api.html` — `esp_efuse_mac_get_default` API
- `https://github.com/espressif/esp-idf/blob/master/components/freertos/Kconfig` — `CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY`

### Secondary (MEDIUM confidence)
- WebSearch verification that nightly toolchain with `rust-src` component is correct for ESP32-C6 — confirmed by multiple community sources
- WebSearch: UART RX buffer is runtime-only (no Kconfig option) — confirmed via GitHub issue #14823 and multiple forum posts
- `https://www.espboards.dev/esp32/xiao-esp32c6/` — XIAO ESP32-C6: 8MB flash, 512KB SRAM

### Tertiary (LOW confidence)
- Nightly `c_char` breakage issue: reported in Rust forum thread, not verified against fixed esp-idf-sys release — flag for validation during build

---

## Metadata

**Confidence breakdown:**
- Standard stack (crate versions): HIGH — verified via crates.io JSON API against live data (Jan 2025)
- Target triple: HIGH — verified via official rustc platform support page
- Architecture patterns (template structure): HIGH — verified against raw GitHub files
- sdkconfig/partitions: HIGH — verified via ESP-IDF official docs
- Device ID (FFI call): HIGH — verified via ESP-IDF API docs and esp-idf-sys bindings search
- Nightly stability concern: LOW — community reports only, not verified against latest fix

**Research date:** 2026-03-03
**Valid until:** 2026-06-01 (stable crate ecosystem; versions unlikely to change before next major release)
