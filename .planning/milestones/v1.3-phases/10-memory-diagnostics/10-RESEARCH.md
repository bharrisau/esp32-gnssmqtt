# Phase 10: Memory + Diagnostics - Research

**Researched:** 2026-03-07
**Domain:** Embedded Rust / FreeRTOS task diagnostics / buffer pool pattern (ESP32, esp-idf-svc 0.51.0)
**Confidence:** HIGH

## Summary

Phase 10 addresses two independent hardening goals: (1) exposing stack high-water mark (HWM) for every thread at startup so operators can detect stack pressure, and (2) eliminating per-frame heap allocation in the RTCM relay path by pre-allocating a fixed buffer pool at init.

Both APIs are already available in the project's generated bindings. `uxTaskGetStackHighWaterMark(NULL)` is present in the FreeRTOS bindings included via `esp-idf-sys-0.36.1` (confirmed in `target/.../out/bindings.rs` line 14910). The buffer pool can be implemented using Rust's existing `std::sync::mpsc::sync_channel` as a free-list — no additional crate dependency is required.

The current RTCM channel type is `Receiver<(u16, Vec<u8>)>`. Each `Vec::from(&buf[..expected])` in `gnss.rs` line 245 allocates on the heap per frame. The pool replaces this with `Box<[u8; 1029]>` buffers that circulate between the GNSS RX thread and the RTCM relay thread.

**Primary recommendation:** Implement HARD-04 in one plan (add HWM log lines inside each thread closure) and HARD-03 in a second plan (replace Vec channel with a Box-pool channel pair). Keep plans independent — HWM touches every thread, pool changes touch gnss.rs and rtcm_relay.rs only.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HARD-03 | RTCM frame delivery uses a pre-allocated buffer pool at startup; no per-frame `Vec` allocation in steady state | Channel-as-pool pattern using `sync_channel` + `Box<[u8; 1029]>`: pre-allocate N boxes at init, circulate via free-pool channel. No new crate required. |
| HARD-04 | FreeRTOS task stack high-water mark (HWM) is logged at startup for every spawned thread | `uxTaskGetStackHighWaterMark(NULL)` confirmed in generated bindings. Called from inside each thread closure at entry. `NULL` handle = current task. Returns words (×4 = bytes on ESP32). |
</phase_requirements>

## Standard Stack

### Core (already in Cargo.toml)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-idf-sys | 0.36.1 | Raw FreeRTOS bindings via bindgen | Contains `uxTaskGetStackHighWaterMark`, `xTaskGetCurrentTaskHandle` |
| esp-idf-svc | 0.51.0 | std::sync::mpsc re-export target | `sync_channel` used as free-pool carrier |
| std::sync::mpsc | std | Channel-as-pool free-list | Avoids new dependency; already used throughout codebase |

### No New Dependencies Required

Both requirements are satisfiable with existing dependencies:
- FreeRTOS HWM: `esp_idf_svc::sys::uxTaskGetStackHighWaterMark` (via bindings re-export)
- Buffer pool: `std::sync::mpsc::sync_channel` carrying `Box<[u8; 1029]>`

**Installation:** No new `cargo add` commands needed.

## Architecture Patterns

### Recommended Project Structure

No new files required. Changes are localized to:

```
src/
├── gnss.rs          # pool init + try_recv from free_pool; send Box buffer on rtcm_tx
├── rtcm_relay.rs    # return Box buffer to free_pool after publish
├── main.rs          # add free_pool channel creation; thread HWM log call pattern
├── nmea_relay.rs    # HWM log line at thread entry
├── mqtt.rs          # HWM log line in pump/subscriber/heartbeat
├── wifi.rs          # HWM log line in wifi_supervisor
├── config_relay.rs  # HWM log line at thread entry
├── ota.rs           # HWM log line at thread entry
└── led.rs           # HWM log line in led_task
```

### Pattern 1: Stack HWM Logging (HARD-04)

**What:** At the top of each thread closure, call `uxTaskGetStackHighWaterMark(NULL)` and log the result. `NULL` means "current task" — no need to capture a task handle.

**When to use:** First statement inside every `move ||` closure passed to `thread::Builder::new().spawn()`.

**Return value unit:** Words (not bytes). On ESP32 (RISC-V riscv32imac), word = 4 bytes. Multiply by 4 to get bytes remaining at the HWM point. Note: HWM taken at thread entry gives ~full-stack headroom, not steady-state minimum — the value decreases over the thread's lifetime.

