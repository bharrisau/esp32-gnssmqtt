# Phase 12: Resilience - Research

**Researched:** 2026-03-07
**Domain:** Embedded Rust, ESP-IDF, WiFi/MQTT connectivity timeout, autonomous reboot
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| RESIL-01 | `wifi_supervisor` triggers `esp_restart()` if WiFi has not been connected for a configurable duration (default 10 minutes) | `wifi_supervisor` already tracks `consecutive_failures` and already has a comment "Phase 12 (RESIL-01) will call esp_restart() here". The existing structure needs a time-based accumulator (not just a failure count). |
| RESIL-02 | MQTT pump signals a reboot timer; if MQTT stays disconnected for a configurable duration after WiFi is up (default 5 minutes), device restarts | The MQTT `Disconnected` event already sets LED to `Connecting`. An `AtomicInstant` (or duration accumulator) needs to track how long MQTT has been disconnected so a separate monitor (or the MQTT callback path) can trigger `esp_restart()`. |

</phase_requirements>

---

## Summary

Phase 12 adds two independent connectivity-loss reboot timers. Both timers are additive to the existing watchdog infrastructure (Phase 11) and do not modify it. The patterns required are already fully established in this codebase: `esp_restart()` (used in `watchdog.rs`), atomic state sharing via `AtomicU32`/`AtomicU8` (used throughout), and timer-based supervisor loops (used in `wifi_supervisor`).

For RESIL-01: `wifi_supervisor` already runs a polling loop and already has a `consecutive_failures` counter with an explicit placeholder comment for Phase 12. The requirement specifies a time-based threshold (10 minutes), not a failure-count threshold. The simplest correct implementation accumulates total disconnected time (via a `disconnected_since: Option<Instant>` local variable) and calls `esp_restart()` when the elapsed duration exceeds a configurable constant. No new threads are required.

For RESIL-02: The MQTT event callback (in `mqtt_connect`) fires `EventPayload::Disconnected` and `EventPayload::Connected`. The callback currently stores LED state atomically. Adding an `Arc<AtomicBool>` (or an `Arc<AtomicU64>` storing epoch seconds) that the callback writes on disconnect/connect, combined with either a periodic check in a new timer or in the existing `subscriber_loop` timeout arm, provides the 5-minute MQTT disconnection check. The cleanest approach — consistent with the codebase — is a shared `Arc<AtomicU64>` storing the "disconnected since" timestamp, read periodically by `wifi_supervisor` (which already knows WiFi status) or by a new `resil_loop`.

