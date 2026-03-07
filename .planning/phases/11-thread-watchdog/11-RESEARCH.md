# Phase 11: Thread Watchdog - Research

**Researched:** 2026-03-07
**Domain:** Rust atomics, software watchdog pattern, ESP-IDF esp_restart, FreeRTOS Task WDT
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| WDT-01 | Each critical thread (GNSS RX, MQTT pump) feeds a shared atomic watchdog counter at a regular interval (≤ 5s) | `RELAY_RECV_TIMEOUT` is already 5s on both threads; timeout arm is currently a no-op — this is the exact insertion point. `AtomicU32::fetch_add` or `store` in that arm satisfies the requirement with no structural changes. |
| WDT-02 | A watchdog supervisor thread detects if any critical thread misses 3 consecutive heartbeats and triggers `esp_restart()` | Supervisor sleeps for `RELAY_RECV_TIMEOUT` (5s), reads each counter, compares to last-seen value; 3 unchanged readings = 15s window before `esp_restart()`. Hardware TWDT (30s) backstops the supervisor itself. |

</phase_requirements>

---

## Summary

Phase 11 implements a software watchdog layered on top of the already-present hardware Task Watchdog Timer (TWDT). The firmware already has `CONFIG_ESP_TASK_WDT_EN=y` and `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` in `sdkconfig.defaults`. The existing `recv_timeout` loops in the GNSS RX thread and MQTT pump thread already expire every 5 seconds — Phase 09-02 explicitly left their `Timeout` arms as no-ops with the comment "Phase 11 will feed watchdog heartbeat counters here without structural changes."

The implementation requires exactly two things: (1) a pair of `AtomicU32` heartbeat counters, each incremented inside the `Timeout` arm (and also on every successful iteration) of the two critical threads; (2) a new `watchdog` module containing a supervisor thread that polls both counters at 5-second intervals and calls `esp_restart()` after 3 consecutive missed increments. No restructuring of existing threads is needed.

**Primary recommendation:** Place two static `AtomicU32` counters (one per critical thread) in a new `src/watchdog.rs` module. Export a `spawn_supervisor()` function that takes `Arc` references to both counters and runs the check loop. Wire the supervisor in `main.rs` as the final spawn (Step 18), after all other threads are started.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::sync::atomic::AtomicU32` | stdlib | Heartbeat counter — shared between critical thread and supervisor | Lock-free, no overhead on hot path; single CAS on increment |
| `std::sync::atomic::Ordering::Relaxed` | stdlib | Load/store ordering for heartbeat counters | Counters are monotonically increasing; no happens-before needed across threads — `Relaxed` is correct and maximally efficient |
| `esp_idf_svc::sys::esp_restart()` | esp-idf-svc 0.51.0 | Hard device reboot | Already used conceptually for Phase 12; available via `esp_idf_svc::sys::esp_restart()` (unsafe fn, no args) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `std::sync::Arc` | stdlib | Share `AtomicU32` between spawner and supervisor | Standard pattern — already used throughout codebase for `Arc<AtomicU8>` (LED state) |
| `std::thread::sleep` | stdlib | Supervisor check interval | Already used in wifi_supervisor, heartbeat_loop |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `AtomicU32` monotonic counter | `AtomicBool` last-updated flag | Bool requires atomic reset after each read (compare-exchange); counter is simpler — read, compare to last-seen value, no write needed from supervisor |
| `AtomicU32` monotonic counter | `std::time::Instant` in `Mutex` | Mutex acquisition in hot path; unnecessary complexity |
| Software counter comparison | ESP-IDF TWDT subscription API | TWDT subscription requires each task to call `esp_task_wdt_reset()` — not available from Rust std threads without unsafe FFI per-task handle management. Software counter is simpler and correct. |

**Installation:** No new dependencies required. All components are in `std` or already-present `esp-idf-svc`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── watchdog.rs     # NEW: AtomicU32 counters + spawn_supervisor()
├── gnss.rs         # MODIFY: increment GNSS_RX_HEARTBEAT in recv_timeout Timeout arm
├── mqtt.rs         # MODIFY: increment MQTT_PUMP_HEARTBEAT in connection.next() loop
├── main.rs         # MODIFY: spawn watchdog supervisor (Step 18)
└── config.rs       # MODIFY: add WDT_CHECK_INTERVAL, WDT_MISS_THRESHOLD constants
```

### Pattern 1: Monotonic Counter Heartbeat

