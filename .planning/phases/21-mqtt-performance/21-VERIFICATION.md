---
phase: 21-mqtt-performance
verified: 2026-03-12T04:30:00Z
status: passed
score: 12/12 must-haves verified
re_verification: false
---

# Phase 21: MQTT Performance Verification Report

**Phase Goal:** Eliminate Arc<Mutex<EspMqttClient>> contention by introducing a dedicated MQTT publish thread with a typed channel interface, zero-copy RTCM/NMEA payloads, and publish-side observability counters.
**Verified:** 2026-03-12T04:30:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | A typed MqttMessage enum exists with all required variants (Nmea, Rtcm, Log, Heartbeat, Status, Bench, Subscribe) | VERIFIED | `src/mqtt_publish.rs` lines 48–94: all 7 variants present; `#[allow(dead_code)]` on enum |
| 2  | MQTT_ENQUEUE_ERRORS and MQTT_OUTBOX_DROPS AtomicU32 counters are pub statics | VERIFIED | `src/mqtt_publish.rs` lines 34, 41: both `pub static` with `AtomicU32::new(0)` |
| 3  | publish_thread owns EspMqttClient exclusively (no Arc/Mutex) | VERIFIED | `src/mqtt_publish.rs` line 111: `mut client: EspMqttClient<'static>` parameter; no Arc/Mutex in dispatch |
| 4  | Arc<Mutex<EspMqttClient>> is absent from all of src/ | VERIFIED | grep returns only doc-comment matches in `src/mqtt_publish.rs` (line 9) and `src/ota.rs` (line 11); zero functional code uses |
| 5  | All relay threads (NMEA, RTCM, log, heartbeat, subscriber, OTA) use SyncSender<MqttMessage> | VERIFIED | nmea_relay.rs line 26, rtcm_relay.rs line 36, log_relay.rs line 177, mqtt.rs line 373, mqtt.rs line 188, ota.rs line 72: all accept SyncSender<MqttMessage> |
| 6  | RTCM relay uses bytes::BytesMut + split().freeze() for zero-copy buffer handoff | VERIFIED | `src/rtcm_relay.rs` lines 55–62: `BytesMut::with_capacity(1029)` + `buf.split().freeze()` |
| 7  | NMEA and RTCM topics are consolidated (single topic per stream, not per-type subtopics) | VERIFIED | nmea_relay.rs doc comment line 7; rtcm_relay.rs doc comment line 8; no per-type format!() calls visible |
| 8  | Heartbeat JSON includes mqtt_enqueue_errors and mqtt_outbox_drops fields | VERIFIED | `src/mqtt.rs` lines 425–426, 441: both counters loaded and included in JSON format string |
| 9  | EventPayload::Deleted match arm increments MQTT_OUTBOX_DROPS | VERIFIED | `src/mqtt.rs` lines 163–166: explicit Deleted arm with fetch_add before catch-all |
| 10 | bench:N OTA trigger sends N messages to bench topic and logs sent/dropped/elapsed | VERIFIED | `src/ota.rs` lines 134–153: strip_prefix("bench:"), loop with try_send, log::info! summary |
| 11 | CONFIG_MQTT_REPORT_DELETED_MESSAGES=y in sdkconfig.defaults | VERIFIED | `sdkconfig.defaults` line 64: `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` |
| 12 | main.rs spawns publish_thread with exclusive EspMqttClient and pre-built Arc<str> topics | VERIFIED | `src/main.rs` lines 246–253 (8 topics), lines 274–278 (publish_thread spawn), lines 333–424 (all relay calls use mqtt_tx.clone()) |

