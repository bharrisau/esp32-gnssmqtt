---
phase: 05-nmea-relay
verified: 2026-03-07T00:00:00Z
status: human_needed
score: 4/5 must-haves verified
human_verification:
  - test: "NMEA sentences arrive on gnss/FFFEB5/nmea/# MQTT topics with $-prefixed payloads"
    expected: "mosquitto_sub -t 'gnss/FFFEB5/nmea/#' receives messages on topics like gnss/FFFEB5/nmea/GNGGA with payloads starting with $"
    why_human: "End-to-end MQTT publishing requires running firmware on device FFFEB5 with UM980 in MODE ROVER — cannot verify programmatically"
  - test: "No relay channel full WARN at normal UM980 output rate"
    expected: "espflash monitor shows no 'NMEA: relay channel full — sentence dropped' lines during normal 10 Hz NMEA output"
    why_human: "Requires hardware saturation test on the live device — cannot be simulated statically"
  - test: "UART RX thread continues reading without stalling while relay is active"
    expected: "NMEA sentences arrive continuously (not in bursts) in espflash monitor while mosquitto_sub shows concurrent MQTT delivery"
    why_human: "Thread decoupling behavior requires live observation — static analysis confirms try_send is non-blocking but cannot verify absence of stall"
---

# Phase 5: NMEA Relay Verification Report

**Phase Goal:** Consume (sentence_type, raw_sentence) tuples from the Receiver<(String, String)> returned by gnss::spawn_gnss, and publish each sentence's raw bytes to the MQTT topic `gnss/{device_id}/nmea/{sentence_type}` at QoS 0 retain=false, with a bounded 64-sentence channel (drop-on-full without stalling the UART RX thread).
**Verified:** 2026-03-07
**Status:** human_needed — all structural checks pass; hardware behavior requires human confirmation
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | gnss.rs uses a bounded channel (64) so the RX thread can drop sentences without blocking UART reads | VERIFIED | `mpsc::sync_channel::<(String, String)>(64)` at gnss.rs:65; `try_send` match at gnss.rs:108 |
| 2 | nmea_relay.rs has a spawn_relay() function that consumes Receiver<(String, String)> and publishes each sentence to MQTT | VERIFIED | src/nmea_relay.rs exists (57 lines), `pub fn spawn_relay(client, device_id, nmea_rx)` at line 27; enqueue at line 45 |
| 3 | mod nmea_relay declared in main.rs and spawn_relay called at Step 14 with nmea_rx moved in | VERIFIED | `mod nmea_relay;` at main.rs:34; `nmea_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), nmea_rx)` at main.rs:140; `_nmea_rx` placeholder absent |
| 4 | Topic format is gnss/{device_id}/nmea/{sentence_type} at QoS 0 retain=false | VERIFIED | nmea_relay.rs:39 `format!("gnss/{}/nmea/{}", device_id, sentence_type)`; line 45 `QoS::AtMostOnce, false` |
| 5 | Device publishes NMEA sentences to gnss/FFFEB5/nmea/{TYPE} visible on MQTT broker with $-prefixed payloads | NEEDS HUMAN | Hardware verification documented in 05-02-SUMMARY.md as approved by human; cannot verify programmatically |