**What:** Each critical thread increments a shared `AtomicU32` at every loop iteration that demonstrates progress (both on data received AND on timeout). The supervisor records the counter value each check cycle and counts how many consecutive cycles the value was unchanged.

**When to use:** Any thread with a known maximum idle period — already enforced by `recv_timeout` in this codebase.

**Example:**
```rust
// In watchdog.rs
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub static GNSS_RX_HEARTBEAT: AtomicU32 = AtomicU32::new(0);
pub static MQTT_PUMP_HEARTBEAT: AtomicU32 = AtomicU32::new(0);

pub fn spawn_supervisor() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || supervisor_loop())
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("watchdog spawn failed: {:?}", e))
}

fn supervisor_loop() -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "WDT sup", hwm_words, hwm_words * 4);

    let mut last_gnss: u32 = 0;
    let mut last_mqtt: u32 = 0;
    let mut gnss_misses: u32 = 0;
    let mut mqtt_misses: u32 = 0;

    loop {
        std::thread::sleep(crate::config::WDT_CHECK_INTERVAL);

        let gnss_now = GNSS_RX_HEARTBEAT.load(Ordering::Relaxed);
        let mqtt_now = MQTT_PUMP_HEARTBEAT.load(Ordering::Relaxed);

        if gnss_now == last_gnss {
            gnss_misses += 1;
            log::warn!("WDT: GNSS RX heartbeat missed ({}/{})", gnss_misses, crate::config::WDT_MISS_THRESHOLD);
            if gnss_misses >= crate::config::WDT_MISS_THRESHOLD {
                log::error!("WDT: GNSS RX thread hung — rebooting");
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
        } else {
            gnss_misses = 0;
            last_gnss = gnss_now;
        }

        if mqtt_now == last_mqtt {
            mqtt_misses += 1;
            log::warn!("WDT: MQTT pump heartbeat missed ({}/{})", mqtt_misses, crate::config::WDT_MISS_THRESHOLD);
            if mqtt_misses >= crate::config::WDT_MISS_THRESHOLD {
                log::error!("WDT: MQTT pump thread hung — rebooting");
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
        } else {
            mqtt_misses = 0;
            last_mqtt = mqtt_now;
        }
    }
}
```

```rust
// In gnss.rs RX thread — Timeout arm (currently a no-op comment):
Err(RecvTimeoutError::Timeout) => {
    // No command within 5s — normal during idle. Feed watchdog.
    crate::watchdog::GNSS_RX_HEARTBEAT.fetch_add(1, Ordering::Relaxed);
}
// Also add fetch_add in the Ok arm (or at top of loop):
// Ensures progress is signalled whether data arrives or not.
```

```rust
// In mqtt.rs pump — connection.next() loop:
// pump uses `while let Ok(event) = connection.next()` — no recv_timeout pattern.
// Add fetch_add at the top of the while-let body:
while let Ok(event) = connection.next() {
    crate::watchdog::MQTT_PUMP_HEARTBEAT.fetch_add(1, Ordering::Relaxed);
    match event.payload() { ... }
}
```

### Pattern 2: MQTT Pump Heartbeat Insertion Point

**What:** The MQTT pump uses `connection.next()` (not `recv_timeout`). It is a blocking call that returns whenever the MQTT stack delivers an event. The UM980 sends 10 Hz NMEA, and the MQTT heartbeat publishes every 30s — so `connection.next()` returns very frequently in normal operation.

**When to use:** `connection.next()` is a blocking call. The heartbeat fetch_add should go at the **top** of the `while let Ok(...)` body, executed on every iteration. This means the MQTT pump counter increments on every MQTT event (including internal keepalive/ping events from the broker), guaranteeing updates well within 5 seconds during normal operation.

**Consideration:** During GNSS activity, the MQTT pump processes many `Received` and other events per second. During MQTT disconnect, `connection.next()` may not return until broker reconnects. This is acceptable — MQTT pump being stuck in `connection.next()` for > 15s IS the hung-thread condition that warrants a reboot.

### GNSS RX Thread Heartbeat Insertion Point

The GNSS RX thread uses a bare `loop` with `uart_rx.read(..., NON_BLOCK)`. There is no `recv_timeout` in the RX thread itself — it loops continuously, sleeping 10ms when no data arrives. The heartbeat should be incremented once per loop iteration (or once per 10ms sleep), guaranteeing updates far more frequently than 5 seconds. The GNSS TX thread uses `recv_timeout(RELAY_RECV_TIMEOUT)` but is NOT a "critical thread" per WDT-01 (only GNSS RX and MQTT pump are named). Only GNSS RX and MQTT pump need heartbeats.

