---
phase: 02-connectivity
plan: 01
subsystem: wifi
tags: [esp-idf-svc, wifi, blocking-wifi, reconnect, exponential-backoff, freertos, esp32c6, embedded-svc]

# Dependency graph
requires:
  - phase: 01-scaffold
    provides: "Compilable Rust scaffold with pinned esp-idf-svc =0.51.0 and config.rs credential stubs"
provides:
  - "src/wifi.rs — wifi_connect (full start/connect/netif_up sequence) and wifi_supervisor (reconnect loop with exponential backoff)"
  - "wifi_connect returns BlockingWifi<EspWifi<'static>> — caller keeps handle alive to maintain WiFi driver"
  - "wifi_supervisor runs forever (!): polls every 5s, reconnects with backoff 1s..60s, resets backoff on success"
  - "compile-time credentials in src/config.rs (gitignored): WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_PASS"
affects:
  - 02-04-PLAN  # integration plan wires wifi_connect into main and tests live network connection

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "BlockingWifi pattern: wrap → set_configuration → start → connect → wait_netif_up"
    - "Supervisor pattern: start() once in wifi_connect; only connect() retried in supervisor loop"
    - "Exponential backoff: backoff_secs doubled on failure, capped at 60s, reset to 1 on success"
    - "5s poll interval before connection check prevents reconnect-storm on brief signal loss"

key-files:
  created:
    - src/wifi.rs
  modified:
    - src/main.rs

key-decisions:
  - "wifi.start() called once in wifi_connect only — calling it again in supervisor loop would reinitialise the driver and corrupt state (research anti-pattern)"
  - "5-second outer sleep before each is_connected() check — prevents storm on brief dropout"
  - "backoff sleep placed before wifi.connect() call — always waits before reconnect attempt regardless of previous state"
  - "wait_netif_up() called on reconnect success — ensures IP address is assigned before backoff is reset"
  - "credentials manually filled by user in src/config.rs (gitignored); Task 1 was human-action checkpoint"

patterns-established:
  - "WiFi ownership pattern: wifi_connect returns BlockingWifi handle; caller moves it into wifi_supervisor thread to keep driver alive"
  - "Reconnect-only loop: supervisor never calls start(), only connect() + wait_netif_up()"

requirements-completed: [CONN-01, CONN-03]

# Metrics
duration: ~8min
completed: 2026-03-03
---

# Phase 2 Plan 01: WiFi Module Summary

**BlockingWifi<EspWifi> connect module with full start/connect/netif_up sequence and exponential-backoff (1s..60s) reconnect supervisor polling every 5 seconds**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-03-03T11:36:44Z
- **Completed:** 2026-03-03T11:44:00Z (estimated)
- **Tasks:** 2 (Task 1: human-action checkpoint for credentials; Task 2: wifi.rs implementation)
- **Files modified:** 2

## Accomplishments

- Created src/wifi.rs with wifi_connect and wifi_supervisor public functions
- wifi_connect: wraps modem in BlockingWifi, sets WPA2Personal client configuration with compile-time SSID/password, calls start → connect → wait_netif_up, returns the live handle
- wifi_supervisor: polls is_connected() every 5 seconds; on disconnect sleeps backoff_secs, calls wifi.connect(), on success calls wait_netif_up() and resets backoff to 1; on failure doubles backoff capped at 60s; never calls wifi.start() (driver already running)
- Registered mod wifi in src/main.rs
- config.rs populated with real WiFi and MQTT credentials by user (gitignored — not committed)

## Task Commits

Each task was committed atomically:

1. **Task 1: Populate credentials in config.rs** - Human-action checkpoint (file gitignored, no commit)
2. **Task 2: Create src/wifi.rs — connect and reconnect supervisor** - `c3512e7` (feat)

**Plan metadata:** (committed below in docs commit)

## Files Created/Modified

- `/home/bharris/esp32-gnssmqtt/src/wifi.rs` — WiFi connect factory (returns BlockingWifi handle) and reconnect supervisor (returns !, exponential backoff)
- `/home/bharris/esp32-gnssmqtt/src/main.rs` — added `mod wifi;` declaration