**Primary recommendation:** Add a `WIFI_DISCONNECTED_SINCE: AtomicU64` (boot-time seconds) and a `MQTT_DISCONNECTED_SINCE: AtomicU64` to a new `src/resil.rs` module. `wifi_supervisor` writes these atomics; the MQTT callback writes the MQTT one. A `resil_loop` (or extended `wifi_supervisor`) checks elapsed time and calls `unsafe { esp_idf_svc::sys::esp_restart() }` with a log line before the call.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::time::Instant` | stdlib | Track when disconnection began (monotonic, no epoch needed) | Already used in Phase 9 for timeout tracking; not affected by NTP; monotonic so no wraparound concerns |
| `std::sync::atomic::AtomicU64` | stdlib | Share "disconnected since" epoch across threads | Lock-free; consistent with `AtomicU32` pattern for `UART_TX_ERRORS` and heartbeat counters |
| `esp_idf_svc::sys::esp_restart()` | esp-idf-svc 0.51.0 | Unconditional device reboot | Already used in `watchdog.rs`; unsafe FFI; no args |
| `std::sync::Arc` | stdlib | Share atomic state between callback closure and supervisor loop | Already used for `led_state` (Arc<AtomicU8>); same pattern |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `std::time::Duration` | stdlib | Configurable timeout constants | Already used for `WDT_CHECK_INTERVAL`, `RELAY_RECV_TIMEOUT`, `SLOW_RECV_TIMEOUT` |
| `esp_idf_svc::sys::esp_timer_get_time()` | esp-idf-svc 0.51.0 | Alternative: get monotonic microseconds | Use only if `Instant` is not available in the MQTT callback closure context (closures can use Instant freely in this codebase) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `Instant` stored in Option locally | `AtomicU64` epoch seconds | `AtomicU64` is shareable across threads without `Mutex`; `Option<Instant>` is simpler but only works if the check is in the same thread that observed the disconnect |
| Separate `resil_loop` thread | Extend `wifi_supervisor` | `wifi_supervisor` already checks WiFi state every 5s; adding MQTT check in the same loop avoids an extra thread; but MQTT state must still be communicated via atomic |
| `AtomicU64` for elapsed-ms | `AtomicBool` "is disconnected" + check timestamp in supervisor | Bool approach requires supervisor to independently track when the Bool became true — same complexity, less information. `AtomicU64` storing "seconds since boot at disconnection" is explicit. |

**Installation:** No new dependencies. All components are in `std` or already-present `esp-idf-svc`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── resil.rs        # NEW: WIFI_DISCONNECTED_SINCE, MQTT_DISCONNECTED_SINCE atomics + resil_loop (or inline in wifi_supervisor)
├── wifi.rs         # MODIFY: write WIFI_DISCONNECTED_SINCE on disconnect; call esp_restart() if threshold exceeded
├── mqtt.rs         # MODIFY: write MQTT_DISCONNECTED_SINCE on Disconnected event; clear on Connected event
├── main.rs         # MODIFY: pass Arc to MQTT callback and supervisor; spawn resil_loop if separate thread
└── config.example.rs  # MODIFY: add WIFI_DISCONNECT_REBOOT_SECS, MQTT_DISCONNECT_REBOOT_SECS constants
```

### Pattern 1: Time-Accumulation in wifi_supervisor (RESIL-01)

**What:** `wifi_supervisor` already runs a polling loop with a 5-second sleep. When WiFi is not connected, it enters the reconnect path. A local `Option<Instant>` records when the device first entered disconnected state. On each iteration where WiFi remains disconnected, elapsed time is checked. When elapsed >= `WIFI_DISCONNECT_REBOOT_SECS`, log and call `esp_restart()`.

**When to use:** This is self-contained — the check, state, and action are all inside `wifi_supervisor`. No cross-thread communication needed for RESIL-01.

**Example:**
```rust
// Source: derived from existing wifi_supervisor structure in src/wifi.rs
pub fn wifi_supervisor(mut wifi: BlockingWifi<EspWifi<'static>>, led_state: Arc<AtomicU8>) -> ! {
    // ... HWM log, backoff_secs, consecutive_failures init ...
    let mut disconnected_since: Option<std::time::Instant> = None;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let connected = wifi.is_connected().unwrap_or(false);

        if !connected {
            let since = disconnected_since.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() >= crate::config::WIFI_DISCONNECT_REBOOT_TIMEOUT {
                log::error!(
                    "[RESIL] WiFi disconnected for {:?} — rebooting",
                    since.elapsed()
                );
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
            // ... existing reconnect logic ...
        } else {
            disconnected_since = None;  // Reset on reconnect
            // ... existing success reset ...
        }
    }
}
```

### Pattern 2: Shared AtomicU64 for MQTT Disconnect Time (RESIL-02)

**What:** An `AtomicU64` stores the "seconds since device boot when MQTT disconnected" (or 0 = not currently disconnected). The MQTT callback writes this value on `EventPayload::Disconnected` (set to current time) and clears it on `EventPayload::Connected` (set to 0). A supervisor loop reads it and checks elapsed time.

**When to use:** The MQTT callback is a closure running on the ESP-IDF C MQTT task thread. It cannot hold `Option<Instant>` across calls (different invocations). An `Arc<AtomicU64>` shared with main thread is the correct pattern — matches existing `led_state` (Arc<AtomicU8>).

