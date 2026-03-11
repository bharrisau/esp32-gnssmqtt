# Phase 21: MQTT Performance - Research

**Researched:** 2026-03-11
**Domain:** Rust embedded MQTT publish pipeline, bytes crate, ESP-IDF MQTT outbox
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- Dedicated MQTT publish thread owns `EspMqttClient` exclusively; all relay threads send `MqttMessage` via `SyncSender`
- Single typed enum channel, capacity 256
- All NMEA → `gnss/{id}/nmea` (consolidated), all RTCM → `gnss/{id}/rtcm` (consolidated)
- `bytes` crate for RTCM buffer management (`BytesMut.split()` → `Bytes.freeze()` → send; `.reserve()` reclaims)
- `"bench:N"` OTA trigger for diagnostics
- `MQTT_ENQUEUE_ERRORS` + `MQTT_OUTBOX_DROPS` atomics in heartbeat
- Log relay re-entrancy guard preserved

### Claude's Discretion

- `MqttMessage` enum variant naming and exact field layout
- Whether to keep `Arc<str>` or use `&'static str` for the fixed topics (device_id is runtime, so `Arc<str>` is correct)
- Thread stack size for the publish thread
- Whether the bench topic uses sequential integers or timestamps as payload

### Deferred Ideas (OUT OF SCOPE)

- NMEA topic consolidation is a breaking change for any consumers that subscribe to `gnss/{id}/nmea/GNGGA` etc. — README / changelog note may be needed before milestone close
- HARD-07 (zero-alloc NMEA path) — still future; this phase reduces topic allocation but NMEA `String` from gnss.rs channel is still allocated per-sentence
</user_constraints>

## Summary

Phase 21 redesigns the MQTT publish path from a contended `Arc<Mutex<EspMqttClient>>` shared across five threads (NMEA relay, RTCM relay, log relay, heartbeat, OTA) into a single dedicated publish thread that owns `EspMqttClient` exclusively. All relay threads route `MqttMessage` enum values through a `SyncSender<MqttMessage>` with capacity 256.

The phase adds two observability atomics (`MQTT_ENQUEUE_ERRORS`, `MQTT_OUTBOX_DROPS`), enables the `EventPayload::Deleted` event via `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` in sdkconfig.defaults, and adds a `"bench:N"` diagnostic trigger. The `bytes` crate (version 1.x, default std features) is a clean addition for RTCM buffer management since the project targets std Rust on ESP-IDF.

**Primary recommendation:** Proceed with the locked design. All six key research questions have clear, verified answers. The `bytes` crate works unchanged on this target. `EventPayload::Deleted(MessageId)` is the correct variant for outbox drops. There is no `CONFIG_MQTT_OUTBOX_SIZE_BYTES` key — the outbox is unbounded in ESP-IDF v5.3.3; `MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000` (already set) is the only relevant tuning lever.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `bytes` | 1.11.1 | `BytesMut`/`Bytes` for zero-copy RTCM buffer handoff | Tokio ecosystem standard; reference-counted shared buffers; explicit reclaim via `reserve()` |
| `embedded-svc` | 0.28.1 (pinned) | `EventPayload`, `QoS` types | Already in Cargo.toml — no change |
| `esp-idf-svc` | 0.51.0 (pinned) | `EspMqttClient::enqueue()`, MQTT event callback | Already in Cargo.toml — no change |
| `std::sync::mpsc` | std | `SyncSender<MqttMessage>` inbound channel for publish thread | Already used everywhere in the project |

### Bytes Crate Feature Configuration

The project uses **std Rust** (not bare-metal `no_std`). The `bytes` crate works with default features:

```toml
bytes = "1"
```

No `default-features = false` needed. The crate's `std` feature (enabled by default) is appropriate for esp-idf-svc targets which provide full std.

**Installation:**
```bash
# Add to Cargo.toml [dependencies]
bytes = "1"
```

## Architecture Patterns

### Recommended Thread Layout After Phase 21