## Decisions Made

- **wifi.start() called only once:** The plan and research both document that calling `wifi.start()` again inside the reconnect loop re-initialises the WiFi driver and corrupts internal state. Only `wifi.connect()` is retried in the supervisor loop.
- **5-second poll sleep is outer loop sleep:** The outer `thread::sleep(5s)` happens every iteration regardless of connection state. This prevents any reconnect-storm pattern when the supervisor runs.
- **backoff sleep placed before wifi.connect():** The sleep using `backoff_secs` happens before the connect call — the device always waits before each reconnect attempt, even the first one after a dropout.
- **wait_netif_up() on reconnect:** After a successful `wifi.connect()`, `wait_netif_up()` is called before resetting backoff — ensures an IP address is fully assigned before declaring success.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Registered mod wifi in main.rs**
- **Found during:** Task 2 (creating src/wifi.rs)
- **Issue:** A Rust source file at src/wifi.rs is not compiled unless `mod wifi;` is declared in main.rs. Without it the module is excluded from the compilation unit.
- **Fix:** Added `mod wifi;` to src/main.rs module declarations.
- **Files modified:** src/main.rs
- **Verification:** File review — mod wifi declared at line 15 of main.rs
- **Committed in:** c3512e7 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 2 - Missing Critical)
**Impact on plan:** Fix required for correct compilation. Standard Rust module registration — not scope creep.

## Issues Encountered

`cargo check --target riscv32imac-esp-espidf` cannot be run in this WSL2 environment because the host C compiler (`gcc`) is not installed. Build scripts for esp-idf-sys and build-std crates require a host C toolchain. This is the same pre-existing environment constraint documented in the 02-02 and 02-03 SUMMARYs. The Windows environment (where the full ESP-IDF toolchain including ldproxy is installed) is the correct build target. Compilation and runtime behaviour will be verified at the Plan 04 flash checkpoint.

The Rust source code implements all plan-specified API calls, sequencing, and anti-pattern guards:
- Exact imports confirmed for esp-idf-svc =0.51.0
- Exact wifi_connect and wifi_supervisor signatures as specified
- No wifi.start() in reconnect loop
- Backoff sleep before connect, wait_netif_up on success, backoff cap at 60

## User Setup Required

Task 1 (human-action checkpoint): The user manually filled in compile-time credentials in src/config.rs:
- WIFI_SSID set to their network name
- WIFI_PASS set to their WiFi password
- MQTT_HOST set to broker IP address
- MQTT_PORT, MQTT_USER, MQTT_PASS set as appropriate

src/config.rs is gitignored — credentials are NOT committed to the repository.

## Next Phase Readiness

- src/wifi.rs ready: wifi_connect and wifi_supervisor both implemented and registered
- Plan 04 (human-verify) will wire wifi_connect into main.rs (replacing the Phase 1 idle loop), spawn wifi_supervisor in a dedicated thread, then flash and verify actual WiFi connection to the configured SSID
- wifi_supervisor can be spawned as: `std::thread::Builder::new().stack_size(8192).spawn(move || wifi::wifi_supervisor(wifi)).unwrap()`

---
*Phase: 02-connectivity*
*Completed: 2026-03-03*

## Self-Check: PASSED

- FOUND: src/wifi.rs
- FOUND: src/main.rs (mod wifi registered at line 15)
- FOUND: 02-01-SUMMARY.md
- FOUND: commit c3512e7 (feat(02-01): implement WiFi connect and reconnect supervisor)
- VERIFIED: wifi_connect exported at line 15
- VERIFIED: wifi_supervisor exported at line 49 (returns !)
- VERIFIED: wifi.start() appears only in wifi_connect (line 32), not in supervisor loop
- VERIFIED: backoff sleep before wifi.connect() call (line 62 before line 64)
- VERIFIED: backoff cap `(backoff_secs * 2).min(60)` at lines 73 and 79