**GNSS RX heartbeat location:** Add `fetch_add(1, Relaxed)` at the top of the GNSS RX `loop {}` (every iteration, including the `_ => sleep(10ms)` arm). This is the simplest correct insertion.

### Anti-Patterns to Avoid

- **Heartbeat only in the Ok arm:** If GNSS data stops arriving (UART stall), the RX thread still loops (sleeping 10ms per `NON_BLOCK` read that returns 0 bytes). Heartbeat must increment even on zero-byte reads or the supervisor would falsely detect a hang. Put it at the **top of the outer loop**.
- **AtomicBool with reset:** A bool requires the supervisor to atomically read-and-reset (compare-exchange) so the thread doesn't have to; a counter requires only `load` from supervisor and `fetch_add` from the thread. Counter is simpler.
- **Holding the counter in supervisor-owned state:** Do not put counters inside the supervisor closure. They must be accessible from both the critical thread and the supervisor. `static AtomicU32` (or `Arc<AtomicU32>` passed to each thread) is required.
- **Checking the supervisor thread with the hardware TWDT subscription API:** ESP-IDF's `esp_task_wdt_add(NULL)` can subscribe a task to the hardware TWDT, but from Rust std threads this requires per-thread `esp_task_wdt_reset()` calls mapped to task handles. This is complex, fragile, and unnecessary — the hardware TWDT at 30s already covers the supervisor thread without registration, because if the supervisor thread hangs, no registered task will call `esp_task_wdt_reset()` either (or the idle task check fires first).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Restart function | Custom reset sequence | `esp_idf_svc::sys::esp_restart()` | This is the ESP-IDF canonical reboot; guarantees clean peripheral reset and bootloader handoff |
| Timeout detection | Wall-clock `Instant` comparison | Monotonic counter miss counting | Counters need zero synchronization; no heap allocation; Instant requires `Mutex` or unsafe atomic ops |
| Hardware WDT registration | `esp_task_wdt_add` FFI per thread | None — hardware TWDT at 30s already active | TWDT is already configured (30s); software watchdog catches hangs in 15s (3 × 5s); TWDT is the backstop for supervisor itself |

**Key insight:** The existing `recv_timeout` structure already delivers progress signals at ≤ 5s granularity. Phase 11 only needs to wire those signals to counters — no thread restructuring needed.

---

## Common Pitfalls

### Pitfall 1: GNSS RX thread has no recv_timeout — wrong insertion point
**What goes wrong:** Developer adds heartbeat only to the `recv_timeout` Timeout arm (GNSS TX, NMEA relay, RTCM relay) but the GNSS RX thread is a bare polling loop with `NON_BLOCK` reads. If heartbeat is added to the wrong thread's Timeout arm, the GNSS RX thread is not actually monitored.
**Why it happens:** The STATE.md note "Phase 11 will feed watchdog heartbeat counters here without structural changes" refers to threads with `recv_timeout`. GNSS RX does not use `recv_timeout` — it uses `NON_BLOCK` poll + sleep.
**How to avoid:** Add heartbeat to the GNSS RX outer `loop {}` top, not to any `recv_timeout` match arm. The GNSS RX loop runs every 10ms (sleep when idle); the counter will update far more frequently than 5 seconds.
**Warning signs:** If after a UART stall the supervisor reboots when it shouldn't, or doesn't reboot when it should, check which loop body the `fetch_add` is in.

### Pitfall 2: MQTT pump connection.next() — not a recv_timeout
**What goes wrong:** Developer assumes MQTT pump has a `recv_timeout` like the relay threads and looks for a `Timeout` arm. `pump_mqtt_events` uses `while let Ok(event) = connection.next()` — a blocking call with no Rust-level timeout. There is no `RecvTimeoutError::Timeout` to catch.
**Why it happens:** The MQTT C event loop handles reconnection internally; from Rust's perspective, `connection.next()` either returns an event or blocks. The pump does not use `mpsc::recv_timeout`.
**How to avoid:** Add `MQTT_PUMP_HEARTBEAT.fetch_add(1, Relaxed)` at the top of the `while let Ok(event)` body. Every event (including internal ping/pong) increments the counter. A truly hung pump will fail to call `connection.next()` at all.
**Warning signs:** If the counter doesn't increment during MQTT reconnect storms, the implementation is correct — the pump IS blocked in `connection.next()` during reconnect (this is the condition we want to detect if it lasts > 15s).