```
main.rs                         publish_thread (new)
 │                               │
 ├─ nmea_relay ──MqttMessage──►  │──enqueue()──► EspMqttClient (exclusively owned)
 ├─ rtcm_relay ──MqttMessage──►  │
 ├─ log_relay  ──MqttMessage──►  │  (re-entrancy guard set here, not in log_relay)
 ├─ heartbeat  ──MqttMessage──►  │
 └─ ota_task   ──MqttMessage──►  │  (status publishes only; OTA downloads are separate)
```

### Pattern 1: MqttMessage Enum Design

**What:** A typed enum carrying pre-built topic and payload bytes. All hot-path variants avoid per-message heap allocation where possible (topic as `Arc<str>` cloned at startup, payload as `Vec<u8>` for text or `Bytes` for RTCM).

**When to use:** One enum instance per publish. Publish thread matches variant to extract topic/payload/QoS/retain.

**Example:**

```rust
// Source: CONTEXT.md locked decisions + project patterns
use std::sync::Arc;
use bytes::Bytes;

pub enum MqttMessage {
    /// NMEA sentence: topic pre-built, payload is the raw sentence bytes
    Nmea { topic: Arc<str>, payload: Vec<u8> },
    /// RTCM frame: topic pre-built, payload is reference-counted Bytes from BytesMut pool
    Rtcm { topic: Arc<str>, payload: Bytes },
    /// Log line: topic pre-built, payload is the formatted log string as bytes
    Log  { topic: Arc<str>, payload: Vec<u8> },
    /// Heartbeat JSON: topic pre-built, payload is the JSON string as bytes
    Heartbeat { topic: Arc<str>, payload: Vec<u8>, retain: bool },
    /// Status online/offline: topic pre-built, payload is static
    Status { topic: Arc<str>, payload: &'static [u8], qos: embedded_svc::mqtt::client::QoS, retain: bool },
    /// Bench diagnostic: payload is a counter or timestamp string
    Bench { topic: Arc<str>, payload: Vec<u8> },
}
```

### Pattern 2: BytesMut Split-Freeze-Reserve for RTCM

**What:** RTCM relay writes into a `BytesMut`, calls `.split()` to extract filled bytes as a new `BytesMut`, calls `.freeze()` to convert to immutable `Bytes`, sends `Bytes` via channel. Then calls `.reserve()` on the retained (now-empty) `BytesMut` to reclaim the underlying buffer after the `Bytes` is consumed by the publish thread.

**Verified semantics (from bytes source):**
- `split()` on a `BytesMut` extracts all written data into a new `BytesMut`, leaving the original empty but retaining capacity
- `freeze()` converts the split result to `Bytes` (zero-cost, increments refcount to 2)
- When the publish thread consumes and drops the `Bytes`, refcount drops to 1
- `reserve(N)` on the retained `BytesMut` calls `reserve_inner()` which checks `is_unique()` (refcount == 1). If unique AND `off >= self.len()`, it shifts data backward reclaiming the buffer space in-place — no new allocation

**Critical condition:** Reclamation via `reserve()` only works after the `Bytes` clone is dropped (refcount returns to 1). If the publish thread is slow and the `Bytes` is still live when `reserve()` is called, reclamation fails and a new buffer is allocated. This is acceptable — it means worst-case a new alloc occurs, but steady-state (publish thread fast) reclaims correctly.

```rust
// Source: bytes crate documentation + source analysis
use bytes::{Bytes, BytesMut};

// At startup: allocate working buffer
let mut buf = BytesMut::with_capacity(1029);

// Per RTCM frame (hot path):
buf.extend_from_slice(&frame_data[..frame_len]);   // write into buf
let filled = buf.split().freeze();                  // split → freeze → Bytes (zero-copy)
mqtt_tx.try_send(MqttMessage::Rtcm { topic: rtcm_topic.clone(), payload: filled })?;
// buf is now empty; after publish thread drops Bytes:
buf.reserve(1029);   // reclaims if unique, else allocates fresh
```

