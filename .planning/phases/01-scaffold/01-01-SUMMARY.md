---
phase: 01-scaffold
plan: 01
subsystem: infra
tags: [rust, esp32c6, esp-idf, embuild, riscv32, cargo, esp-idf-svc, esp-idf-hal, esp-idf-sys]

# Dependency graph
requires: []
provides:
  - "Compilable Rust firmware scaffold for ESP32-C6 targeting riscv32imac-esp-espidf"
  - "Three Espressif crates pinned: esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1"
  - "device_id::get() -> String via esp_efuse_mac_get_default eFuse FFI"
  - "config.rs constants stubs: WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_PASS, UART_RX_BUF_SIZE"
  - "ESP-IDF v5.3.3 with managed download via embuild, MCU=esp32c6"
  - "sdkconfig.defaults: 8KB main stack, FreeRTOS canary overflow detection, INFO log level"
  - "partitions.csv: NVS 64KB at 0x9000, phy_init at 0x19000, factory app at 0x20000"
affects: [02-wifi-mqtt, 03-ble-provisioning]

# Tech tracking
tech-stack:
  added:
    - "esp-idf-svc =0.51.0 (WiFi, logging, system services)"
    - "esp-idf-hal =0.45.2 (hardware abstraction)"
    - "esp-idf-sys =0.36.1 (FFI bindings to ESP-IDF C SDK)"
    - "embuild =0.33 (ESP-IDF download/cmake integration)"
    - "log =0.4 (Rust logging facade)"
    - "ldproxy =0.3.4 (ESP-IDF linker proxy)"
    - "ESP-IDF v5.3.3 (managed by embuild)"
  patterns:
    - "Mandatory init sequence: link_patches() then EspLogger::initialize_default() then everything else"
    - "Device ID from last 3 bytes of factory MAC eFuse as 6-char uppercase hex"
    - "Empty-string constant stubs for Phase 2 connectivity credentials"
    - "Pinned crate versions with = specifier for reproducibility"

key-files:
  created:
    - "Cargo.toml - project manifest with pinned Espressif crates"
    - "build.rs - ESP-IDF environment wiring via embuild::espidf::sysenv::output()"
    - ".cargo/config.toml - target triple riscv32imac-esp-espidf, MCU=esp32c6, ESP_IDF_VERSION=v5.3.3"
    - "rust-toolchain.toml - nightly toolchain with rust-src component"
    - "sdkconfig.defaults - main stack 8KB, canary overflow detection, INFO log level"
    - "partitions.csv - NVS 64KB at 0x9000, factory app at 0x20000"
    - "src/main.rs - boot entry: link_patches, EspLogger, device ID log, idle loop"
    - "src/device_id.rs - get() -> String from esp_efuse_mac_get_default FFI"
    - "src/config.rs - compile-time constants stubs for Phase 2"
    - "Cargo.lock - dependency lock for reproducible builds"
  modified: []

key-decisions:
  - "Used esp-idf-svc =0.51.0 / esp-idf-hal =0.45.2 / esp-idf-sys =0.36.1 with = pinning for build reproducibility"
  - "ESP-IDF v5.3.3 managed via embuild (auto-download, no manual SDK setup required)"
  - "Device ID uses last 3 bytes of 6-byte factory MAC to ensure uniqueness (first 3 bytes are Espressif OUI)"
  - "nightly toolchain (not stable) required by esp-idf-sys for RISC-V target support"
  - "CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y for FreeRTOS stack overflow detection with minimal overhead"
  - "CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000: Rust std needs 8KB+ vs ESP-IDF default 3KB"

patterns-established:
  - "Init sequence: link_patches() -> EspLogger::initialize_default() -> application code"
  - "Phase stub pattern: empty-string constants with doc comments pointing to implementing phase"
  - "FFI safety: SAFETY comment documenting buffer validity and function semantics"

requirements-completed: [SCAF-01, SCAF-02, SCAF-03, SCAF-04]

# Metrics
duration: ~60min
completed: 2026-03-03
---

# Phase 1 Plan 01: Scaffold SUMMARY

**Compilable ESP32-C6 firmware scaffold with pinned esp-idf crates, device ID from hardware eFuse, and partition/sdkconfig tuned for Rust std + GNSS-MQTT workload**

## Performance

- **Duration:** ~60 min (includes ESP-IDF v5.3.3 download + submodule init)
- **Started:** 2026-03-03T07:40:00Z (estimated)
- **Completed:** 2026-03-03T08:43:18Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments

- `cargo build` succeeds targeting riscv32imac-esp-espidf, producing binary at `target/riscv32imac-esp-espidf/debug/esp32-gnssmqtt`
- All three Espressif crates pinned with `=` version specifiers: esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1
- Device ID module reads last 3 bytes of factory MAC from hardware eFuse via `esp_efuse_mac_get_default` FFI
- Config module defines Phase 2 constant stubs (WIFI_SSID, MQTT_HOST, UART_RX_BUF_SIZE) with safe empty defaults
- Main boot sequence follows mandatory init order: link_patches -> EspLogger -> device ID log -> idle loop
- FreeRTOS canary stack overflow detection enabled; main task stack set to 8000 bytes
- NVS partition at standard 0x9000 with 64KB (0x10000) exactly as required by SCAF-04