**Getting elapsed seconds from an AtomicU64 timestamp:**
```rust
// Source: derived from existing AtomicU32 patterns in watchdog.rs and gnss.rs
use std::sync::atomic::{AtomicU64, Ordering};

// In resil.rs or a suitable module:
pub static MQTT_DISCONNECTED_AT: AtomicU64 = AtomicU64::new(0);
// Convention: 0 = MQTT is currently connected (or not yet disconnected)
// Non-zero = seconds-since-boot value when MQTT disconnected

// In mqtt_connect callback (EventPayload::Disconnected arm):
let now_secs = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(1);  // fallback: 1 (non-zero) signals "disconnected"
MQTT_DISCONNECTED_AT.compare_exchange(0, now_secs, Ordering::Relaxed, Ordering::Relaxed).ok();
// compare_exchange: only set if currently 0 (not already tracking a disconnect)

// In mqtt_connect callback (EventPayload::Connected arm):
MQTT_DISCONNECTED_AT.store(0, Ordering::Relaxed);  // Clear on reconnect

// In supervisor check (runs every 5s, only when WiFi is connected):
let disconnected_at = MQTT_DISCONNECTED_AT.load(Ordering::Relaxed);
if disconnected_at != 0 {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let elapsed = now_secs.saturating_sub(disconnected_at);
    if elapsed >= crate::config::MQTT_DISCONNECT_REBOOT_SECS {
        log::error!("[RESIL] MQTT disconnected for {}s (WiFi up) — rebooting", elapsed);
        unsafe { esp_idf_svc::sys::esp_restart(); }
    }
}
```

**Alternative using Instant (simpler, if check is in wifi_supervisor which already has WiFi state):**

A second `Option<Instant>` local to `wifi_supervisor` (or a new `resil_loop`) can be set when an MQTT-disconnected signal is received. The cleanest approach: pass an `Arc<AtomicU64>` from `main.rs` to both the MQTT callback and the WiFi supervisor. The MQTT callback writes it; the WiFi supervisor reads it only when `connected == true`.

### Pattern 3: Log-then-restart discipline

**What:** Both RESIL-01 and RESIL-02 success criteria require "the reboot is logged before it occurs". The log call must precede `esp_restart()`.

**When to use:** Always — for all reboot triggers in this codebase.

**Example:**
```rust
// Source: matches existing watchdog.rs pattern
log::error!("[RESIL-01] WiFi disconnected for {}s — rebooting",
    since.elapsed().as_secs());
unsafe { esp_idf_svc::sys::esp_restart(); }
```

Note: `log::error!` uses the ESP-IDF UART logger. The logger flushes synchronously on `error!` level — the log line is guaranteed to appear before the restart. `log::info!` may be buffered; using `log::error!` ensures the message is transmitted before `esp_restart()` clears the hardware.

### Architecture Decision: Extend wifi_supervisor vs. New resil_loop Thread

**Option A — Extend wifi_supervisor (recommended):**
- RESIL-01 is entirely internal to `wifi_supervisor` (local `Option<Instant>`)
- RESIL-02 uses a shared `Arc<AtomicU64>` (or static); `wifi_supervisor` reads it and checks only when WiFi is connected
- No new thread; no new stack allocation; all logic in one place

**Option B — Separate resil_loop thread:**
- Spawned in `main.rs` after MQTT connect
- Receives `Arc<AtomicU64>` and a `Arc<BlockingWifi>` (or WiFi state atomic)
- Cleaner separation, but WiFi state sharing is more complex (BlockingWifi is not `Clone`; would need a separate AtomicBool for "wifi is connected" signal)

**Recommendation: Option A.** The WiFi supervisor already polls WiFi state every 5s. Adding a MQTT timeout check in the same loop (conditioned on `connected == true`) is minimal code with no new thread overhead.

### Anti-Patterns to Avoid