**Fallback (if bytes crate unsuitable):** `MqttMessage::Rtcm` carries `(Box<[u8; 1029]>, usize, SyncSender<Box<[u8; 1029]>>)` — pool buffer returned after enqueue. This is the existing design from CONTEXT.md.

### Pattern 3: Re-entrancy Guard Migration

The `LOG_REENTERING` atomic in `log_relay.rs` currently guards the `enqueue()` call inside the log relay thread. After migration, the publish thread sets `LOG_REENTERING` before calling `enqueue()` and clears it after — the structural responsibility moves to the publish thread.

```rust
// Inside the publish thread's match arm for MqttMessage::Log:
// Source: existing log_relay.rs LOG_REENTERING pattern
LOG_REENTERING.store(true, Ordering::Relaxed);
let _ = client.enqueue(&topic, QoS::AtMostOnce, false, &payload);
LOG_REENTERING.store(false, Ordering::Relaxed);

// For all other variants: LOG_REENTERING must NOT be set
// (setting it for Heartbeat/Nmea/Rtcm would suppress log relay from logging its own errors)
```

### Pattern 4: bench:N Dispatch

**What:** Parse `"bench:N"` in the OTA trigger handler (same dispatch point as `"reboot"` and `"softap"`). Send N bench messages via the publish channel.

**When to use:** Initiated by operator; runs on OTA task thread, uses the same `SyncSender<MqttMessage>`.

```rust
// In ota.rs ota_task(), after "reboot" and "softap" checks:
if let Some(rest) = json.trim().strip_prefix("bench:") {
    let count: u64 = rest.trim().parse().unwrap_or(0);
    log::info!("Bench: starting {} message burst", count);
    let start = std::time::Instant::now();
    let mut sent = 0u64;
    let mut dropped = 0u64;
    for i in 0..count {
        let payload = format!("{}", i).into_bytes();
        match mqtt_tx.try_send(MqttMessage::Bench { topic: bench_topic.clone(), payload }) {
            Ok(()) => sent += 1,
            Err(_) => dropped += 1,
        }
    }
    log::info!("Bench: {} sent, {} dropped in {:.3}s",
        sent, dropped, start.elapsed().as_secs_f32());
    continue;
}
```

### Anti-Patterns to Avoid

- **Holding mutex across loop iterations:** Current code acquires `Arc<Mutex>` per message — this is what phase 21 eliminates entirely. Never reintroduce mutex-per-message on the hot path.
- **Setting LOG_REENTERING for non-log variants:** Only set the guard when publishing log messages. Setting it for NMEA/RTCM/heartbeat would suppress the log relay from forwarding its own diagnostic output.
- **Calling enqueue() with timeout:** `enqueue()` is non-blocking by design (adds to outbox). Use `try_send()` on the channel with `TrySendError` counting instead of blocking.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Reference-counted byte buffers | Custom Arc<Vec<u8>> pool | `bytes` crate `Bytes`/`BytesMut` | Handles refcount, split, freeze, reclaim — all edge cases covered |
| RTCM buffer pool | Manual channel-based pool (existing Box pattern) | `bytes` crate | Simpler API; reclaim semantics are automatic; existing Box pool is correct but more complex |
| Topic string sharing | `String` cloned per message | `Arc<str>` built at startup | Zero per-message allocation on hot path |

**Key insight:** The existing Box pool for RTCM is correct and has been validated in production. The `bytes` crate is an enhancement, not a correctness fix. The planner should treat the fallback (Box pool) as a valid alternative if bytes crate adds complexity.

## Common Pitfalls

### Pitfall 1: CONFIG_MQTT_OUTBOX_SIZE_BYTES Does Not Exist in v5.3.3

**What goes wrong:** The CONTEXT.md asks the researcher to find the "correct Kconfig key for total outbox capacity." There is no such key in ESP-IDF v5.3.3.

