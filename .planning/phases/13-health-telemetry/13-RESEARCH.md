# Phase 13: Health Telemetry - Research

**Researched:** 2026-03-07
**Domain:** ESP32 firmware — atomic counters, ESP-IDF heap/timer APIs, MQTT publish patterns
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- On every reconnect (at heartbeat thread start), publish retained `"online"` to `gnss/{device_id}/status` — overwrites LWT "offline" retained message
- Retained "online" publish happens ONCE per reconnect, NOT repeated on every heartbeat tick
- Health JSON goes to `gnss/{device_id}/heartbeat` (replaces the existing `b"online"` payload in `heartbeat_loop`)
- Make interval configurable in `src/config.rs` as a named constant (e.g., `HEARTBEAT_INTERVAL_SECS`)
- Default value: 30 seconds (the existing cadence — 60s was a spec placeholder)
- Include ALL available metrics: `uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`
- Counters are cumulative since last boot (no reset on publish) — METR-02 spec
- `GNSS_RX_HEARTBEAT` excluded — watchdog mechanism, not a meaningful health metric
- Health JSON published to `/heartbeat` with `retain=false` (ephemeral)
- LWT on `/status` handles offline indication; no stale health data needed

### Claude's Discretion
- Exact JSON serialization approach (manual `format!` string consistent with existing codebase pattern — no serde dependency)
- How uptime is measured (`esp_timer_get_time() / 1_000_000` or similar ESP-IDF call)
- How `heap_free` is obtained (`esp_get_free_heap_size()` via `esp-idf-svc::sys`)
- Whether to add new atomics in `gnss.rs` or in a new `telemetry.rs` module

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| METR-01 | Device publishes `{"uptime_s":N,"heap_free":N,"nmea_drops":N,"rtcm_drops":N}` to `gnss/{device_id}/status` every 60 seconds (updated per decisions: 30s cadence, `/heartbeat` topic, additional fields) | `esp_get_free_heap_size()` confirmed in `esp-idf-sys-0.36.1` example; `esp_timer_get_time()` confirmed in `esp-idf-svc-0.51.0` timer.rs; `format!()` JSON pattern used in ota.rs |
| METR-02 | NMEA and RTCM drop counters are atomic; incremented at each `TrySendError::Full` drop site in gnss.rs | `AtomicU32` static pattern confirmed in gnss.rs (`UART_TX_ERRORS`), watchdog.rs (`GNSS_RX_HEARTBEAT`), resil.rs (`MQTT_DISCONNECTED_AT`); `TrySendError::Full` sites at gnss.rs:209 (NMEA) and gnss.rs:294 (RTCM) |
</phase_requirements>

---

## Summary

Phase 13 extends the existing `heartbeat_loop` in `src/mqtt.rs` with two changes: (1) publish retained "online" to `/status` once at thread start (clears the LWT "offline" retained message on reconnect), and (2) replace the static `b"online"` payload with a JSON health snapshot built via `format!()`. Two new `AtomicU32` statics (`NMEA_DROPS` and `RTCM_DROPS`) must be added and incremented at the `TrySendError::Full` sites in `src/gnss.rs`. A config constant `HEARTBEAT_INTERVAL_SECS` replaces the hardcoded `30s` sleep.

This phase has minimal risk: all patterns already exist in the codebase. The ESP-IDF sys APIs for heap (`esp_get_free_heap_size()`) and uptime (`esp_timer_get_time()`) are confirmed present in the linked crate versions. The `format!()` JSON construction pattern is used in `ota.rs`. The `Arc<Mutex<EspMqttClient>>` + `enqueue()` publish pattern is the established non-blocking approach.