- **Reboot without log:** `esp_restart()` must always be preceded by `log::error!(...)`. The success criteria explicitly require the reboot to be logged first.
- **Resetting MQTT disconnect timer on WiFi reconnect:** The 5-minute MQTT timer starts only after WiFi is up. If WiFi drops and reconnects, the MQTT disconnect timer should reset too — otherwise a 4-minute pre-WiFi-outage MQTT disconnect could combine with 1 minute of MQTT-reconnect-after-WiFi-recovery to trigger a false RESIL-02 reboot.
- **Using `log::info!` before esp_restart():** ESP-IDF's `log::info!` may be buffered in some configurations. Use `log::error!` for pre-reboot messages — it is synchronously flushed at the hardware level.
- **Checking MQTT timeout when WiFi is down:** RESIL-02 specifies "WiFi is connected but MQTT remains disconnected". When WiFi is down, MQTT will also be disconnected — this is expected and should NOT count toward the 5-minute MQTT timer.
- **Not clearing MQTT disconnect timer on WiFi reconnect:** When WiFi reconnects, the MQTT stack will also attempt reconnect automatically (configured via `reconnect_timeout: Some(5s)`). The MQTT disconnect timer must be cleared on WiFi disconnect so it restarts fresh once WiFi is back.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Device reboot | Custom reset sequence | `unsafe { esp_idf_svc::sys::esp_restart() }` | ESP-IDF canonical reboot; clean peripheral teardown; bootloader handoff |
| Elapsed time measurement | Manual tick counting | `std::time::Instant::elapsed()` or `AtomicU64` timestamp comparison | Monotonic, no overflow, no NTP dependency |
| Cross-thread time notification | `Mutex<Option<Instant>>` | `AtomicU64` (0 = no disconnect, non-zero = timestamp) | Lock-free; matches established atomic pattern in this codebase |

**Key insight:** Both timeouts are simple elapsed-time checks. There is no need for a separate timer task, FreeRTOS software timers, or esp_timer callbacks. Periodic polling every 5 seconds (already happening in `wifi_supervisor`) provides 5-second granularity, which is fine for 10-minute and 5-minute thresholds.

---

## Common Pitfalls

### Pitfall 1: Counting failures instead of measuring time
**What goes wrong:** RESIL-01 uses the existing `consecutive_failures` counter and triggers reboot at `MAX_WIFI_RECONNECT_ATTEMPTS`. With exponential backoff capped at 60s, 20 failures ≠ 10 minutes (it is ~600s with 60s backoff, but only ~30s with 1s backoff at the start). The requirement specifies a time duration, not a failure count.
**Why it happens:** `consecutive_failures` already exists in `wifi_supervisor`; it's tempting to reuse it.
**How to avoid:** Use `Option<Instant>` (started at first disconnect) and compare `elapsed()` to `WIFI_DISCONNECT_REBOOT_TIMEOUT`. The failure counter can remain for LED state logic.
**Warning signs:** Device reboots too quickly (after 3 failures × 1s backoff = 3s) or too slowly (never reaches the count threshold if backoff is variable).

### Pitfall 2: MQTT disconnect timer running during WiFi outage
**What goes wrong:** MQTT will be disconnected whenever WiFi is down. If the 5-minute MQTT timer is not gated on "WiFi is up", a 6-minute total outage (5 min WiFi down, 1 min WiFi up but MQTT reconnecting) could trigger RESIL-02 — even though the system is behaving correctly.
**Why it happens:** The MQTT `Disconnected` event fires on WiFi loss, and the timer would start then.
**How to avoid:** In the MQTT reboot check, only evaluate elapsed time when `wifi.is_connected()` returns true. Clear or pause the MQTT disconnect timer whenever WiFi transitions to disconnected.
**Warning signs:** Device reboots shortly after WiFi outage ends, before MQTT has had time to reconnect.