**Why it happens:** The Kconfig (`v5.3.3/components/mqtt/esp-mqtt/Kconfig`) only defines:
- `MQTT_OUTBOX_EXPIRED_TIMEOUT_MS` (already tuned to 5000ms in sdkconfig.defaults)
- `MQTT_REPORT_DELETED_MESSAGES` (default n — must enable for `EventPayload::Deleted`)
- `MQTT_OUTBOX_DATA_ON_EXTERNAL_MEMORY` (external RAM flag — not relevant)
- No `MQTT_OUTBOX_SIZE_BYTES` or any total capacity limit key

**How to avoid:** Do not add a nonexistent Kconfig key to sdkconfig.defaults. The only outbox knobs available are expiry (already tuned) and deleted message reporting (needs enabling).

**Warning signs:** `CONFIG_MQTT_OUTBOX_SIZE_BYTES` in any plan task is incorrect for this ESP-IDF version.

### Pitfall 2: EventPayload::Deleted Requires sdkconfig Change

**What goes wrong:** `EventPayload::Deleted(MessageId)` fires in the Rust callback only if `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` is set in sdkconfig.defaults. Without it, no event fires when outbox messages expire — `MQTT_OUTBOX_DROPS` stays zero even when drops occur.

**Why it happens:** The Kconfig default is `n`. The existing catch-all `m => log::warn!(...)` branch in `mqtt_connect()` sees nothing because the C MQTT task never posts the event.

**How to avoid:** Add `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` to sdkconfig.defaults as part of this phase. Match `EventPayload::Deleted(_)` in the callback and increment `MQTT_OUTBOX_DROPS`.

**Warning signs:** `MQTT_OUTBOX_DROPS` counter stays zero even after inducing artificial disconnects during bench testing.

### Pitfall 3: BytesMut reserve() Reclamation Is Conditional

**What goes wrong:** Calling `reserve()` immediately after `try_send()` — before the publish thread has consumed and dropped the `Bytes` — means the refcount is still 2, `is_unique()` returns false, and `reserve()` allocates a new buffer instead of reclaiming.

**Why it happens:** The RTCM relay thread calls `reserve()` in the same iteration that sent the `Bytes`. Whether the publish thread has consumed it yet is a race.

**How to avoid:** This is expected behavior. Accept that occasional new allocations occur when the publish thread is slow. Do not add synchronization to ensure the `Bytes` is consumed before `reserve()` — the whole point is to avoid blocking. Document in code that reserve() is a best-effort reclaim.

**Warning signs:** Heap growing monotonically at 1 RTCM frame/sec (~1029 bytes/sec) instead of staying flat. This indicates reclaim is never working.

### Pitfall 4: Log Relay Re-entrancy Guard Scope

**What goes wrong:** Setting `LOG_REENTERING = true` for ALL message variants in the publish thread. This would block the log relay from forwarding any output while the publish thread is processing an NMEA or RTCM message — at 40 msg/sec this means nearly continuous suppression.

**Why it happens:** It's tempting to simplify by always setting the guard.

**How to avoid:** Only set `LOG_REENTERING` when the publish thread calls `enqueue()` for a `MqttMessage::Log` variant. All other variants must leave `LOG_REENTERING` clear.

### Pitfall 5: OTA Task Still Uses EspMqttClient Directly

**What goes wrong:** The OTA task (`ota.rs`) currently holds `Arc<Mutex<EspMqttClient>>` for progress status publishes. After phase 21, the `Arc<Mutex>` is removed. If OTA is not migrated, it will hold a dangling clone.

**Why it happens:** OTA is in a different file (`ota.rs`) and is easy to miss during the migration.

**How to avoid:** Pass `SyncSender<MqttMessage>` to `spawn_ota` instead of `Arc<Mutex<EspMqttClient>>`. OTA status messages become `MqttMessage::Status` variants sent via the channel.

### Pitfall 6: Subscriber Thread Still Needs EspMqttClient