**Primary recommendation:** Extend `heartbeat_loop` in-place (no new thread), add two atomics in `gnss.rs`, wire `HEARTBEAT_INTERVAL_SECS` into `config.example.rs`. Four touch points total: `gnss.rs`, `mqtt.rs`, `config.example.rs`, `main.rs` (comment update only).

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp-idf-sys` (via `esp-idf-svc::sys`) | 0.36.1 | Raw ESP-IDF C bindings: `esp_get_free_heap_size()`, `esp_timer_get_time()` | Already a direct Cargo.toml dependency; full path access confirmed in phases 10/11 |
| `std::sync::atomic::AtomicU32` | std | Drop counter storage | Established project pattern; `AtomicU64` not available on Xtensa ESP32 target |
| `embedded_svc::mqtt::client::QoS` | 0.28.1 | MQTT QoS enum | Already imported in mqtt.rs |
| `std::sync::{Arc, Mutex}` | std | Shared MQTT client handle | Established pattern: ota.rs, mqtt.rs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `format!()` macro | std | Manual JSON construction — no serde | Consistent with ota.rs pattern; avoids new dependency |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `esp_timer_get_time()` / 1_000_000 for uptime | `std::time::SystemTime` | `SystemTime` is not guaranteed monotonic and has no clock source on ESP32; `esp_timer_get_time()` is the monotonic timer since boot (microseconds) |
| `esp_get_free_heap_size()` for heap | custom tracking | `esp_get_free_heap_size()` is a single FFI call; no custom tracking needed |
| Manual `format!()` for JSON | `serde_json` | serde adds ~50KB to binary; project has no serde dep; ota.rs pattern is sufficient for 5-field flat JSON |

**Installation:** No new dependencies needed.

---

## Architecture Patterns

### Existing heartbeat_loop (current state)
```
src/mqtt.rs:heartbeat_loop()
  - 5s initial delay
  - loop { enqueue(b"online", retain=true); sleep(30s) }
```

### Modified heartbeat_loop (Phase 13 target)
```
src/mqtt.rs:heartbeat_loop()
  - 5s initial delay
  - ONE-TIME: enqueue("online", retain=true) to gnss/{id}/status  <- clears LWT
  - loop {
      read atomics: NMEA_DROPS, RTCM_DROPS, UART_TX_ERRORS (from gnss.rs)
      read uptime_s: esp_timer_get_time() / 1_000_000
      read heap_free: esp_get_free_heap_size()
      build JSON with format!()
      enqueue(json_bytes, retain=false) to gnss/{id}/heartbeat
      sleep(HEARTBEAT_INTERVAL_SECS)
    }
```

### Pattern 1: Atomic Counter for Drop Tracking (METR-02)
**What:** `static NMEA_DROPS: AtomicU32` and `static RTCM_DROPS: AtomicU32` in gnss.rs, incremented at each `TrySendError::Full` site.
**When to use:** Any cross-thread statistic that is write-heavy and read-rarely.
**Example:**
```rust
// Source: gnss.rs (existing UART_TX_ERRORS pattern)
static NMEA_DROPS: AtomicU32 = AtomicU32::new(0);
static RTCM_DROPS: AtomicU32 = AtomicU32::new(0);

// At TrySendError::Full for NMEA (gnss.rs ~line 209):
Err(TrySendError::Full(_)) => {
    NMEA_DROPS.fetch_add(1, Ordering::Relaxed);
    log::warn!("NMEA: relay channel full — sentence dropped");
}

// At TrySendError::Full for RTCM (gnss.rs ~line 294):
Err(TrySendError::Full((_, returned_buf, _))) => {
    RTCM_DROPS.fetch_add(1, Ordering::Relaxed);
    log::warn!("RTCM: relay channel full — frame dropped");
    let _ = free_pool_tx_clone.try_send(returned_buf);
}
```

### Pattern 2: ESP-IDF Uptime and Heap (METR-01)
**What:** Direct `esp-idf-svc::sys` FFI calls for monotonic uptime and free heap.
**When to use:** When a metric is only needed once per heartbeat (no caching needed).
**Example:**
```rust
// Source: esp-idf-svc-0.51.0/src/timer.rs (uses same call internally)
let uptime_us = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
let uptime_s = uptime_us / 1_000_000;

