---
phase: 01-scaffold
verified: 2026-03-03T10:30:00Z
status: passed
score: 5/5 must-haves verified
re_verification: false
---

# Phase 1: Scaffold Verification Report

**Phase Goal:** Establish a working Rust firmware scaffold for the ESP32-C6 that compiles cleanly and boots on real hardware.
**Verified:** 2026-03-03
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| #   | Truth                                                                                                        | Status     | Evidence                                                                                        |
| --- | ------------------------------------------------------------------------------------------------------------ | ---------- | ----------------------------------------------------------------------------------------------- |
| 1   | `cargo build` completes without errors targeting `riscv32imac-esp-espidf`                                    | VERIFIED   | Binary exists at `target/riscv32imac-esp-espidf/debug/esp32-gnssmqtt` (8.77 MB). Cargo.lock present and resolved. Commit d6a3125 records build success. |
| 2   | All three Espressif crates are pinned with `=` version specifiers in Cargo.toml                              | VERIFIED   | `esp-idf-svc = "=0.51.0"`, `esp-idf-hal = "=0.45.2"`, `esp-idf-sys = "=0.36.1"` in Cargo.toml. Cargo.lock confirms resolved versions 0.51.0, 0.45.2, 0.36.1. |
| 3   | `sdkconfig.defaults` has FreeRTOS stack overflow detection enabled and main stack >= 8000 bytes              | VERIFIED   | `CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y` on line 17. `CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000` on line 13. |
| 4   | `partitions.csv` has an NVS partition of exactly 0x10000 (64KB) starting at 0x9000                          | VERIFIED   | Line 2: `nvs, data, nvs, 0x9000, 0x10000,`. Factory partition corrected to 0x3E0000 (fixed from scaffold's 0xF0000). |
| 5   | `device_id::get()` compiles and is called from main with the result logged via `log::info!`                  | VERIFIED   | `mod device_id;` on line 12 of main.rs. `let id = device_id::get();` on line 23. `log::info!("Device ID: {}", id)` on line 25. Hardware confirmed: device prints `Device ID: FFFEB5` on boot. |

**Score:** 5/5 truths verified

---

### Required Artifacts

| Artifact                                              | Expected                                             | Status   | Details                                                                                      |
| ----------------------------------------------------- | ---------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------- |
| `Cargo.toml`                                          | Pinned `=0.51.0`, `=0.45.2`, `=0.36.1`             | VERIFIED | All three crates pinned. Lock file confirms resolved versions match.                         |
| `.cargo/config.toml`                                  | Target triple, espflash runner, env vars             | VERIFIED | `riscv32imac-esp-espidf`, `runner = "espflash flash --monitor"`, `MCU = "esp32c6"`, `ESP_IDF_VERSION = "v5.3.3"`. |
| `build.rs`                                            | ESP-IDF wiring via embuild + Windows partitions copy | VERIFIED | `embuild::espidf::sysenv::output()` on line 2. Windows-compatible `partitions.csv` copy logic added (lines 15–36). |
| `rust-toolchain.toml`                                 | Pinned nightly with rust-src                         | VERIFIED | `channel = "nightly"`, `components = ["rust-src"]`. Fallback pin documented in comments.    |
| `sdkconfig.defaults`                                  | Canary overflow detection, 8KB stack, custom partition | VERIFIED | `CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY=y`, `CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000`, `CONFIG_PARTITION_TABLE_CUSTOM=y`, `CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y`. |
| `partitions.csv`                                      | NVS 64KB at 0x9000, factory app at 0x20000          | VERIFIED | NVS: `0x9000, 0x10000`. phy_init: `0x19000, 0x1000`. Factory: `0x20000, 0x3E0000`.         |
| `src/main.rs`                                         | link_patches, EspLogger, device ID log               | VERIFIED | Mandatory init sequence implemented and ordered correctly. 34 lines, substantive.            |
| `src/device_id.rs`                                    | `get() -> String` via `esp_efuse_mac_get_default`   | VERIFIED | `pub fn get() -> String` with live FFI call. `unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) }`. 31 lines. |
| `src/config.rs`                                       | Phase 2 constant stubs with safe empty defaults      | VERIFIED | `WIFI_SSID`, `WIFI_PASS`, `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`, `UART_RX_BUF_SIZE = 4096`. 22 lines. |
| `target/riscv32imac-esp-espidf/debug/esp32-gnssmqtt` | Compiled firmware binary                             | VERIFIED | 8,770,660 bytes. Exists and is current. Hardware flash confirmed by 01-02 checkpoint.        |

---

### Key Link Verification

| From               | To                            | Via                                              | Status   | Details                                                                                  |
| ------------------ | ----------------------------- | ------------------------------------------------ | -------- | ---------------------------------------------------------------------------------------- |
| `build.rs`         | ESP-IDF SDK on host           | `embuild::espidf::sysenv::output()`             | WIRED    | Call present on line 2. Cargo.lock shows `embuild = "0.33"` resolved.                   |
| `src/main.rs`      | `src/device_id.rs`            | `mod device_id; device_id::get()`               | WIRED    | `mod device_id;` line 12. `let id = device_id::get();` line 23. Result logged line 25.  |
| `src/device_id.rs` | `esp_efuse_mac_get_default`   | `unsafe { esp_efuse_mac_get_default(...) }` FFI | WIRED    | Import on line 8, call on line 23, result asserted line 24, used in format! line 30.    |
| `src/main.rs`      | `src/config.rs`               | `mod config;`                                    | PARTIAL  | Module declared on line 11. No constants accessed in Phase 1 main — intentional by design (stubs for Phase 2). Not a defect. |

---

### Requirements Coverage

| Requirement | Source Plan | Description                                                                              | Status    | Evidence                                                                                              |
| ----------- | ----------- | ---------------------------------------------------------------------------------------- | --------- | ----------------------------------------------------------------------------------------------------- |
| SCAF-01     | 01-01       | Project compiles for ESP32-C6 and flashes via `espflash`                                 | SATISFIED | Binary artifact at 8.77 MB. Hardware flash confirmed (commit c1cbd42, device boots and logs).         |
| SCAF-02     | 01-01       | Three Espressif crates pinned with `=` specifiers from known-good template               | SATISFIED | `=0.51.0`, `=0.45.2`, `=0.36.1` in Cargo.toml. Cargo.lock confirms exact resolution.                |
| SCAF-03     | 01-01       | `sdkconfig.defaults` sets UART RX ring buffer 4096+ bytes, FreeRTOS overflow detection  | SATISFIED | FreeRTOS canary detection confirmed. UART RX buffer: no Kconfig exists in ESP-IDF v5 (see note below). `UART_RX_BUF_SIZE = 4096` defined in `src/config.rs` for Phase 2 runtime use. sdkconfig documents this correctly. |
| SCAF-04     | 01-01       | `partitions.csv` defines NVS partition of at least 64KB                                  | SATISFIED | NVS at 0x9000, size 0x10000 (exactly 64KB). Custom partition table enabled via sdkconfig.            |
| SCAF-05     | 01-02       | Device ID module reads hardware eFuse/MAC at runtime and returns a stable unique string  | SATISFIED | Hardware verified: `Device ID: FFFEB5` printed on boot. Stable across two power cycles (human checkpoint in plan 01-02). |

**Note on SCAF-03 (UART RX buffer):** The requirement literally says "sdkconfig.defaults sets UART RX ring buffer to 4096+ bytes." In ESP-IDF v5.x there is no Kconfig option for UART RX ring buffer size — it is a runtime-only parameter passed to `uart_driver_install()`. The implementation correctly addresses the intent: `UART_RX_BUF_SIZE = 4096` is defined in `src/config.rs` for Phase 2 to consume, and `sdkconfig.defaults` documents this design decision with an explanatory comment. This is the architecturally correct approach for ESP-IDF v5, and the requirement's intent (ensuring >=4096 bytes is specified and available for Phase 2) is fully met.

**Note on REQUIREMENTS.md typo:** SCAF-01 in REQUIREMENTS.md reads `riscv32imc-esp-espidf` (missing `a`). The correct triple for ESP32-C6 is `riscv32imac-esp-espidf`, which is what `.cargo/config.toml` and all implementation files correctly use. This is a documentation typo, not an implementation defect. No impact on goal achievement.

**Orphaned requirements:** None. All five Phase 1 requirements (SCAF-01 through SCAF-05) are claimed by plans 01-01 and 01-02 and verified.

---

### Anti-Patterns Found

| File              | Line | Pattern                             | Severity | Impact                                                                 |
| ----------------- | ---- | ----------------------------------- | -------- | ---------------------------------------------------------------------- |
| `src/main.rs`     | 8    | `use esp_idf_svc::hal::prelude::*;` | Info     | Unused import in Phase 1 (Peripherals trait not used). Will be needed in Phase 2. May generate compiler warning but does not affect goal. |
| `src/config.rs`   | 9-16 | Empty string constants              | Info     | Intentional Phase 2 stubs, documented as such. Not a defect.          |
| `build.rs`        | 9-11 | Comment: "first cargo build may fail" | Info   | Documents known limitation of Windows embuild workaround. Honest documentation of a quirk, not a hidden defect. |

No blocker or warning anti-patterns found.

---

### Human Verification Completed

The plan 01-02 included a `checkpoint:human-verify` gate (Task 2) that was completed:

**Test: Stable device ID across power cycles**
- Device ID `FFFEB5` observed on boot via serial monitor
- Same ID confirmed after two consecutive power cycles
- No panic or watchdog errors in boot log
- Human approval recorded in plan 01-02 checkpoint: `approved`

Serial output confirmed:
```
I (278) esp32_gnssmqtt: === esp32-gnssmqtt booting ===
I (288) esp32_gnssmqtt: Device ID: FFFEB5
I (288) esp32_gnssmqtt: Build: esp32-gnssmqtt 0.1.0
I (5298) esp32_gnssmqtt: Heartbeat — Device ID: FFFEB5
```

---

### Deviations From Plan (Resolved by Implementation)

The following deviations were discovered during plan execution and correctly fixed:

1. **partitions.csv factory size:** Changed from 0xF0000 (960KB) to 0x3E0000 (3.875MB) to cover 4MB flash device. Commit 48280b6.
2. **sdkconfig.defaults missing `CONFIG_PARTITION_TABLE_CUSTOM=y`:** Added. Commit 48280b6.
3. **sdkconfig.defaults missing `CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y`:** Added (ESP-IDF defaults to 2MB). Commits 48280b6, 4386e8a.
4. **build.rs partitions copy for Windows:** embuild cannot create symlinks without Developer Mode; file copy workaround added. Commit 035958e.

All four deviations are committed, present in the codebase, and verified correct.

---

## Summary

**Phase goal achieved.** The Rust firmware scaffold for the ESP32-C6 compiles cleanly (8.77 MB binary artifact, Cargo.lock resolved), passes all structural checks (pinned crates, correct target triple, partition table, sdkconfig), and has been verified booting on real hardware with a stable eFuse-derived device ID (`FFFEB5`) across power cycles.

All 5 SCAF requirements are satisfied. No gaps remain. Phase 2 (Connectivity) can proceed.

**Phase 2 handoff data:**
- Device ID: `FFFEB5`
- MQTT topic prefix: `gnss/FFFEB5/`
- Config stubs ready: `src/config.rs` — `WIFI_SSID`, `WIFI_PASS`, `MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`, `MQTT_PASS`, `UART_RX_BUF_SIZE`

---

_Verified: 2026-03-03_
_Verifier: Claude (gsd-verifier)_
