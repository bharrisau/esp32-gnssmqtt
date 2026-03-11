# Phase 21: MQTT Performance - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Redesign the MQTT publish path to eliminate mutex contention and per-message heap allocation at 5 Hz GNSS output. All relay threads (NMEA, RTCM, log, heartbeat) route through a single dedicated publish thread. Add observability counters for enqueue failures and outbox drops. Add a bench trigger to diagnose message loss in the field.

</domain>

<decisions>
## Implementation Decisions

### Problem trigger
- Messages being lost in field; root cause unclear (could be NMEA channel drops, MQTT outbox overflow, or publish thread contention)
- Need a diagnostic tool and improved observability before optimising blind

### Bench trigger
- New OTA trigger payload: `"bench:N"` publishes N test messages to `gnss/{id}/bench` as fast as possible, then logs summary (sent, dropped, elapsed)
- Format `"bench:100"` — same dispatch as existing triggers (`"reboot"`, `"softap"`), count embedded in payload
- Permanent in firmware (useful for ongoing field diagnostics)
- Topic: `gnss/{device_id}/bench`, payload: sequential message numbers or timestamps

### Dedicated publish thread
- All `enqueue()` calls move to one dedicated publish thread — no more `Arc<Mutex<EspMqttClient>>` shared across relay threads
- Publish thread owns `EspMqttClient` exclusively; all other threads send `MqttMessage` via `SyncSender`
- `MqttMessage` is a typed enum: topic as `Arc<str>` (pre-built at startup) + payload bytes
- Channel capacity: 256 messages (~4s headroom at 64 msg/sec steady state)
- Publish thread increments `MQTT_ENQUEUE_ERRORS: AtomicU32` on every `enqueue()` failure

### Topic consolidation
- All NMEA sentences → single topic `gnss/{id}/nmea` (sentence type visible in `$GNGGA...` payload prefix)
- All RTCM frames → single topic `gnss/{id}/rtcm` (message type in binary header)
- All other topics (log, heartbeat, status, bench) pre-built as `Arc<str>` at startup and cloned into relay threads — zero allocation on hot path

### Buffer management (RTCM)
- Prefer `bytes` crate: write RTCM frame into `BytesMut`, call `.split()` to extract filled portion, `freeze()` to get `Bytes` for the `MqttMessage`, send to publish thread
- After publish, call `.reserve()` on the retained `BytesMut` — reclaims the split buffer if the `Bytes` refcount has dropped to 1 (i.e. it has been consumed by the publish thread)
- Researcher must verify: `BytesMut.split()` + `Bytes.freeze()` + `.reserve()` reclaim semantics on ESP32-C6 (no_std + alloc)
- Fallback if `bytes` crate is unsuitable: `MqttMessage::Rtcm` variant carries `(Box<[u8; 1029]>, usize, SyncSender<Box<[u8; 1029]>>)` — publish thread returns buffer to pool after enqueue

### Payload type
- All text payloads (NMEA sentences, log messages) sent as `Vec<u8>` in `MqttMessage` — avoids UTF-8 roundtrip in publish thread, aligns with `enqueue(&[u8])` API

### Outbox observability
- `MQTT_ENQUEUE_ERRORS: AtomicU32` — incremented by publish thread when `enqueue()` returns error
- `MQTT_OUTBOX_DROPS: AtomicU32` — incremented by MQTT event callback on the outbox-dropped event
- Researcher to identify: exact `EventPayload` variant for outbox drop notification in esp-idf-svc (likely `EventPayload::Deleted` or similar — currently handled by the catch-all `m => log::warn!(...)` branch)
- Both new counters included in heartbeat JSON alongside existing `nmea_drops`, `rtcm_drops`, `uart_tx_errors`

### Outbox size
- Current: `out_buffer_size: 2048` (single-message send buffer), `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000`
- Missing: outbox total capacity (`CONFIG_MQTT_OUTBOX_SIZE_BYTES` or equivalent) — may be too small for 40 msg/sec bursts
- Researcher to determine: correct ESP-IDF Kconfig key for total outbox capacity and recommended value for 5 Hz GNSS

### Scope
- All publish paths migrate: NMEA relay, RTCM relay, log relay, heartbeat + status
- `Arc<Mutex<EspMqttClient>>` removed entirely after migration
- Log relay re-entrancy guard (`LOG_REENTERING` atomic) must be preserved — the publish thread sets it before calling `enqueue()` and clears after, same as today

### Claude's Discretion
- `MqttMessage` enum variant naming and exact field layout
- Whether to keep `Arc<str>` or use `&'static str` for the fixed topics (if device_id is known at compile time — it isn't, so `Arc<str>` is correct)
- Thread stack size for the publish thread
- Whether the bench topic uses sequential integers or timestamps as payload

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/mqtt.rs:mqtt_connect()`: returns `Arc<Mutex<EspMqttClient>>` — refactor to return `(EspMqttClient, SyncSender<MqttMessage>)` or similar; event callback closure is reusable unchanged
- `src/gnss.rs:NMEA_DROPS`, `RTCM_DROPS`, `UART_TX_ERRORS`: pattern for `AtomicU32` counters read by heartbeat — add `MQTT_ENQUEUE_ERRORS` and `MQTT_OUTBOX_DROPS` following same pattern
- `src/mqtt.rs:heartbeat_loop`: currently holds `Arc<Mutex>` for publish; simplifies to just sending `MqttMessage` to the channel

### Established Patterns
- All relay threads use `recv_timeout(RELAY_RECV_TIMEOUT)` loops with HWM logging at entry — publish thread follows same pattern
- `sync_channel` with documented capacity comment — publish thread inbound channel uses same convention
- `try_send` with `TrySendError::Full` / `Disconnected` match arms — all callers sending to publish channel use this
- `src/ota.rs` OTA trigger dispatch: `"bench:N"` payload parsed alongside `"reboot"` and `"softap"` in the same dispatch branch in `mqtt.rs`

### Integration Points
- `src/mqtt.rs`: central change point — `mqtt_connect()` signature changes; event callback adds new counter increments
- `src/nmea_relay.rs`: replace `client: Arc<Mutex<EspMqttClient>>` param with `mqtt_tx: SyncSender<MqttMessage>`; topic arg changes from dynamic format to pre-built `Arc<str>`
- `src/rtcm_relay.rs`: same param swap; pool buffer management changes if using `bytes` crate
- `src/log_relay.rs`: same param swap; re-entrancy guard must stay but now guards publish thread, not relay thread
- `src/main.rs`: `mqtt_connect()` call site changes; new `mqtt_publish_thread` spawn

</code_context>

<specifics>
## Specific Ideas

- Bench trigger counts: `"bench:100"` sends 100 messages; publish thread logs count sent, enqueue errors, elapsed time after completion
- BytesMut reclaim intent: `.split()` on write → `Bytes.freeze()` → send; original `BytesMut` calls `.reserve()` to reclaim when refcount drops — researcher to verify exact API
- Missing outbox size config: researcher to find the correct Kconfig key for ESP-IDF MQTT total outbox capacity

</specifics>

<deferred>
## Deferred Ideas

- NMEA topic consolidation is a breaking change for any consumers that subscribe to `gnss/{id}/nmea/GNGGA` etc. — README / changelog note may be needed before milestone close
- HARD-07 (zero-alloc NMEA path, per REQUIREMENTS.md future requirements) — still future; this phase reduces topic allocation but NMEA String from gnss.rs channel is still allocated per-sentence

</deferred>

---

*Phase: 21-mqtt-performance*
*Context gathered: 2026-03-11*
