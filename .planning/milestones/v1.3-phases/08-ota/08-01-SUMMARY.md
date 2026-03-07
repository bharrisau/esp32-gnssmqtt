---
phase: 08-ota
plan: 01
subsystem: infra
tags: [esp32, ota, partition-table, sha2, rollback, watchdog, espflash]

# Dependency graph
requires:
  - phase: 07-rtcm-relay
    provides: Working MQTT+UART pipeline on factory partition layout
provides:
  - Dual-slot OTA partition table (otadata + ota_0 + ota_1 each 1.875MB)
  - Bootloader rollback enabled via CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
  - Task watchdog extended to 30s for OTA partition erase survival
  - sha2 crate dependency for streaming SHA-256 OTA verification
affects:
  - 08-02 (OTA implementation — depends on partition layout and sha2 crate)

# Tech tracking
tech-stack:
  added:
    - sha2 = "0.10" (pure Rust no_std SHA-256, RustCrypto)
    - block-buffer, crypto-common, digest, generic-array, typenum, const-oid (sha2 transitive deps)
  patterns:
    - Dual-slot OTA partition layout: nvs(0x9000) + otadata(0xF000) + ota_0(0x20000) + ota_1(0x200000)
    - sdkconfig.defaults as the source of truth for all Kconfig overrides including OTA rollback

key-files:
  created: []
  modified:
    - partitions.csv
    - sdkconfig.defaults
    - Cargo.toml

key-decisions:
  - "Removed phy_init partition — ESP-IDF v5 embeds phy calibration in NVS; not required on ESP32-C6"
  - "sha2 = { version = 0.10, default-features = false, features = [oid] } — oid feature benign; drop if linker rejects"
  - "CONFIG_ESP_TASK_WDT_TIMEOUT_S=30 chosen over runtime TWDT subscribe/feed — simpler and sufficient for single OTA thread"
  - "nvs shrunk from 0x10000 to 0x6000 to make space for otadata at 0xF000 — WiFi creds compiled in config.rs so NVS wipe on reflash is safe"

patterns-established:
  - "OTA partition math: otadata must be 4KB-aligned, 8KB size (two 4KB OTA select entries)"
  - "espflash erase-flash required before new partition table takes effect — existing factory partition blocks bootloader from recognizing OTA slots"

requirements-completed:
  - OTA-01

# Metrics
duration: 8min
completed: 2026-03-07
---

# Phase 8 Plan 01: OTA Prerequisites Summary

**Dual-slot OTA partition table (otadata + ota_0 + ota_1, 1.875MB each) plus rollback config and sha2 crate — firmware compiles clean and device boots cleanly from ota_0 after USB reflash**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-03-07T04:00:00Z
- **Completed:** 2026-03-07T04:06:16Z
- **Tasks:** 3 of 3 complete
- **Files modified:** 3

## Accomplishments

- Replaced factory-only partition table with dual-slot OTA layout fitting within 4MB flash
- Added `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` and 30s watchdog extension to sdkconfig.defaults
- Added `sha2 = "0.10"` pure-Rust SHA-256 crate dependency; `cargo build --release` succeeds with zero errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Redesign partitions.csv for dual-slot OTA layout** - `aee957e` (feat)
2. **Task 2: Add rollback config + watchdog extension + sha2 dependency** - `dbd3ee1` (feat)
3. **Task 3: Checkpoint — Verify hardware boots cleanly from ota_0 after USB reflash** - human-verified (approved)

## Files Created/Modified

- `partitions.csv` - Replaced factory-only layout with otadata(0xF000) + ota_0(0x20000/1.875MB) + ota_1(0x200000/1.875MB)
- `sdkconfig.defaults` - Added CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y and CONFIG_ESP_TASK_WDT_TIMEOUT_S=30
- `Cargo.toml` - Added sha2 = "0.10" with default-features = false

## Decisions Made

- Removed `phy_init` partition row — ESP-IDF v5 embeds phy calibration data in NVS; a separate phy partition is not required on ESP32-C6
- NVS shrunk from 0x10000 to 0x6000 to make room for otadata at 0xF000 — WiFi credentials are compiled in via config.rs constants so NVS wipe on reflash is safe
- `sha2 = { version = "0.10", default-features = false, features = ["oid"] }` — oid feature is no_std-compatible; plan notes to drop it if linker rejects
- `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` chosen as simpler alternative to runtime TWDT subscribe/feed pattern; handles the 4-8s OTA partition erase window

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None — `cargo build --release` succeeded on first attempt with sha2 and the sdkconfig additions.

## User Setup Required

**Hardware checkpoint completed.** User confirmed device boots cleanly from ota_0 after `espflash erase-flash` + `espflash flash --release --monitor`. Serial log showed clean boot, WiFi connected, MQTT connected, no watchdog panic.

## Next Phase Readiness

Hardware checkpoint passed — device confirmed booting cleanly from ota_0:
- Phase 08-02 can implement `ota.rs` module (OTA download + write + SHA-256 verification loop)
- `mark_running_slot_valid()` must be called early in `main()` after MQTT connects — documented as Phase 8 pitfall in STATE.md
- OTA thread must run independently of MQTT pump — documented pattern in RESEARCH.md

## Self-Check: PASSED

- partitions.csv: FOUND
- sdkconfig.defaults: FOUND
- Cargo.toml: FOUND
- 08-01-SUMMARY.md: FOUND
- Task 1 commit aee957e: FOUND
- Task 2 commit dbd3ee1: FOUND

---
*Phase: 08-ota*
*Completed: 2026-03-07*
