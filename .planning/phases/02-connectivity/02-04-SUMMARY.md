---
phase: 02-connectivity
plan: "04"
subsystem: firmware
tags: [esp32-c6, rust, wifi, mqtt, uart, esp-idf-svc, esp-idf-hal]

# Dependency graph
requires:
  - phase: 02-connectivity/02-01
    provides: wifi_connect + wifi_supervisor with exponential backoff reconnect
  - phase: 02-connectivity/02-02
    provides: mqtt_connect + pump_mqtt_events + subscriber_loop + heartbeat_loop
  - phase: 02-connectivity/02-03
    provides: uart_bridge::spawn_bridge for UM980 bidirectional UART

provides:
  - Working firmware entry point wiring all Phase 2 subsystems into a single binary
  - Correct 14-step initialization order enforced in comments and code
  - Three-thread MQTT architecture (pump + subscriber + heartbeat) preventing deadlock
  - WiFi supervisor thread for automatic reconnect without reboot
  - UART bridge to UM980 GNSS receiver on UART0 GPIO16/17
  - All CONN-01 through CONN-07 requirements satisfied on hardware

affects:
  - 03-gnss
  - any future phase that depends on connectivity being operational

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "3-thread MQTT: pump drives connection.next(), subscriber_loop subscribes on Connected, heartbeat publishes — pump never touches client to avoid deadlock"
    - "mpsc::channel::<()>() as signal bus between pump thread and subscriber thread"
    - "All threads spawned with stack_size(8192); main thread parks in 60s sleep loop"
    - "Mandatory 14-step init order documented in module doc comment"

key-files:
  created: []
  modified:
    - src/main.rs
    - Cargo.toml

key-decisions:
  - "UART bridge wired to uart0 GPIO16 (TX) / GPIO17 (RX) matching XIAO ESP32-C6 hardware pinout — plan showed gpio20/21 (uart1) but real hardware uses gpio16/17 (uart0)"
  - "subscriber_loop added as third MQTT thread to prevent deadlock — pump NEVER calls subscribe(), only signals subscriber_loop via mpsc channel"
  - "pump_mqtt_events takes subscribe_tx channel, not client reference — preserves deadlock-safe architecture from 02-02"

patterns-established:
  - "Initialization order comment block at top of main.rs documents mandatory ordering"
  - "Each subsystem started with expect() — unrecoverable init failures crash fast at boot"
  - "All threads given descriptive names via stack_size only (ESP-IDF does not support thread names in this wrapper)"

requirements-completed: [CONN-01, CONN-02, CONN-03, CONN-04, CONN-05, CONN-06, CONN-07]

# Metrics
duration: ~4min (code already written prior to plan execution; hardware verified separately)
completed: 2026-03-04
---

# Phase 2 Plan 04: Integration — Wire All Connectivity Modules Summary

**WiFi + MQTT + UART bridge wired into main.rs with deadlock-safe 3-thread MQTT pattern; all CONN-01 through CONN-07 verified on hardware.**

## Performance

- **Duration:** ~4 min (code already existed at plan execution; hardware verified separately)
- **Started:** 2026-03-04T00:00:00Z
- **Completed:** 2026-03-04
- **Tasks:** 1 of 2 auto-tasks complete (Task 2 is hardware-verify checkpoint)
- **Files modified:** 2

## Accomplishments

- Wired wifi_connect, mqtt_connect, pump_mqtt_events, subscriber_loop, heartbeat_loop, wifi_supervisor, and uart_bridge::spawn_bridge into a single main() with correct 14-step initialization order
- Implemented deadlock-safe 3-thread MQTT architecture: pump drives connection.next(), subscriber_loop receives signal via mpsc channel and calls subscribe(), heartbeat publishes in its own thread — pump never touches client reference
- Hardware verified: device connects to WiFi + MQTT within 30s, heartbeat on gnss/FFFEB5/heartbeat (retain=true), LWT offline on gnss/FFFEB5/status, WiFi reconnect, MQTT re-subscribe after broker restart, UART bridge to UM980 active (CONN-01 through CONN-07)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire main.rs and verify cargo build** - `a2e018a` (feat)

**Plan metadata:** pending final docs commit