**Score:** 12/12 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/mqtt_publish.rs` | MqttMessage enum, publish_thread fn, MQTT_ENQUEUE_ERRORS, MQTT_OUTBOX_DROPS | VERIFIED | 192 lines; all 4 exports present and pub; dispatch() fn handles all 7 variants |
| `Cargo.toml` | bytes = "1" dependency | VERIFIED | Line 13: `bytes = "1"` |
| `src/nmea_relay.rs` | spawn_relay with SyncSender<MqttMessage> | VERIFIED | Signature line 25–29; no Arc/Mutex in file |
| `src/log_relay.rs` | LOG_REENTERING pub, spawn_log_relay with SyncSender | VERIFIED | Line 32: `pub static LOG_REENTERING`; line 176 spawn_log_relay takes SyncSender; relay loop does NOT set LOG_REENTERING |
| `src/mqtt.rs` | heartbeat_loop SyncSender, mqtt_connect returns 3-tuple, subscriber_loop SyncSender | VERIFIED | heartbeat_loop line 372; mqtt_connect return type line 39 is `(EspMqttClient, SyncSender, Receiver)` 3-tuple; subscriber_loop line 187 takes SyncSender |
| `src/rtcm_relay.rs` | BytesMut zero-copy, SyncSender<MqttMessage>, single topic | VERIFIED | Lines 55–90; buf.split().freeze() zero-copy; single rtcm_topic per frame |
| `src/ota.rs` | spawn_ota SyncSender, bench:N trigger | VERIFIED | spawn_ota line 408 takes SyncSender; bench trigger lines 134–153 |
| `src/main.rs` | publish_thread spawned, 8 Arc<str> topics, all relays use mqtt_tx.clone() | VERIFIED | Lines 246–253, 274–278, 283–424; relay spawn calls confirmed |
| `sdkconfig.defaults` | CONFIG_MQTT_REPORT_DELETED_MESSAGES=y | VERIFIED | Line 64 confirmed |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/mqtt_publish.rs` | `crate::log_relay::LOG_REENTERING` | Direct atomic access in dispatch() Log arm | VERIFIED | Lines 160–162: store(true), enqueue, store(false) |
| `src/mqtt_publish.rs` | `esp_idf_svc::mqtt::client::EspMqttClient` | Exclusively owned by publish_thread fn param | VERIFIED | Line 111: `mut client: EspMqttClient<'static>` |
| `src/nmea_relay.rs` | `src/mqtt_publish.rs::MqttMessage` | SyncSender<MqttMessage> try_send, Nmea variant | VERIFIED | Lines 52–65: MqttMessage::Nmea constructed and try_sent |
| `src/mqtt.rs heartbeat_loop` | `src/mqtt_publish.rs` | SyncSender<MqttMessage>::try_send for Heartbeat and Status | VERIFIED | Lines 394–406 (Status), 450–457 (Heartbeat) |
| `src/rtcm_relay.rs` | `bytes::BytesMut / bytes::Bytes` | buf.split().freeze() for zero-copy RTCM handoff | VERIFIED | Lines 55, 62: BytesMut::with_capacity, split().freeze() |
| `src/ota.rs` | `src/mqtt_publish.rs::MqttMessage` | SyncSender<MqttMessage>::try_send for Status, Heartbeat, Bench | VERIFIED | Lines 35–39 (publish_status via Heartbeat), 374–382 (Status), 142–149 (Bench) |
| `src/main.rs` | `src/mqtt_publish.rs::publish_thread` | std::thread::Builder spawn with EspMqttClient and Receiver | VERIFIED | Lines 274–278: spawn(move || mqtt_publish::publish_thread(mqtt_client, mqtt_rx)) |
| `src/main.rs` | `gnss/{id}/nmea, gnss/{id}/rtcm, etc.` | Arc::from(format!(...)) at startup, cloned into threads | VERIFIED | Lines 246–253: all 8 topics built as Arc<str>; relay spawns receive .clone() |
| `src/mqtt.rs subscriber_loop` | `src/mqtt_publish.rs::MqttMessage::Subscribe` | try_send Subscribe variant; publish_thread calls client.subscribe() | VERIFIED | subscriber_loop lines 225–230; dispatch() lines 186–189: client.subscribe().map(|_| ()) |
| `src/mqtt.rs` EventPayload::Deleted | `src/mqtt_publish.rs::MQTT_OUTBOX_DROPS` | fetch_add in mqtt_connect callback | VERIFIED | Lines 163–166: explicit Deleted arm increments MQTT_OUTBOX_DROPS |

---

### Requirements Coverage

The requirement IDs PERF-21-1, PERF-21-2, PERF-21-3, PERF-21-4, OBS-21-1, OBS-21-2, and DIAG-21-1 are declared in the ROADMAP.md for Phase 21, but **none of these IDs appear in REQUIREMENTS.md**. The REQUIREMENTS.md traceability table ends at Phase 18 (TELEM-01, MAINT-03) and was not updated for Phase 21. The IDs are therefore phase-local tracking labels used in plan frontmatter only.

Cross-referencing the plan-level requirement claims against the codebase:

