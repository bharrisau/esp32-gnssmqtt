---
phase: 20-field-testing-fixes
plan: "03"
subsystem: gnss
tags: [nvs, config-relay, um980, persistence, esp32]

# Dependency graph
requires:
  - phase: 17-ntrip-client
    provides: NVS usage patterns (EspNvs, EspNvsPartition, set_blob/get_blob)
  - phase: 19-pre-2-0-bugfix
    provides: config_relay.rs with hash-dedup; main.rs UM980 reboot monitor stub (warning-only)
provides:
  - UM980 GNSS config persisted to NVS gnss/gnss_config blob on every MQTT delivery
  - config_relay::apply_config() made pub for use by main.rs reboot monitor
  - UM980 reboot monitor reads NVS blob and re-applies config within 500ms of $devicename banner
affects: [field-testing, gnss-reliability, provisioning]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - NVS blob save after side-effect (save only when config actually applied, not on hash-dedup skip)
    - EspNvs::new with read/write=true for save, read-only=false for load
    - get_blob returns &[u8] slice (not usize); use data.len() and pass slice directly

key-files:
  created: []
  modified:
    - src/config_relay.rs
    - src/main.rs

key-decisions:
  - "get_blob() returns Ok(Some(&[u8])) not Ok(Some(usize)) — fixed during Task 2 (Rule 1 auto-fix)"
  - "save_gnss_config called only after apply_config succeeds, not on hash-dedup skip — ensures NVS only holds applied configs"
  - "UM980 reboot monitor stack increased to 8192 (from 4096) to safely accommodate 512-byte vec allocation"
  - "NVS namespace 'gnss' / key 'gnss_config' — follows pattern of 'prov' and 'ntrip' namespaces"

patterns-established:
  - "Pattern: NVS blob persistence after side-effect — save only when action completes, not on guard/dedup paths"

requirements-completed:
  - FEAT-2

# Metrics
duration: 17min
completed: 2026-03-11
---

# Phase 20 Plan 03: UM980 Config NVS Persistence and Auto Re-apply Summary

**UM980 GNSS config persisted to NVS blob on every MQTT delivery; automatic re-apply on UM980 reboot via $devicename banner detection**

## Performance

- **Duration:** ~17 min
- **Started:** 2026-03-11T13:21:01Z
- **Completed:** 2026-03-11T13:38:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `config_relay::spawn_config_relay` now accepts `nvs_partition: EspNvsPartition<NvsDefault>` and saves raw payload to NVS `gnss/gnss_config` blob after each new (non-duplicate) config
- `apply_config()` made `pub` so main.rs UM980 reboot monitor can call it directly
- UM980 reboot monitor upgraded from warning-only stub to full NVS read + re-apply: reads blob, calls `apply_config()`, logs success; silently skips on no saved config (first boot / factory reset)
- Stack size of reboot monitor thread increased to 8192 for safe 512-byte vec allocation

## Task Commits

Each task was committed atomically:

1. **Task 1: Add NVS persistence to config_relay.rs and make apply_config pub** - `5c828d7` (feat)
2. **Task 2: Wire NVS read and config re-apply into main.rs UM980 reboot monitor** - `e7cb290` (feat)

## Files Created/Modified
- `src/config_relay.rs` - Added NVS imports, `nvs_partition` param, `save_gnss_config()` helper, `apply_config()` made pub
- `src/main.rs` - `spawn_config_relay` call updated with `nvs.clone()`; UM980 reboot monitor upgraded to full NVS read + re-apply

## Decisions Made
- `EspNvs::new(..., "gnss", false)` (read-only) in reboot monitor vs `true` (read-write) in save helper — correct intent per operation
- `save_gnss_config` placed after `apply_config` in the relay loop so NVS only stores successfully applied configs (not bypassed by hash-dedup)
- NVS namespace `"gnss"` / key `"gnss_config"` follows existing convention (`"prov"`, `"ntrip"`)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed get_blob return type mismatch**
- **Found during:** Task 2 (Wire NVS read and config re-apply into main.rs UM980 reboot monitor)
- **Issue:** Plan code used `Ok(Some(size))` and `&buf[..size]` treating the return as `usize`. Actual `get_blob` return type is `Ok(Some(&[u8]))` — a slice into the buffer, not a count.
- **Fix:** Changed pattern to `Ok(Some(data))` and passed `data` directly; used `data.len()` for log message
- **Files modified:** src/main.rs
- **Verification:** `cargo clippy -- -D warnings` passes clean; `cargo build --release` succeeds
- **Committed in:** `e7cb290` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug in plan code sample)
**Impact on plan:** Necessary type correction. No scope creep.

## Issues Encountered
- Release build initially failed with "couldn't create a temp dir" for riscv32 target — created missing deps directory manually, retry succeeded

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- FEAT-2 complete: UM980 config survives power cycles via NVS persistence + automatic re-apply
- Plan 20-04 (remaining field fixes) can proceed
- Hardware verification: send GNSS config via MQTT, power-cycle UM980 UART, verify re-apply log within 2s

---
*Phase: 20-field-testing-fixes*
*Completed: 2026-03-11*
