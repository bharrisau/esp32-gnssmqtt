---
phase: 01-scaffold
plan: 02
subsystem: infra
tags: [rust, esp32c6, esp-idf, espflash, hardware, efuse, device-id, partitions, sdkconfig]

# Dependency graph
requires:
  - phase: 01-scaffold/01-01
    provides: "Compilable firmware binary at target/riscv32imac-esp-espidf/debug/esp32-gnssmqtt"
provides:
  - "Hardware-verified firmware boot on physical XIAO ESP32-C6"
  - "Confirmed device ID FFFEB5 — stable across power cycles, eFuse-derived, permanent"
  - "4 scaffold defects diagnosed and fixed: partition size, sdkconfig flash config, build.rs Windows symlink workaround"
  - "Phase 1 complete: all SCAF-01 through SCAF-05 requirements satisfied"
affects: [02-wifi-mqtt, 03-ble-provisioning]

# Tech tracking
tech-stack:
  added:
    - "espflash (firmware flash + serial monitor via cargo run)"
  patterns:
    - "partitions.csv factory partition must cover the full remaining flash (4MB device: 0x20000 base + 0x3E0000 size = 4MB)"
    - "CONFIG_PARTITION_TABLE_CUSTOM=y required in sdkconfig.defaults when using custom partitions.csv"
    - "CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y required; ESP-IDF defaults to 2MB detection which causes flash overflow"
    - "build.rs must copy partitions.csv to ESP-IDF cmake source directory on Windows (no symlinks without Developer Mode)"

key-files:
  created:
    - ".planning/phases/01-scaffold/01-02-SUMMARY.md - this file"
  modified:
    - "partitions.csv - factory partition size corrected from 0xF0000 (960KB) to 0x3E0000 (3.875MB)"
    - "sdkconfig.defaults - added CONFIG_PARTITION_TABLE_CUSTOM=y and CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y"
    - "build.rs - added Windows-compatible file copy of partitions.csv into ESP-IDF cmake source directory"

key-decisions:
  - "Device ID FFFEB5 confirmed as this hardware unit's permanent identifier — derived from eFuse, not generated"
  - "Factory partition must extend to end of flash minus NVS/phy_init; original scaffold used undersized 960KB which caused espflash flash overflow error"
  - "CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y required because ESP-IDF autodetect defaults to 2MB and produces binary larger than detected flash"
  - "Windows embuild cannot create symlinks to partitions.csv without Developer Mode enabled — build.rs file copy is the correct workaround"

patterns-established:
  - "Hardware verification checkpoint: always flash and power-cycle before declaring Phase 1 complete"
  - "Device ID is the canonical identifier for Phase 2 MQTT topic paths: gnss/FFFEB5/heartbeat, gnss/FFFEB5/status, etc."

requirements-completed: [SCAF-05]

# Metrics
duration: ~30min (hardware flash + power-cycle verification)
completed: 2026-03-03
---

# Phase 1 Plan 02: Hardware Flash and Device ID Verification SUMMARY

**ESP32-C6 firmware flashed and verified on physical hardware — device ID FFFEB5 stable across power cycles, 4 scaffold defects fixed to enable successful flash**

## Performance

- **Duration:** ~30 min (flash, fix scaffold issues, power-cycle verification)
- **Started:** 2026-03-03 (checkpoint approval)
- **Completed:** 2026-03-03
- **Tasks:** 2
- **Files modified:** 3 (partitions.csv, sdkconfig.defaults, build.rs)

## Accomplishments

- Firmware flashed successfully to the physical XIAO ESP32-C6 via `cargo run` (espflash flash + monitor)
- Serial output confirmed: device ID `FFFEB5` printed on boot, heartbeat every 5 seconds
- Device ID `FFFEB5` verified stable across two consecutive power cycles — eFuse-derived, permanent
- 4 scaffold defects diagnosed and fixed during flash attempt (see Deviations)
- Phase 1 complete: all requirements SCAF-01 through SCAF-05 satisfied

## Confirmed Hardware Output

```
I (278) esp32_gnssmqtt: === esp32-gnssmqtt booting ===
I (288) esp32_gnssmqtt: Device ID: FFFEB5
I (288) esp32_gnssmqtt: Build: esp32-gnssmqtt 0.1.0
I (5298) esp32_gnssmqtt: Heartbeat — Device ID: FFFEB5
```

**Device ID: FFFEB5** — This is the permanent identifier for this hardware unit. It will appear in all Phase 2 MQTT topic paths (e.g., `gnss/FFFEB5/heartbeat`, `gnss/FFFEB5/status`).

## Task Commits

This plan was a human-checkpoint plan — no automated task commits were created by the executor. The scaffold fixes were committed as part of the checkpoint resolution.

**Plan metadata commit:** (recorded in final commit below)

## Files Created/Modified

- `partitions.csv` - Factory partition size corrected from 0xF0000 (960KB) to 0x3E0000 (3.875MB)
- `sdkconfig.defaults` - Added `CONFIG_PARTITION_TABLE_CUSTOM=y` and `CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y`
- `build.rs` - Added Windows-compatible file copy of `partitions.csv` to ESP-IDF cmake source directory

## Decisions Made

