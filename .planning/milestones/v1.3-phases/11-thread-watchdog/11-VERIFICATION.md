---
phase: 11-thread-watchdog
verified: 2026-03-07T00:00:00Z
status: human_needed
score: 9/9 must-haves verified
human_verification:
  - test: "Boot device and observe serial log for '[WDT] supervisor started' line"
    expected: "Log line appears within 5 seconds of boot, confirming supervisor thread launched successfully"
    why_human: "Requires flashing firmware and monitoring UART output — cannot verify thread scheduling at runtime programmatically"
  - test: "Run device for 60 seconds under nominal operation (GNSS data flowing, MQTT connected)"
    expected: "No spurious reboots occur; heartbeat counters are incrementing (no WDT warn log lines)"
    why_human: "Requires live device — cannot verify absence of false-positive hang detection without runtime execution"
---

# Phase 11: Thread Watchdog Verification Report

**Phase Goal:** Critical threads are supervised so that a silent hang — a thread that stops progressing without panicking — triggers an automatic device reboot
**Verified:** 2026-03-07
**Status:** human_needed (all automated checks passed; 2 runtime items require device testing)
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth                                                                                                                            | Status     | Evidence                                                                                      |
|----|----------------------------------------------------------------------------------------------------------------------------------|------------|-----------------------------------------------------------------------------------------------|
| 1  | Two static AtomicU32 counters (GNSS_RX_HEARTBEAT, MQTT_PUMP_HEARTBEAT) exist in src/watchdog.rs and are pub                    | VERIFIED   | watchdog.rs lines 19, 25: `pub static GNSS_RX_HEARTBEAT` and `pub static MQTT_PUMP_HEARTBEAT` |
| 2  | spawn_supervisor() function exists in watchdog.rs and spawns a loop thread with 4096-byte stack                                  | VERIFIED   | watchdog.rs lines 31-37: `pub fn spawn_supervisor()` with `.stack_size(4096)`                 |
| 3  | WDT_CHECK_INTERVAL and WDT_MISS_THRESHOLD constants exist in config.rs and config.example.rs with documented rationale          | VERIFIED   | config.rs lines 40-45, config.example.rs lines 40-45: both constants present with doc comments |
| 4  | supervisor_loop reads both counters every WDT_CHECK_INTERVAL, counts misses, and calls unsafe esp_restart() after WDT_MISS_THRESHOLD consecutive misses | VERIFIED | watchdog.rs lines 39-88: full loop + miss counter logic + `unsafe { esp_idf_svc::sys::esp_restart(); }` at lines 67 and 81 |
| 5  | src/watchdog.rs is declared as mod watchdog in main.rs                                                                          | VERIFIED   | main.rs line 43: `mod watchdog;`                                                              |
| 6  | GNSS RX thread increments GNSS_RX_HEARTBEAT at the top of its outer loop (before the match uart_rx.read arm)                   | VERIFIED   | gnss.rs line 171: `crate::watchdog::GNSS_RX_HEARTBEAT.fetch_add(1, Ordering::Relaxed);` is the first statement inside `loop {`, before `match uart_rx.read(...)` |
| 7  | MQTT pump thread increments MQTT_PUMP_HEARTBEAT at the top of the while-let body before match event.payload()                  | VERIFIED   | mqtt.rs line 92: `crate::watchdog::MQTT_PUMP_HEARTBEAT.fetch_add(1, Ordering::Relaxed);` is first statement inside `while let Ok(event) = connection.next() {` |
| 8  | watchdog::spawn_supervisor() is called from main.rs as Step 18 (final spawn, after all other threads)                          | VERIFIED   | main.rs lines 198-202: Step 18 comment, `watchdog::spawn_supervisor().expect(...)`, after OTA spawn at line 194 |
| 9  | sdkconfig.defaults contains CONFIG_ESP_TASK_WDT_PANIC=y so the hardware TWDT reboots if the supervisor itself hangs            | VERIFIED   | sdkconfig.defaults line 43: `CONFIG_ESP_TASK_WDT_PANIC=y` with explanatory comment block     |

**Score:** 9/9 truths verified

### Required Artifacts

| Artifact              | Expected                                               | Status     | Details                                                              |
|-----------------------|--------------------------------------------------------|------------|----------------------------------------------------------------------|
| `src/watchdog.rs`     | GNSS_RX_HEARTBEAT, MQTT_PUMP_HEARTBEAT, spawn_supervisor() | VERIFIED | 89 lines; all three pub items present and substantive; supervisor_loop is complete with miss-counting logic and two esp_restart() call sites |
| `src/config.example.rs` | WDT_CHECK_INTERVAL, WDT_MISS_THRESHOLD constants    | VERIFIED   | Lines 40-45; Duration::from_secs(5) and u32 = 3 with rationale doc comments |
| `src/config.rs`       | WDT_CHECK_INTERVAL, WDT_MISS_THRESHOLD constants       | VERIFIED   | Lines 40-45; identical values to config.example.rs as required      |
| `src/gnss.rs`         | GNSS_RX_HEARTBEAT.fetch_add at top of outer loop       | VERIFIED   | Line 171; first statement inside `loop {}`, before the `match uart_rx.read(...)` — correctly catches UART stall (Ok(0)) paths |
| `src/mqtt.rs`         | MQTT_PUMP_HEARTBEAT.fetch_add at top of while-let body | VERIFIED   | Line 92; first statement inside `while let Ok(event) = connection.next() {}`, before `match event.payload()` |
| `src/main.rs`         | mod watchdog declaration + spawn_supervisor() Step 18  | VERIFIED   | Line 43: `mod watchdog;`; lines 198-202: Step 18 spawn as final thread before operational log |
| `sdkconfig.defaults`  | CONFIG_ESP_TASK_WDT_PANIC=y                            | VERIFIED   | Line 43; present with comment explaining 15s software / 30s hardware layered defense |

