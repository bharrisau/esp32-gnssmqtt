---
phase: 16-remote-logging
plan: 01
subsystem: logging
tags: [esp-idf, vprintf, ffi, c-shim, atomic, sync-channel, mqtt, cc-rs]

# Dependency graph
requires:
  - phase: 05-nmea-relay
    provides: relay thread pattern (Arc<Mutex<EspMqttClient>>, enqueue, recv_timeout)
  - phase: 14-quick-additions
    provides: SLOW_RECV_TIMEOUT constant in config.rs
provides:
  - ESP-IDF vprintf hook capturing all log output (both Rust log:: and C component logs)
  - Bounded sync_channel(32) for non-blocking log forwarding (LOG-03)
  - Re-entrancy guard (AtomicBool) preventing feedback loops in relay thread
  - spawn_log_relay function for wiring in Plan 02
affects: [16-02-wiring]

# Tech tracking
tech-stack:
  added: [cc = "1" build-dependency]
  patterns:
    - C-to-Rust FFI via #[no_mangle] extern "C" functions
    - embuild cincl_args shell-token parsing for cc::Build include paths
    - OnceLock<SyncSender> for global channel sender without locking on hot path
    - AtomicBool re-entrancy guard (Relaxed ordering) for interrupt-safe guard check

key-files:
  created:
    - src/log_shim.c
    - src/log_relay.rs
  modified:
    - build.rs
    - Cargo.toml
    - src/main.rs

key-decisions:
  - "cc::Build include paths parsed from embuild::espidf::sysenv::cincl_args() — strip outer shell quotes and split -isystem/-I/-D tokens; direct use of cincl.args.split_whitespace().flag() fails due to shell quoting"
  - "mod log_relay added to main.rs in Plan 01 (not Plan 02 as stated) to enable cargo build verification of Rust module compilation"
  - "spawn_log_relay returns anyhow::Result<()> — SyncSender stored in LOG_TX OnceLock; main.rs does not hold the sender"
  - "stack_size(4096) for log relay thread — HWM logged at entry for monitoring"

patterns-established:
  - "C shim + Rust FFI pattern: log_shim.c extern declarations matched by #[no_mangle] pub extern 'C' in log_relay.rs"
  - "build.rs cincl_args parsing: strip outer quotes, match prefix (-isystem/-I/-D), use cc::Build::include()"

requirements-completed: [LOG-01, LOG-03]

# Metrics
duration: 6min
completed: 2026-03-08
---

# Phase 16 Plan 01: Remote Logging Infrastructure Summary

**ESP-IDF vprintf hook (log_shim.c) + Rust relay module (log_relay.rs) with AtomicBool re-entrancy guard and bounded sync_channel — captures all log output for MQTT forwarding without blocking the calling thread**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-08T02:52:34Z
- **Completed:** 2026-03-08T02:58:52Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- C vprintf hook captures all ESP-IDF log output (Rust log:: and native C components) via `esp_log_set_vprintf`
- Re-entrancy guard (AtomicBool) prevents feedback loops: hook checks `rust_log_is_reentering()` before forwarding
- Bounded `sync_channel(32)` with `try_send` satisfies LOG-03 (never blocks calling thread)
- `spawn_log_relay` spawns 4096-byte relay thread that publishes `gnss/{device_id}/log` at QoS 0

## Task Commits

Each task was committed atomically:

1. **Task 1: C shim (log_shim.c) and build system integration** - `c4cd802` (feat)
2. **Task 2: Rust log relay module (log_relay.rs)** - `db6e4c8` (feat)

## Files Created/Modified
- `src/log_shim.c` - vprintf hook: mqtt_log_vprintf, install_mqtt_log_hook, va_copy pattern, re-entrancy guard check
- `src/log_relay.rs` - LOG_REENTERING AtomicBool, LOG_TX OnceLock, rust_log_is_reentering, rust_log_try_send (FFI), spawn_log_relay
- `build.rs` - cc::Build with parsed ESP-IDF cincl_args include paths; embuild called first
- `Cargo.toml` - cc = "1" added to [build-dependencies]
- `src/main.rs` - mod log_relay added for compilation verification

## Decisions Made
- `cc::Build` include paths extracted by parsing `embuild::espidf::sysenv::cincl_args().args` as shell tokens — strip surrounding `"` and classify by `-isystem`/`-I`/`-D` prefix. Naive `split_whitespace().flag()` passes double-quoted flags to gcc causing "No such file or directory".
- `mod log_relay` added to `main.rs` in Plan 01 (not Plan 02) to allow `cargo build --release` to compile and verify the Rust module. The plan text contains a contradiction: verification requires compilation, compilation requires `mod` declaration. Adding `mod` without calling any functions satisfies both constraints.
- `spawn_log_relay` returns `anyhow::Result<()>` — the `SyncSender` is stored in `LOG_TX` (global `OnceLock`) so no handle needs to be retained by `main.rs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Parsed cincl_args shell tokens instead of passing raw flags**
- **Found during:** Task 1 (build.rs cc::Build integration)
- **Issue:** The plan says "cc crate reads ESP-IDF include paths from environment variables set by embuild" but cc-rs does not automatically pick up IDF includes. Passing `cincl.args.split_whitespace()` tokens directly via `build.flag()` fails because the string uses shell quoting (`"-isystem/path"` with literal quotes) which gcc receives as a quoted literal, not a flag.
- **Fix:** Parse cincl_args tokens: strip outer `"`, classify by prefix, use `build.include()` for `-isystem`/`-I` paths and `build.flag()` for `-D` defines.
- **Files modified:** build.rs
- **Verification:** `cargo build --release` passes; gcc receives `-I /path/to/include` without shell-quote artifacts.
- **Committed in:** c4cd802 (Task 1 commit)

**2. [Rule 3 - Blocking] Added mod log_relay to main.rs**
- **Found during:** Task 2 (verification step)
- **Issue:** Plan says main.rs does not yet declare `mod log_relay`, but `cargo build --release` cannot compile or verify `src/log_relay.rs` without the declaration.
- **Fix:** Added `mod log_relay;` to the module declarations in main.rs.
- **Files modified:** src/main.rs
- **Verification:** `cargo build --release` compiles log_relay.rs and passes.
- **Committed in:** db6e4c8 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 bug fix in build.rs flag parsing, 1 blocking issue with module declaration)
**Impact on plan:** Both fixes necessary for compilation to succeed. No scope creep — `spawn_log_relay` is not called, `install_mqtt_log_hook` is not called.

## Issues Encountered
- build.rs cincl_args integration required understanding the shell-quoting format of embuild's output. The embuild `cincl_args.args` field is a space-separated string where multi-word flags or paths with special chars are wrapped in `"..."` shell quotes. cc-rs `flag()` takes OsStr and does not interpret shell quoting — it passes the string verbatim to the compiler.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `src/log_shim.c` ready; `install_mqtt_log_hook()` not yet called
- `src/log_relay.rs` ready; `spawn_log_relay()` not yet called
- Plan 02 wires both: calls `spawn_log_relay` early in main(), then calls `install_mqtt_log_hook()` after relay thread is started

---
*Phase: 16-remote-logging*
*Completed: 2026-03-08*
