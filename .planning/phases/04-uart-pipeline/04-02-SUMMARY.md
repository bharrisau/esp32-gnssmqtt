---
phase: 04-uart-pipeline
plan: "02"
subsystem: gnss
tags: [rust, esp32c6, uart, mpsc, gnss, um980, nmea]

# Dependency graph
requires:
  - phase: 04-uart-pipeline plan 01
    provides: gnss::spawn_gnss returning (Sender<String>, Receiver<(String, String)>)
provides:
  - uart_bridge::spawn_bridge(cmd_tx: Sender<String>) — TX-only stdin bridge, no UART peripheral ownership
  - main.rs Step 7 wired with gnss::spawn_gnss + uart_bridge::spawn_bridge
  - gnss_cmd_tx and nmea_rx held alive in main.rs idle loop for Phase 5/6 handoff
affects:
  - Phase 5 NMEA relay (consumes nmea_rx)
  - Phase 6 remote config (clones gnss_cmd_tx)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - TX-only bridge: uart_bridge no longer owns hardware; sends to mpsc Sender<String> instead of writing UART directly
    - Channel endpoint retention: main.rs idle loop holds _gnss_cmd_tx and _nmea_rx to prevent premature thread exit

key-files:
  created: []
  modified:
    - src/uart_bridge.rs
    - src/main.rs

key-decisions:
  - "uart_bridge refactored to accept Sender<String> — UART ownership exclusively in gnss.rs, no Arc sharing needed"
  - "gnss_cmd_tx.clone() passed to uart_bridge::spawn_bridge — allows main.rs to retain original for Phase 6"
  - "idle loop bindings _gnss_cmd_tx and _nmea_rx explicit — documents Phase 5/6 handoff points in code"

patterns-established:
  - "Channel ownership pattern: Receiver held in main idle loop, Sender cloned to subsystems that need it"
  - "UART exclusive ownership: one module owns the UartDriver, others interact via mpsc channels"

requirements-completed: [UART-01, UART-02, UART-03]

# Metrics
duration: ~5min (code already present from prior session; build verification + summary creation)
completed: 2026-03-06
---

# Phase 04 Plan 02: Wire UART Pipeline Summary

**uart_bridge.rs refactored to TX-only stdin bridge using Sender<String>, main.rs Step 7 replaced with gnss::spawn_gnss + uart_bridge::spawn_bridge, full UART pipeline wired and firmware compiles**

## Performance

- **Duration:** ~5 min (tasks pre-committed from prior session)
- **Started:** 2026-03-06T00:00:00Z
- **Completed:** 2026-03-06
- **Tasks:** 2 of 3 auto tasks complete (Task 3 is hardware checkpoint)
- **Files modified:** 2

## Accomplishments

- Removed Thread A and UART peripheral ownership from uart_bridge.rs — gnss.rs RX thread now owns that path
- spawn_bridge signature changed from UART peripherals to `Sender<String>` — no more Arc<UartDriver> in bridge
- main.rs Step 7 now calls gnss::spawn_gnss, receives (gnss_cmd_tx, nmea_rx), passes clone to uart_bridge
- Channel endpoints held alive in idle loop with explicit bindings and comments for Phase 5/6 handoff

## Task Commits

Each task was committed atomically:

1. **Task 1: Refactor uart_bridge.rs to TX-only stdin bridge** - `c55083d` (refactor)
2. **Task 2: Wire main.rs Step 7 and add mod gnss declaration** - `8f31bd2` (feat)
3. **Task 3: Hardware verification** - pending checkpoint

## Files Created/Modified

- `src/uart_bridge.rs` — Removed Thread A, removed UART peripheral params, spawn_bridge now accepts Sender<String>, Thread B sends via cmd_tx.send() on Enter
- `src/main.rs` — mod gnss added, Step 7 replaced with gnss::spawn_gnss call, idle loop holds _gnss_cmd_tx and _nmea_rx alive

## Decisions Made

- uart_bridge.rs imports reduced to only `std::io::{Read, Write}` — no esp_idf_svc imports, no Arc
- gnss_cmd_tx.clone() sent to uart_bridge so main.rs retains original for Phase 6 MQTT→UM980 command forwarding
- Explicit `let _gnss_cmd_tx` and `let _nmea_rx` bindings in idle loop prevent silent drop and document Phase 5/6 integration points

## Deviations from Plan

None - plan executed exactly as written. Both tasks were already committed when this execution session began (prior session completed the code changes). Build verified clean with `cargo build` using default target `riscv32imac-esp-espidf`.

Note: The plan's verify commands reference `riscv32imc-esp-espidf` which is incorrect for ESP32-C6 — the correct target is `riscv32imac-esp-espidf` (as specified in `.cargo/config.toml`). Build is clean with the correct target.

## Issues Encountered

- Plan verify commands used wrong build target (`riscv32imc-esp-espidf` instead of `riscv32imac-esp-espidf`). The correct target is configured in `.cargo/config.toml` and `cargo build` without explicit target flag succeeds cleanly.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Full UART pipeline wired: UM980 bytes → gnss.rs RX thread → mpsc channel, stdin → uart_bridge Thread B → gnss_cmd_tx → gnss.rs TX thread → UM980
- Hardware verification (Task 3) requires device FFFEB5 and espflash monitor
- Phase 5 NMEA relay: consume `nmea_rx: Receiver<(String, String)>` already held in main.rs
- Phase 6 remote config: clone `gnss_cmd_tx: Sender<String>` already held in main.rs
- No blockers — firmware compiles cleanly

---
*Phase: 04-uart-pipeline*
*Completed: 2026-03-06*
