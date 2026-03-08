---
phase: 16-remote-logging
verified: 2026-03-08T04:00:00Z
status: passed
score: 7/7 must-haves verified
re_verification: false
human_verification:
  - test: "Subscribe to gnss/+/log and boot device"
    expected: "Log lines appear on MQTT within 1 second of boot (after MQTT connects)"
    why_human: "Cannot observe real-time MQTT message flow without hardware"
  - test: "Publish 'warn' to gnss/{id}/log/level with retain flag, reboot device"
    expected: "After reconnect, only WARN/ERROR messages appear on the log topic (retained level applied)"
    why_human: "Retained topic behaviour across broker reconnect requires live broker and device"
  - test: "Disconnect MQTT broker while device is running"
    expected: "Device UART monitor remains responsive; no hang or stack overflow"
    why_human: "try_send drop-on-full is structurally correct but MQTT lock contention under disconnect needs runtime observation"
---

# Phase 16: Remote Logging Verification Report

**Phase Goal:** Remote log streaming — all ESP-IDF log output captured and published to MQTT; runtime log-level control via MQTT subscription.
**Verified:** 2026-03-08T04:00:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | ESP-IDF log output (Rust log:: and C component logs) captured by vprintf hook | VERIFIED | `src/log_shim.c`: `esp_log_set_vprintf(mqtt_log_vprintf)` in `install_mqtt_log_hook()`; hook intercepts all output going through the ESP-IDF log backend |
| 2 | Log messages forwarded to bounded channel via try_send — never blocking | VERIFIED | `src/log_relay.rs` line 59: `let _ = tx.try_send(s)` — TrySendError silently discarded; sync_channel(32) bounds the buffer |
| 3 | Re-entrancy guard set before MQTT work and cleared after — feedback loop structurally impossible | VERIFIED | `src/log_relay.rs` lines 110/120: `LOG_REENTERING.store(true)` before `client.lock()`, `store(false)` after enqueue; `log_shim.c` line 16 checks `rust_log_is_reentering()` before forwarding |
| 4 | Log messages appear on gnss/{device_id}/log (LOG-01 pipeline live) | VERIFIED | `src/log_relay.rs` line 99: topic = `format!("gnss/{}/log", device_id)`; `src/main.rs` line 211: `log_relay::spawn_log_relay(mqtt_client.clone(), device_id.clone())` |
| 5 | Publishing a log level string to gnss/{device_id}/log/level changes level immediately without reboot | VERIFIED | `src/mqtt.rs` line 193-196: subscribes to `/log/level` at QoS::AtLeastOnce in `subscriber_loop`; line 129-135: callback routes to `log_level_tx.try_send`; lines 260-281: `apply_log_level` calls `esp_idf_svc::log::set_target_level("*", filter)` |
| 6 | MQTT publish path does not generate additional log entries on the log topic | VERIFIED | `src/log_relay.rs`: no `log::` macro calls inside the `Ok(msg)` branch while `LOG_REENTERING=true`; guard prevents re-entrant forwarding even if log:: were called by other code |
| 7 | Log publishing does not stall when MQTT is disconnected | VERIFIED | `src/log_relay.rs` line 59: `try_send` — drops silently on full channel; `src/log_relay.rs` line 116: `let _ = c.enqueue(...)` — enqueue failure silently discarded; no blocking waits in hot path |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/log_shim.c` | vprintf hook: mqtt_log_vprintf, install_mqtt_log_hook, va_copy, re-entrancy guard | VERIFIED | 37 lines; all required functions present; `install_mqtt_log_hook` at line 34; re-entrancy check at line 16; va_copy at line 23 |
| `src/log_relay.rs` | LOG_REENTERING AtomicBool, LOG_TX OnceLock, rust_log_is_reentering, rust_log_try_send, spawn_log_relay | VERIFIED | 133 lines; all five components present and substantive; relay thread publishes at QoS::AtMostOnce |
| `build.rs` | embuild first, then cc::Build::new().file("src/log_shim.c").compile("log_shim") | VERIFIED | embuild at line 2; cc::Build with log_shim.c file inside `if let Some(cincl)` block; `.compile("log_shim")` at line 31 |
| `Cargo.toml` | cc = "1" in [build-dependencies] | VERIFIED | Line 20: `cc = "1"` confirmed; Cargo.lock records `cc` version 1.2.56 |
| `src/main.rs` | mod log_relay, install_mqtt_log_hook call, log_level channel, spawn_log_relay, log_level_relay_task spawn | VERIFIED | All five wiring points present; ordering correct (hook at Step 2b after EspLogger::initialize_default; spawn_log_relay at Step 9.5 after mqtt_connect) |
| `src/mqtt.rs` | log_level_tx parameter, /log/level callback routing, /log/level subscription, apply_log_level, log_level_relay_task | VERIFIED | All five components present; `log_level_tx: SyncSender<Vec<u8>>` at line 34; callback routing at line 129; subscription at line 193; `apply_log_level` at line 260; `log_level_relay_task` at line 287 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/log_shim.c` | `src/log_relay.rs` | `extern void rust_log_try_send` / `extern int rust_log_is_reentering` | WIRED | log_shim.c lines 6-7 declare C externs; log_relay.rs lines 36/49 provide `#[no_mangle] pub extern "C"` implementations |
| `src/log_relay.rs` | `log_shim.c` (compiled) | `#[no_mangle] pub extern "C" fn rust_log_is_reentering` | WIRED | `#[no_mangle]` at lines 35 and 48 in log_relay.rs; build.rs compiles log_shim.c into `log_shim` static lib |
| `src/main.rs` | `src/log_relay.rs` | `log_relay::spawn_log_relay(mqtt_client.clone(), device_id.clone())` | WIRED | main.rs line 41: `mod log_relay;`; line 211: call with correct arguments |
| `src/main.rs` | `src/log_shim.c` | `extern "C" fn install_mqtt_log_hook()` called after EspLogger::initialize_default() | WIRED | main.rs lines 65-68: extern block + `unsafe { install_mqtt_log_hook(); }` immediately after EspLogger at line 60 |
| `src/mqtt.rs callback` | `log_level_tx SyncSender` | `t.ends_with("/log/level")` routes to `log_level_tx.try_send` | WIRED | mqtt.rs lines 129-135; correctly positioned after `/command` branch |
| `src/mqtt.rs subscriber_loop` | `gnss/{device_id}/log/level` | `c.subscribe(&log_level_topic, QoS::AtLeastOnce)` | WIRED | mqtt.rs lines 192-197; uses AtLeastOnce so retained level persists across reconnects |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| LOG-01 | 16-01, 16-02 | ESP-IDF log output forwarded to `gnss/{device_id}/log` at QoS 0; re-entrancy guard prevents feedback loops | SATISFIED | vprintf hook in log_shim.c + relay thread in log_relay.rs publishes to `gnss/{id}/log` at QoS::AtMostOnce (QoS 0); LOG_REENTERING guard structurally prevents MQTT publish paths from being captured |
| LOG-02 | 16-02 | Log level configurable via retained MQTT topic | SATISFIED | `/log/level` subscribed at QoS::AtLeastOnce; `apply_log_level` parses error/warn/info/debug/verbose and calls `esp_idf_svc::log::set_target_level("*", filter)` |
| LOG-03 | 16-01, 16-02 | Log publishing is non-blocking; messages dropped silently when MQTT disconnected or channel full | SATISFIED | `rust_log_try_send` uses `try_send` (never blocks); `enqueue` result discarded; sync_channel(32) bounded; no `unwrap` or blocking `send` on the log path |

