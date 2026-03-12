---
phase: 25-web-ui-remaining-gap-crate-skeletons
plan: 03
subsystem: infra
tags: [no_std, embedded, gap-crates, embassy, esp32, softap, dns, logging]

# Dependency graph
requires:
  - phase: 24-rinex-files-gnss-ota-gap-crate
    provides: gnss-ota gap crate template and BLOCKER.md structure
provides:
  - gnss-softap crate: SoftApPortal trait + ProvisioningCredentials struct
  - gnss-dns crate: CaptiveDnsResponder trait
  - gnss-log crate: LogHook, LogSink traits and LogLevel enum
  - BLOCKER.md for each crate documenting the specific no_std gap
affects:
  - future embassy port of provisioning.rs (SoftAP + DNS)
  - future embassy port of log_relay.rs (LogHook)
  - NOSTD-04b requirement closure

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Gap crate skeleton: #![no_std] trait-only lib.rs with zero external deps + BLOCKER.md"
    - "BLOCKER.md structure: background, per-blocker sections (status/tracking/impact), recommended path"

key-files:
  created:
    - crates/gnss-softap/Cargo.toml
    - crates/gnss-softap/src/lib.rs
    - crates/gnss-softap/BLOCKER.md
    - crates/gnss-dns/Cargo.toml
    - crates/gnss-dns/src/lib.rs
    - crates/gnss-dns/BLOCKER.md
    - crates/gnss-log/Cargo.toml
    - crates/gnss-log/src/lib.rs
    - crates/gnss-log/BLOCKER.md
  modified: []

key-decisions:
  - "gnss-softap BLOCKER: WPA2 password gap resolved in esp-radio 0.16.x; active gap is no_std HTTP server with multi-field form POST parsing (picoserve maturity)"
  - "gnss-dns: classified SOLVABLE — no turnkey crate exists but ~50 lines of embassy-net UDP + DNS response construction is sufficient; no fundamental blocker"
  - "gnss-log: Rust log::Log side is portable no_std today (no blocker); C component capture requires one C FFI call to esp_log_set_vprintf (not pure-Rust but not a fundamental impossibility)"

patterns-established:
  - "Gap crate: trait-only #![no_std] skeleton with no external deps — crates/* auto-included via workspace members = ['crates/*']"
  - "BLOCKER.md: distinguish RESOLVED / ACTIVE / SOLVABLE / PARTIAL BLOCKER clearly per gap item"

requirements-completed: [NOSTD-04b]

# Metrics
duration: 3min
completed: 2026-03-12
---

# Phase 25 Plan 03: gnss-softap, gnss-dns, gnss-log Gap Crate Skeletons Summary

**Three no_std trait-only gap crates documenting SoftAP portal, captive DNS, and MQTT log hook gaps for the embassy/no_std migration path**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-12T09:19:27Z
- **Completed:** 2026-03-12T09:22:56Z
- **Tasks:** 2
- **Files modified:** 9 (all created)

## Accomplishments

- Created `gnss-softap` with `SoftApPortal` trait and `ProvisioningCredentials` struct; BLOCKER.md correctly identifies WPA2 as resolved in esp-radio 0.16.x and multi-field HTTP form parsing as the active gap
- Created `gnss-dns` with `CaptiveDnsResponder` trait; BLOCKER.md classifies gap as SOLVABLE (no turnkey crate, ~50 lines needed, embassy-net UDP is production-ready)
- Created `gnss-log` with `LogHook`, `LogSink`, and `LogLevel`; BLOCKER.md distinguishes Rust log::Log side (portable, no blocker) from C component capture (one FFI call required)
- All three crates compile for `thumbv7em-none-eabihf`; `cargo clippy -- -D warnings` clean

## Task Commits

Each task was committed atomically:

1. **Task 1: gnss-softap and gnss-dns gap crate skeletons** - `3ef66af` (feat)
2. **Task 2: gnss-log gap crate skeleton** - `f58914e` (feat)

## Files Created/Modified

- `crates/gnss-softap/Cargo.toml` - Package declaration, no external deps
- `crates/gnss-softap/src/lib.rs` - SoftApPortal trait, ProvisioningCredentials struct
- `crates/gnss-softap/BLOCKER.md` - WPA2 resolved; HTTP form parsing is active gap
- `crates/gnss-dns/Cargo.toml` - Package declaration, no external deps
- `crates/gnss-dns/src/lib.rs` - CaptiveDnsResponder trait
- `crates/gnss-dns/BLOCKER.md` - SOLVABLE; ~50 lines of DNS response construction
- `crates/gnss-log/Cargo.toml` - Package declaration, no external deps
- `crates/gnss-log/src/lib.rs` - LogHook, LogSink traits and LogLevel enum
- `crates/gnss-log/BLOCKER.md` - Rust side portable; C capture requires one FFI call

## Decisions Made

- gnss-softap BLOCKER.md captures WPA2 as RESOLVED (esp-radio 0.16.x) and HTTP form parsing as the remaining ACTIVE gap (picoserve maturity unconfirmed; manual ~50-line fallback feasible)
- gnss-dns classified SOLVABLE explicitly — the distinction from "blocked" is important for planning; the gap is crate availability, not implementation feasibility
- gnss-log BLOCKER.md splits the gap in two: Rust log::Log (no blocker today) vs. C component capture (one FFI call, not pure-Rust but not impossible) — the recommended path adds a `c-log-capture` feature gate

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- NOSTD-04b complete: all three remaining gap crates (gnss-softap, gnss-dns, gnss-log) have trait skeletons and BLOCKER.md documentation
- Phase 25 gap crate work is done; these crates will receive implementations as the embassy ecosystem matures
- When picoserve form parsing is confirmed production-ready, gnss-softap-embassy backend can be implemented against the SoftApPortal trait

---
*Phase: 25-web-ui-remaining-gap-crate-skeletons*
*Completed: 2026-03-12*