**What goes wrong:** The subscriber thread (`subscriber_loop`) must call `client.subscribe()` directly — it cannot route through the publish channel because `subscribe()` is not a publish operation. This thread keeps its `Arc<Mutex>` reference (or gains a direct `EspMqttClient` ref from the publish thread).

**Why it happens:** The design intent is that only `enqueue()` (publish) moves to the dedicated thread. `subscribe()` calls still need client access.

**How to avoid:** Keep the subscriber mechanism as-is, using the same `Arc<Mutex>` approach. Alternatively, add a `Subscribe` variant to `MqttMessage` and have the publish thread call `subscribe()` too — this eliminates the last `Arc<Mutex>`. Either is valid. If using the channel approach, `Subscribe` variant needs a callback or signal channel to confirm subscription.

**Simpler recommendation:** Add `Subscribe { topic: String, qos: QoS, signal: SyncSender<()> }` to `MqttMessage`. Publish thread calls `subscribe()` and sends signal on completion. This fully eliminates `Arc<Mutex>`.

## Code Examples

### Pre-building Topic Strings at Startup

```rust
// Source: CONTEXT.md locked decisions
// In mqtt_connect or a new setup function in main.rs:
let nmea_topic: Arc<str> = Arc::from(format!("gnss/{}/nmea", device_id).as_str());
let rtcm_topic: Arc<str> = Arc::from(format!("gnss/{}/rtcm", device_id).as_str());
let log_topic:  Arc<str> = Arc::from(format!("gnss/{}/log", device_id).as_str());
let hb_topic:   Arc<str> = Arc::from(format!("gnss/{}/heartbeat", device_id).as_str());
let status_topic: Arc<str> = Arc::from(format!("gnss/{}/status", device_id).as_str());
let bench_topic: Arc<str> = Arc::from(format!("gnss/{}/bench", device_id).as_str());

// Clone into each relay thread at spawn time — zero allocation on hot path.
```

### Publish Thread Core Loop

```rust
// Source: project patterns (recv_timeout loop, HWM at entry, try_send pattern)
pub fn publish_thread(
    mut client: EspMqttClient<'static>,
    mqtt_rx: Receiver<MqttMessage>,
) -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "MQTT publish", hwm_words, hwm_words * 4);

    loop {
        match mqtt_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
            Ok(msg) => {
                let is_log = matches!(msg, MqttMessage::Log { .. });
                if is_log {
                    crate::log_relay::LOG_REENTERING.store(true, Ordering::Relaxed);
                }
                let result = match &msg {
                    MqttMessage::Nmea { topic, payload } =>
                        client.enqueue(topic, QoS::AtMostOnce, false, payload),
                    MqttMessage::Rtcm { topic, payload } =>
                        client.enqueue(topic, QoS::AtMostOnce, false, payload),
                    MqttMessage::Log { topic, payload } =>
                        client.enqueue(topic, QoS::AtMostOnce, false, payload),
                    MqttMessage::Heartbeat { topic, payload, retain } =>
                        client.enqueue(topic, QoS::AtMostOnce, *retain, payload),
                    MqttMessage::Status { topic, payload, qos, retain } =>
                        client.enqueue(topic, *qos, *retain, payload),
                    MqttMessage::Bench { topic, payload } =>
                        client.enqueue(topic, QoS::AtMostOnce, false, payload),
                };
                if is_log {
                    crate::log_relay::LOG_REENTERING.store(false, Ordering::Relaxed);
                }
                if let Err(_e) = result {
                    crate::mqtt_state::MQTT_ENQUEUE_ERRORS
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Publish thread: channel closed — parking");
                loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
            }
        }
    }
}
```

### EventPayload::Deleted in Callback

```rust
// Source: embedded-svc/src/mqtt/client.rs confirmed variant list
// In mqtt_connect event callback (added to existing match):
EventPayload::Deleted(_msg_id) => {
    // Outbox message expired and was deleted before being sent.
    // Requires CONFIG_MQTT_REPORT_DELETED_MESSAGES=y in sdkconfig.defaults.
    crate::mqtt_state::MQTT_OUTBOX_DROPS.fetch_add(1, Ordering::Relaxed);
}
```