**Timing note:** For meaningful diagnostics, log HWM both at thread entry (to confirm configured stack is sane) AND optionally after the first major operation. Phase 10 only requires at-startup logging per the success criteria.

**Example:**
```rust
// Source: esp-idf-sys-0.36.1 generated bindings (target/.../out/bindings.rs:14910)
// At top of thread closure:
let hwm = unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) };
log::info!("GNSS RX thread started, stack HWM: {} words ({} bytes)",
    hwm, hwm * 4);
```

**Access path:** `esp_idf_svc::sys` re-exports `esp_idf_sys::*`. The function is at `esp_idf_svc::sys::uxTaskGetStackHighWaterMark`. Alternatively use `esp_idf_svc::hal::sys::uxTaskGetStackHighWaterMark` — both resolve to the same generated binding.

**Threads requiring HWM lines (per success criteria + current code):**

| Thread | Source file | Function | Current stack |
|--------|------------|---------|--------------|
| GNSS RX | gnss.rs | (closure in spawn_gnss) | 12288 |
| GNSS TX | gnss.rs | (closure in spawn_gnss) | 8192 |
| MQTT pump | mqtt.rs | pump_mqtt_events | 8192 |
| NMEA relay | nmea_relay.rs | spawn_relay closure | 8192 |
| RTCM relay | rtcm_relay.rs | spawn_relay closure | 8192 |
| Config relay | config_relay.rs | spawn_config_relay closure | 8192 |
| WiFi supervisor | wifi.rs | wifi_supervisor | 8192 |
| OTA task | ota.rs | spawn_ota closure | ? |
| LED task | led.rs | led_task | 8192 |
| UART bridge | uart_bridge.rs | spawn_bridge closure | 8192 |

Note: "Watchdog supervisor" and "status publisher" mentioned in success criteria do not exist yet — those are Phase 11 and Phase 13. Phase 10 HWM should cover currently-existing threads.

### Pattern 2: Channel-as-Pool (HARD-03)

**What:** A `sync_channel(N)` carries pre-allocated `Box<[u8; 1029]>` buffers as a "free list". The producer (GNSS RX) takes from the free pool, fills the buffer, sends it. The consumer (RTCM relay) publishes the buffer contents, then returns the buffer to the free pool.

**Pool size recommendation:** 4 buffers. At 1-4 RTCM frames/sec (MSM7 at 1Hz for up to 4 constellations), the relay processes each frame quickly (a single MQTT enqueue). 4 buffers cover the existing 32-slot data channel burst capacity while keeping pool memory fixed at 4 × 1029 = 4116 bytes.

**Channel type change:** `rtcm_rx: Receiver<(u16, Vec<u8>)>` becomes `rtcm_rx: Receiver<(u16, Box<[u8; 1029]>, usize)>` — message type, buffer, and valid byte count (since not every frame is 1029 bytes).

**Free pool channel:** `SyncSender<Box<[u8; 1029]>>` / `Receiver<Box<[u8; 1029]>>` with capacity N (the pool size). Both the GNSS RX closure and the RTCM relay need access: GNSS RX gets the `Receiver` end (to take buffers), RTCM relay gets the `Sender` end (to return buffers).

**Example — init and seeding the pool:**
```rust
// In spawn_gnss or main.rs before spawning:
const RTCM_POOL_SIZE: usize = 4;
// free_pool_tx goes to rtcm_relay (to return buffers)
// free_pool_rx goes to gnss RX thread (to take buffers)
let (free_pool_tx, free_pool_rx) = std::sync::mpsc::sync_channel::<Box<[u8; 1029]>>(RTCM_POOL_SIZE);
for _ in 0..RTCM_POOL_SIZE {
    free_pool_tx.send(Box::new([0u8; 1029])).expect("pool seed failed");
}
```

**Example — GNSS RX thread consuming from pool:**
```rust
// Instead of Box::new([0u8; 1029]) per frame:
match free_pool_rx.try_recv() {
    Ok(mut frame_buf) => {
        // fill frame_buf[0..expected] with frame bytes (already in state machine buf)
        frame_buf[..expected].copy_from_slice(&buf[..expected]);
        match rtcm_tx.try_send((msg_type, frame_buf, expected)) {
            Ok(_) => {}
            Err(TrySendError::Full((_, returned_buf, _))) => {
                // channel full: return buffer to pool, drop frame
                let _ = free_pool_tx.try_send(returned_buf);
                log::warn!("RTCM: relay channel full — frame dropped");
            }
            Err(TrySendError::Disconnected(_)) => {
                log::error!("RTCM: relay channel disconnected");
            }
        }
    }
    Err(_) => {
        // Pool exhausted: all buffers in flight — drop frame, log warning
        log::warn!("RTCM: buffer pool exhausted — frame dropped (pool size: {})", RTCM_POOL_SIZE);
    }
}
```

