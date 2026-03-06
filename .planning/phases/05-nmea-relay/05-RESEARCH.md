# Phase 5: NMEA Relay - Research

**Researched:** 2026-03-07
**Domain:** ESP-IDF Rust MQTT publishing, mpsc bounded channels, thread architecture
**Confidence:** HIGH

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| NMEA-01 | Device publishes each valid NMEA sentence to `gnss/{device_id}/nmea/{SENTENCE_TYPE}` (e.g. `gnss/ABC123/nmea/GNGLL`) | `EspMqttClient::enqueue()` on `Arc<Mutex<EspMqttClient<'static>>>` — same pattern as heartbeat_loop; topic built as `format!("gnss/{}/nmea/{}", device_id, sentence_type)` |
| NMEA-02 | UART reader and MQTT publisher are decoupled via a bounded channel (max 64 sentences); if channel is full, sentences are dropped rather than blocking the UART task | Replace current unbounded `mpsc::channel` in gnss.rs with `mpsc::sync_channel(64)`; gnss RX thread calls `nmea_tx.try_send()` — drops on `TrySendError::Full` |

</phase_requirements>

---

## Summary

Phase 5 connects two already-running subsystems: the GNSS NMEA pipeline (Phase 4) and the MQTT client stack (Phase 2). The work is a single new thread that consumes `Receiver<(String, String)>` from `gnss::spawn_gnss` and calls `EspMqttClient::enqueue()` for each sentence.

The only non-trivial engineering decisions are: (1) NMEA-02 requires switching gnss.rs from an unbounded `mpsc::channel` to a bounded `mpsc::sync_channel(64)` so that the RX thread can use `try_send()` and drop sentences without blocking when the MQTT publisher falls behind; and (2) choosing QoS 0 (AtMostOnce) for NMEA publishing because NMEA sentences are time-critical streaming data — retransmission of stale sentences is useless and wastes broker bandwidth.

All patterns needed — `Arc<Mutex<EspMqttClient>>`, `enqueue()`, thread spawn, topic string formatting, `device_id` access — are already established and hardware-verified in the codebase. No new crate dependencies are required.

**Primary recommendation:** Add `src/nmea_relay.rs` with one public function `spawn_relay(client, device_id, nmea_rx)`. In main.rs, move `nmea_rx` from the idle loop into the relay thread at Step 14. Modify gnss.rs to use `sync_channel(64)` and `try_send()`.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp_idf_svc::mqtt::client::EspMqttClient` | via `=0.51.0` | MQTT publish via `enqueue()` | Already used in heartbeat_loop; `Arc<Mutex<>>` wrapper in place |
| `embedded_svc::mqtt::client::QoS` | via `=0.28.1` | QoS enum (`AtMostOnce` = QoS 0) | Used in mqtt.rs for heartbeat and subscribe |
| `std::sync::mpsc::sync_channel` | std | Bounded channel (capacity 64) for NMEA-02 requirement | `sync_channel(N)` replaces `channel()` in gnss.rs; `SyncSender::try_send()` enables non-blocking drop |
| `std::sync::{Arc, Mutex}` | std | Share `EspMqttClient` with relay thread | Established pattern — `Arc<Mutex<EspMqttClient<'static>>>` |
| `std::thread::Builder` | std | `.stack_size(8192).spawn()` | Universal pattern in this codebase |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `log` | `0.4` | `log::warn!` on `try_send` full, `log::info!` on relay start | All structured output |
| `anyhow` | `1` | `anyhow::Result<()>` return from `spawn_relay` | Consistent with `spawn_gnss`, `spawn_bridge` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `enqueue()` (async, returns enqueue ID) | `publish()` (blocking) | `publish()` blocks until broker ACKs; at 10+ sentences/sec this would stall the relay thread behind network RTT. `enqueue()` returns immediately — pump thread drains the outbox. Heartbeat uses `enqueue()` for the same reason. |
| QoS 0 (AtMostOnce) | QoS 1 (AtLeastOnce) | QoS 1 adds broker ACK + retransmit; for real-time NMEA this wastes bandwidth retransmitting stale positional data. QoS 0 is correct for streaming sensor data. |
| Drop on full channel | Block / apply backpressure to UART RX | Blocking the UART RX thread risks UART FIFO overflow and sentence corruption. Dropping NMEA sentences is the specified behavior (NMEA-02). |
| `sync_channel(64)` | `sync_channel(32)` or `sync_channel(128)` | 64 gives ~6 seconds of buffer at 10 sentences/sec across ~6 common sentence types. Enough to survive transient MQTT delays without unbounded memory growth. |

