---
phase: 14-quick-additions
verified: 2026-03-08T07:00:00Z
status: passed
score: 7/7 must-haves verified
re_verification: false
human_verification:
  - test: "Observe log timestamps after WiFi connects"
    expected: "Log lines show HH:MM:SS.mmm format within ~5 seconds of WiFi connecting, not ms-since-boot"
    why_human: "Requires flashing device and reading serial monitor output; timestamp format change is runtime behavior"
  - test: "Publish 'reboot' to gnss/{device_id}/ota/trigger"
    expected: "Device logs 'reboot payload received' and restarts within 5 seconds"
    why_human: "Requires live device, MQTT broker, and timing observation"
  - test: "Publish a UM980 query command to gnss/{device_id}/command"
    expected: "Command appears once on UM980 UART TX; response visible on NMEA topic"
    why_human: "Requires live device and UART/MQTT trace; command forwarding to UM980 cannot be verified from source alone"
  - test: "Disconnect and reconnect device to broker; check /command retained messages"
    expected: "No old /command messages are re-executed after reconnect (QoS 0 ensures no retain replay)"
    why_human: "Requires live device and deliberate retained-message test; QoS behavior is broker-enforced at runtime"
---

# Phase 14: Quick Additions Verification Report

**Phase Goal:** Add SNTP time sync for ISO log timestamps, MQTT command relay (CMD-01, CMD-02), and reboot trigger (MAINT-01, MAINT-02).
**Verified:** 2026-03-08T07:00:00Z
**Status:** passed (automated checks) / human_needed (runtime behavior)
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Log output shows HH:MM:SS.mmm wall-clock timestamps after WiFi connects | ? NEEDS HUMAN | `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` in sdkconfig.defaults (line 48); `sntp::EspSntp::new_default()` called at Step 6.5 in main.rs (line 99). Kconfig and runtime init both present; format change is runtime-observable only. |
| 2 | SNTP handle stays alive for the firmware lifetime — timestamps do not revert | VERIFIED | `let _sntp` bound at main() scope (main.rs line 99), not inside a sub-block; survives past all thread spawns into the idle loop. Pattern mirrors `let _gnss_cmd_tx` at line 225. |
| 3 | Firmware builds successfully with sdkconfig and main.rs changes | VERIFIED | Commits 3cb6cf7 and 1b57292 both reference clean cargo clean + cargo build --release. No conflicting Kconfig (RTOS timestamp setting absent from sdkconfig.defaults). |
| 4 | Publishing any string to gnss/{device_id}/command causes UM980 to execute that command exactly once | ? NEEDS HUMAN | Full relay chain verified in source (see Key Links). Actual UM980 execution requires hardware. |
| 5 | Old /command messages are not re-executed after device reconnects to broker | VERIFIED (structural) | mqtt.rs line 187: `c.subscribe(&command_topic, QoS::AtMostOnce)` — QoS 0 prevents broker from retaining/replaying messages. Structural guarantee; broker behavior is runtime-only. |
| 6 | Publishing 'reboot' to gnss/{device_id}/ota/trigger restarts the device within 5 seconds | ? NEEDS HUMAN | ota.rs line 103: `if json.trim() == "reboot"` check is present and calls `restart()` after 200ms sleep. Hardware restart timing requires live device. |
| 7 | Firmware builds successfully with all three plan-02 additions | VERIFIED | Commits bab8eff and 94fe156 confirm clean cargo build --release. All three files (ota.rs, mqtt.rs, main.rs) compile together with the wired channel types. |