**Example — RTCM relay returning buffer:**
```rust
Ok((message_type, frame_buf, frame_len)) => {
    let topic = format!("gnss/{}/rtcm/{}", device_id, message_type);
    match client.lock() {
        Ok(mut c) => {
            let _ = c.enqueue(&topic, QoS::AtMostOnce, false, &frame_buf[..frame_len]);
        }
        Err(e) => log::warn!("RTCM relay: mutex poisoned: {:?}", e),
    }
    // Return buffer to pool — MUST happen regardless of enqueue success/failure
    let _ = free_pool_tx.send(frame_buf);
}
```

**Key constraint:** The `Box<[u8; 1029]>` buffer used in `RxState::RtcmBody` is currently allocated per-frame (`Box::new([0u8; 1029])` on line 216 of gnss.rs). With the pool, this allocation is replaced by a pool `try_recv()`. The `RxState::RtcmBody` variant must still hold the buffer during accumulation — no change to enum structure needed if the pool buffer is just moved into the state.

### Anti-Patterns to Avoid

- **Returning buffer on TrySendError::Full:** When the data channel is full and the frame must be dropped, the pool buffer MUST be returned to the free pool, not leaked. If the pool buffer is inside the `TrySendError::Full` payload, extract it: `Err(TrySendError::Full((_, buf, _))) => { let _ = free_pool_tx.try_send(buf); }`.
- **Holding Mutex across pool operations:** Do not hold the MQTT client mutex while waiting for pool operations. The pool channels are non-blocking (`try_recv`, `try_send`).
- **Pool size too small:** With pool size < 2, any RTCM burst can exhaust the pool before the relay processes the first frame. Minimum safe size is 2; 4 is recommended.
- **`unwrap()` on pool send at init:** Pool seed sends should panic (`expect`) — failure means misconfig. In steady-state, use `try_send` and treat failure as non-fatal.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Object pool / arena | Custom RefCell-based pool, unsafe global arrays | `sync_channel` as free-list | Already available, thread-safe, ownership-tracked |
| Stack HWM | Canary pattern, custom stack size probing | `uxTaskGetStackHighWaterMark(NULL)` | FreeRTOS built-in, already in bindings, O(1) |
| Buffer type | Custom fixed-size struct with capacity | `Box<[u8; 1029]>` | Matches existing RxState::RtcmBody pattern; zero new complexity |

**Key insight:** The channel-as-pool pattern uses Rust's ownership system to track buffer lifecycle — a buffer is either in the free pool (owned by `free_pool` channel), in-flight in the state machine (`RxState::RtcmBody`), or in transit to the relay (`rtcm_rx` channel). No unsafe code, no reference counting overhead beyond what already exists.

## Common Pitfalls

### Pitfall 1: HWM Called Before Stack Is "Warm"

**What goes wrong:** `uxTaskGetStackHighWaterMark(NULL)` at thread entry returns a value close to the full stack size because no stack frames have been used yet. This looks healthy but doesn't reflect steady-state usage.

**Why it happens:** HWM is a watermark (minimum ever seen), so at thread entry it equals ~(stack_size - startup overhead). The value only becomes useful diagnostically after the thread has executed its deepest call path.

**How to avoid:** Log at entry for "configured stack is sane" check. The success criterion only requires startup logging, which is fine for Phase 10. Document in code that the logged value is entry-point HWM, not steady-state minimum.

**Warning signs:** All threads logging HWM near their configured stack size (e.g., 8192 bytes reporting 7900+ bytes free) is normal at startup. Values below 500 bytes would be concerning.

### Pitfall 2: Buffer Leak on Disconnect Path

**What goes wrong:** When `rtcm_rx` returns `RecvTimeoutError::Disconnected`, the relay thread exits without returning any in-flight buffers to the pool. The pool starves on restart if the thread is restarted.

**Why it happens:** The disconnect path `break`s the loop before the buffer is returned.