**Score:** 4/5 truths verified statically; 1 requires human confirmation (hardware end-to-end)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/gnss.rs` | Bounded channel (sync_channel 64) with try_send drop semantics | VERIFIED | sync_channel(64) at line 65; TrySendError Full/Disconnected match at lines 108-116; TrySendError imported at line 29 |
| `src/nmea_relay.rs` | NMEA relay thread — drains receiver, calls enqueue() per sentence | VERIFIED | 57-line file, substantive implementation, for loop over &nmea_rx, mutex-per-sentence, QoS 0 retain=false |
| `src/main.rs` | nmea_relay module declaration and spawn_relay() call at Step 14 | VERIFIED | mod nmea_relay at line 34; spawn_relay call at line 140; _nmea_rx placeholder removed |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| src/gnss.rs | nmea_tx (SyncSender) | mpsc::sync_channel(64) | WIRED | `let (nmea_tx, nmea_rx) = mpsc::sync_channel::<(String, String)>(64)` at line 65 |
| src/gnss.rs RX thread | nmea_tx.try_send() | TrySendError match | WIRED | `match nmea_tx.try_send((sentence_type, s.to_string()))` at line 108; Full and Disconnected arms handle both error cases |
| src/nmea_relay.rs | EspMqttClient::enqueue() | Arc<Mutex<EspMqttClient<'static>>> | WIRED | `client.lock()` then `c.enqueue(&topic, QoS::AtMostOnce, false, raw.as_bytes())` at lines 42-49 |
| src/main.rs | nmea_relay::spawn_relay() | mqtt_client.clone(), device_id.clone(), nmea_rx | WIRED | `nmea_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), nmea_rx)` at line 140 |
| nmea_relay::spawn_relay | gnss::spawn_gnss Receiver | nmea_rx moved into relay thread | WIRED | nmea_rx not retained after line 140; `_nmea_rx` placeholder absent (grep confirms) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| NMEA-01 | 05-01-PLAN, 05-02-PLAN | Device publishes each valid NMEA sentence to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` | SATISFIED (structural) / NEEDS HUMAN (hardware) | spawn_relay publishes to correct topic format; hardware approval documented in 05-02-SUMMARY |
| NMEA-02 | 05-01-PLAN, 05-02-PLAN | UART reader and MQTT publisher decoupled via bounded channel (max 64); full channel drops without blocking UART task | SATISFIED | sync_channel(64) + try_send with TrySendError::Full drop-on-full; UART RX thread never blocks on channel send |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | — | — | None found |

No TODO, FIXME, placeholder, stub return, or empty handler patterns detected in gnss.rs, nmea_relay.rs, or main.rs.

### Human Verification Required

#### 1. NMEA sentences on MQTT broker

**Test:** Flash firmware and send MODE ROVER to UM980 via uart_bridge stdin. Subscribe to `gnss/FFFEB5/nmea/#` with `mosquitto_sub -h 10.86.32.41 -u user -P C65hSJsm -t 'gnss/FFFEB5/nmea/#' -v`
**Expected:** Messages arrive on topics like `gnss/FFFEB5/nmea/GNGGA`, `gnss/FFFEB5/nmea/GNRMC`; each payload starts with `$`
**Why human:** End-to-end MQTT publish requires live firmware on device FFFEB5 with UM980 in ROVER mode

#### 2. No relay channel full warnings at normal rate

**Test:** Observe espflash monitor during sustained UM980 NMEA output at normal 10 Hz rate
**Expected:** No `NMEA: relay channel full — sentence dropped` WARN lines appear in serial log
**Why human:** Requires running device with hardware GNSS input to saturate the pipeline; static analysis only confirms the drop path exists

#### 3. UART RX thread not stalled by relay

**Test:** Observe that NMEA sentences arrive continuously (not in bursts) in espflash monitor while MQTT delivery is simultaneously confirmed
**Expected:** Continuous sentence output with no visible bursty catch-up behavior
**Why human:** Thread scheduling behavior requires live observation; try_send being non-blocking is confirmed statically but absence of any other blocking path requires runtime validation

### Commit Verification

All commits referenced in SUMMARY files exist in git history:

| Commit | Description | Status |
|--------|-------------|--------|
| 72e3e00 | feat(05-01): switch gnss.rs to sync_channel(64) with try_send drop semantics | EXISTS |
| beb279d | feat(05-01): create nmea_relay.rs with spawn_relay() — NMEA-01 | EXISTS |
| 6686ca8 | feat(05-02): wire nmea_relay into main.rs at Step 14 | EXISTS |

### Gaps Summary

No structural gaps found. All code artifacts exist, are substantive (not stubs), and are correctly wired. The phase goal is fully implemented in the codebase.

The only open item is hardware end-to-end confirmation (NMEA-01 observable truth #5). The 05-02-SUMMARY.md documents human approval of hardware verification on 2026-03-07, including:
- NMEA sentences arrived on `gnss/FFFEB5/nmea/#` topics with `$`-prefixed payloads
- Throughput tested at 10 msg/sec — no channel full warnings, no UART RX stall

The structural implementation fully supports the claimed hardware behavior. Status is `human_needed` because hardware verification cannot be replicated programmatically in this environment.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