### Pitfall 3: MQTT callback cannot call EspMqttClient methods (re-entrant deadlock)
**What goes wrong:** If the MQTT callback tries to call `client.publish()` or any `EspMqttClient` method to signal a disconnect, it deadlocks — the C MQTT task holds its internal mutex during event dispatch.
**Why it happens:** EspMqttClient with `new_cb` dispatches events directly on the C MQTT task thread while holding the internal mutex. Re-entrant calls from within the callback will deadlock.
**How to avoid:** The callback MUST only use `AtomicU64::store`, `SyncSender::try_send`, and similar non-blocking, non-MQTT operations. Writing to a static atomic is safe. This is explicitly documented in the existing `mqtt_connect` comment: "The callback MUST NOT call any EspMqttClient methods."
**Warning signs:** Device hangs immediately on first MQTT disconnection event.

### Pitfall 4: esp_restart() placement relative to log flush
**What goes wrong:** Using `log::debug!` or `log::info!` before `esp_restart()` may result in the message not appearing in the monitor output because the logging buffer wasn't flushed before the restart.
**Why it happens:** ESP-IDF's default logging over USB-JTAG may buffer lower-priority log messages. `esp_restart()` clears hardware state before the buffer is transmitted.
**How to avoid:** Always use `log::error!` immediately before `esp_restart()`. The `error!` level is synchronously flushed to the console. This matches the pattern already used in `watchdog.rs`.
**Warning signs:** Post-reboot log shows the bootloader startup message without the preceding reboot-reason log line.

### Pitfall 5: SystemTime vs. Instant for cross-closure timestamp
**What goes wrong:** `std::time::Instant` cannot be sent across thread boundaries as a raw value — it is an opaque type. Storing it in an `AtomicU64` requires conversion to a numeric form (seconds or microseconds).
**Why it happens:** `Instant` is an opaque OS primitive; its internal representation is not exposed.
**How to avoid:** Use `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()` to get a `u64` seconds value storable in `AtomicU64`. For purely same-thread use (e.g., local `Option<Instant>` in `wifi_supervisor`), `Instant` is fine.
**Warning signs:** Compiler error "trait `Send` is not implemented" or "cannot convert Instant to u64".

---

## Code Examples

### RESIL-01: wifi_supervisor with disconnect timeout
```rust
// Source: extension of existing src/wifi.rs wifi_supervisor
pub fn wifi_supervisor(mut wifi: BlockingWifi<EspWifi<'static>>, led_state: Arc<AtomicU8>) -> ! {
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "WiFi sup", hwm_words, hwm_words * 4);
    let mut backoff_secs: u64 = 1;
    let mut consecutive_failures: u32 = 0;
    let mut disconnected_since: Option<std::time::Instant> = None;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let connected = wifi.is_connected().unwrap_or(false);

        if !connected {
            let since = disconnected_since.get_or_insert_with(std::time::Instant::now);
            let elapsed = since.elapsed();
            if elapsed >= crate::config::WIFI_DISCONNECT_REBOOT_TIMEOUT {
                log::error!("[RESIL-01] WiFi disconnected for {}s — rebooting",
                    elapsed.as_secs());
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
            // ... existing reconnect backoff logic ...
        } else {
            disconnected_since = None;
            backoff_secs = 1;
            consecutive_failures = 0;
        }
    }
}
```