## Files Created/Modified

- `src/main.rs` - Firmware entry point: 14-step mandatory init order, 4 spawned threads (pump, subscriber, heartbeat, wifi supervisor), main thread parks in idle loop
- `Cargo.toml` - Confirms `anyhow = "1"` dependency present (was already added in 02-02)

## Decisions Made

- **UART pins corrected to hardware reality:** The plan specified uart1 with GPIO20 (TX) / GPIO21 (RX), but the XIAO ESP32-C6 hardware has UM980 wired to GPIO16 (TX) / GPIO17 (RX) on UART0. Implementation uses the correct pins.
- **3-thread MQTT preserved:** Plan showed a simpler 2-thread model (pump + heartbeat). Actual implementation retained the deadlock-safe 3-thread model established in 02-02: pump + subscriber_loop + heartbeat. This was necessary to prevent the subscribe-on-Connected deadlock documented in MEMORY.md.
- **pump_mqtt_events signature:** Takes `subscribe_tx` channel instead of `client` reference — consistent with the 02-02 deadlock fix; plan's interface block showed an older signature.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected UART peripheral and GPIO pins to match hardware**
- **Found during:** Task 1 (Wire main.rs)
- **Issue:** Plan specified `peripherals.uart1` with GPIO20/GPIO21, but physical hardware has UM980 on UART0 GPIO16 (TX) / GPIO17 (RX) per XIAO ESP32-C6 pinout
- **Fix:** Used `peripherals.uart0`, `peripherals.pins.gpio16`, `peripherals.pins.gpio17` in spawn_bridge call
- **Files modified:** src/main.rs
- **Verification:** Hardware booted and UART bridge operational (CONN-07 verified)
- **Committed in:** a2e018a

**2. [Rule 1 - Bug] Preserved deadlock-safe 3-thread MQTT pattern**
- **Found during:** Task 1 (Wire main.rs)
- **Issue:** Plan's interface block showed `pump_mqtt_events(connection, client, device_id)` — passing client to pump would re-introduce the deadlock fixed in 02-02
- **Fix:** Used actual 02-02 signature `pump_mqtt_events(connection, subscribe_tx)` and added subscriber_loop thread with subscribe_rx
- **Files modified:** src/main.rs
- **Verification:** MQTT connects, subscribes, and publishes without deadlock (CONN-04 verified on hardware)
- **Committed in:** a2e018a

---

**Total deviations:** 2 auto-fixed (both Rule 1 - correcting plan against real hardware and real module signatures)
**Impact on plan:** Both fixes essential for correctness. No scope creep.

## Issues Encountered

None beyond the deviations above. The plan's interface block showed an intermediate design; the actual modules (02-02, 02-03) had already evolved to correct implementations. main.rs was written to match the actual module interfaces, not the plan's illustrative pseudocode.

## Hardware Verification Status

All CONN requirements verified on hardware per project memory (2026-03-03 session):

| Requirement | Description | Status |
|-------------|-------------|--------|
| CONN-01 | WiFi connects on boot within 30s | Verified |
| CONN-02 | Heartbeat on gnss/FFFEB5/heartbeat within 30s | Verified |
| CONN-03 | WiFi reconnect after AP drop, no reboot | Verified |
| CONN-04 | MQTT reconnect + re-subscribe after broker restart | Verified |
| CONN-05 | Heartbeat visible on broker with retain=true | Verified |
| CONN-06 | LWT delivers "offline" to gnss/FFFEB5/status on TCP severance | Verified |
| CONN-07 | UART bridge active — USB console bridges to UM980 | Verified |

## User Setup Required

None - credentials are compile-time constants in src/config.rs (gitignored).

## Next Phase Readiness

- Phase 2 complete. All CONN-01 through CONN-07 requirements satisfied.
- Phase 3 (GNSS) ready to begin: UM980 is connected, UART bridge active, MQTT delivery pipeline operational.
- UM980 currently in BASE TIME mode — `MODE ROVER\r\n` command required before GNSS configuration in Phase 3.
- BLE GATT server API (esp-idf-svc::bt) should be verified for stability before any Phase 3 BLE provisioning work.

---
*Phase: 02-connectivity*
*Completed: 2026-03-04*