**How to avoid:** The disconnect path does not hold a buffer (buffers are returned after each frame publish). The pool drain happens during frame handling. In the break path, there are no buffers in hand to return. This pitfall is moot given the current code structure, but worth noting for review.

### Pitfall 3: `RxState::RtcmBody` Box Allocation Still Active

**What goes wrong:** The existing `Box::new([0u8; 1029])` in `RxState::RtcmHeader` arm (gnss.rs line 216) remains in place even after adding pool logic, resulting in two heap allocations per frame.

**Why it happens:** The pool buffer must replace the `Box::new([0u8; 1029])` call in the header transition, not be a wrapper around it.

**How to avoid:** Remove the `Box::new([0u8; 1029])` in the `RtcmHeader` → `RtcmBody` transition. Replace with `free_pool_rx.try_recv()`. If pool is empty, return `RxState::Idle` with a warning.

### Pitfall 4: Channel Type Signature Change Breaks Callers

**What goes wrong:** Changing `rtcm_rx: Receiver<(u16, Vec<u8>)>` to `Receiver<(u16, Box<[u8; 1029]>, usize)>` requires updating `spawn_gnss`'s return type, `rtcm_relay::spawn_relay`'s parameter, and the call site in `main.rs`.

**Why it happens:** Rust's type system requires all three to be updated atomically.

**How to avoid:** Define a type alias: `type RtcmFrame = (u16, Box<[u8; 1029]>, usize);` in a shared location (or in gnss.rs with pub visibility). Update all three sites in one plan.

## Code Examples

Verified patterns from official sources:

### Stack HWM at Thread Entry
```rust
// Source: esp-idf-sys-0.36.1 bindings.rs:14910 (confirmed in project target/ directory)
// pub fn uxTaskGetStackHighWaterMark(xTask: TaskHandle_t) -> UBaseType_t;
// NULL handle = current task. UBaseType_t = u32 on ESP32.
let hwm_words = unsafe {
    esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
};
log::info!("GNSS RX thread started, stack HWM: {} words ({} bytes free)",
    hwm_words, hwm_words * 4);
```

### Pool Initialization in spawn_gnss
```rust
// Pool: RTCM_POOL_SIZE Box<[u8;1029]> buffers pre-allocated at init
// free_pool_rx → gnss RX thread (take a buffer before each frame)
// free_pool_tx → rtcm_relay thread (return buffer after publish)
const RTCM_POOL_SIZE: usize = 4;
let (free_pool_tx, free_pool_rx) =
    std::sync::mpsc::sync_channel::<Box<[u8; 1029]>>(RTCM_POOL_SIZE);
for _ in 0..RTCM_POOL_SIZE {
    free_pool_tx.send(Box::new([0u8; 1029]))
        .expect("RTCM pool init: send failed — channel closed at init?");
}
```

### Pool Exhaustion Drop Path
```rust
// In RxState::RtcmHeader → RtcmBody transition (replaces Box::new):
match free_pool_rx.try_recv() {
    Ok(mut frame_buf) => {
        frame_buf[0] = buf[0];
        frame_buf[1] = buf[1];
        frame_buf[2] = buf[2];
        RxState::RtcmBody { buf: frame_buf, len: 3, expected }
    }
    Err(_) => {
        // All pool buffers in use — relay is behind. Drop this frame.
        log::warn!("RTCM: buffer pool exhausted ({} slots) — frame dropped",
            RTCM_POOL_SIZE);
        RxState::Idle
    }
}
```