### RESIL-02: AtomicU64 timestamp for MQTT disconnect tracking
```rust
// Source: derived from AtomicU32 pattern in src/watchdog.rs and src/gnss.rs

// In resil.rs (or at module level of mqtt.rs / wifi.rs):
use std::sync::atomic::{AtomicU64, Ordering};

/// Stores the Unix epoch second when MQTT last disconnected.
/// 0 = MQTT is currently connected (or never disconnected).
/// Set by MQTT callback on Disconnected; cleared on Connected.
pub static MQTT_DISCONNECTED_AT: AtomicU64 = AtomicU64::new(0);

// Helper to get current epoch seconds:
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1) // fallback: 1 ensures it's treated as "disconnected"
}

// In MQTT callback (Disconnected arm):
EventPayload::Disconnected => {
    log::warn!("MQTT disconnected");
    led_state.store(LedState::Connecting as u8, Ordering::Relaxed);
    // Record disconnect time only if not already recording (compare_exchange: only write if 0)
    crate::resil::MQTT_DISCONNECTED_AT
        .compare_exchange(0, crate::resil::now_secs(), Ordering::Relaxed, Ordering::Relaxed)
        .ok();
}

// In MQTT callback (Connected arm):
EventPayload::Connected(_) => {
    log::info!("MQTT connected");
    led_state.store(LedState::Connected as u8, Ordering::Relaxed);
    crate::resil::MQTT_DISCONNECTED_AT.store(0, Ordering::Relaxed);
    // ... subscribe_tx.try_send(()) ...
}

// In wifi_supervisor loop (only when WiFi is connected):
if connected {
    disconnected_since = None;  // clear RESIL-01 timer
    let mqtt_disc_at = crate::resil::MQTT_DISCONNECTED_AT.load(Ordering::Relaxed);
    if mqtt_disc_at != 0 {
        let elapsed = now_secs().saturating_sub(mqtt_disc_at);
        if elapsed >= crate::config::MQTT_DISCONNECT_REBOOT_SECS {
            log::error!("[RESIL-02] MQTT disconnected for {}s (WiFi up) — rebooting", elapsed);
            unsafe { esp_idf_svc::sys::esp_restart(); }
        }
    }
    // ... existing success reset ...
}
```

### Config constants (config.example.rs additions)
```rust
// Source: follows existing WDT_CHECK_INTERVAL / WDT_MISS_THRESHOLD pattern

/// Reboot if WiFi has not been connected for this duration.
/// RESIL-01: default 10 minutes. Configurable via this constant.
pub const WIFI_DISCONNECT_REBOOT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(10 * 60);

/// Reboot if MQTT has not been connected for this duration while WiFi is up.
/// RESIL-02: default 5 minutes. Configurable via this constant.
pub const MQTT_DISCONNECT_REBOOT_SECS: u64 = 5 * 60;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `consecutive_failures` count for reboot trigger (placeholder) | `Option<Instant>` elapsed time for WiFi reboot | Phase 12 | Duration-based threshold matches the requirement (10 min) regardless of backoff timing |
| No MQTT disconnect reboot | `AtomicU64` timestamp + periodic check | Phase 12 | Prevents device stuck in "WiFi up, MQTT down" limbo indefinitely |
| `MAX_WIFI_RECONNECT_ATTEMPTS` comment "Phase 12 will call esp_restart() here" | Proper time-based restart | Phase 12 | The count-based guard can remain for LED state; time-based adds the actual recovery |

**Existing infrastructure confirmed complete:**
- `unsafe { esp_idf_svc::sys::esp_restart() }` — already used in `watchdog.rs`, pattern established
- `CONFIG_ESP_TASK_WDT_PANIC=y` — already set (Phase 11); hardware backstop in place
- MQTT callback `new_cb` approach — Phase 11 summary confirms this is the live implementation; callback dispatches atomics safely

---

## Open Questions

1. **Where to place MQTT_DISCONNECTED_AT static — resil.rs or mqtt.rs?**
   - What we know: `UART_TX_ERRORS` lives in `gnss.rs` (the module that increments it); `GNSS_RX_HEARTBEAT` lives in `watchdog.rs` (the supervisor module). Both patterns exist.
   - What's unclear: Whether a new `resil.rs` module is worth creating or whether adding to `wifi.rs` is sufficient.
   - Recommendation: Create `src/resil.rs` with the static and helper. This matches the `watchdog.rs` pattern (separate module for resilience infrastructure), keeps `wifi.rs` and `mqtt.rs` clean, and makes Phase 13 health telemetry easier (single import for all resilience counters).

2. **Should RESIL-02 reset the MQTT disconnect timer when WiFi drops?**
   - What we know: The requirement says "WiFi is connected but MQTT remains disconnected". If WiFi drops, MQTT will also disconnect — that should not count.
   - What's unclear: Whether the timer should be paused (stopped but not reset) or fully reset on WiFi reconnect.
   - Recommendation: **Fully reset** (store 0) the `MQTT_DISCONNECTED_AT` when WiFi disconnects. After WiFi reconnects, MQTT will attempt to reconnect (5s timeout configured). The 5-minute timer starts fresh from the moment WiFi is back up. This prevents the combined-outage false-trigger described in Pitfall 2.

3. **Is `MAX_WIFI_RECONNECT_ATTEMPTS` still needed after RESIL-01?**
   - What we know: The constant currently drives LED error state after 20 failures. With RESIL-01, the device will reboot well before reaching 20 failures at full 60s backoff (10 min / 60s = ~10 failures).
   - What's unclear: Whether the constant should be kept as a LED-error threshold or removed.
   - Recommendation: Keep `MAX_WIFI_RECONNECT_ATTEMPTS` for LED state signaling (shows red error LED before the reboot threshold). No code change needed; the reboot check in `wifi_supervisor` is independent of the failure counter.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — embedded target (ESP32-C6); no host-side test runner configured |
| Config file | none |
| Quick run command | `cargo build --release 2>&1` |
| Full suite command | `cargo build --release 2>&1` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| RESIL-01 | `wifi_supervisor` calls `esp_restart()` after 10-min WiFi outage; log precedes restart | manual-only (embedded) | `cargo build --release` verifies compile; runtime: observe log + reboot in flash monitor | N/A |
| RESIL-02 | MQTT disconnect timer triggers `esp_restart()` after 5 min with WiFi up; log precedes restart | manual-only (embedded) | `cargo build --release` verifies compile; runtime: block MQTT broker port and observe | N/A |

**Manual-only justification:** Embedded firmware targeting ESP32-C6 hardware. No host-side test framework. Functional verification requires `espflash flash --monitor` and deliberate connectivity disruption (block WiFi AP, block MQTT port, or use very short timeout constants during dev testing).

**Dev testing tip:** Set `WIFI_DISCONNECT_REBOOT_TIMEOUT` to `Duration::from_secs(30)` and `MQTT_DISCONNECT_REBOOT_SECS` to `30` for rapid verification. Restore to 600s / 300s before final commit.

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release`
- **Phase gate:** Successful flash + log shows "[RESIL-01]" or "[RESIL-02]" reboot reason + device reconnects normally after restart

