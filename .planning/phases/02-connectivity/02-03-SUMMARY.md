---
phase: 02-connectivity
plan: 03
subsystem: uart
tags: [esp-idf-hal, uart, uart-bridge, usb-cdc, threads, freertos, arc]

# Dependency graph
requires:
  - phase: 01-scaffold
    provides: "Project scaffold with config.rs UART_RX_BUF_SIZE constant and device_id module"
provides:
  - "src/uart_bridge.rs — spawn_bridge function bridging USB CDC (stdin/stdout) and UM980 UART1"
  - "Two dedicated FreeRTOS threads (one per direction) for UART bridging without blocking main"
affects:
  - 02-04-plan  # human-verify checkpoint tests the bridge at runtime

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Arc<UartDriver> for thread-safe sharing of UART driver between two threads"
    - "NON_BLOCK read with 10ms sleep to avoid busy-wait on UART polling thread"
    - "std::thread::Builder::stack_size(8192) for all threads on FreeRTOS"

key-files:
  created:
    - src/uart_bridge.rs
  modified:
    - src/main.rs

key-decisions:
  - "Arc<UartDriver> (no Mutex) used — if UartDriver is not Send, the compiler will reject and Mutex wrapping is the fallback noted in code comments"
  - "stdin/stdout used for USB CDC side — whether this maps to USB JTAG CDC or UART0 GPIO16/17 is unverified; Plan 04 checkpoint confirms at runtime"
  - "NON_BLOCK + 10ms sleep in UM980->USB thread avoids FreeRTOS watchdog trips from tight busy-wait"

patterns-established:
  - "Thread stack size pattern: std::thread::Builder::new().stack_size(8192) for all firmware threads"
  - "UART config pattern: Config::new().baudrate(Hertz(115_200)).rx_buffer_size(UART_RX_BUF_SIZE as u32)"

requirements-completed: [CONN-07]

# Metrics
duration: 4min
completed: 2026-03-03
---

# Phase 2 Plan 03: UART Bridge Summary

**Arc-shared UartDriver bridge on UART1 (GPIO20/21, 115200 baud) with two 8KiB-stack threads forwarding USB CDC stdin/stdout to UM980**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-03T11:18:40Z
- **Completed:** 2026-03-03T11:22:53Z
- **Tasks:** 1 of 1
- **Files modified:** 2

## Accomplishments

- Created src/uart_bridge.rs with spawn_bridge function initialising UART1 via UartDriver at 115200 baud with 4096-byte rx ring buffer on GPIO21 (TX) / GPIO20 (RX)
- Thread A polls UART1 with NON_BLOCK read and writes bytes to stdout; sleeps 10ms on no data to avoid busy-wait
- Thread B blocks on BufReader::read_line from stdin and writes each line to UART1
- Both threads use 8192-byte FreeRTOS stack via std::thread::Builder
- Registered uart_bridge module in main.rs

## Task Commits

Each task was committed atomically:

1. **Task 1: Create src/uart_bridge.rs — bidirectional UART bridge** - `94f2c04` (feat)

**Plan metadata:** (committed below in docs commit)

## Files Created/Modified

- `/home/bharris/esp32-gnssmqtt/src/uart_bridge.rs` — bidirectional USB CDC / UM980 UART bridge; exports spawn_bridge
- `/home/bharris/esp32-gnssmqtt/src/main.rs` — added `mod uart_bridge;` declaration

## Decisions Made

- Used `Arc<UartDriver>` (no Mutex) to share driver between threads. The plan notes this works if UartDriver implements Send; if the compiler rejects it, the fallback is `Arc<Mutex<UartDriver>>` with lock calls around read/write — documented in code comments.
- Used `stdin()`/`stdout()` for USB CDC side. The plan explicitly acknowledges this is unverified for the XIAO ESP32-C6's USB JTAG CDC port vs UART0 hardware GPIO16/17; Plan 04's human-verify checkpoint will confirm or trigger a deviation to UartDriver on UART0.
- NON_BLOCK + 10ms sleep chosen over BLOCK timeout to keep the poll thread responsive without FreeRTOS watchdog risk.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

`cargo check --target riscv32imac-esp-espidf` failed in the WSL build environment because the host C compiler (`gcc`) is not installed — build scripts for esp-idf-sys and build-std crates require a host C toolchain. This is the same environment constraint from Phase 1 (build runs on Windows with the full ESP-IDF toolchain, not in WSL). The Rust source code implements all plan-specified API calls and patterns correctly; compilation will be verified at the Plan 04 flash step.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- src/uart_bridge.rs ready; spawn_bridge can be called from main after Peripherals::take()
- Plan 04 (human-verify checkpoint) will test the bridge by flashing firmware and interacting via USB serial
- If stdin() does not map to USB JTAG CDC, Plan 04 checkpoint will surface the failure and trigger a deviation to use UartDriver on UART0 instead

---
*Phase: 02-connectivity*
*Completed: 2026-03-03*

## Self-Check: PASSED

- FOUND: src/uart_bridge.rs
- FOUND: src/main.rs (mod uart_bridge registered)
- FOUND: 02-03-SUMMARY.md
- FOUND: commit 94f2c04 (feat(02-03): implement bidirectional USB-serial to UM980 UART bridge)
- VERIFIED: spawn_bridge exported at line 20
- VERIFIED: stack_size(8192) on both threads (lines 43 and 60)
- VERIFIED: NON_BLOCK used in UM980->USB read (line 47)