- **Device ID FFFEB5 is this device's permanent identifier:** eFuse OTP values are written at manufacturing and cannot change; the same ID will appear on every boot for the lifetime of this hardware unit
- **Factory partition extent:** Must cover remaining flash from 0x20000 to end of flash (4MB device = 0x3E0000 bytes of factory space). The scaffold plan specified 0xF0000 (960KB) which was insufficient for the compiled binary
- **CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y required:** ESP-IDF defaults to 2MB flash detection even on 4MB devices; without this, espflash rejects the binary as too large for the detected flash
- **Windows build.rs file copy:** embuild generates a CMakeLists.txt that references partitions.csv; on Windows without Developer Mode, symlinks fail with permission errors. The fix copies the file during build instead

## Deviations from Plan

### Auto-fixed Issues (resolved during human checkpoint)

**1. [Rule 1 - Bug] partitions.csv factory partition size was 0xF0000 (960KB) — too small for debug binary**
- **Found during:** Task 1 (Flash firmware to hardware)
- **Issue:** espflash rejected the flash image with an overflow error — the debug binary exceeded the 960KB factory partition. The original scaffold value was a placeholder that was never validated against actual binary size.
- **Fix:** Changed factory partition size from `0xF0000` to `0x3E0000` (3.875MB) in `partitions.csv`, covering the full remaining flash on the 4MB device.
- **Files modified:** `partitions.csv`
- **Verification:** espflash accepted the image and flashed successfully

**2. [Rule 2 - Missing Critical] sdkconfig.defaults missing CONFIG_PARTITION_TABLE_CUSTOM=y**
- **Found during:** Task 1 (Flash firmware to hardware)
- **Issue:** Without this key, ESP-IDF ignores the custom `partitions.csv` and falls back to the default partition table, which has no NVS at the standard offset and no factory app at 0x20000.
- **Fix:** Added `CONFIG_PARTITION_TABLE_CUSTOM=y` to `sdkconfig.defaults`
- **Files modified:** `sdkconfig.defaults`
- **Verification:** Build used correct partition table; boot log showed no partition errors

**3. [Rule 2 - Missing Critical] sdkconfig.defaults missing CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y**
- **Found during:** Task 1 (Flash firmware to hardware)
- **Issue:** ESP-IDF autodetects flash size but defaults to 2MB if detection is ambiguous. The XIAO ESP32-C6 has 4MB flash; without this setting, espflash computed the partition end exceeded the "detected" 2MB limit and refused to flash.
- **Fix:** Added `CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y` to `sdkconfig.defaults`
- **Files modified:** `sdkconfig.defaults`
- **Verification:** espflash detected 4MB flash and accepted the image

**4. [Rule 3 - Blocking] build.rs needed to copy partitions.csv on Windows (no symlinks without Developer Mode)**
- **Found during:** Task 1 (Flash firmware to hardware)
- **Issue:** embuild generates CMakeLists.txt referencing `partitions.csv` via a path that requires the file to exist in the ESP-IDF cmake source directory. On Windows without Developer Mode enabled, creating symlinks fails with `ERROR_PRIVILEGE_NOT_HELD`. The build failed at the cmake configuration step.
- **Fix:** Added file copy logic to `build.rs` that copies `partitions.csv` from the project root into the ESP-IDF cmake source directory during the build script phase.
- **Files modified:** `build.rs`
- **Verification:** `cargo build` and `cargo run` succeeded without symlink errors

---

**Total deviations:** 4 (1 bug fix, 2 missing critical configs, 1 blocking Windows build issue)
**Impact on plan:** All 4 fixes were necessary for the firmware to flash. None add scope — they correct the scaffold's incomplete hardware configuration. A fresh clone on Windows will work correctly after these fixes.

## Issues Encountered

- espflash flash overflow on initial attempt: resolved by correcting factory partition size to cover full 4MB flash
- Custom partition table not being used: resolved by adding CONFIG_PARTITION_TABLE_CUSTOM=y
- Flash size mismatch causing binary rejection: resolved by adding CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y
- Windows symlink permission error in embuild cmake step: resolved by file copy in build.rs

## User Setup Required

None. All fixes are committed to the repository; `cargo run` works on Windows without Developer Mode after these changes.

## Next Phase Readiness

- **Phase 2 (Connectivity) can begin:** Device boots correctly, device ID `FFFEB5` is confirmed, hardware is operational
- **MQTT topic paths for Phase 2:** Use `gnss/FFFEB5/heartbeat`, `gnss/FFFEB5/status`, `gnss/FFFEB5/nmea/{TYPE}` as the topic templates
- **config.rs stubs ready:** WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_PASS are defined as empty strings waiting for Phase 2 values
- **No blockers:** All Phase 1 requirements met; hardware verified working

---
*Phase: 01-scaffold*
*Completed: 2026-03-03*

## Self-Check: PASSED

- FOUND: .planning/phases/01-scaffold/01-02-SUMMARY.md
- FOUND: commit c1cbd42 (SUMMARY.md)
- FOUND: commit a72a963 (STATE.md + ROADMAP.md + REQUIREMENTS.md)
- FOUND: SCAF-05 checked off in REQUIREMENTS.md
- FOUND: Phase 1 marked complete in ROADMAP.md (2/2 plans)
- FOUND: STATE.md position advanced to Phase 2 ready