### Return Buffer After Publish
```rust
// In rtcm_relay, after enqueue:
Ok((message_type, frame_buf, frame_len)) => {
    let topic = format!("gnss/{}/rtcm/{}", device_id, message_type);
    match client.lock() {
        Ok(mut c) => {
            let _ = c.enqueue(&topic, QoS::AtMostOnce, false, &frame_buf[..frame_len]);
        }
        Err(e) => log::warn!("RTCM relay: mutex poisoned: {:?}", e),
    }
    // Return buffer to pool — must happen even if enqueue failed
    if free_pool_tx.send(frame_buf).is_err() {
        log::error!("RTCM relay: free pool channel closed — buffer leaked");
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `Vec::from(&buf[..expected])` per RTCM frame | Pool-allocated `Box<[u8; 1029]>` | Phase 10 | Eliminates per-frame heap allocation in steady state |
| No HWM visibility | `uxTaskGetStackHighWaterMark(NULL)` at thread entry | Phase 10 | Stack pressure visible in logs at startup |

**Deprecated/outdated:**
- `Receiver<(u16, Vec<u8>)>`: Replace with `Receiver<(u16, Box<[u8; 1029]>, usize)>` (or a type alias).

## Open Questions

1. **Free pool channel direction in spawn_gnss**
   - What we know: `spawn_gnss` currently returns `(cmd_tx, nmea_rx, rtcm_rx)`. The pool requires `free_pool_tx` to go to rtcm_relay and `free_pool_rx` to stay with the GNSS RX closure.
   - What's unclear: Whether to create the pool inside `spawn_gnss` (cleanest encapsulation, passes `free_pool_tx` through to return value or as a parameter to `rtcm_relay::spawn_relay`) or in `main.rs` (exposes internals but consistent with channel creation pattern already in main.rs).
   - Recommendation: Create pool inside `spawn_gnss`, pass `free_pool_tx` out as a fourth return value. `main.rs` passes it to `rtcm_relay::spawn_relay`. This keeps pool logic co-located with RTCM frame production.

2. **Type alias vs inline tuple**
   - What we know: `(u16, Box<[u8; 1029]>, usize)` is verbose.
   - What's unclear: Whether a type alias in gnss.rs is sufficient or a new struct is warranted.
   - Recommendation: Simple tuple is fine for this phase. A struct can be added in Phase 13 if health telemetry adds drop counters to the frame path.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | None — embedded ESP32 target; no native test runner available |
| Config file | none |
| Quick run command | `cargo build --release 2>&1 \| tail -5` |
| Full suite command | `cargo build --release 2>&1` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HARD-04 | Each thread logs HWM line at startup | smoke (visual log inspection) | `cargo build --release` compiles without error | ❌ Wave 0: verify by log inspection at runtime |
| HARD-03 | No `Vec::new()` in RTCM hot path in steady state | code audit | `grep -n "Vec::from\|Vec::new" src/gnss.rs src/rtcm_relay.rs` returns 0 hits in RTCM path | ✅ grep-based, runnable now |
| HARD-03 | Pool exhaustion drops frame + logs warning (not panic) | code review | Inspect `Err(_)` arm of `free_pool_rx.try_recv()` path | ❌ Wave 0: add path |

### Sampling Rate

- **Per task commit:** `cargo build --release 2>&1 | grep -E "^error"`
- **Per wave merge:** `cargo build --release 2>&1`
- **Phase gate:** Full release build green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] Pool exhaustion code path — does not exist yet; created in 10-01-PLAN
- [ ] HWM log lines — do not exist yet; created in 10-02-PLAN (or 10-01 if combined)
- No test framework install needed — project uses build verification only

## Sources

### Primary (HIGH confidence)

- `target/riscv32imac-esp-espidf/release/build/esp-idf-sys-e9d2dea3ab2ed781/out/bindings.rs` lines 14910, 15090 — `uxTaskGetStackHighWaterMark` and `xTaskGetCurrentTaskHandle` confirmed present with exact signatures
- `esp-idf-sys-0.36.1/src/include/esp-idf/bindings.h` — `freertos/task.h` included, confirming HWM function will be in generated bindings
- `src/gnss.rs` lines 216, 245 — current per-frame `Box::new` and `Vec::from` heap allocation sites confirmed
- `src/main.rs` — all thread spawn sites enumerated with stack sizes
- `Cargo.toml` — no new dependencies needed (std::sync::mpsc available)

### Secondary (MEDIUM confidence)

- `INCLUDE_uxTaskGetStackHighWaterMark: u32 = 1` (bindings.rs line 3231) — FreeRTOS config flag confirming function is compiled in
- `INCLUDE_xTaskGetCurrentTaskHandle: u32 = 1` (bindings.rs line 3236) — current task handle API confirmed enabled
- esp-idf-sys-0.36.1 `src/include/esp-idf/bindings.h` — `esp_task_wdt.h` also included (relevant for Phase 11)

### Tertiary (LOW confidence)

- None — all critical claims verified against local build artifacts.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — verified against local bindings.rs generated file
- Architecture: HIGH — channel-as-pool pattern verified against existing codebase patterns; no external crate needed
- Pitfalls: HIGH — derived from direct code inspection of gnss.rs state machine

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable — esp-idf-sys version pinned in Cargo.toml)