### Pitfall 3: Static counter visibility — module-level statics
**What goes wrong:** Counters defined inside a function closure are not accessible from other modules.
**Why it happens:** Closures capture by value or reference; a counter defined inside the thread closure can't be shared.
**How to avoid:** Define counters as `pub static GNSS_RX_HEARTBEAT: AtomicU32 = AtomicU32::new(0);` at module level in `watchdog.rs`. Reference from gnss.rs as `crate::watchdog::GNSS_RX_HEARTBEAT`. This matches the existing pattern for `UART_TX_ERRORS` in `gnss.rs`.
**Warning signs:** Compile error "cannot borrow as mutable" or "value used after move" when trying to share the counter.

### Pitfall 4: esp_restart() requires unsafe
**What goes wrong:** `esp_restart()` is an `unsafe fn` in `esp_idf_svc::sys`. Calling it without `unsafe {}` block causes compile error.
**Why it happens:** FFI functions from C are always `unsafe` in Rust.
**How to avoid:** Wrap in `unsafe { esp_idf_svc::sys::esp_restart(); }`. This is correct and safe in this context — we intentionally want a hard reset.

### Pitfall 5: Supervisor stack size — 4096 is sufficient, don't over-allocate
**What goes wrong:** Allocating 8192 bytes for supervisor stack when it only needs ~1-2KB of actual use (no heap, no large buffers, just loop + u32 arithmetic).
**Why it happens:** Cargo-culting the 8192 pattern from other threads.
**How to avoid:** Use 4096 bytes for the watchdog supervisor. It has no I/O, no string formatting beyond log calls, no UART buffers. HWM will confirm headroom.

---

## Code Examples

### esp_restart usage
```rust
// Source: esp-idf-svc sys bindings (unsafe FFI)
unsafe { esp_idf_svc::sys::esp_restart(); }
// No return value, no error — unconditionally reboots the device.
// Safe to call from any context (FreeRTOS task, interrupt-safe).
```

### AtomicU32 static counter pattern (matches existing UART_TX_ERRORS pattern)
```rust
// Source: existing src/gnss.rs — UART_TX_ERRORS uses this exact pattern
use std::sync::atomic::{AtomicU32, Ordering};

pub static GNSS_RX_HEARTBEAT: AtomicU32 = AtomicU32::new(0);

// In critical thread — top of outer loop:
GNSS_RX_HEARTBEAT.fetch_add(1, Ordering::Relaxed);

// In supervisor — read without consuming:
let current = GNSS_RX_HEARTBEAT.load(Ordering::Relaxed);
if current == last_seen { misses += 1; } else { misses = 0; last_seen = current; }
```

### Config constants to add to config.rs / config.example.rs
```rust
/// Watchdog supervisor check interval.
/// Each critical thread must update its heartbeat counter at least this often.
/// RELAY_RECV_TIMEOUT (5s) on GNSS TX and relay threads guarantees this for the relay path.
/// GNSS RX loop and MQTT pump loop update much more frequently.
pub const WDT_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Number of consecutive missed heartbeat checks before triggering esp_restart().
/// 3 checks × 5s interval = 15s maximum hang before reboot.
/// Hardware TWDT fires at 30s — software WDT catches hangs first.
pub const WDT_MISS_THRESHOLD: u32 = 3;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `esp_task_wdt_add` per-task C API | Software counter + `esp_restart()` | — | Simpler; no unsafe per-thread handle management; correct for Rust std threads |
| ESP-IDF hardware TWDT only (30s) | Software WDT (15s) + hardware TWDT (30s) backstop | Phase 11 | Software WDT catches hangs faster; hardware catches supervisor itself |

**Hardware watchdog context:**
- `CONFIG_ESP_TASK_WDT_EN=y` — Task WDT enabled
- `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` — fires if idle task is not scheduled for 30s
- `CONFIG_ESP_TASK_WDT_PANIC=n` — logs but does NOT panic (does not call `abort()`) without this being set; WDT prints warning and continues
- `CONFIG_BOOTLOADER_WDT_ENABLE=y`, `CONFIG_BOOTLOADER_WDT_TIME_MS=9000` — bootloader WDT 9s

**Important:** `CONFIG_ESP_TASK_WDT_PANIC` is NOT set. This means the hardware TWDT will log a warning but NOT reboot automatically. For WDT-02 success criterion 3 ("hardware watchdog eventually reboots"), this flag may need to be set to `y` in `sdkconfig.defaults`. Without `CONFIG_ESP_TASK_WDT_PANIC=y`, the hardware TWDT only logs — it does not reboot.

**Action required:** Either set `CONFIG_ESP_TASK_WDT_PANIC=y` in `sdkconfig.defaults` (makes TWDT fatal — reboots on supervisor hang), or accept that the hardware TWDT backstop only produces a log message. The success criterion says "eventually reboots" — this requires the panic config.

---

## Open Questions

1. **CONFIG_ESP_TASK_WDT_PANIC — enable or accept soft behavior?**
   - What we know: Current sdkconfig has `# CONFIG_ESP_TASK_WDT_PANIC is not set` — TWDT produces a log but does NOT reboot
   - What's unclear: Whether WDT-02 success criterion 3 ("eventually reboots") requires enabling TWDT panic
   - Recommendation: **Enable `CONFIG_ESP_TASK_WDT_PANIC=y`** in `sdkconfig.defaults`. This is the correct behavior — if the supervisor itself hangs, the hardware must eventually reboot. Without this, criterion 3 is not met. The OTA partition erase risk (why TWDT was extended to 30s) is unaffected — OTA calls its own TWDT reset during erase.