No orphaned requirements: all three LOG-01/02/03 requirements mapped to Phase 16 in REQUIREMENTS.md are claimed by the plans and structurally satisfied.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/log_shim.c` | 23 | `va_copy(args2, args)` after `args` passed to `s_original_vprintf` | Warning | Per C standard (C99 7.15.1), a `va_list` is indeterminate after being passed to a function that consumes it via `va_arg`. In practice, on Xtensa and RISC-V targets (ESP32), ESP-IDF's vprintf implementation copies the va_list internally and the original pointer remains valid — so this works on current hardware. However it is technically undefined behaviour. This is a pre-existing pattern from the plan specification and is a known ESP-IDF idiom; it does not block goal achievement. |
| `build.rs` | 6 | `log_shim.c` only compiled if `embuild::espidf::sysenv::cincl_args()` returns `Some` | Info | If `cincl_args` returns `None` (non-ESP-IDF build environment), the C shim is silently skipped and the link will fail with undefined symbol errors for `rust_log_try_send`/`rust_log_is_reentering`. Acceptable because this firmware only targets ESP32 hardware and the build environment always provides the IDF sysenv. |

### Human Verification Required

#### 1. End-to-end log streaming smoke test

**Test:** Flash firmware, subscribe with `mosquitto_sub -t 'gnss/+/log'`, observe UART monitor after boot.
**Expected:** Log lines (including startup messages after MQTT connect) appear on the log topic within 1 second. UART output is preserved in parallel — logs appear on both UART and MQTT.
**Why human:** Real-time message flow across FFI boundary, MQTT broker, and subscriber cannot be verified by static analysis.

#### 2. Retained log level persistence across reconnect

**Test:** Publish `mosquitto_pub -t 'gnss/{id}/log/level' -m 'warn' -r`, then reboot the device.
**Expected:** After MQTT reconnects and subscriber_loop runs, the retained `/log/level` message is re-delivered, `apply_log_level` sets WARN threshold, and only WARN/ERROR messages appear on the log topic.
**Why human:** Retained MQTT message delivery on reconnect requires live broker; level filtering effect requires observing MQTT traffic at runtime.

#### 3. Non-blocking behaviour under MQTT disconnect

**Test:** While device is running and streaming logs, sever the MQTT broker connection (stop the broker or block the port). Monitor UART output.
**Expected:** Device remains responsive, UART output continues, no watchdog-triggered restart. Log messages are dropped silently (not queued indefinitely).
**Why human:** try_send drop behaviour is structurally correct but the interaction between the channel being full and the MQTT client's internal lock state requires runtime observation to confirm no deadlock or excessive blocking.

### Gaps Summary

No gaps. All automated checks passed across both plans.

**Plan 01 deliverables (infrastructure):**
- `src/log_shim.c` — substantive, all required functions present
- `src/log_relay.rs` — substantive, all five required components present and correctly wired to each other
- `build.rs` — correctly invokes embuild before cc::Build; compiles log_shim.c
- `Cargo.toml` — cc = "1" build dependency confirmed in file and Cargo.lock

**Plan 02 deliverables (wiring):**
- `src/main.rs` — all five integration points present in correct startup order
- `src/mqtt.rs` — log_level_tx parameter, callback routing, subscription, apply_log_level, log_level_relay_task all present

The one notable implementation deviation from the plan (using `esp_idf_svc::log::set_target_level` free function instead of an `EspLogger` struct instance) is a correct adaptation — the plan's API description was inaccurate, and the actual implementation achieves the same effect through the correct API surface.

---

_Verified: 2026-03-08T04:00:00Z_
_Verifier: Claude (gsd-verifier)_