**Installation:** No new dependencies required. All libraries are already in `Cargo.toml`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── gnss.rs          # MODIFIED — sync_channel(64) + try_send() for NMEA-02
├── nmea_relay.rs    # NEW — relay thread: Receiver<(String, String)> → MQTT publish
├── main.rs          # MODIFIED — move nmea_rx from idle loop into spawn_relay call
├── mqtt.rs          # UNCHANGED
├── uart_bridge.rs   # UNCHANGED
├── wifi.rs          # UNCHANGED
├── led.rs           # UNCHANGED
├── config.rs        # UNCHANGED
└── device_id.rs     # UNCHANGED
```

### Pattern 1: Bounded Channel in gnss.rs (NMEA-02)

**What:** Replace `mpsc::channel()` with `mpsc::sync_channel(64)` so the RX thread can call `try_send()` and drop without blocking.
**When to use:** Inside `spawn_gnss`, replacing the existing channel creation.

```rust
// Source: std::sync::mpsc documentation + NMEA-02 requirement
// BEFORE (unbounded — current gnss.rs):
let (nmea_tx, nmea_rx) = mpsc::channel::<(String, String)>();

// AFTER (bounded 64 — NMEA-02 compliant):
let (nmea_tx, nmea_rx) = mpsc::sync_channel::<(String, String)>(64);

// Return type of spawn_gnss changes from:
//   Receiver<(String, String)>        [channel()]
// to:
//   Receiver<(String, String)>        [sync_channel() also returns Receiver — same type]
// NOTE: Receiver<T> is identical for both channel() and sync_channel().
// Only the Sender type changes: Sender<T> → SyncSender<T>.
```

**Key fact:** `sync_channel()` returns `(SyncSender<T>, Receiver<T>)`. The `Receiver<T>` is the same type as from `channel()`. The relay thread receives from a `Receiver<(String, String)>` — its code is unaffected by this change.

### Pattern 2: Non-Blocking Send in gnss.rs RX Thread (NMEA-02)

**What:** Replace `nmea_tx.send()` with `nmea_tx.try_send()` to drop silently when the relay thread is behind.
**When to use:** In the GNSS RX thread where sentences are forwarded to the channel.

```rust
// Source: std::sync::mpsc::SyncSender::try_send docs + NMEA-02 spec
// BEFORE (blocking on full — current gnss.rs):
let _ = nmea_tx.send((sentence_type, s.to_string()));

// AFTER (drop on full — NMEA-02 compliant):
use std::sync::mpsc::TrySendError;
match nmea_tx.try_send((sentence_type, s.to_string())) {
    Ok(_) => {}
    Err(TrySendError::Full(_)) => {
        log::warn!("NMEA: relay channel full — sentence dropped");
    }
    Err(TrySendError::Disconnected(_)) => {
        log::error!("NMEA: relay channel disconnected");
    }
}
```

### Pattern 3: NMEA Relay Thread

**What:** Drain `Receiver<(String, String)>`, publish each sentence to MQTT topic `gnss/{device_id}/nmea/{TYPE}`.
**When to use:** Body of `spawn_relay` in `nmea_relay.rs`.

```rust
// Source: Derived from heartbeat_loop in src/mqtt.rs (verified on device FFFEB5)
// + for-in-receiver blocking pattern from mqtt.rs subscriber_loop
use esp_idf_svc::mqtt::client::EspMqttClient;
use embedded_svc::mqtt::client::QoS;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;

pub fn spawn_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    nmea_rx: Receiver<(String, String)>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            log::info!("NMEA relay thread started");
            for (sentence_type, raw) in &nmea_rx {
                let topic = format!("gnss/{}/nmea/{}", device_id, sentence_type);
                match client.lock() {
                    Err(e) => log::warn!("NMEA relay: mutex poisoned: {:?}", e),
                    Ok(mut c) => {
                        match c.enqueue(&topic, QoS::AtMostOnce, false, raw.as_bytes()) {
                            Ok(_) => {}
                            Err(e) => log::warn!("NMEA relay: enqueue failed: {:?}", e),
                        }
                    }
                }
            }
            log::error!("NMEA relay: receiver closed — thread exiting");
        })
        .expect("nmea relay thread spawn failed");
    Ok(())
}
```

### Pattern 4: main.rs Integration

**What:** Pass `mqtt_client.clone()`, `device_id.clone()`, and `nmea_rx` to `spawn_relay`. Remove the idle `let _nmea_rx = nmea_rx` placeholder.
**When to use:** main.rs, new Step 14 (or inserted between Step 13 and the idle loop).

```rust
// Source: Derived from existing thread spawn pattern in src/main.rs
// Step 14 (new): NMEA relay — consumes nmea_rx, publishes to MQTT
let relay_client = mqtt_client.clone();
let relay_device_id = device_id.clone();
nmea_relay::spawn_relay(relay_client, relay_device_id, nmea_rx)
    .expect("NMEA relay thread spawn failed");