// Source: esp-idf-sys-0.36.1/examples/unsafe_call.rs
let heap_free = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };
```

### Pattern 3: Manual JSON Construction (METR-01 payload)
**What:** `format!()` macro builds flat JSON with integer fields — no serde.
**When to use:** Flat structure, no optional fields, no string escaping needed.
**Example:**
```rust
// Source: ota.rs format!() JSON pattern
let json = format!(
    "{{\"uptime_s\":{},\"heap_free\":{},\"nmea_drops\":{},\"rtcm_drops\":{},\"uart_tx_errors\":{}}}",
    uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors
);
```

### Pattern 4: One-Time Retained Publish Before Loop
**What:** Publish retained "online" to `/status` before entering the loop — clears LWT.
**When to use:** Thread restart = reconnect event; initial state correction needed.
**Example:**
```rust
// Source: ota.rs and mqtt.rs enqueue() pattern
let status_topic = format!("gnss/{}/status", device_id);
match client.lock() {
    Err(e) => log::warn!("Heartbeat: status mutex poisoned: {:?}", e),
    Ok(mut c) => match c.enqueue(&status_topic, QoS::AtLeastOnce, true, b"online") {
        Ok(_) => log::info!("Heartbeat: published retained online to {}", status_topic),
        Err(e) => log::warn!("Heartbeat: status publish failed: {:?}", e),
    },
}
```

### Anti-Patterns to Avoid
- **retain=true for the health JSON heartbeat:** The `/heartbeat` topic must use `retain=false`. The decision is locked: stale health data is unwanted; LWT covers offline indication.
- **Resetting counters on publish:** Counters are cumulative since boot (METR-02). Do NOT reset to zero after each read.
- **Repeating the retained "online" publish on every tick:** The retained publish happens ONCE before the loop. Repeating it every 30s causes unnecessary broker writes.
- **Using `AtomicU64` for counters:** Not available on ESP32 Xtensa target (confirmed in resil.rs comment). Use `AtomicU32`.
- **Using `client.publish()` (blocking) for retained status:** Use `enqueue()` (non-blocking) consistent with the existing heartbeat pattern. `publish()` blocks until acknowledged; `enqueue()` queues and returns immediately.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Uptime measurement | Custom boot-time tracking with `SystemTime` | `esp_timer_get_time()` via `esp-idf-svc::sys` | Monotonic since boot, microsecond precision, single FFI call, already used internally by `esp-idf-svc` timer module |
| Free heap query | Custom heap tracking via allocation hooks | `esp_get_free_heap_size()` via `esp-idf-svc::sys` | Confirmed in `esp-idf-sys-0.36.1` example; standard ESP-IDF call |
| JSON serialization | Custom serializer | `format!()` macro | For 5 integer fields, `format!()` is readable, safe, and zero-dependency |

**Key insight:** All needed APIs are already present in the Cargo.toml dependencies — no new crates needed.

---

## Common Pitfalls

### Pitfall 1: RTCM Drop Counter — Buffer Must Still Return to Pool
**What goes wrong:** Adding `RTCM_DROPS.fetch_add(1, ...)` at the `TrySendError::Full` site while removing or misplacing the `free_pool_tx_clone.try_send(returned_buf)` call.
**Why it happens:** The full error arm is more complex than the NMEA arm — it carries the returned buffer tuple and must return it to the pool.
**How to avoid:** The `fetch_add` line goes BEFORE or AFTER the existing `free_pool_tx_clone.try_send(returned_buf)` — never replace it. Both lines must be present.
**Warning signs:** RTCM frames start dropping after 4 RTCM relay full events (pool exhausted).

### Pitfall 2: heartbeat_loop Stack Size
**What goes wrong:** `format!()` for the JSON string plus `esp_timer_get_time()` / `esp_get_free_heap_size()` calls may push the stack beyond the current 8192-byte allocation.
**Why it happens:** The JSON string is heap-allocated (`String` via `format!`), but `format!` uses a temporary stack buffer during formatting. Combined with the existing HWM values, headroom may be tight.
**How to avoid:** Keep stack at 8192 — the JSON string is short (~80 bytes), `format!` is well within budget. Verify with HWM log at thread entry (already present in heartbeat_loop).
**Warning signs:** Crash / stack overflow immediately after heartbeat thread starts.

### Pitfall 3: `enqueue()` vs `publish()` for Retained Status Publish
**What goes wrong:** Using `publish()` for the one-time retained "online" publish blocks until broker ACKs. If MQTT is not yet fully established (e.g., 5s initial delay passes but broker is slow), this can stall the heartbeat thread indefinitely.
**Why it happens:** `publish()` is blocking (waits for ACK); `enqueue()` is non-blocking (queues and returns).
**How to avoid:** Use `enqueue()` for both the retained status publish and the periodic heartbeat JSON publish. Consistent with the existing pattern.
**Warning signs:** Heartbeat thread does not reach its loop; first publish hangs.

### Pitfall 4: Module Visibility of New Atomics
**What goes wrong:** `NMEA_DROPS` and `RTCM_DROPS` defined as private statics in `gnss.rs` and accessed from `mqtt.rs` via `crate::gnss::NMEA_DROPS` — compile error if not `pub`.
**Why it happens:** Default Rust static visibility is private to the module.
**How to avoid:** Declare as `pub static NMEA_DROPS: AtomicU32` (matching `pub static GNSS_RX_HEARTBEAT` in watchdog.rs).
**Warning signs:** `error[E0603]: static ... is private` at compile time.

### Pitfall 5: `esp_timer_get_time()` Return Type
**What goes wrong:** `esp_timer_get_time()` returns `i64` (microseconds since boot). Casting directly to `u64` then dividing is safe for boot times up to ~9.2 × 10^12 seconds. But `/` before cast can silently truncate.
**Why it happens:** The raw return is `i64`; treating it as `u64` for display is fine but the division order matters.
**How to avoid:** `let uptime_s = unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1_000_000;` — divide while still `i64`, then cast to display. Consistent with `esp-idf-svc` timer.rs usage.
**Warning signs:** Negative uptime values in published JSON.

---

## Code Examples

Verified patterns from codebase:

### Reading UART_TX_ERRORS from outside gnss.rs (cross-module atomic access)
```rust
// Source: gnss.rs UART_TX_ERRORS pattern + watchdog.rs GNSS_RX_HEARTBEAT cross-module read
// In mqtt.rs heartbeat_loop:
let uart_tx_errors = crate::gnss::UART_TX_ERRORS.load(Ordering::Relaxed);
let nmea_drops = crate::gnss::NMEA_DROPS.load(Ordering::Relaxed);
let rtcm_drops = crate::gnss::RTCM_DROPS.load(Ordering::Relaxed);
```

### Config constant addition
```rust
// Source: config.example.rs pattern (existing WDT_CHECK_INTERVAL, RELAY_RECV_TIMEOUT)
/// Heartbeat publish interval in seconds.
/// Default: 30s (existing cadence). Lower values increase broker message rate.
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
```

### Heartbeat sleep using the new constant
```rust
// Replace the hardcoded Duration::from_secs(30) in heartbeat_loop
std::thread::sleep(std::time::Duration::from_secs(crate::config::HEARTBEAT_INTERVAL_SECS));
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Static `b"online"` heartbeat payload | Structured JSON health snapshot | Phase 13 | Operators can observe device vitals remotely |
| No LWT clearing on reconnect | Retained "online" published to `/status` on thread start | Phase 13 | MQTT status topic accurately reflects live vs. offline state |
| Hardcoded 30s sleep in heartbeat_loop | Configurable `HEARTBEAT_INTERVAL_SECS` constant | Phase 13 | Interval tunable without code change |