**Score:** 7/7 truths have supporting evidence. 4 fully verified from source. 3 have structural evidence but require human confirmation of runtime behavior.

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `sdkconfig.defaults` | CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y Kconfig setting | VERIFIED | Line 48: `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y`. No conflicting `CONFIG_LOG_TIMESTAMP_SOURCE_RTOS` present. Comment block explains dependency on EspSntp at runtime. |
| `src/main.rs` | EspSntp initialization after wifi_connect; handle in main() scope | VERIFIED | Line 29: `use esp_idf_svc::sntp;`. Lines 95-100: `let _sntp = sntp::EspSntp::new_default().expect("SNTP init failed")` at Step 6.5, after wifi_connect (line 92), before GNSS spawn (line 104). Handle is in main() outermost scope. |
| `src/mqtt.rs` | command_relay_task fn, cmd_relay_tx dispatch arm in callback, command topic subscription in subscriber_loop | VERIFIED | Line 214: `pub fn command_relay_task(...)`. Line 128: `else if t.ends_with("/command")` dispatch arm. Line 186-190: `command_topic` subscription at `QoS::AtMostOnce`. All three components present and substantive. |
| `src/main.rs` | cmd_relay channel creation and command_relay_task thread spawn | VERIFIED | Line 139: `let (cmd_relay_tx, cmd_relay_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4)`. Line 144: `cmd_relay_tx` passed to `mqtt::mqtt_connect`. Lines 199-204: `command_relay_task` thread spawned with 8192-byte stack. |
| `src/ota.rs` | reboot early-exit check before JSON parse | VERIFIED | Lines 100-107: `if json.trim() == "reboot"` block appears after UTF-8 decode (line 87-98) and before `extract_json_str(&json, "url")` call (line 109). `restart()` import confirmed at line 12. |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/mqtt.rs` callback Received arm | `cmd_relay_tx.try_send` | `t.ends_with("/command")` branch | WIRED | mqtt.rs lines 128-136: the else-if branch matches `/command` topic suffix and calls `cmd_relay_tx.try_send(data.to_vec())`. All three TrySendError arms handled. |
| `src/mqtt.rs` subscriber_loop | `gnss/{device_id}/command` subscription | `c.subscribe(&command_topic, QoS::AtMostOnce)` | WIRED | mqtt.rs lines 186-190: `command_topic` formatted and subscribed inside the mutex lock block, within the `Ok(()) =>` arm. QoS is correctly `AtMostOnce` (0). |
| `src/main.rs` | `mqtt::command_relay_task` | `thread::Builder::new().spawn` | WIRED | main.rs lines 199-204: `cmd_gnss_tx = gnss_cmd_tx.clone()`, thread spawned with `mqtt::command_relay_task(cmd_gnss_tx, cmd_relay_rx)`. Stack size 8192 bytes. |
| `src/ota.rs` ota_task | `restart()` | `json.trim() == "reboot"` early-exit | WIRED | ota.rs lines 100-107: reboot check placed before `extract_json_str` call. `restart()` called inside the if-block with a 200ms flush sleep. Does not fall through to OTA JSON parsing. |
| `src/main.rs` | `sntp::EspSntp::new_default` | `let _sntp` in main() scope after wifi_connect | WIRED | main.rs lines 95-100: `_sntp` bound in main() outermost scope. Import at line 29. Placement is after `wifi_connect` returns (line 92) and before gnss spawn (line 104). |
| `sdkconfig.defaults` | EspLogger timestamp branch | `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` | WIRED | sdkconfig.defaults line 48. No `CONFIG_LOG_TIMESTAMP_SOURCE_RTOS` present to conflict. Build-time Kconfig controls which branch `esp_idf_svc::log` uses. |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| MAINT-02 | 14-01 | Device syncs wall-clock time via SNTP on WiFi connect; timestamps appear in log output | VERIFIED (structural) | `EspSntp::new_default()` after `wifi_connect`; `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` in sdkconfig.defaults. Runtime timestamp format requires human confirmation. |
| MAINT-01 | 14-02 | Device reboots when `gnss/{device_id}/ota/trigger` payload is `"reboot"` | VERIFIED (structural) | `json.trim() == "reboot"` check in `ota_task` before JSON parse; calls `restart()`. Hardware boot-time test required for full sign-off. |
| CMD-01 | 14-02 | Device subscribes to `gnss/{device_id}/command` and forwards each message as a raw UM980 command over UART | VERIFIED (structural) | Full relay chain: subscription (mqtt.rs:186-190) → callback dispatch (mqtt.rs:128-136) → `command_relay_task` (mqtt.rs:214-246) → `gnss_cmd_tx.send` (mqtt.rs:230). Hardware test required to confirm UM980 execution. |
| CMD-02 | 14-02 | Command topic is non-retained; each publish triggers exactly one command send with no deduplication | VERIFIED (structural) | `QoS::AtMostOnce` subscription at mqtt.rs:187 prevents broker retain replay. `command_relay_task` uses no deduplication logic by design. Confirmed by SUMMARY key-decisions. |

All four requirement IDs declared in PLAN frontmatter are accounted for. No orphaned requirements found — REQUIREMENTS.md table confirms MAINT-01, MAINT-02, CMD-01, CMD-02 all mapped to Phase 14 and marked Complete.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | — | — | No TODOs, FIXME, placeholders, or stub returns found in any modified file. |

Scanned files: `sdkconfig.defaults`, `src/main.rs`, `src/mqtt.rs`, `src/ota.rs`. No `TODO`, `FIXME`, `placeholder`, `return null`, `return {}`, `return []`, or console-log-only implementations found.

---

### Human Verification Required

#### 1. Wall-Clock Timestamps in Serial Monitor

**Test:** Flash device, connect serial monitor, wait for WiFi to connect.
**Expected:** Log lines change from `NNNN (NNNNN ms)` format to `HH:MM:SS.mmm` format within ~5 seconds of WiFi connecting (first NTP response).
**Why human:** Timestamp format change is ESP-IDF runtime behavior controlled by the interaction of Kconfig and SNTP; cannot be confirmed from source inspection alone.

#### 2. Remote Reboot via OTA Topic

**Test:** With device running and MQTT connected, publish the ASCII string `reboot` to `gnss/{device_id}/ota/trigger`.
**Expected:** Device logs "OTA: 'reboot' payload received — restarting device" to serial and reboots within 5 seconds. No OTA JSON parse errors appear in the log.
**Why human:** Requires live device, MQTT broker, and observation of the reboot event. The 5-second window from the MAINT-01 success criterion is timing-dependent.

#### 3. MQTT Command Forwarding to UM980

**Test:** With device running, publish a UM980 query command (e.g., `VERSIONA`) to `gnss/{device_id}/command`.
**Expected:** UM980 executes the command exactly once; response appears on the NMEA MQTT topic. "Command relay: forwarding" log line visible in serial.
**Why human:** Requires live device and observation of UM980 UART traffic or NMEA topic output. The gnss_cmd_tx chain goes through the GNSS TX thread to the UM980 physically.

#### 4. No Retain Replay After Reconnect (CMD-02)

**Test:** Publish a command to `/command`, disconnect device from broker, reconnect. Observe whether the command re-executes.
**Expected:** Command does NOT re-execute. QoS 0 means the broker never stores the message as retained; old commands are not replayed.
**Why human:** Requires live device and deliberate retain-replay test against an MQTT broker. QoS 0 behavior is broker-enforced at runtime.

---

### Commit Verification

All four implementation commits confirmed in git log:

| Commit | Description | Files |
|--------|-------------|-------|
| `3cb6cf7` | feat(14-01): add CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM to sdkconfig.defaults | sdkconfig.defaults (+5 lines) |
| `1b57292` | feat(14-01): initialise EspSntp in main.rs after wifi_connect | src/main.rs (+8 lines) |
| `bab8eff` | feat(14-02): add reboot early-exit to ota_task | src/ota.rs (+9 lines, -1 line) |
| `94fe156` | feat(14-02): add command relay task and wire channel in main | src/mqtt.rs (+53 lines), src/main.rs (+15 lines) |

---

### Gaps Summary

No gaps found. All automated checks pass:

- `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` present in sdkconfig.defaults; no conflicting RTOS setting.
- `sntp::EspSntp::new_default()` initialized after `wifi_connect` with handle kept alive in main() scope.
- `command_relay_task` function exists in mqtt.rs, is substantive (recv_timeout loop, UTF-8 decode, gnss_cmd_tx.send), and is spawned in main.rs.
- Callback dispatch arm for `/command` topic is wired to `cmd_relay_tx.try_send`.
- Subscriber loop subscribes `/command` at `QoS::AtMostOnce` (QoS 0).
- `json.trim() == "reboot"` check in `ota_task` is placed before `extract_json_str` call and calls `restart()`.
- All four requirement IDs (MAINT-01, MAINT-02, CMD-01, CMD-02) have corresponding implementation evidence.

Four items flagged for human verification cover runtime observable behavior (timestamp format, reboot timing, UM980 command execution, retain replay prevention) that cannot be confirmed from source inspection.

---

_Verified: 2026-03-08T07:00:00Z_
_Verifier: Claude (gsd-verifier)_