## Task Commits

Each task was committed atomically:

1. **Task 1: Create project configuration files** - `7ef080d` (chore)
2. **Task 2: Create Rust source files and verify build** - `d6a3125` (feat)

## Files Created/Modified

- `Cargo.toml` - Project manifest with pinned Espressif crates and build profiles
- `build.rs` - ESP-IDF environment wiring via embuild::espidf::sysenv::output()
- `.cargo/config.toml` - Target riscv32imac-esp-espidf, linker ldproxy, MCU=esp32c6, ESP_IDF_VERSION=v5.3.3
- `rust-toolchain.toml` - nightly toolchain with rust-src component (required for RISC-V no-std target)
- `sdkconfig.defaults` - Main stack 8000 bytes, canary overflow detection, INFO log level
- `partitions.csv` - NVS 64KB at 0x9000, phy_init at 0x19000, factory app at 0x20000
- `src/main.rs` - Firmware entry: mandatory init sequence + device ID log + idle loop
- `src/device_id.rs` - get() -> String reading 6-byte factory MAC eFuse, returns last 3 bytes as hex
- `src/config.rs` - Compile-time constants stubs for WiFi/MQTT/UART (Phase 2 fills these in)
- `Cargo.lock` - Dependency lock for reproducible builds

## Decisions Made

- **Pinned versions over ranges:** `=0.51.0` vs `"0.51"` — ESP32 Rust crate ecosystem has frequent breaking changes; exact pins prevent unexpected upgrades breaking the build
- **embuild managed SDK:** embuild auto-downloads ESP-IDF v5.3.3 and its toolchain, no manual `idf.py` setup required — reduces onboarding friction
- **nightly toolchain:** esp-idf-sys requires nightly features (build-std) for the RISC-V target; stable Rust cannot cross-compile std for riscv32imac-esp-espidf
- **Device ID from eFuse:** Hardware OTP eFuse guarantees uniqueness and stability across power cycles; no random generation needed

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Initialized missing ESP-IDF git submodules**
- **Found during:** Task 2 (cargo build)
- **Issue:** embuild downloaded ESP-IDF v5.3.3 repo but git submodules (esp-mqtt, lwip, etc.) were not initialized. CMake failed with "Missing esp-mqtt submodule" then "lwip/src/include is not a directory"
- **Fix:** Ran `git submodule update --init --recursive` in `.embuild/espressif/esp-idf/v5.3.3/`. Additionally ran `git checkout HEAD -- .` in the lwip submodule which had files staged as deleted.
- **Files modified:** `.embuild/espressif/esp-idf/v5.3.3/` (submodule state — not committed)
- **Verification:** `ls components/lwip/lwip/src/include/` returned compat, lwip, netif directories
- **Not committed:** This is build tooling state, not source code

**2. [Rule 3 - Blocking] Installed missing ldproxy linker**
- **Found during:** Task 2 (cargo build after fixing submodules)
- **Issue:** Rust compilation succeeded but linking failed with "linker ldproxy not found" — ldproxy must be installed separately from the Rust toolchain
- **Fix:** Ran `cargo install ldproxy` (installed v0.3.4)
- **Files modified:** `~/.cargo/bin/ldproxy.exe` (host tool, not project source)
- **Verification:** Subsequent `cargo build` succeeded with `Finished dev profile`
- **Not committed:** System-level tool installation

---

**Total deviations:** 2 auto-fixed (both Rule 3 - Blocking)
**Impact on plan:** Both fixes are environment setup issues not ESP32-C6 Rust ecosystem problems. Any fresh clone will need these same steps. Consider adding a README with setup steps.

## Issues Encountered

- ESP-IDF v5.3.3 git submodules not auto-initialized by embuild: resolved by running `git submodule update --init --recursive` in the ESP-IDF directory
- lwip submodule had files staged as deleted (prior interrupted tool install): resolved by `git checkout HEAD -- .` in that submodule
- ldproxy not pre-installed: resolved by `cargo install ldproxy`

## User Setup Required

None for firmware source code. However, for a fresh development environment clone:
1. `cargo install ldproxy` — linker proxy (not installed by rustup)
2. First `cargo build` will auto-download ESP-IDF v5.3.3 and run submodule init (can take 10-15 min)

## Next Phase Readiness

- Phase 2 (WiFi + MQTT) can begin immediately: config.rs stubs are in place, Cargo.toml already has esp-idf-svc which includes WiFi and MQTT client
- `src/config.rs` constants (WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_PASS) are ready to fill in
- `config.rs::UART_RX_BUF_SIZE = 4096` ready for Phase 2 `uart_driver_install()` call
- Concern: nightly Rust toolchain may produce pointer type mismatches in newer nightly versions — `rust-toolchain.toml` has a comment with the fallback pin `nightly-2024-12-01`

---
*Phase: 01-scaffold*
*Completed: 2026-03-03*
