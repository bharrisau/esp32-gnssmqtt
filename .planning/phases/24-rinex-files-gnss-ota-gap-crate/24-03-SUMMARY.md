---
phase: 24-rinex-files-gnss-ota-gap-crate
plan: "03"
subsystem: infra
tags: [nostd, ota, embedded, traits, gap-crate, esp32-c6, esp-hal]

# Dependency graph
requires:
  - phase: 22-workspace-nostd-audit
    provides: workspace restructure with crates/* glob, resolver="2"
provides:
  - OtaSlot trait with capacity, erase, write_chunk, verify_crc32, set_as_boot_target methods
  - OtaManager trait with booted_slot_index and inactive_slot methods
  - BLOCKER.md documenting esp-storage#3259 and esp-hal-ota C6 limitations
  - gnss-ota crate compiling as no_std against bare-metal thumbv7em-none-eabihf target
affects: [25-web-ui-gap-skeletons, future-nostd-ota-implementation]

# Tech tracking
tech-stack:
  added: []
  patterns: [gap-crate-trait-only-no-external-deps, no_std-trait-definitions-with-core-types]

key-files:
  created:
    - crates/gnss-ota/Cargo.toml
    - crates/gnss-ota/src/lib.rs
    - crates/gnss-ota/BLOCKER.md
  modified: []

key-decisions:
  - "gnss-ota is trait-only with no external dependencies — implementations are feature-gated additions for later phases"
  - "OtaSlot.write_chunk takes offset parameter to support non-sequential write recovery, even though flash constrains sequential writes in practice"
  - "BLOCKER.md references specific GitHub issues and assesses esp-hal-ota on three axes: C6 untested, pointer magic from ESP-IDF internals, no embedded-storage abstraction"

patterns-established:
  - "Gap crate pattern: no_std trait definitions with zero external dependencies — same pattern as gnss-nvs"
  - "BLOCKER.md pattern: date-stamped engineering record with specific issue tracker links and recommended path forward"

requirements-completed: [NOSTD-04a]

# Metrics
duration: 2min
completed: 2026-03-12
---

# Phase 24 Plan 03: gnss-ota Gap Crate Summary

**no_std OTA trait interface (OtaSlot + OtaManager) with date-stamped BLOCKER.md citing esp-storage#3259 and esp-hal-ota C6 gaps**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-12T07:26:16Z
- **Completed:** 2026-03-12T07:28:15Z
- **Tasks:** 1
- **Files modified:** 3 created

## Accomplishments

- Created gnss-ota crate with OtaSlot and OtaManager trait definitions, zero external dependencies
- Verified no_std compilation against thumbv7em-none-eabihf (bare-metal Cortex-M4)
- Documented two concrete blockers preventing nostd OTA on ESP32-C6 with GitHub issue references

## Task Commits

Each task was committed atomically:

1. **Task 1: gnss-ota crate skeleton — trait + BLOCKER.md** - `6b1e46a` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `crates/gnss-ota/Cargo.toml` - Crate manifest, no external deps, workspace member via crates/* glob
- `crates/gnss-ota/src/lib.rs` - #![no_std] OtaSlot and OtaManager trait definitions using core::fmt::Debug
- `crates/gnss-ota/BLOCKER.md` - Engineering record: esp-storage lacks partition table API (esp-rs/esp-hal#3259); esp-hal-ota untested on C6 and uses ESP-IDF internal struct layouts

## Decisions Made

- Trait-only crate with no external dependencies, following the same gap crate pattern as gnss-nvs
- OtaSlot.write_chunk takes `offset: usize` to make the interface explicit about byte positioning
- BLOCKER.md structured with three issues for esp-hal-ota: C6 untested, pointer magic risk, no embedded-storage abstraction

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- gnss-ota crate is a workspace member, compiles no_std clean, and defines the stable trait interface for future implementation
- Phase 25 (web-ui + gap skeletons) can reference these traits as the OTA abstraction layer
- Actual implementation awaits esp-rs/esp-hal#3259 (partition table API in esp-storage)

---
*Phase: 24-rinex-files-gnss-ota-gap-crate*
*Completed: 2026-03-12*
