---
phase: 13-health-telemetry
verified: 2026-03-07T15:10:00Z
status: human_needed
score: 4/4 must-haves verified
human_verification:
  - test: "Flash device and run mosquitto_sub -t 'gnss/+/heartbeat' -- verify JSON payload with all 5 fields (uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors) arrives approximately every 30 seconds"
    expected: "JSON message appears every ~30s with all 5 numeric fields present and non-stale uptime_s incrementing across messages"
    why_human: "Embedded target — requires live device + MQTT broker; cargo build cannot verify broker delivery or payload timing"
  - test: "Disconnect device from broker then reconnect; subscribe with mosquitto_sub -r -t 'gnss/+/status' and observe the retained message"
    expected: "Retained 'online' message appears on gnss/{device_id}/status after reconnect, overwriting the LWT 'offline'"
    why_human: "Requires live broker with LWT retain semantics; cannot verify broker retain-message overwrite programmatically"
  - test: "Saturate NMEA or RTCM channel (reduce channel capacity or increase GNSS output rate) and observe nmea_drops or rtcm_drops in subsequent heartbeat payloads"
    expected: "Counter values increase in heartbeat JSON, confirming fetch_add paths are exercised at runtime"
    why_human: "Requires hardware-induced channel saturation; static analysis confirms code path exists but runtime exercise needs live device"
---

# Phase 13: Health Telemetry Verification Report

**Phase Goal:** Expose real-time health telemetry (drop counters + heartbeat JSON) via MQTT so operators can monitor GNSS pipeline health without additional hardware.
**Verified:** 2026-03-07T15:10:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Every 30 seconds, a JSON payload appears on gnss/{device_id}/heartbeat with fields uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors | ? HUMAN NEEDED | Code path confirmed: mqtt.rs:239-248 builds the 5-field JSON and enqueues to heartbeat_topic with retain=false; sleep uses HEARTBEAT_INTERVAL_SECS (30). Runtime delivery requires live device. |
| 2 | On reconnect, a retained 'online' message appears on gnss/{device_id}/status, overwriting the LWT 'offline' | ? HUMAN NEEDED | Code path confirmed: mqtt.rs:219 enqueues b"online" to status_topic with QoS::AtLeastOnce and retain=true, once before the heartbeat loop. Runtime broker behaviour requires live device. |
| 3 | NMEA and RTCM drop events in gnss.rs increment their respective atomic counters, not just log a warning | ✓ VERIFIED | gnss.rs:219 NMEA_DROPS.fetch_add(1, Ordering::Relaxed) at TrySendError::Full in NmeaLine arm; gnss.rs:305 RTCM_DROPS.fetch_add(1, Ordering::Relaxed) at TrySendError::Full in RtcmBody arm. Pool buffer return (free_pool_tx_clone.try_send) preserved at line 310. |
| 4 | The heartbeat interval is a named constant in config.rs, not a hardcoded literal | ✓ VERIFIED | config.example.rs:64 declares `pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;`. mqtt.rs:254 sleeps `crate::config::HEARTBEAT_INTERVAL_SECS`. grep for `from_secs(30)` in mqtt.rs returns no results in heartbeat_loop. |