2. **Should the supervisor subscribe to the hardware TWDT (call esp_task_wdt_add)?**
   - What we know: `esp_task_wdt_add(NULL)` subscribes the calling task; the task must then call `esp_task_wdt_reset()` at intervals < TWDT timeout. This requires unsafe FFI.
   - What's unclear: Whether this adds meaningful value given `CONFIG_ESP_TASK_WDT_PANIC=y` + idle task monitoring
   - Recommendation: Do NOT subscribe to TWDT from the supervisor. The hardware TWDT monitors the idle task; if the supervisor hangs and blocks all tasks, idle task won't run and TWDT fires. Keep the implementation simple.

3. **Which threads are "critical" — just GNSS RX and MQTT pump?**
   - What we know: WDT-01 names "GNSS RX thread and MQTT pump thread" explicitly
   - What's unclear: Whether NMEA relay or RTCM relay should also be monitored
   - Recommendation: Phase 11 monitors only what WDT-01 specifies (GNSS RX, MQTT pump). NMEA/RTCM relays can be added in a future phase if needed.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — embedded target (ESP32-C6); no host-side test runner configured |
| Config file | none |
| Quick run command | `cargo build --release 2>&1` |
| Full suite command | `cargo build --release 2>&1` (compile correctness is the primary automated check) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WDT-01 | GNSS RX and MQTT pump each increment a counter ≤ every 5s | manual-only (embedded) | `cargo build --release` verifies compile; runtime verification by log inspection | N/A |
| WDT-02 | Supervisor detects 3 missed beats and calls `esp_restart()` | manual-only (embedded) | `cargo build --release` verifies compile; functional test requires physically hanging a thread | N/A |

**Manual-only justification:** This is an embedded firmware project targeting ESP32-C6 hardware. There is no host-side test framework (no `#[test]`, no `cargo test` configuration, no `tests/` directory). All functional verification occurs via `espflash flash --monitor` and log inspection on physical hardware.

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release`
- **Phase gate:** Successful flash + log observation of "[WDT] supervisor started" and no spurious reboots during 60s nominal operation

### Wave 0 Gaps
None — no test infrastructure to create. Build verification is the automated gate.

---

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection — `src/gnss.rs`, `src/mqtt.rs`, `src/wifi.rs`, `src/main.rs`, `src/config.example.rs`, `sdkconfig.defaults`
- Compiled sdkconfig at `target/.../out/sdkconfig` — hardware WDT configuration verified

### Secondary (MEDIUM confidence)
- ESP-IDF v5 documentation pattern for `esp_restart()` — consistent with `esp_idf_svc::sys` FFI binding observed in existing codebase
- `AtomicU32` pattern — matches existing `UART_TX_ERRORS` static in `src/gnss.rs` (verified by inspection)

### Tertiary (LOW confidence)
- None — all findings derived from direct code and config inspection

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; all stdlib + existing crate features
- Architecture: HIGH — insertion points directly identified in existing source; GNSS RX loop and MQTT pump `while let` are unambiguous
- Pitfalls: HIGH — derived from direct code reading, not speculation
- Hardware WDT behavior: MEDIUM — sdkconfig values confirmed but runtime behavior of `CONFIG_ESP_TASK_WDT_PANIC=n` inferred from ESP-IDF docs pattern

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable domain; dependencies pinned)