log::info!("NMEA relay started");

// idle loop no longer needs _nmea_rx binding
```

**CRITICAL:** `nmea_rx` is moved into `spawn_relay`. Remove the `let _nmea_rx = nmea_rx;` placeholder that currently keeps the receiver alive. The relay thread holds it now.

### Pattern 5: Publish API — enqueue() vs publish()

**What:** `enqueue()` returns immediately after placing the message in the MQTT outbox. `publish()` blocks until the broker ACKs. For high-rate sensor streaming, use `enqueue()`.
**When to use:** NMEA relay always. Heartbeat already uses `enqueue()`.

```rust
// Source: src/mqtt.rs heartbeat_loop (verified on device FFFEB5)
// Signature (from embedded-svc 0.28):
// fn enqueue(&mut self, topic: &str, qos: QoS, retain: bool, payload: &[u8]) -> Result<MessageId>

c.enqueue(
    &topic,           // "gnss/{device_id}/nmea/{SENTENCE_TYPE}"
    QoS::AtMostOnce,  // QoS 0 — no retransmit; streaming data
    false,            // retain = false — consumers want current, not cached
    raw.as_bytes(),   // payload = raw NMEA string bytes (ASCII, includes $)
)
```

### Anti-Patterns to Avoid

- **Using `publish()` instead of `enqueue()`**: `publish()` blocks the relay thread waiting for a broker ACK on each sentence. At 10 sentences/sec this creates backpressure that will fill the bounded channel. Use `enqueue()`.
- **Setting retain = true on NMEA topics**: Consumers subscribe to live sentences. A retained stale position would mislead consumers that connect after a GPS fix is lost. `retain = false`.
- **QoS 1 for NMEA relay**: NMEA sentences are time-stamped positional data. Retransmitting a sentence from 5 seconds ago is harmful, not helpful. QoS 0 is correct.
- **Holding the Mutex lock across the for-loop iteration**: The lock must be acquired and released for each sentence. Do not move `client.lock()` outside the for-loop — that would starve other threads (heartbeat, subscriber) for the entire relay session.
- **Blocking `nmea_tx.send()` in gnss.rs with sync_channel**: After switching to `sync_channel(64)`, the RX thread MUST use `try_send()`, not `send()`. `send()` on a full `SyncSender` blocks — this would stall UART reads and cause FIFO overflow.
- **Forgetting `mod nmea_relay;` in main.rs**: Same class of error as the `mod gnss;` pitfall from Phase 4. Must be added to the mod declarations block.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Backpressure / drop-on-full | Custom ring buffer or channel wrapper | `std::sync::mpsc::sync_channel(64)` + `try_send()` | Built-in bounded channel semantics; `TrySendError::Full` is the exact signal needed |
| Thread-safe MQTT client sharing | Custom locking or message-passing to a publish thread | `Arc<Mutex<EspMqttClient<'static>>>` | Already in place; same pattern as heartbeat and subscriber threads |
| Topic string construction | Pre-allocated static topic table | `format!("gnss/{}/nmea/{}", device_id, sentence_type)` at publish time | UM980 outputs ~6-10 sentence types; allocating per-sentence String is fine at 10Hz on an ESP32-C6 with heap |
| MQTT async outbox | Manual queue / second thread | `enqueue()` (ESP-IDF MQTT client's internal outbox) | ESP-IDF MQTT already implements an outbox; `enqueue()` is the non-blocking write path into it |

**Key insight:** The entire relay is glue code between two existing subsystems. Zero new concurrency primitives needed beyond switching `channel()` to `sync_channel(64)`.

---

## Common Pitfalls

### Pitfall 1: sync_channel + send() = Blocking UART RX Thread

**What goes wrong:** gnss.rs switches to `sync_channel(64)` but the RX thread still calls `nmea_tx.send()`. When the relay thread is slow, the channel fills and `send()` blocks — the UART RX thread stops reading bytes. The UART FIFO (4096 bytes) fills in under 1 second at 115200 baud, causing sentence corruption.
**Why it happens:** `SyncSender::send()` is blocking. `Sender::send()` (from unbounded `channel()`) is always non-blocking.
**How to avoid:** Change `nmea_tx.send(...)` to `nmea_tx.try_send(...)` in gnss.rs RX thread whenever switching to `sync_channel`.
**Warning signs:** Log shows UART RX pausing; sentences appear in bursts; WARN "relay channel full" floods the log.

### Pitfall 2: Mutex Lock Contention Slowing NMEA Throughput

**What goes wrong:** The relay thread holds `client.lock()` while `enqueue()` processes. If `enqueue()` has internal latency (e.g. waiting for outbox space), the heartbeat or subscriber threads starve on the Mutex.
**Why it happens:** `EspMqttClient::enqueue()` is generally fast (writes to an internal ring buffer), but under load it can block briefly.
**How to avoid:** Keep the lock scope tight — lock, enqueue, release immediately. Do not hold the lock between sentences. The for-loop pattern in Pattern 3 does this correctly because each iteration acquires and releases.
**Warning signs:** Heartbeat publish latency grows beyond 30s; subscriber does not re-subscribe on reconnect.

### Pitfall 3: nmea_rx Dropped in Idle Loop

**What goes wrong:** The idle loop still holds `let _nmea_rx = nmea_rx;` after `spawn_relay` is added. If `spawn_relay` also takes ownership of `nmea_rx`, the code won't compile. If the idle loop placeholder is kept instead and `nmea_rx` is NOT passed to `spawn_relay`, the relay thread has no receiver.
**Why it happens:** `nmea_rx` is `Receiver<(String, String)>` — not Clone. It can only be owned by one place. Currently it's in the idle loop as a placeholder.
**How to avoid:** Remove `let _nmea_rx = nmea_rx;` from the idle loop. Pass `nmea_rx` directly to `spawn_relay`. The idle loop comment documents that Phase 5 consumed it.
**Warning signs:** Compile error "use of moved value: nmea_rx" or — worse — relay thread silently never receiving because `nmea_rx` was left as the idle placeholder.

### Pitfall 4: EspMqttClient enqueue() Fails When Disconnected

**What goes wrong:** WiFi or MQTT connection drops. The relay thread continues calling `enqueue()`, which fails (returns Err). If the error is not logged and the loop continues silently, sentences pile up in the gnss.rs bounded channel and get dropped.
**Why it happens:** `enqueue()` returns `Err` when the MQTT client is not connected. This is expected behavior.
**How to avoid:** Log the error at WARN level (not ERROR — disconnects are expected in production). Do not exit the relay loop on a single publish failure. The pump thread will reconnect; publishes will succeed again once connected.
**Warning signs:** `log::warn!("NMEA relay: enqueue failed")` appearing in logs — normal during disconnect/reconnect cycles.

### Pitfall 5: Sentence Rate and Stack Size

**What goes wrong:** The UM980 at full output rate can produce 10+ sentence types at 1Hz each. Stack allocation inside the relay thread (topic String, format! buffer) must fit within the 8192-byte thread stack.
**Why it happens:** Each `format!` allocates a `String` on the heap (not stack), but format temporaries use stack. `format!("gnss/{}/nmea/{}", device_id, sentence_type)` with a 6-char device_id and ~6-char sentence type produces a ~28-byte string — well within bounds.
**How to avoid:** Use `format!` as shown. Do not pre-allocate large buffers on the stack inside the relay thread. 8192 bytes is sufficient.
**Warning signs:** Stack overflow log from FreeRTOS — would appear as a hard fault or watchdog reset.

---

## Code Examples

Verified patterns from official sources and existing codebase:

### Bounded Channel Creation (std)
```rust
// Source: std::sync::mpsc documentation
// In gnss.rs spawn_gnss — replaces mpsc::channel():
use std::sync::mpsc::{self, Receiver, SyncSender};

let (nmea_tx, nmea_rx) = mpsc::sync_channel::<(String, String)>(64);
// nmea_tx: SyncSender<(String, String)>  — in gnss.rs RX thread
// nmea_rx: Receiver<(String, String)>    — returned to caller (same type as before)
```

### try_send with Drop Semantics
```rust
// Source: std::sync::mpsc::SyncSender docs
use std::sync::mpsc::TrySendError;

match nmea_tx.try_send((sentence_type, s.to_string())) {
    Ok(_) => {}
    Err(TrySendError::Full(_)) => {
        log::warn!("NMEA: relay channel full — sentence dropped");
    }
    Err(TrySendError::Disconnected(_)) => {
        log::error!("NMEA: relay channel disconnected");
    }
}
```

### enqueue() Call (from mqtt.rs heartbeat — verified on device FFFEB5)
```rust
// Source: src/mqtt.rs heartbeat_loop (hardware-verified, device FFFEB5)
// Adapted for NMEA relay:
match c.enqueue(&topic, QoS::AtMostOnce, false, raw.as_bytes()) {
    Ok(_) => {}
    Err(e) => log::warn!("NMEA relay: enqueue failed: {:?}", e),
}
```

### Topic String Construction
```rust
// Source: Derived from mqtt.rs topic patterns (verified pattern)
// Pattern: "gnss/{device_id}/nmea/{SENTENCE_TYPE}"
// Examples: "gnss/FFFEB5/nmea/GNGGA", "gnss/FFFEB5/nmea/GNRMC"
let topic = format!("gnss/{}/nmea/{}", device_id, sentence_type);
// sentence_type comes from gnss.rs RX thread: extracted between $ and first comma
// e.g. "$GNGGA,..." → "GNGGA"
```

### device_id Access Pattern
```rust
// Source: src/main.rs (verified)
// device_id is a String produced by device_id::get() at boot
// Format: last 3 MAC bytes as uppercase hex, e.g. "FFFEB5"
// Pass as device_id.clone() to spawn_relay — relay thread takes ownership of the String
let relay_device_id = device_id.clone();
nmea_relay::spawn_relay(relay_client, relay_device_id, nmea_rx)?;
```

---

## State of the Art

| Old Approach | Current Approach | Notes |
|--------------|------------------|-------|
| `nmea_rx` held idle in main loop (`_nmea_rx` placeholder) | `nmea_rx` moved into relay thread | Phase 5 goal — activate the placeholder |
| Unbounded `mpsc::channel()` in gnss.rs | Bounded `mpsc::sync_channel(64)` | NMEA-02 requires drop-on-full semantics |
| `nmea_tx.send()` in gnss RX thread | `nmea_tx.try_send()` | Required after switch to `SyncSender` |

**Not deprecated:**
- `Arc<Mutex<EspMqttClient<'static>>>` — still the correct sharing pattern
- `Builder::new().stack_size(8192).spawn()` — still the only way to control thread stack
- `enqueue()` over `publish()` — correct for non-blocking high-rate publishing
- `for x in &receiver` blocking iteration — correct for the relay thread (block until sentence arrives)

---

## Open Questions

1. **Does `enqueue()` block when the MQTT outbox is full?**
   - What we know: `heartbeat_loop` uses `enqueue()` without visible issue at 1 message per 30 seconds. The MQTT outbox size is controlled by `CONFIG_MQTT_OUTBOX_CHUNK_SIZE` and related sdkconfig options.
   - What's unclear: At 10+ sentences/sec continuous, whether the ESP-IDF MQTT outbox saturates and `enqueue()` starts blocking or returning `Err`.
   - Recommendation: At 10 sentences/sec with ~100 bytes/sentence = 1KB/sec — well within typical WiFi throughput. If `enqueue()` returns `Err`, log at WARN and continue. The bounded channel (NMEA-02) prevents unbounded memory growth even if enqueue fails.
   - **Confidence: MEDIUM** — low-rate heartbeat works; 10Hz NMEA is untested but within expected capacity.

2. **Does UM980 require MODE ROVER before NMEA output?**
   - What we know: PROJECT.md notes "UM980 current state: BASE TIME mode — needs `MODE ROVER` before GNSS relay phase". The UM980 cmd_tx Sender is retained in main.rs for Phase 6.
   - What's unclear: Whether Phase 5 should send a one-shot `MODE ROVER\r\n` command at relay startup via `gnss_cmd_tx`, or defer that entirely to Phase 6.
   - Recommendation: Phase 5 scope is NMEA relay only (NMEA-01, NMEA-02). Mode configuration is Phase 6 (CONF-01 through CONF-03). However, the planner should note that hardware verification will require manual `MODE ROVER` command via uart_bridge stdin, OR a startup command in main.rs, to see NMEA output.
   - **Confidence: HIGH** — scope boundary is clear; hardware workaround (manual command via bridge) available.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None detected — embedded target; no test runner exists |
| Config file | None |
| Quick run command | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` |
| Full suite command | Flash + `espflash monitor` + MQTT broker subscription observation |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NMEA-01 | Each NMEA sentence published to correct MQTT topic | manual-only | N/A — requires hardware + MQTT broker | N/A |
| NMEA-02 | Channel is bounded (64); full channel drops without blocking UART | manual-only | N/A — requires hardware saturation test | N/A |

**Note on host-side unit testing:** The relay logic (`format!` topic construction, `try_send` drop behavior) is too tightly coupled to `EspMqttClient` and `SyncSender` to test in isolation without mocking. The pure-logic portion (topic string format) is trivially correct and not worth a test file. Full validation is hardware observation.

**Hardware verification procedure:**
1. Flash firmware with NMEA relay
2. Manually send `MODE ROVER` via uart_bridge stdin (or wire to UM980 already in ROVER mode)
3. Subscribe to `gnss/FFFEB5/nmea/#` on MQTT broker: `mosquitto_sub -h 10.86.32.41 -u user -P C65hSJsm -t 'gnss/FFFEB5/nmea/#' -v`
4. Verify: messages arrive on `gnss/FFFEB5/nmea/GNGGA`, `gnss/FFFEB5/nmea/GNRMC`, etc.
5. Verify: payload matches raw NMEA string starting with `$` (e.g. `$GNGGA,123519,...`)
6. Verify: espflash monitor shows no "relay channel full" WARN lines at normal UM980 output rate

### Sampling Rate
- **Per task commit:** `cargo build --target riscv32imc-esp-espidf` — verify compile success
- **Per wave merge:** `cargo build` + flash + MQTT broker subscription shows NMEA sentences arriving
- **Phase gate:** Hardware verification — `mosquitto_sub gnss/FFFEB5/nmea/#` shows sentence types from UM980, payloads are valid NMEA strings, no UART stall observed

### Wave 0 Gaps
- None — no test files needed. Validation is entirely hardware observation. `cargo build` compile check covers structural correctness.

---

## Sources

### Primary (HIGH confidence)
- `src/mqtt.rs` — `heartbeat_loop` (verified on device FFFEB5): `enqueue()` API, `Arc<Mutex<EspMqttClient<'static>>>` pattern, QoS enum import path, for-in-receiver blocking pattern
- `src/mqtt.rs` — `subscriber_loop` (verified): Mutex lock pattern, error handling on client methods
- `src/gnss.rs` — `spawn_gnss` (verified on device FFFEB5): current `mpsc::channel()` call site to be changed to `sync_channel(64)`; `nmea_tx.send()` call site to be changed to `try_send()`
- `src/main.rs` — Step 14 placeholder (verified): `_gnss_cmd_tx` and `_nmea_rx` bindings document Phase 5/6 handoff points
- `.planning/milestones/v1.0-REQUIREMENTS.md` — NMEA-01 and NMEA-02 exact requirement text

### Secondary (MEDIUM confidence)
- `std::sync::mpsc` standard library documentation — `sync_channel(N)` returns `(SyncSender<T>, Receiver<T>)`; `SyncSender::try_send()` returns `TrySendError::Full` or `TrySendError::Disconnected`; `Receiver<T>` is the same type from both `channel()` and `sync_channel()`
- Phase 4 RESEARCH.md — established patterns for thread spawn, NMEA type extraction, stack size conventions

### Tertiary (LOW confidence)
- None — all findings verified from project codebase or std documentation.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries in Cargo.toml, already in use and hardware-verified
- Architecture: HIGH — all patterns derived from existing verified-working project code
- NMEA-01 implementation: HIGH — direct adaptation of heartbeat_loop pattern
- NMEA-02 bounded channel: HIGH — std::sync::mpsc::sync_channel is well-documented; try_send semantics are clear
- enqueue() throughput at 10Hz: MEDIUM — heartbeat verified at 1/30Hz; 10Hz untested but within expected capacity

**Research date:** 2026-03-07
**Valid until:** 2026-06-07 (90 days — pinned crate versions, no external dependency changes expected)