**Score:** 4/4 truths structurally verified; 2/4 require human confirmation for runtime delivery

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/gnss.rs` | pub NMEA_DROPS, pub RTCM_DROPS, pub UART_TX_ERRORS atomics; incremented at TrySendError::Full sites | ✓ VERIFIED | Lines 60, 64, 69: all three pub static AtomicU32 present. Lines 219, 305: fetch_add at both drop sites. Pool return preserved at line 310. |
| `src/mqtt.rs` | Extended heartbeat_loop: one-time retained online publish + JSON health snapshot | ✓ VERIFIED | Lines 217-223: retained online enqueue before loop. Lines 225-255: loop reads 3 counters + 2 system calls, builds 5-field JSON, enqueues to heartbeat_topic with retain=false. |
| `src/config.example.rs` | HEARTBEAT_INTERVAL_SECS constant | ✓ VERIFIED | Line 64: `pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;` with doc comment. |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| src/mqtt.rs:heartbeat_loop | src/gnss.rs:NMEA_DROPS / RTCM_DROPS / UART_TX_ERRORS | crate::gnss::NMEA_DROPS.load(Ordering::Relaxed) | ✓ WIRED | mqtt.rs:227-229: all three cross-module loads present via crate::gnss:: full path |
| src/mqtt.rs:heartbeat_loop | gnss/{device_id}/status (broker) | c.enqueue(&status_topic, QoS::AtLeastOnce, true, b"online") | ✓ WIRED | mqtt.rs:219: enqueue with retain=true (third parameter) and b"online" payload, outside loop |
| src/mqtt.rs:heartbeat_loop | gnss/{device_id}/heartbeat (broker) | c.enqueue(&heartbeat_topic, QoS::AtMostOnce, false, json.as_bytes()) | ✓ WIRED | mqtt.rs:248: enqueue with retain=false (third parameter), json.as_bytes() payload, inside loop |
| src/main.rs | src/mqtt.rs:heartbeat_loop | std::thread::spawn -> mqtt::heartbeat_loop | ✓ WIRED | main.rs:159: .spawn(move \|\| mqtt::heartbeat_loop(hb_client, hb_device_id)) |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| METR-01 | 13-01-PLAN.md | Device publishes heartbeat JSON with health fields to MQTT | ✓ SATISFIED (with note) | Implementation publishes 5-field JSON to gnss/{device_id}/heartbeat every 30s. REQUIREMENTS.md text says "/status" topic and "60 seconds" and "4 fields" — these are stale draft values. The PLAN's must_haves (the authoritative spec) match the implementation: /heartbeat topic, 30s interval, 5 fields. |
| METR-02 | 13-01-PLAN.md | NMEA and RTCM drop counters are atomic; incremented at each TrySendError::Full drop site | ✓ SATISFIED | gnss.rs:219 NMEA_DROPS.fetch_add at NmeaLine::Full; gnss.rs:305 RTCM_DROPS.fetch_add at RtcmBody::Full. Both use Ordering::Relaxed. UART_TX_ERRORS already existed and is now pub. |

**Note on METR-01 description drift:** REQUIREMENTS.md line 39 describes the topic as `gnss/{device_id}/status`, the interval as 60 seconds, and only 4 fields. The PLAN's must_haves specify `gnss/{device_id}/heartbeat`, 30 seconds, and 5 fields (adding `uart_tx_errors`). The implementation matches the PLAN. REQUIREMENTS.md was not updated to reflect the final design decision. This is a documentation gap, not an implementation gap — the PLAN is the governing contract for execution.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| None | — | — | — | — |

No TODO/FIXME/placeholder comments, empty implementations, or hardcoded literals found in modified files. The previously hardcoded `from_secs(30)` in the old heartbeat_loop has been replaced by `crate::config::HEARTBEAT_INTERVAL_SECS`.

---

### Commit Verification

Both commits claimed in SUMMARY.md exist and modify the correct files:

- `3d26005` — `feat(13-01): add NMEA_DROPS and RTCM_DROPS atomics` — modifies `src/gnss.rs` only
- `e6ee247` — `feat(13-01): add HEARTBEAT_INTERVAL_SECS constant; extend heartbeat_loop` — modifies `src/config.example.rs` and `src/mqtt.rs`

---

### Human Verification Required

#### 1. Heartbeat JSON delivery on broker

**Test:** Flash device. Subscribe with `mosquitto_sub -t 'gnss/+/heartbeat'`. Wait 40 seconds.
**Expected:** JSON message `{"uptime_s":N,"heap_free":N,"nmea_drops":0,"rtcm_drops":0,"uart_tx_errors":0}` appears approximately every 30 seconds. `uptime_s` increases across messages.
**Why human:** Embedded target — requires live device with MQTT broker. Build verification confirms the enqueue call exists; broker delivery cannot be confirmed statically.

#### 2. Retained online overwrites LWT offline on reconnect

**Test:** With device connected, subscribe with `mosquitto_sub -r -t 'gnss/+/status'`. Observe "online". Power-cycle or disconnect the device abruptly (so LWT fires). Reconnect. Observe the /status topic again.
**Expected:** After reconnect, broker delivers retained "online" — the LWT "offline" is overwritten.
**Why human:** Requires live broker with LWT retain semantics; the timing of LWT delivery vs. reconnect online publish can only be verified on hardware.

#### 3. Drop counter increments under channel saturation

**Test:** Temporarily reduce NMEA or RTCM channel capacity to 1 slot (or increase UM980 output rate), flash, and watch heartbeat payloads.
**Expected:** `nmea_drops` or `rtcm_drops` field increments in successive heartbeat messages, confirming the fetch_add paths are exercised.
**Why human:** Requires hardware-induced channel saturation. Static analysis confirms the code paths exist and are correct; runtime exercise requires a live device under load.

---

### Gaps Summary

No structural gaps found. All artifacts exist, are substantive, and are wired correctly. All must-haves from the PLAN frontmatter are satisfied in the codebase.

One documentation inconsistency was identified: REQUIREMENTS.md METR-01 contains stale draft values (topic: /status, interval: 60s, 4 fields) that do not match the final implementation (topic: /heartbeat, interval: 30s, 5 fields). The implementation correctly follows the PLAN's must_haves. The REQUIREMENTS.md text should be updated to reflect the delivered design, but this does not block phase completion.

Three items require human verification via live device + MQTT broker before phase sign-off is complete.

---

_Verified: 2026-03-07T15:10:00Z_
_Verifier: Claude (gsd-verifier)_
