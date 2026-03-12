---
phase: 22-workspace-nostd-audit
plan: "01"
subsystem: infra
tags: [cargo-workspace, resolver-2, firmware, embedded, riscv32imac-esp-espidf, gnss-server]

# Dependency graph
requires: []
provides:
  - Cargo workspace root with resolver=2, members: firmware, gnss-server, crates/*
  - firmware/ package (esp32-gnssmqtt-firmware) with all source and embedded build config
  - gnss-server/ stub package building for host target
  - firmware/.cargo/config.toml scoping embedded target to firmware/ only
  - crates/ directory ready for future gap crates (gnss-nvs, gnss-ota, etc.)
affects: [23-mqtt-rtcm3-gnss-nvs, 24-rinex-gnss-ota, 25-webui-gap-skeletons]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Cargo workspace with resolver=2 preventing std feature unification into no_std crates"
    - "Embedded target scoped to firmware/.cargo/config.toml; workspace root has no build.target"
    - "panic=abort via -C panic=abort rustflag in firmware/.cargo/config.toml (not via cargo-features since member profiles are ignored by workspace builds)"

key-files:
  created:
    - Cargo.toml (workspace root — replaces former package root)
    - firmware/Cargo.toml (package: esp32-gnssmqtt-firmware)
    - firmware/.cargo/config.toml (embedded target + rustflags including -C panic=abort)
    - firmware/rust-toolchain.toml (copy of root; nightly for firmware/ context)
    - gnss-server/Cargo.toml
    - gnss-server/src/main.rs
    - crates/.gitkeep
  modified:
    - .cargo/config.toml (removed build.target; workspace root is now target-agnostic)
    - rust-toolchain.toml (preserved at workspace root; firmware/ also has copy)

key-decisions:
  - "panic=immediate-abort replaced with -C panic=abort rustflag in firmware/.cargo/config.toml — Cargo silently ignores [profile] sections in workspace member packages; panic=immediate-abort cargo-feature cannot be scoped per-package in workspace profiles"
  - "Profiles ([profile.release], [profile.dev]) moved to workspace root without panic setting — opt-level=s for release, opt-level=z for dev apply workspace-wide"
  - "rust-toolchain.toml kept at workspace root (nightly) — required because firmware/Cargo.toml was previously the root and nightly drove all builds; with workspace restructure, workspace root needs nightly to handle any nightly-only build scenarios"
  - "Firmware builds from firmware/ directory — .cargo/config.toml with [unstable] build-std is only read when cargo is invoked from firmware/ or a subdirectory; from workspace root, pass --target riscv32imac-esp-espidf explicitly"

patterns-established:
  - "Workspace build for server: cargo build -p gnss-server (host target, no --target flag)"
  - "Workspace build for firmware: cd firmware && cargo check (uses firmware/.cargo/config.toml)"
  - "Or from root: cargo check -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf (build-std must then be passed via -Z flag or via nightly from firmware/)"

requirements-completed: [INFRA-01]

# Metrics
duration: 10min
completed: "2026-03-12"
---

# Phase 22 Plan 01: Workspace Restructure Summary

**Cargo workspace established with firmware/ and gnss-server/ members, resolver=2, embedded target scoped to firmware/.cargo/config.toml, and panic=abort delivered via rustflag rather than cargo-features**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-03-12T02:24:01Z
- **Completed:** 2026-03-12T02:34:45Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- All firmware source (20 .rs files + C shim) moved from repo root into firmware/ via git mv with history preserved
- Workspace root Cargo.toml with resolver=2 and three members (firmware, gnss-server, crates/*)
- gnss-server stub compiles cleanly for host target (cargo build -p gnss-server)
- firmware builds from firmware/ directory using .cargo/config.toml embedded settings (5m 36s, used cached ESP-IDF artefacts)
- Root .cargo/config.toml cleared of build.target; embedded target only active when building inside firmware/

## Task Commits

Each task was committed atomically:

1. **Task 1: Create workspace scaffolding** - `e943bb8` (chore)
2. **Task 2: Move firmware files into firmware/** - `e47a811` (feat)
3. **Task 3: Build verification + deviation fixes** - `7666a6f` (fix)

## Files Created/Modified

- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/Cargo.toml` - Workspace root definition (resolver=2, profiles, workspace.dependencies)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/.cargo/config.toml` - Cleared of build.target (comments only)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/.cargo/config.toml` - Embedded target config + -C panic=abort rustflag
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/Cargo.toml` - Package definition (name=esp32-gnssmqtt-firmware)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/src/` - All 20 firmware source files (moved via git mv)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/build.rs` - Moved via git mv
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/partitions.csv` - Moved via git mv
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/sdkconfig.defaults` - Moved via git mv
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/espflash.toml` - Moved via git mv
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/firmware/rust-toolchain.toml` - Moved via git mv (copy remains at root)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/gnss-server/Cargo.toml` - Server stub package
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/gnss-server/src/main.rs` - Empty binary stub
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/crates/.gitkeep` - Placeholder for future gap crates

## Decisions Made

- **panic=immediate-abort replaced with -C panic=abort rustflag:** Cargo workspace builds ignore `[profile]` sections in member packages. The `cargo-features = ["panic-immediate-abort"]` only works when the package is the root. Moving to workspace means profiles must be at workspace root, but `panic = "immediate-abort"` cannot be scoped per-package (Cargo rejects it in package overrides) and would break gnss-server host builds if set globally. Solution: `-C panic=abort` rustflag in firmware/.cargo/config.toml achieves the same abort-on-panic codegen for the embedded target only.

- **Firmware builds from firmware/ directory:** The `[unstable] build-std` setting in `.cargo/config.toml` is only read when cargo runs from inside the directory containing that config file (or a subdirectory). When running from workspace root, `firmware/.cargo/config.toml` is not applied. This is documented Cargo behaviour. Users building firmware should `cd firmware && cargo build` or `cargo check`. The plan anticipated this possibility and noted it in Task 3.

- **rust-toolchain.toml kept at workspace root:** Previously the nightly toolchain drove all builds from the root package. With workspace restructure, the workspace root still needs nightly to ensure Cargo commands from root (like `cargo metadata` or `cargo build -p gnss-server`) use the same toolchain. A copy is also in firmware/ for when building from that directory.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Workspace member profiles ignored by Cargo — panic=abort moved to rustflag**
- **Found during:** Task 3 (build verification)
- **Issue:** `cargo build -p gnss-server` failed with "panic strategy incompatible with immediate-abort" because workspace-level `panic = "immediate-abort"` profile applied to host builds. Cargo does not support `panic` in per-package profile overrides. Member package `[profile]` sections are silently ignored.
- **Fix:** Removed `cargo-features = ["panic-immediate-abort"]` and `panic = "immediate-abort"` from workspace Cargo.toml profiles. Added `-C panic=abort` to `rustflags` in `firmware/.cargo/config.toml` (applies only to embedded target builds from firmware/ directory).
- **Files modified:** Cargo.toml, firmware/Cargo.toml, firmware/.cargo/config.toml
- **Verification:** `cargo build -p gnss-server` completes with "Finished"; firmware `cargo check` from firmware/ completes with "Finished"
- **Committed in:** 7666a6f (Task 3 commit)

**2. [Rule 3 - Blocking] rust-toolchain.toml required at workspace root for nightly toolchain**
- **Found during:** Task 2 (cargo metadata verification)
- **Issue:** After git mv of rust-toolchain.toml to firmware/, workspace root defaulted to stable toolchain. `cargo metadata` failed with "the cargo feature panic-immediate-abort requires a nightly version of Cargo" (before the panic-immediate-abort approach was dropped).
- **Fix:** Kept rust-toolchain.toml at workspace root (the Write tool recreated it after git mv). Both root and firmware/ now have the same nightly toolchain spec.
- **Files modified:** rust-toolchain.toml (root, recreated)
- **Committed in:** e47a811 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2x Rule 3 blocking)
**Impact on plan:** Both fixes required for correct workspace operation. No scope creep. The panic=abort semantic outcome is preserved; only the mechanism changed from cargo-features to rustflag.

## Issues Encountered

- The plan's `firmware/Cargo.toml` template included `cargo-features = ["panic-immediate-abort"]` and `[profile.*]` sections. These are incompatible with workspace member builds: member profiles are ignored, and `panic-immediate-abort` in a member manifest has no effect on workspace builds but causes parse errors if the workspace root lacks the cargo-feature declaration. Resolved by removing both from firmware/Cargo.toml and restructuring workspace profiles.

## User Setup Required

None - no external service configuration required. Building firmware still requires the ESP-IDF toolchain installed (unchanged from pre-restructure).

## Next Phase Readiness

- Workspace structure is ready for Phase 23 (MQTT + RTCM3 + gnss-nvs)
- gnss-server/ stub is in place for server implementation starting Phase 23
- crates/ directory ready for gnss-nvs gap crate in Phase 23
- Cargo workspace is validated: both members resolve, firmware check completes, server builds
- resolver=2 confirmed active to prevent std feature leakage into future no_std gap crates

---
*Phase: 22-workspace-nostd-audit*
*Completed: 2026-03-12*