---

## Open Questions

1. **Should `UART_TX_ERRORS` be made `pub` in gnss.rs?**
   - What we know: It's currently `static UART_TX_ERRORS: AtomicU32` (private). The comment says "Will be read by the health telemetry subsystem (Phase 13)".
   - What's unclear: Whether it was intentionally left private pending this phase.
   - Recommendation: Make it `pub static` alongside `NMEA_DROPS` and `RTCM_DROPS` — all three are read by `mqtt.rs:heartbeat_loop`. Consistent with `pub static GNSS_RX_HEARTBEAT` in watchdog.rs.

2. **Where to define `NMEA_DROPS` and `RTCM_DROPS` — in `gnss.rs` or a new `telemetry.rs`?**
   - What we know: CONTEXT.md marks this as "Claude's Discretion". `UART_TX_ERRORS` is in gnss.rs. Placing new counters there keeps all GNSS drop accounting together.
   - Recommendation: Add to `gnss.rs` alongside `UART_TX_ERRORS` — no new module needed, consistent ownership, single file to modify for METR-02.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None detected — embedded firmware (no `tests/`, no `pytest.ini`, no `jest.config.*`) |
| Config file | None |
| Quick run command | `cargo build --release` (compile-time type/borrow check) |
| Full suite command | `cargo build --release` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| METR-01 | JSON published to `/heartbeat` with 5 fields at 30s cadence | manual-only (requires live ESP32 + MQTT broker) | `cargo build --release` (type-checks format! args, API calls) | N/A |
| METR-02 | `NMEA_DROPS` / `RTCM_DROPS` incremented at `TrySendError::Full` | manual-only (requires observable channel saturation) | `cargo build --release` (compile-checks atomic usage) | N/A |