### Key Link Verification

| From                               | To                                      | Via                                                          | Status  | Details                                                                                              |
|------------------------------------|-----------------------------------------|--------------------------------------------------------------|---------|------------------------------------------------------------------------------------------------------|
| watchdog.rs supervisor_loop        | esp_idf_svc::sys::esp_restart()         | `unsafe { esp_idf_svc::sys::esp_restart(); }`                | WIRED   | Called at lines 67 and 81 inside the GNSS and MQTT threshold checks respectively                    |
| watchdog.rs supervisor_loop        | crate::config::WDT_CHECK_INTERVAL       | `std::thread::sleep(crate::config::WDT_CHECK_INTERVAL)`      | WIRED   | Line 55: sleep call uses the config constant; lines 46-47 also reference it in log output            |
| src/gnss.rs GNSS RX outer loop     | crate::watchdog::GNSS_RX_HEARTBEAT      | `fetch_add(1, Ordering::Relaxed)` at top of loop {}          | WIRED   | gnss.rs line 171; Ordering already imported (line 31); full crate:: path used, no new import needed |
| src/mqtt.rs while let Ok(event)    | crate::watchdog::MQTT_PUMP_HEARTBEAT    | `fetch_add(1, Ordering::Relaxed)` at top of while-let body   | WIRED   | mqtt.rs line 92; Ordering already imported (line 9); full crate:: path used                         |
| src/main.rs Step 18                | watchdog::spawn_supervisor()            | direct call after OTA spawn, before operational log          | WIRED   | main.rs line 200; `.expect("watchdog supervisor spawn failed")` on line 201                         |

### Requirements Coverage

| Requirement | Source Plan    | Description                                                                                              | Status    | Evidence                                                                                                   |
|-------------|----------------|----------------------------------------------------------------------------------------------------------|-----------|------------------------------------------------------------------------------------------------------------|
| WDT-01      | 11-01, 11-02   | Each critical thread (GNSS RX, MQTT pump) feeds a shared atomic watchdog counter at a regular interval (≤ 5s) | SATISFIED | GNSS RX increments every ~10ms (every loop iteration including idle/Ok(0) paths); MQTT pump increments on every connection.next() event including internal ping/pong (≤ 5s during normal connectivity) |
| WDT-02      | 11-01, 11-02   | A watchdog supervisor thread detects if any critical thread misses 3 consecutive heartbeats and triggers esp_restart() | SATISFIED | supervisor_loop checks every WDT_CHECK_INTERVAL (5s); calls esp_restart() after gnss_misses >= 3 or mqtt_misses >= 3; hardware TWDT panic mode (CONFIG_ESP_TASK_WDT_PANIC=y) backstops supervisor hang |

No orphaned requirements found. REQUIREMENTS.md maps both WDT-01 and WDT-02 to Phase 11; both are claimed by plans 11-01 and 11-02.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| None | —    | —       | —        | —      |

No TODO/FIXME/placeholder comments, empty implementations, or stub handlers found in any modified file.

### Human Verification Required

#### 1. Supervisor thread startup confirmation

**Test:** Flash firmware, open serial monitor (`cargo espflash monitor`), boot device.
**Expected:** Within the first 5 seconds of boot log, the line `[WDT] supervisor started — check interval 5s, miss threshold 3` appears, followed by `[HWM] WDT sup: N words (M bytes) stack remaining at entry`.
**Why human:** Thread scheduling and startup log output require a live device — cannot verify runtime thread launch or HWM value programmatically.

#### 2. No spurious reboots under nominal operation

**Test:** Allow device to run for 60 seconds with GNSS data flowing and MQTT connected. Observe serial log for unexpected `[WDT]` warn/error lines or unplanned reboots.
**Expected:** No `[WDT] GNSS RX heartbeat missed` or `[WDT] MQTT pump heartbeat missed` log lines appear. No reboot occurs. Normal operation log lines continue.
**Why human:** Requires runtime device behavior — cannot verify absence of false-positive hang detection without executing the firmware on hardware.

### Gaps Summary

No gaps. All 9 automated must-haves are verified against the actual codebase. Both requirement IDs (WDT-01, WDT-02) are fully satisfied by the implementation. Two human verification items remain — these are runtime/device behaviors that require flashing and observing the firmware on physical hardware.

Key implementation quality observations:
- The GNSS RX heartbeat is correctly placed at the **outer** `loop {}` level (line 171), not inside any `match uart_rx.read(...)` arm. This ensures the counter increments even when UART returns `Ok(0)` (UART stall), satisfying the plan's critical pitfall avoidance requirement.
- The MQTT pump heartbeat fires on every `connection.next()` iteration including internal MQTT ping/pong events, which means the counter updates well within the 5s WDT_CHECK_INTERVAL during normal broker connectivity.
- The supervisor is spawned last (Step 18) so all monitored threads are alive before the first 5s check interval elapses, preventing false positives at startup.
- `CONFIG_ESP_TASK_WDT_PANIC=y` adds the hardware TWDT backstop — if the supervisor thread itself hangs, the idle task stops being scheduled and the hardware TWDT fires at 30s, completing the layered defense.
- All 6 phase commits (`7d54c81`, `d41d8e1`, `c743cbf`, `1d0ac88`, `b054504`, `bca00de`) exist in git history and correspond to the documented tasks.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