| Requirement | Plans Claiming It | Implementation Evidence | Status |
|-------------|-------------------|------------------------|--------|
| PERF-21-1 | 01, 02, 03 | publish_thread owns EspMqttClient; all relays use SyncSender; Arc<Mutex<EspMqttClient>> absent from src/ | SATISFIED |
| PERF-21-2 | 01, 02, 03 | SyncSender<MqttMessage> in nmea_relay, log_relay, heartbeat_loop, rtcm_relay, ota, subscriber_loop | SATISFIED |
| PERF-21-3 | 03 | rtcm_relay uses BytesMut + split().freeze(); pool buffer returned before try_send | SATISFIED |
| PERF-21-4 | 02, 03 | Arc<str> topics built at startup in main.rs lines 246–253; relay threads receive clones (zero hot-path alloc) | SATISFIED |
| OBS-21-1 | 01, 02 | MQTT_ENQUEUE_ERRORS and MQTT_OUTBOX_DROPS AtomicU32 statics; heartbeat JSON includes both fields | SATISFIED |
| OBS-21-2 | 03 | EventPayload::Deleted arm in mqtt_connect increments MQTT_OUTBOX_DROPS; CONFIG_MQTT_REPORT_DELETED_MESSAGES=y | SATISFIED |
| DIAG-21-1 | 03 | bench:N trigger in ota_task; sends N MqttMessage::Bench; logs sent/dropped/elapsed | SATISFIED |

**Note:** PERF-21-* / OBS-21-* / DIAG-21-* IDs are absent from REQUIREMENTS.md. These are phase-internal tracking labels. REQUIREMENTS.md should be updated to include Phase 21 entries in the traceability table, but this does not block the phase goal — the functionality is verified in the codebase.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/mqtt_publish.rs` | 28–29 | Stale TODO comment: "TODO(plan02): make LOG_REENTERING pub" — work was done in Plan 01 | Info | No functional impact; comment is misleading but harmless |
| `src/mqtt_publish.rs` | 159 | Stale TODO comment referencing Plan 02 dependency already resolved | Info | No functional impact |
| `src/mqtt_publish.rs` | 47, 109 | `#[allow(dead_code)]` on MqttMessage enum and publish_thread fn | Info | Expected during module creation; both items ARE used by main.rs (wired in Plan 03); may need removal post-wiring |

All three findings are Info-level only. The `#[allow(dead_code)]` attributes were expected during the phased rollout and remain now that main.rs wires both items — they are over-permissive but do not suppress any real dead code. They can be removed in a cleanup pass without functional change.

---

### Human Verification Required

#### 1. Runtime mutex contention reduction

**Test:** Deploy firmware to device FFFEB5, connect MQTT broker, confirm 5 Hz GNSS output for 60+ seconds with no NMEA_DROPS increments in heartbeat JSON
**Expected:** `nmea_drops` field in heartbeat stays at 0 under normal operation; publish thread handles 40 msg/s NMEA without channel saturation
**Why human:** Channel throughput at real hardware rates cannot be verified by static analysis

#### 2. Outbox deletion telemetry

**Test:** Set CONFIG_MQTT_REPORT_DELETED_MESSAGES=y, trigger a broker disconnect while NMEA is streaming, reconnect; observe heartbeat for `mqtt_outbox_drops` increment
**Expected:** `mqtt_outbox_drops` increments when messages expire in the outbox during disconnect; remains 0 under normal connected operation
**Why human:** Requires real MQTT broker behavior and network simulation

#### 3. bench:N throughput measurement

**Test:** Publish `bench:1000` to `gnss/{id}/ota/trigger`; observe device log for "Bench: N sent, N dropped in Xs" output
**Expected:** High sent count, low or zero dropped count at 1000 messages; elapsed time reflects channel throughput
**Why human:** Requires live MQTT interaction and log observation on device FFFEB5

---

### Gaps Summary

None. All 12 observable truths are verified, all artifacts pass all three levels (exists, substantive, wired), and all key links are confirmed wired. The only findings are:

1. Stale TODO comments in `src/mqtt_publish.rs` (informational, no functional impact)
2. REQUIREMENTS.md does not include Phase 21 requirement IDs in its traceability table (documentation gap, not a code defect)

---

_Verified: 2026-03-12T04:30:00Z_
_Verifier: Claude (gsd-verifier)_