**Manual-only justification:** This is an embedded firmware project. The only execution environment is the target ESP32 hardware with a live MQTT broker. No host-side unit test harness exists or is planned. Verification is performed by flashing and observing the MQTT broker with `mosquitto_sub`.

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release`
- **Phase gate:** Full build green + live device verification per VERIFICATION.md

### Wave 0 Gaps
None — no test infrastructure expected or needed for this project.

---

## Sources

### Primary (HIGH confidence)
- `esp-idf-sys-0.36.1/examples/unsafe_call.rs` (local registry) — confirms `esp_get_free_heap_size()` is available as a direct FFI call
- `esp-idf-svc-0.51.0/src/timer.rs` (local registry) — confirms `esp_timer_get_time()` is the uptime source used internally by the svc crate
- `esp-idf-svc-0.51.0/src/mqtt/client.rs` (local registry) — confirms `enqueue()` and `publish()` signatures, both take `(topic, QoS, retain, &[u8])`
- `src/gnss.rs` (project source) — confirms `UART_TX_ERRORS` static pattern, `TrySendError::Full` sites at lines ~209 and ~294
- `src/mqtt.rs` (project source) — confirms `heartbeat_loop` current implementation, `enqueue()` usage pattern
- `src/ota.rs` (project source) — confirms `format!()` JSON construction pattern, `publish_status()` locking pattern
- `src/watchdog.rs` (project source) — confirms `pub static AtomicU32` cross-module pattern
- `src/resil.rs` (project source) — confirms `AtomicU32` (not `AtomicU64`) is the correct type for ESP32 Xtensa target
- `src/config.example.rs` (project source) — confirms constant naming convention and doc comment style

### Secondary (MEDIUM confidence)
- None needed — all relevant APIs confirmed in local registry sources.

### Tertiary (LOW confidence)
- None.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all APIs confirmed in local crate sources (esp-idf-sys 0.36.1, esp-idf-svc 0.51.0)
- Architecture: HIGH — all patterns are direct extensions of existing project code; no novel approaches
- Pitfalls: HIGH — derived from direct code inspection of the exact files being modified

**Research date:** 2026-03-07
**Valid until:** 2026-06-07 (stable embedded crates; no moving targets)
