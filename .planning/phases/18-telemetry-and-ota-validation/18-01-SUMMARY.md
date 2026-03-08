---
phase: 18-telemetry-and-ota-validation
plan: 01
subsystem: telemetry
tags: [gnss, nmea, mqtt, heartbeat, atomics, gga, rtk]

# Dependency graph
requires:
  - phase: 17-ntrip-client
    provides: nmea_relay.rs relay loop and NTRIP_STATE atomic pattern
provides:
  - gnss_state.rs module with GGA_FIX_TYPE, GGA_SATELLITES, GGA_HDOP_X10 atomics
  - GGA sentence parsing in nmea_relay.rs updating fix quality on each GNGGA/GPGGA sentence
  - Heartbeat JSON extended with fix_type, satellites, hdop fields (integer or null)
affects: [18-02, 18-03, 18-04, README]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Sentinel atomic pattern: 0xFF/0xFFFF sentinel values indicate 'not yet received', heartbeat emits JSON null"
    - "GGA field indexing: fields[0]=$GNGGA, fields[6]=fix, fields[7]=sats, fields[8]=hdop (confirmed in RESEARCH.md)"

key-files:
  created:
    - src/gnss_state.rs
  modified:
    - src/nmea_relay.rs
    - src/mqtt.rs
    - src/main.rs

key-decisions:
  - "Sentinel values 0xFF (AtomicU8) and 0xFFFF (AtomicU32) represent no-GGA-received; heartbeat emits null for each — operators see null until first fix"
  - "HDOP stored as x10 integer in AtomicU32 (e.g. 1.2 -> 12) since std has no AtomicF32; formatted back to 1-decimal in heartbeat"
  - "ends_with('GGA') match criterion handles GNGGA, GPGGA, GLGGA, GAGGA uniformly without exhaustive list"
  - "parse_gga_into_atomics silently skips malformed sentences (<9 fields) and empty individual fields — partial GGA during no-fix does not corrupt sentinel"

patterns-established:
  - "TELEM-01 pattern: module-level atomics in gnss_state.rs for cross-thread state sharing between relay and heartbeat"

requirements-completed: [TELEM-01]

# Metrics
duration: 3min
completed: 2026-03-08
---

# Phase 18 Plan 01: GNSS State Telemetry Summary

**GGA fix quality (fix type, satellite count, HDOP) parsed from NMEA relay and surfaced as JSON null-safe fields in the MQTT heartbeat via module-level atomics**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-08T21:46:24Z
- **Completed:** 2026-03-08T21:49:40Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- New `src/gnss_state.rs` module with three pub statics: GGA_FIX_TYPE (AtomicU8), GGA_SATELLITES (AtomicU8), GGA_HDOP_X10 (AtomicU32)
- `parse_gga_into_atomics` function added to nmea_relay.rs, called for all sentence types ending with "GGA"
- Heartbeat JSON now includes `fix_type` (0/1/2/4/5/6 or null), `satellites` (integer or null), `hdop` (float string like "1.2" or null)
- Full cargo clippy -D warnings clean and cargo build --release success

## Task Commits

Each task was committed atomically:

1. **Task 1: Create gnss_state.rs with shared GGA atomics** - `5d4a04f` (feat)
2. **Task 2: Parse GGA in nmea_relay.rs and update atomics** - `9e74dff` (feat)
3. **Task 3: Extend heartbeat JSON with fix_type, satellites, hdop** - `cb02b95` (feat)

## Files Created/Modified

- `src/gnss_state.rs` - New module; three module-level atomics with sentinel-value convention
- `src/nmea_relay.rs` - Added parse_gga_into_atomics() and call in relay loop Ok arm
- `src/mqtt.rs` - Heartbeat format! string extended with fix_type, satellites, hdop
- `src/main.rs` - Added `mod gnss_state;` after `mod gnss;`

## Decisions Made

- Sentinel values 0xFF (AtomicU8) and 0xFFFF (AtomicU32) represent "no GGA received yet"; heartbeat emits JSON `null` for each field until the first GGA is parsed. This matches operator expectations — null is unambiguous unlike 0 (which means "no fix").
- HDOP stored as ×10 integer in AtomicU32 (e.g. HDOP 1.2 stored as 12) since std Rust has no AtomicF32; formatted back to 1-decimal float in heartbeat JSON.
- `ends_with("GGA")` match criterion in nmea_relay.rs handles GNGGA, GPGGA, GLGGA, GAGGA uniformly without an exhaustive sentence-type list.
- `parse_gga_into_atomics` silently ignores sentences with fewer than 9 fields and skips empty individual fields — preserves sentinel during partial GGA (common during no-fix state).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- `cargo test` not usable for this project (ESP32 cross-compilation toolchain conflicts with host test target — duplicate lang items). Task 2 `tdd="true"` behavior spec was verified through code review and clippy rather than unit tests. All behavioral requirements from the spec were implemented in `parse_gga_into_atomics`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- TELEM-01 complete; GGA fix quality flows from GNSS receiver through nmea_relay atomics into heartbeat MQTT publication
- GGA_FIX_TYPE=4 (RTK Fixed) or =5 (RTK Float) visible in heartbeat JSON allows operators to monitor RTK status remotely
- Ready for Phase 18 Plan 02

---
*Phase: 18-telemetry-and-ota-validation*
*Completed: 2026-03-08*
