---
phase: 06-remote-config
verified: 2026-03-07T00:00:00Z
status: passed
score: 7/7 must-haves verified
human_verification:
  - test: "Publish JSON config payload to gnss/FFFEB5/config and observe per-command forwarding with custom delay"
    expected: "Log shows 'new config payload', then each command forwarded with ~200ms spacing when delay_ms:200 is set"
    why_human: "Requires live MQTT broker, flashed firmware, and espflash monitor — not automatable from source alone"
  - test: "Force MQTT reconnect after publishing retained config payload"
    expected: "Log shows 'payload unchanged (hash 0x...)' — commands are NOT re-forwarded to UM980"
    why_human: "Requires end-to-end MQTT reconnect cycle observable only in device serial monitor"
  - test: "Publish empty/null payload to clear retained message"
    expected: "Log shows 'retained message cleared, skipping' — no commands forwarded"
    why_human: "Requires MQTT retained-message lifecycle observable only via hardware"
  - test: "Publish plain text (no leading {) with newline-delimited commands"
    expected: "Each non-empty line forwarded with 100ms default delay between commands"
    why_human: "Requires live hardware and serial monitor to observe timing"
---

# Phase 6: Remote Config Verification Report

**Phase Goal:** Subscribe to `gnss/{device_id}/config` (QoS 1), parse the payload, and forward each command line-by-line to the UM980 over UART TX via the existing `gnss_cmd_tx: Sender<String>` channel. A per-command delay is applied between writes. Config is only applied when it changes (hash-based deduplication). Error handling is log-only.

**Verified:** 2026-03-07
**Status:** human_needed (all automated checks pass; runtime behavior requires hardware observation)
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | `config_relay.rs` compiles with `spawn_config_relay`, `djb2_hash`, `apply_config`, and `parse_config_json` | VERIFIED | All four functions present at lines 26, 69, 85, 133. `cargo build` produces zero errors. |
| 2  | `pump_mqtt_events` routes `EventPayload::Received` payloads to `config_tx` channel without blocking | VERIFIED | `EventPayload::Received { data, .. }` arm at mqtt.rs:94 calls `config_tx.send(data.to_vec())` — mpsc send only, no client method called |
| 3  | Hash deduplication skips unchanged payloads and guards against empty payloads | VERIFIED | Empty guard at config_relay.rs:38; djb2 hash computed at line 43; `hash == last_hash` check at line 45 with skip and log |
| 4  | Per-command delay of 100ms default (overridable via `delay_ms` JSON field) is implemented | VERIFIED | `let delay_ms: u64 = 100` in plain-text path (line 106); `parse_config_json` extracts `delay_ms` from JSON with `.unwrap_or(100)` fallback (line 144); `thread::sleep(Duration::from_millis(delay_ms))` at line 123 |
| 5  | `gnss_cmd_tx.send()` failure causes log + abandon remaining commands (no panic) | VERIFIED | `Err(e)` arm at config_relay.rs:116-120 logs error and returns immediately — no unwrap, no panic |
| 6  | Config topic subscribed at QoS 1 | VERIFIED | `subscriber_loop` at mqtt.rs:130 calls `c.subscribe(&topic, QoS::AtLeastOnce)` for topic `gnss/{device_id}/config` |
| 7  | `config_relay` fully wired into `main.rs` (channel, pump call, Step 15 spawn) | VERIFIED | `mod config_relay` at main.rs:35; Step 9b channel at line 112; `config_tx` passed to `pump_mqtt_events` at line 117; `spawn_config_relay(gnss_cmd_tx.clone(), config_rx)` at line 151 |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/config_relay.rs` | `spawn_config_relay`, `djb2_hash`, `apply_config`, `parse_config_json` | VERIFIED | 169-line file, all four functions present and substantive — no stubs |
| `src/mqtt.rs` | `pump_mqtt_events` with `config_tx: Sender<Vec<u8>>` parameter and `Received` routing arm | VERIFIED | Signature at line 77; Received arm at lines 94-100; arm placed before catch-all `m @ _` |
| `src/main.rs` | Step 9b channel, Step 10 pump call with `config_tx`, Step 15 relay spawn | VERIFIED | All three changes present; init-order comment block includes Step 15 at line 20 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/mqtt.rs pump_mqtt_events` | `config_tx` channel | `config_tx.send(data.to_vec())` in `EventPayload::Received` arm | WIRED | Pattern `config_tx\.send\(data\.to_vec` found at mqtt.rs:96 |
| `src/config_relay.rs apply_config` | `gnss_cmd_tx` | `gnss_cmd_tx.send(cmd.to_string())` | WIRED | Pattern `gnss_cmd_tx\.send\(cmd\.to_string` found at config_relay.rs:113 |
| `src/main.rs Step 10 (pump spawn)` | `mqtt::pump_mqtt_events` | `config_tx` passed as third argument | WIRED | Pattern `pump_mqtt_events.*config_tx` at main.rs:117 |
| `src/main.rs Step 15` | `config_relay::spawn_config_relay` | `gnss_cmd_tx.clone()` and `config_rx` moved in | WIRED | Pattern `spawn_config_relay\(gnss_cmd_tx\.clone` at main.rs:151 |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| CONF-01 | 06-01-PLAN, 06-02-PLAN | Device subscribes to `gnss/{device_id}/config` (QoS 1) and forwards received payload line-by-line to UM980 over UART TX | SATISFIED (code) / NEEDS HUMAN (runtime) | Subscription in `subscriber_loop` (mqtt.rs:130 with QoS::AtLeastOnce); payload routing from pump to relay via mpsc; line-by-line forwarding via `gnss_cmd_tx.send(cmd.to_string())` in relay. Hardware observation documented in 06-02-SUMMARY.md. |
| CONF-02 | 06-01-PLAN, 06-02-PLAN | Device queues received config messages and only applies them after the UART driver has been fully initialized and is ready to accept writes | SATISFIED | UART initialized by `gnss::spawn_gnss` before relay starts (main.rs steps 7 before 15); djb2 hash dedup prevents re-application on reconnect; empty payload guard prevents clearing events from triggering commands |
| CONF-03 | 06-01-PLAN, 06-02-PLAN | Device applies a per-command delay between UART TX writes to allow the UM980 processing window | SATISFIED (code) / NEEDS HUMAN (timing) | `thread::sleep(Duration::from_millis(delay_ms))` after each send; 100ms default; overridable via JSON `delay_ms` field. Timing observable only via hardware. |

