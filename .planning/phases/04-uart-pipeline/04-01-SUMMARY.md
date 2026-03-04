---
phase: 04-uart-pipeline
plan: 01
subsystem: gnss
tags: [uart, nmea, mpsc, esp32-c6, um980, rust]

# Dependency graph
requires:
  - phase: 02-connectivity
    provides: UartDriver initialization pattern, Arc<UartDriver> thread-sharing pattern

provides:
  - "src/gnss.rs: spawn_gnss() public function returning (Sender<String>, Receiver<(String,String)>)"
  - "RX thread: NON_BLOCK UART polling + NMEA sentence assembly + stdout mirror"
  - "TX thread: blocking mpsc drain + CRLF-terminated UART write to UM980"

affects:
  - 04-02-PLAN (wires gnss.rs into main.rs, replaces uart_bridge use)
  - 05-nmea-mqtt (consumes Receiver<(String,String)> for MQTT publish)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Arc<UartDriver> for thread-safe UART sharing without Mutex (read/write take &self)"
    - "NON_BLOCK + 10ms sleep polling avoids FreeRTOS watchdog"
    - "mpsc channels as clean interface boundary between GNSS module and callers"
    - "512-byte line accumulator with overflow detection for UM980 proprietary sentences"

key-files:
  created:
    - src/gnss.rs
  modified: []

key-decisions:
  - "Arc<UartDriver> suffices for thread-safe UART sharing — read() and write() both take &self, no Mutex needed"
  - "512-byte line_buf chosen to cover longest UM980 proprietary sentences (larger than standard 82-byte NMEA limit)"
  - "Temporary mod gnss; in main.rs used for build verification, then reverted — Plan 02 owns the permanent wire-up"
  - "Non-NMEA lines (no leading $) are logged at WARN and dropped — UM980 response lines (CMD,OK) fall into this category"

patterns-established:
  - "GNSS module owns UartDriver exclusively — no other module accesses UM980 UART directly"
  - "spawn_gnss returns (cmd_tx, nmea_rx) — caller holds both ends, module owns the UART"

requirements-completed:
  - UART-01
  - UART-02
  - UART-03

# Metrics
duration: 2min
completed: 2026-03-04
---

# Phase 4 Plan 01: gnss.rs Summary

**UM980 UART hub in src/gnss.rs: Arc-shared UartDriver, NON_BLOCK RX sentence assembly, mpsc channels returning (Sender<String>, Receiver<(String,String)>)**

## Performance

- **Duration:** 2 min
- **Started:** 2026-03-04T12:40:23Z
- **Completed:** 2026-03-04T12:42:28Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments

- Created `src/gnss.rs` with `spawn_gnss()` matching the locked signature from the plan
- RX thread assembles NMEA sentences from fragmented NON_BLOCK reads into a 512-byte accumulator, mirrors each line to stdout, and forwards `(sentence_type, raw)` tuples via mpsc
- TX thread blocks on mpsc receiver and writes CRLF-terminated command strings to UM980
- Build verified: zero errors in gnss.rs (only expected `dead_code` warning for unused spawn_gnss)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create src/gnss.rs with spawn_gnss, RX thread, and TX thread** - `48d5ac2` (feat)

## Files Created/Modified

- `/home/bharris/esp32-gnssmqtt/src/gnss.rs` - GNSS UART hub: spawn_gnss(), RX assembly thread, TX command thread

## Decisions Made

- `Arc<UartDriver>` chosen over `Arc<Mutex<UartDriver>>` — `UartDriver::read` and `write` both take `&self`, making Mutex unnecessary; matches the pattern already established in `uart_bridge.rs`
- 512-byte `line_buf` chosen to handle UM980 proprietary sentences which exceed the standard 82-byte NMEA maximum
- Build verification performed by temporarily adding `mod gnss;` to `main.rs`, confirming zero compile errors, then reverting; the permanent wire-up is Plan 02's responsibility

## Deviations from Plan

None — plan executed exactly as written.

Note: The plan's verify command used `--target riscv32imc-esp-espidf` but the project's `.cargo/config.toml` default target is `riscv32imac-esp-espidf`. Build was run with the correct default target (`cargo build`) and succeeded cleanly.

## Issues Encountered

- The plan's verify command (`cargo build --target riscv32imc-esp-espidf`) fails with "MCU esp32c6 not supported" because the correct target for ESP32-C6 is `riscv32imac-esp-espidf` (with `a` for atomic). Running `cargo build` (default target from `.cargo/config.toml`) succeeded cleanly.

## Next Phase Readiness

- `src/gnss.rs` is ready for Plan 02 to wire into `main.rs`: replace `uart_bridge::spawn_bridge` call with `gnss::spawn_gnss`, remove `mod uart_bridge`, add `mod gnss`
- `(cmd_tx, nmea_rx)` return values are unused in this plan — Plan 02 will store them for Phase 5 consumption
- UART-01, UART-02, UART-03 requirements satisfied by this implementation

## Self-Check

- `src/gnss.rs` — FOUND
- commit `48d5ac2` — FOUND
- `04-01-SUMMARY.md` — FOUND

## Self-Check: PASSED

---
*Phase: 04-uart-pipeline*
*Completed: 2026-03-04*