### Wave 0 Gaps
None — no test infrastructure to create. Build verification is the automated gate.

---

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection — `src/wifi.rs` (wifi_supervisor), `src/mqtt.rs` (callback pattern, EventPayload), `src/watchdog.rs` (esp_restart pattern, AtomicU32 static), `src/config.example.rs` (constant patterns), `src/main.rs` (spawn order)
- Phase 11 SUMMARY.md — confirms `new_cb` is the live MQTT implementation (not a blocking pump), confirms `CONFIG_ESP_TASK_WDT_PANIC=y` is set
- `sdkconfig.defaults` — confirmed `CONFIG_ESP_TASK_WDT_PANIC=y` present

### Secondary (MEDIUM confidence)
- ESP-IDF `esp_restart()` behavior: synchronous, clears all hardware state, triggers bootloader — consistent with observed Phase 11 usage in watchdog.rs
- `log::error!` flush guarantee before `esp_restart()`: consistent with ESP-IDF documentation pattern for pre-reset logging

### Tertiary (LOW confidence)
- None — all findings derived from direct code inspection

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; all stdlib + already-present crate features; patterns exist verbatim in codebase
- Architecture: HIGH — exact insertion points identified (wifi_supervisor local var, MQTT callback arms); matches established AtomicU32/Arc patterns
- Pitfalls: HIGH — derived from direct code reading and Phase 11 documentation, not speculation
- MQTT callback thread-safety: HIGH — existing comment in mqtt_connect explicitly documents the re-entrancy constraint; confirmed by Phase 11 design decision

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable domain; dependencies pinned; no fast-moving ecosystem)