All three CONF requirements have supporting code. Hardware verification was documented as approved in 06-02-SUMMARY.md for device FFFEB5, but that is a SUMMARY claim. The runtime behavior items are flagged for human confirmation below.

### Anti-Patterns Found

None. No TODOs, FIXMEs, placeholder returns, or empty implementations found in any of the three modified files. All function bodies are substantive.

### Human Verification Required

The following items cannot be verified from source alone — they require flashed firmware, a live MQTT broker, and serial monitor observation.

#### 1. CONF-01: JSON Config Payload Forwarding

**Test:** Flash firmware and publish:
```
mosquitto_pub -h <host> -u <user> -P <pass> \
  -t 'gnss/FFFEB5/config' -r \
  -m '{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}'
```
**Expected:** Serial monitor shows `Config relay: new config payload, hash 0x...`, then `Config relay: sending command: "MODE ROVER"`, then (approximately 200ms later) `Config relay: sending command: "CONFIGSAVE"`.
**Why human:** Requires live device, MQTT broker connection, and timing observation.

#### 2. CONF-02: Hash Deduplication on Reconnect

**Test:** After step 1, power-cycle device or force MQTT reconnect without changing the retained payload.
**Expected:** Serial monitor shows `Config relay: payload unchanged (hash 0x...), skipping` — no "sending command" lines appear.
**Why human:** Requires MQTT retained-message lifecycle and device reconnect cycle, observable only via hardware.

#### 3. CONF-02: Empty Payload Guard

**Test:** Clear the retained message with:
```
mosquitto_pub -h <host> -u <user> -P <pass> -t 'gnss/FFFEB5/config' -r -n
```
**Expected:** Serial monitor shows `Config relay: empty payload — retained message cleared, skipping` — no commands forwarded.
**Why human:** Requires retained-message deletion and hardware observation.

#### 4. CONF-03: Plain Text Fallback with Default Delay

**Test:** Publish plain text (no leading `{`):
```
mosquitto_pub -h <host> -u <user> -P <pass> \
  -t 'gnss/FFFEB5/config' -r \
  -m $'MODE ROVER\nCONFIGSAVE'
```
**Expected:** Both commands forwarded with approximately 100ms between them (default delay, no JSON header).
**Why human:** Default delay timing requires serial monitor observation.

### Gaps Summary

No gaps found. All artifacts exist, are substantive, and are wired. The build compiles to zero errors. Commits 938ee8e, c2fc72f, and 685acdd confirm the three atomic changes documented in SUMMARYs exist in git history with matching diffs.

The four human-verification items above are behavioral (runtime) tests that require hardware. They were claimed as approved in 06-02-SUMMARY.md for device FFFEB5. If that hardware approval is accepted, the phase status is effectively "passed." The human_needed classification is used here because the verifier cannot independently confirm runtime behavior from source alone.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