### New sdkconfig.defaults Entry

```
# MQTT outbox observability (Phase 21)
# Enables EventPayload::Deleted event when outbox messages expire before delivery.
# Required for MQTT_OUTBOX_DROPS atomic counter in heartbeat.
# Default is n; without this no deleted-message events fire.
CONFIG_MQTT_REPORT_DELETED_MESSAGES=y
```

### MQTT_ENQUEUE_ERRORS and MQTT_OUTBOX_DROPS Atomics

```rust
// In a new mqtt_state.rs module or alongside existing atomics in gnss.rs / mqtt.rs:
// Source: existing NMEA_DROPS / RTCM_DROPS pattern in gnss.rs
pub static MQTT_ENQUEUE_ERRORS: AtomicU32 = AtomicU32::new(0);
pub static MQTT_OUTBOX_DROPS: AtomicU32 = AtomicU32::new(0);
```

In heartbeat JSON format string:
```rust
// Extend the existing json format! in heartbeat_loop (or in publish thread for new design):
let enqueue_errors = crate::mqtt_state::MQTT_ENQUEUE_ERRORS.load(Ordering::Relaxed);
let outbox_drops = crate::mqtt_state::MQTT_OUTBOX_DROPS.load(Ordering::Relaxed);
// Add to format string: "\"mqtt_enqueue_errors\":{},\"mqtt_outbox_drops\":{},"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `Arc<Mutex<EspMqttClient>>` shared across all threads | Dedicated publish thread, exclusive ownership | Phase 21 | Eliminates per-message mutex acquisition; no deadlock risk |
| Per-sentence topic `format!("gnss/{}/nmea/{}", ...)` | Pre-built `Arc<str>` topics, zero hot-path alloc | Phase 21 | One format! at startup instead of 40+/sec |
| RTCM uses Box pool returned via channel | RTCM uses `bytes::Bytes` with `BytesMut.reserve()` reclaim | Phase 21 | Simpler API; reference-counted handoff; automatic reclaim |
| No MQTT publish observability | `MQTT_ENQUEUE_ERRORS` + `MQTT_OUTBOX_DROPS` in heartbeat | Phase 21 | Field diagnosis of outbox overflow and publish failure |

**Deprecated/outdated:**
- `Arc<Mutex<EspMqttClient>>`: Removed entirely after migration. No more mutex clones distributed to relay threads.
- Per-message topic `format!()` in NMEA relay: Replaced with pre-built `Arc<str>`.
- Per-message topic `format!()` in RTCM relay: Replaced with pre-built `Arc<str>`.

## Open Questions

1. **Subscriber thread migration**
   - What we know: Subscriber loop calls `client.subscribe()` — not an enqueue operation
   - What's unclear: Whether the planner should (a) keep subscriber with a direct client reference or (b) add `Subscribe { topic, qos, signal }` variant to `MqttMessage`
   - Recommendation: Add `Subscribe` variant to fully eliminate `Arc<Mutex>`. Publish thread calls `client.subscribe()` and signals completion via the response channel. This is the cleanest design.

2. **OTA task publish path**
   - What we know: `ota.rs` calls `publish_status()` which holds `Arc<Mutex<EspMqttClient>>`; also calls `enqueue()` directly for trigger clear
   - What's unclear: Whether the planner should fully migrate OTA to use `SyncSender<MqttMessage>` or leave it with a direct `EspMqttClient` reference temporarily
   - Recommendation: Fully migrate OTA. Pass `SyncSender<MqttMessage>` to `spawn_ota`. This eliminates the last consumer of `Arc<Mutex>`.

3. **LOG_REENTERING visibility**
   - What we know: `LOG_REENTERING` is `static` in `log_relay.rs`, not pub
   - What's unclear: Whether to make it `pub` for access from the publish thread module, or move it to a shared module
   - Recommendation: Make `LOG_REENTERING` pub in `log_relay.rs` so the publish thread can access it as `crate::log_relay::LOG_REENTERING`. No module restructure needed.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Hardware-only; no automated test suite exists |
| Config file | N/A |
| Quick run command | `cargo clippy -- -D warnings` (Clippy clean required per project convention) |
| Full suite command | `cargo build --release` then flash and observe on device FFFEB5 |

### Phase Requirements → Test Map

| Behavior | Test Type | Command | Notes |
|----------|-----------|---------|-------|
| Publish thread starts, HWM logged | smoke | `cargo build --release` + flash | Verify log output |
| NMEA messages appear on `gnss/{id}/nmea` (consolidated) | smoke | MQTT subscriber + flash | Breaking topic change from `/nmea/GNGGA` etc. |
| RTCM messages appear on `gnss/{id}/rtcm` | smoke | MQTT subscriber + flash | Breaking topic change from `/rtcm/{type}` |
| `MQTT_ENQUEUE_ERRORS` appears in heartbeat JSON | smoke | MQTT subscribe `/heartbeat` | Zero expected under normal conditions |
| `MQTT_OUTBOX_DROPS` appears in heartbeat JSON | smoke | MQTT subscribe `/heartbeat` | Verify `CONFIG_MQTT_REPORT_DELETED_MESSAGES=y` |
| `bench:100` trigger sends 100 messages to `/bench` | manual | Publish `"bench:100"` to `/ota/trigger` | Check log output for count/elapsed |
| Clippy clean | automated | `cargo clippy -- -D warnings` | Required per CLAUDE.md |
| Log relay re-entrancy preserved (no feedback loop) | smoke | Flash + observe MQTT log topic | Watch for log→log recursion |

### Sampling Rate
- **Per task commit:** `cargo clippy -- -D warnings` (mandatory)
- **Per wave merge:** `cargo build --release` (catch linker/size issues)
- **Phase gate:** Hardware smoke test on device FFFEB5 before marking complete

### Wave 0 Gaps
- None — no new test files needed; all validation is hardware smoke + clippy

## Sources

### Primary (HIGH confidence)
- Local file: `.embuild/espressif/esp-idf/v5.3.3/components/mqtt/esp-mqtt/Kconfig` — all MQTT Kconfig symbols and defaults for the exact version in use
- `https://raw.githubusercontent.com/esp-rs/embedded-svc/master/src/mqtt/client.rs` — complete `EventPayload` enum definition (fetched live)
- `https://docs.rs/crate/bytes/latest/source/Cargo.toml.orig` — bytes crate feature flags (fetched live)
- `https://raw.githubusercontent.com/tokio-rs/bytes/master/src/bytes_mut.rs` — `reserve()` reclamation implementation analysis (fetched live)
- Existing project source files: `src/mqtt.rs`, `src/nmea_relay.rs`, `src/rtcm_relay.rs`, `src/log_relay.rs`, `src/ota.rs`, `src/main.rs`, `sdkconfig.defaults`

### Secondary (MEDIUM confidence)
- `https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32s3/api-reference/protocols/mqtt.html` — ESP-MQTT docs confirming `CONFIG_MQTT_REPORT_DELETED_MESSAGES` behavior
- `https://docs.rs/crate/esp-idf-svc/latest/source/CHANGELOG.md` — confirmed `get_outbox_size()` added in 0.52.0 (not in 0.51.0 used by this project)

### Tertiary (LOW confidence)
- WebSearch results on BytesMut reclamation behavior — cross-verified against source code fetch

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — bytes crate features verified from Cargo.toml source; ESP-IDF Kconfig read from local disk
- Architecture: HIGH — all patterns derived directly from existing project code and verified API definitions
- Pitfalls: HIGH — Kconfig absence verified from local file; EventPayload::Deleted verified from source; reserve() semantics verified from implementation

**Research date:** 2026-03-11
**Valid until:** 2026-06-11 (stable APIs; bytes 1.x is mature; ESP-IDF v5.3.3 Kconfig won't change)
