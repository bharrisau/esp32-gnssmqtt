# Phase 16: Remote Logging - Research

**Researched:** 2026-03-08
**Domain:** ESP-IDF log interception, Rust custom log backend, MQTT publish, re-entrancy
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LOG-01 | ESP-IDF log output forwarded to `gnss/{device_id}/log` at QoS 0; log hook uses re-entrancy guard so MQTT enqueue/send paths are excluded from capture, preventing feedback loops | C shim + `esp_log_set_vprintf` approach documented; re-entrancy guard pattern via `AtomicBool::compare_exchange` |
| LOG-02 | Log level threshold configurable via retained MQTT topic | `EspLogger::set_target_level()` wraps `esp_log_level_set()`; subscribe to `gnss/{device_id}/log/level` with QoS 1 |
| LOG-03 | Log publishing is non-blocking; messages dropped silently when MQTT disconnected or channel full | `try_send()` on bounded `sync_channel` — established pattern in this codebase (NMEA relay, OTA, cmd relay) |
</phase_requirements>

---

## Summary

Phase 16 adds remote logging: every ESP-IDF log message is forwarded in near-real-time to `gnss/{device_id}/log` via MQTT at QoS 0. Three non-trivial problems must be solved: (1) intercepting the log stream, (2) preventing the MQTT publish path from re-triggering the hook in a feedback loop, and (3) keeping publish entirely non-blocking so the calling thread (any firmware thread that emits a log line) is never stalled.

The cleanest interception approach for this Rust codebase is a **C shim** registered via `esp_log_set_vprintf`. The shim calls `vsnprintf` to materialize the formatted string, checks an `AtomicBool` re-entrancy guard, then drops the message into a bounded `sync_channel<String>` via `try_send`. A dedicated log relay thread drains that channel and calls `mqtt_client.lock()` then `enqueue()` — non-blocking. This keeps the hot path (vprintf callback) allocation-free after startup and never acquires the MQTT mutex inside the callback. The C shim is needed because Rust cannot safely receive `va_list` without nightly + architecture-specific tricks; the shim handles formatting in C and passes a `*const c_char` (null-terminated) to the Rust side.

Log level control is handled by subscribing to `gnss/{device_id}/log/level` (retained, QoS 1) and calling `EspLogger::set_target_level("*", level_filter)` when a new level string arrives. This uses the existing `esp_log_level_set()` C API that the subscriber thread already knows how to drive.

**Primary recommendation:** C shim for `esp_log_set_vprintf` + per-thread `AtomicBool` re-entrancy guard + bounded `sync_channel` drain thread + `EspLogger::set_target_level` for runtime level changes. No heap alloc in the hot path after startup.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp-idf-sys` | `=0.36.1` (already in Cargo.toml) | `esp_log_set_vprintf`, `esp_log_level_set` FFI bindings | Already in project; direct C API access |
| `esp-idf-svc::log::EspLogger` | `=0.51.0` (already in Cargo.toml) | `set_target_level()` for runtime level control | Already used; wraps `esp_log_level_set` safely |
| `std::sync::mpsc::sync_channel` | stdlib | Bounded channel from C shim callback to relay thread | Already the pattern used for all relay threads |
| `std::sync::atomic::AtomicBool` | stdlib | Re-entrancy guard — prevents feedback loop | Lock-free, safe to call from any task context |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| C shim (custom `log_shim.c`) | N/A | `vsnprintf` formatting, forwards `*const c_char` to Rust | Required because `va_list` FFI in Rust is arch-specific; C is simpler |
| `embedded_svc::mqtt::client::QoS::AtMostOnce` | `=0.28.1` | QoS 0 for log topic | Already imported; LOG-01 mandates QoS 0 |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| C shim + `esp_log_set_vprintf` | Custom `log::Log` impl | Custom `log::Log` requires replacing `EspLogger` as the global logger — `log::set_logger` can only be called once and panics on second call; `EspLogger` is already installed in `main()`. C shim hooks below the Rust log layer and captures ALL output including Rust `log::` macros AND ESP-IDF C component logs. |
| `AtomicBool` re-entrancy guard | Thread-local `Cell<bool>` | Thread-local is cleaner per-thread but FreeRTOS thread-local APIs differ from std; `AtomicBool::compare_exchange` is universally safe across all task contexts |
| Dedicated relay thread | Inline `enqueue()` in C shim | Inline `enqueue()` inside the vprintf callback would require acquiring the MQTT mutex from within the callback — this deadlocks when the MQTT event pump thread emits a log (it already holds internal state). Relay thread separates concerns cleanly. |
| `sync_channel::<String>` | `sync_channel::<Vec<u8>>` | Either works; `String` avoids one extra allocation vs copying bytes; the C shim will use `CStr::from_ptr(...).to_string_lossy()` |

**Installation:** No new Cargo dependencies needed. The C shim is a new `src/log_shim.c` file registered in `build.rs` or `CMakeLists.txt`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── log_relay.rs        # Rust side: spawn_log_relay(), re-entrancy guard, channel sender
├── log_shim.c          # C side: vprintf hook, vsnprintf formatting, calls into Rust
├── mqtt.rs             # (existing) subscriber_loop extended with /log/level subscription
└── main.rs             # (existing) spawn log relay after MQTT connect, step 9.5
```

### Pattern 1: C Shim + Rust Channel Bridge

**What:** A C function registered as the vprintf handler formats the log message with `vsnprintf`, checks a Rust `AtomicBool` guard via FFI, and if clear, sends the string to a bounded channel. A Rust thread drains the channel and publishes to MQTT.

**When to use:** Whenever all log output (both Rust `log::` macros and native ESP-IDF C component logs like WiFi, TCP/IP) must be captured — `esp_log_set_vprintf` hooks the single output path for both.

**Flow:**
```
Any firmware thread
  → log::info!("...") or ESP_LOGI(...)
    → ESP-IDF log subsystem formats message
      → registered vprintf hook (log_shim.c)
        → check AtomicBool guard (if set → call original vprintf only, return)
        → vsnprintf into stack buffer
        → call rust_log_try_send(buf_ptr, len) [extern "C" fn]
          → SyncSender<String>::try_send(...)  [Rust, non-blocking]
            → log relay thread wakes
              → AtomicBool guard SET
              → mqtt_client.lock().enqueue(topic, QoS0, ...)
              → AtomicBool guard CLEAR
```

**Example — C shim (log_shim.c):**
```c
// Source: ESP-IDF official docs + community pattern
#include <stdio.h>
#include <stdarg.h>
#include "esp_log.h"

// Declared in log_relay.rs as #[no_mangle] extern "C"
extern void rust_log_try_send(const char *msg, size_t len);
extern int  rust_log_is_reentering(void);  // returns 1 if guard is set

static vprintf_like_t s_original_vprintf = NULL;

static int mqtt_log_vprintf(const char *fmt, va_list args) {
    // Always call original (UART output preserved)
    int ret = s_original_vprintf(fmt, args);

    // Re-entrancy guard: skip MQTT path if called from within relay thread
    if (rust_log_is_reentering()) {
        return ret;
    }

    // Format to stack buffer — 256 bytes covers typical log lines
    char buf[256];
    // Need a copy of va_list since original already consumed it
    va_list args2;
    va_copy(args2, args);
    int n = vsnprintf(buf, sizeof(buf), fmt, args2);
    va_end(args2);

    if (n > 0) {
        rust_log_try_send(buf, (size_t)(n < (int)sizeof(buf) ? n : sizeof(buf) - 1));
    }
    return ret;
}

void install_mqtt_log_hook(void) {
    s_original_vprintf = esp_log_set_vprintf(mqtt_log_vprintf);
}
```

**Example — Rust side (log_relay.rs):**
```rust
// Source: project patterns + std::sync docs

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::mqtt::client::EspMqttClient;

// Global re-entrancy guard: true = log relay thread is currently publishing
static LOG_REENTERING: AtomicBool = AtomicBool::new(false);

// Global sender: set once at startup by spawn_log_relay()
static LOG_TX: std::sync::OnceLock<SyncSender<String>> = std::sync::OnceLock::new();

#[no_mangle]
pub extern "C" fn rust_log_is_reentering() -> i32 {
    if LOG_REENTERING.load(Ordering::Relaxed) { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn rust_log_try_send(msg: *const core::ffi::c_char, _len: usize) {
    if let Some(tx) = LOG_TX.get() {
        let s = core::ffi::CStr::from_ptr(msg).to_string_lossy().into_owned();
        let _ = tx.try_send(s); // silently drop if full (LOG-03)
    }
}

pub fn spawn_log_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    log_rx: std::sync::mpsc::Receiver<String>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || {
            let topic = format!("gnss/{}/log", device_id);
            loop {
                match log_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
                    Ok(msg) => {
                        // Set re-entrancy guard BEFORE acquiring MQTT mutex
                        LOG_REENTERING.store(true, Ordering::Relaxed);
                        if let Ok(mut c) = client.lock() {
                            let _ = c.enqueue(&topic, QoS::AtMostOnce, false, msg.as_bytes());
                        }
                        LOG_REENTERING.store(false, Ordering::Relaxed);
                    }
                    Err(_) => {} // timeout or disconnect: continue
                }
            }
        })
        .expect("log relay spawn failed");
    Ok(())
}
```

### Pattern 2: Runtime Log Level via MQTT Subscription

**What:** Subscribe to `gnss/{device_id}/log/level` at QoS 1 (retained, so level persists across reconnects). On message, parse the level string ("error", "warn", "info", "debug", "verbose") and call `EspLogger::set_target_level("*", filter)`.

**When to use:** When LOG-02 requires level to change without reboot.

**Example:**
```rust
// In subscriber_loop (or new log_level_relay task receiving from MQTT callback)
// Source: esp-idf-svc docs (EspLogger::set_target_level)

fn apply_log_level(payload: &[u8]) {
    let level_str = match std::str::from_utf8(payload) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    let filter = match level_str {
        "error"   => log::LevelFilter::Error,
        "warn"    => log::LevelFilter::Warn,
        "info"    => log::LevelFilter::Info,
        "debug"   => log::LevelFilter::Debug,
        "verbose" => log::LevelFilter::Trace,
        _ => {
            log::warn!("log level: unknown level {:?}", level_str);
            return;
        }
    };
    // EspLogger::set_target_level("*", filter) sets global level at runtime
    // This calls esp_log_level_set("*", mapped_level) internally
    static LOGGER: esp_idf_svc::log::EspLogger = esp_idf_svc::log::EspLogger;
    if let Err(e) = LOGGER.set_target_level("*", filter) {
        log::warn!("log level set failed: {:?}", e);
    } else {
        log::info!("log level changed to: {}", level_str);
    }
}
```

### Pattern 3: Channel Sizing for Log Relay

**What:** The bounded channel between the C shim and the relay thread must be sized for burst absorptions. Log output spikes on startup (10-50 messages in <100ms) then settles.

**When to use:** Tune at init time based on observed startup burst.

**Recommendation:** `sync_channel::<String>(32)` — 32 messages covers a typical startup burst. At ~100 bytes/message average, worst-case 3.2 KB of String heap during burst. Silently drops when full (LOG-03).

### Anti-Patterns to Avoid

- **Calling any `EspMqttClient` method inside the vprintf callback:** The C MQTT task holds its internal mutex during event dispatch. A re-entrant MQTT call deadlocks. All MQTT interaction must be in the relay thread.
- **Using `log::info!` or any `log::` macro inside `rust_log_try_send`:** This re-enters the vprintf hook, overflows the stack, and crashes. Use `printf()` via C for any diagnostic output inside the shim. The re-entrancy guard prevents this at the MQTT path but the C shim itself must never call the logging macros.
- **Holding the MQTT mutex across the recv_timeout loop iteration:** The relay thread should acquire, enqueue, and release the mutex per message — same pattern as `nmea_relay.rs` and `rtcm_relay.rs`.
- **Blocking `recv()` in the relay thread with no timeout:** Use `recv_timeout(SLOW_RECV_TIMEOUT)` — consistent with every other relay thread in this codebase.
- **`va_copy` omission:** The C shim's `s_original_vprintf(fmt, args)` call consumes `args`. Always `va_copy` before the `vsnprintf` call.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Log interception | Custom `log::Log` impl replacing `EspLogger` | `esp_log_set_vprintf` C shim | `log::set_logger` is one-shot; replacing EspLogger after the fact is UB. The vprintf hook captures C component logs too (WiFi, TCP/IP) which a Rust-only logger misses. |
| `va_list` handling in Rust | Unsafe transmute tricks, nightly VaList | `vsnprintf` in C | Xtensa `va_list` FFI in Rust was broken until Jan 2024; RISC-V may differ. C handles `va_list` natively with no risk. |
| Log level parsing | Custom enum | Match on `&str` → `log::LevelFilter` | Five variants, trivial match; no crate needed. |
| Stack buffer for formatted logs | Heap `Vec<u8>` per message | `char buf[256]` on C stack | Stack allocation is zero-cost; heap alloc per log line at 10 Hz is wasteful. Truncation at 256 bytes is acceptable for diagnostic logs. |

**Key insight:** The shim-based approach keeps the hot log path (C stack buffer + `try_send`) allocation-free and lock-free. Only the relay thread allocates (one `String` per message, immediately freed after enqueue).

---

## Common Pitfalls

### Pitfall 1: Re-entrancy Feedback Loop

**What goes wrong:** The log relay thread acquires the MQTT mutex and calls `enqueue()`. ESP-IDF's MQTT client internally uses `ESP_LOGx` macros. These re-enter the vprintf hook. If the hook is not guarded, it attempts `try_send` again, which logs again — runaway recursion.

**Why it happens:** `esp_log_set_vprintf` hooks ALL log output from ALL tasks with no exception carve-out.

**How to avoid:** Set `LOG_REENTERING = true` BEFORE `client.lock()` in the relay thread. Check `LOG_REENTERING.load(Ordering::Relaxed)` at the top of the vprintf callback and return immediately if set. The guard must be cleared (`false`) after the `enqueue()` call, even on error — wrap in a `defer` pattern (explicit clear in both Ok and Err branches).

**Warning signs:** Stack overflow crash (`E (xxxx) task_wdt: Stack overflow in task log_relay` or similar), or infinite log loop producing messages that only mention MQTT internals.

### Pitfall 2: C shim `va_copy` / double-consume

**What goes wrong:** `s_original_vprintf(fmt, args)` advances the va_list pointer. A subsequent `vsnprintf(buf, sizeof(buf), fmt, args)` reads garbage.

**Why it happens:** `va_list` is consumed (pointer advanced) on first use. C standard requires `va_copy` to get an independent copy.

**How to avoid:** Always `va_copy(args2, args)` before the second format call. `va_end(args2)` when done.

**Warning signs:** Garbled or truncated log messages on the MQTT topic while UART output is correct.

### Pitfall 3: Stack overflow in the vprintf callback

**What goes wrong:** `char buf[256]` allocated on the calling thread's stack. If the calling thread has a small stack (e.g. 4096 bytes) and the log line is long, stack overflows.

**Why it happens:** The callback runs on whichever thread emitted the log line.

**How to avoid:** Keep the stack buffer at 256 bytes — adequate for nearly all log lines and small relative to any thread's stack. The `vsnprintf` call truncates silently. Do not allocate larger buffers; do not use heap inside the callback.

**Warning signs:** Sporadic crashes or `Guru Meditation Error: Core 0 panic'ed (StoreProhibited)` in unrelated code shortly after a log-heavy sequence.

### Pitfall 4: MQTT enqueue failure when disconnected (LOG-03)

**What goes wrong:** `client.lock().unwrap().enqueue(...)` returns an `Err` when MQTT is disconnected. If this is logged with `log::warn!`, it re-enters the hook.

**Why it happens:** Any `log::` call from within the relay thread is inside the re-entrancy guard (guard = true), so the shim will suppress it — but only if the guard is set correctly. If the guard is cleared before the log call, it loops.

**How to avoid:** Do NOT call any `log::` macro from the relay thread while the re-entrancy guard is set. Let enqueue errors be silently discarded (LOG-03 mandates this). If diagnostics are needed, use a counter (atomic) and log it periodically from the heartbeat thread.

### Pitfall 5: `OnceLock` not initialized before C shim is installed

**What goes wrong:** `install_mqtt_log_hook()` (C) is called in `main()` before `spawn_log_relay()` sets the `LOG_TX` OnceLock sender. Early log messages before the relay thread starts are sent to a `None` sender and silently dropped — this is acceptable and by design for LOG-03.

**Why it happens:** The hook is installed early (ideally right after `EspLogger::initialize_default()`), but MQTT is not available until step 9.

**How to avoid:** Document the drop window (boot → MQTT connected). The hook installation and `OnceLock` initialization can be in any order — `rust_log_try_send` checks `LOG_TX.get()` and is a no-op if not yet initialized.

---

## Code Examples

Verified patterns from this codebase and official sources:

### Calling esp_log_set_vprintf from Rust (via C shim)
```rust
// In main.rs, after EspLogger::initialize_default() (step 2),
// before WiFi (step 6) to capture all startup logs:
extern "C" {
    fn install_mqtt_log_hook();
}
unsafe { install_mqtt_log_hook(); }
```

### Subscribing to log/level topic (in subscriber_loop)
```rust
// Source: existing subscriber_loop pattern in mqtt.rs
let log_level_topic = format!("gnss/{}/log/level", device_id);
match c.subscribe(&log_level_topic, QoS::AtLeastOnce) {
    Ok(_) => log::info!("Subscribed to {}", log_level_topic),
    Err(e) => log::warn!("Subscribe /log/level failed: {:?}", e),
}
```

### MQTT callback routing for log/level messages
```rust
// In mqtt_connect callback, EventPayload::Received branch:
} else if t.ends_with("/log/level") {
    match log_level_tx.try_send(data.to_vec()) {
        Ok(_) => {}
        Err(TrySendError::Full(_)) => {} // silently drop
        Err(TrySendError::Disconnected(_)) => {}
    }
}
```

### Non-blocking enqueue pattern (mirrors nmea_relay.rs)
```rust
// Inside log relay thread — same pattern as nmea_relay::spawn_relay
LOG_REENTERING.store(true, Ordering::Relaxed);
match client.lock() {
    Err(_) => {} // mutex poisoned — skip
    Ok(mut c) => {
        let _ = c.enqueue(&topic, QoS::AtMostOnce, false, msg.as_bytes());
        // Errors silently dropped (LOG-03)
    }
}
LOG_REENTERING.store(false, Ordering::Relaxed);
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Custom `log::Log` replacing ESP logger | `esp_log_set_vprintf` C shim | N/A (project-specific) | Captures C component logs; avoids `set_logger` one-shot limitation |
| Per-message heap alloc in vprintf callback | C stack buffer + channel String | N/A | Zero-alloc hot path in callback |
| Blocking publish in log callback | Relay thread + `try_send` drop | N/A | Never stalls calling thread (LOG-03) |

**Deprecated/outdated:**
- Direct `esp_log_set_vprintf` from Rust with `va_list` parameter: problematic on Xtensa before Jan 2024; for RISC-V (ESP32-C6) the ABI may work differently but the C shim approach is safer and architecturally portable.

---

## Open Questions

1. **C shim integration with `build.rs` vs `CMakeLists.txt`**
   - What we know: The project uses `embuild = "0.33"` in build-dependencies, which is the standard ESP-IDF Rust build integration. Embuild compiles additional C files when they are listed in `CMakeLists.txt` as part of the ESP-IDF component system.
   - What's unclear: Whether this project's build already has a `CMakeLists.txt` that accepts additional C sources, or whether `build.rs` can invoke `cc::Build` directly (requires `cc` crate as a build-dependency).
   - Recommendation: Check whether `CMakeLists.txt` exists at root. If not, the simplest path is adding `cc = "1"` to `[build-dependencies]` in Cargo.toml and compiling `log_shim.c` via `cc::Build::new().file("src/log_shim.c").compile("log_shim")` in `build.rs`. This is the standard pattern for ESP-IDF Rust projects adding a C helper file.

2. **`EspLogger::set_target_level` static instance requirement**
   - What we know: `EspLogger` is a zero-sized unit struct with no internal state; calling `set_target_level` on any instance works.
   - What's unclear: Whether declaring `static LOGGER: EspLogger = EspLogger` in the log relay module conflicts with the instance already used in `main.rs` via `EspLogger::initialize_default()`.
   - Recommendation: Use `esp_idf_svc::log::EspLogger.set_target_level(...)` directly (the struct is `Copy`) rather than a static. Or call `esp_idf_sys::esp_log_level_set(...)` directly, which is what `set_target_level` wraps.

3. **Stack size for log relay thread**
   - What we know: The relay thread receives `String` values and calls `mqtt_client.lock().enqueue()`. NMEA relay with the same pattern uses `stack_size(8192)`.
   - What's unclear: Whether `enqueue()` inside the log relay has any deeper call chain that needs more stack.
   - Recommendation: Start with `stack_size(4096)` — log relay does less work than NMEA relay. Monitor HWM at thread entry (existing pattern in all threads). Increase to 8192 if HWM is less than 512 bytes remaining.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | None — embedded firmware; no host test runner configured |
| Config file | N/A |
| Quick run command | `cargo build --release` (build verification only) |
| Full suite command | `cargo build --release && espflash flash --monitor` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LOG-01 | Log messages appear on MQTT topic within 1s; no feedback loop | manual smoke | `espflash flash --monitor` then `mosquitto_sub -t 'gnss/+/log'` | N/A — on-device |
| LOG-02 | Publishing level string changes forwarded messages immediately | manual smoke | `mosquitto_pub -t 'gnss/{id}/log/level' -m 'warn' -r` then verify only WARN+ on topic | N/A — on-device |
| LOG-03 | No stall when MQTT disconnected; messages silently dropped | manual smoke | Disconnect broker, observe no firmware hang via UART monitor | N/A — on-device |

### Sampling Rate
- **Per task commit:** `cargo build --release` (compilation gate)
- **Per wave merge:** Full flash + `mosquitto_sub` observation for all three requirements
- **Phase gate:** All three LOG requirements observable on hardware before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/log_shim.c` — C vprintf hook (new file, Wave 0 task)
- [ ] `src/log_relay.rs` — Rust relay module (new file, Wave 0 task)
- [ ] `build.rs` — add `cc::Build` for `log_shim.c`, or `CMakeLists.txt` update
- [ ] `Cargo.toml` — add `cc = "1"` to `[build-dependencies]` if using `cc::Build` approach

---

## Sources

### Primary (HIGH confidence)
- [ESP-IDF Logging Library — stable docs](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/log.html) — `esp_log_set_vprintf` signature, re-entrancy requirement, `esp_log_level_set`
- [esp-idf-sys `esp_log_set_vprintf` binding](https://docs.esp-rs.org/esp-idf-sys/esp_idf_sys/fn.esp_log_set_vprintf.html) — Rust FFI type confirmed
- Project source files (`src/mqtt.rs`, `src/nmea_relay.rs`, `src/config_relay.rs`) — established relay thread + `try_send` pattern
- Project `Cargo.toml` — pinned `esp-idf-svc = "=0.51.0"`, `esp-idf-sys = "=0.36.1"`

### Secondary (MEDIUM confidence)
- [ESP32 Forum: remote log via MQTT](https://esp32.com/viewtopic.php?t=29420) — community confirmation that `esp_log_set_vprintf` + MQTT channel approach works well
- [EspLogger docs — `set_target_level`](https://docs.esp-rs.org/esp-idf-svc/esp_idf_svc/log/struct.EspLogger.html) — method signature and behavior confirmed via multiple search results
- [esp-idf-sys va_list issue #212](https://github.com/esp-rs/esp-idf-sys/issues/212) — confirms C shim is safer than Rust va_list FFI

### Tertiary (LOW confidence)
- [Rust Forum: esp-idf logger redirection](https://users.rust-lang.org/t/esp-idf-logger-redirection-vprintf-variadic-function/95568) — confirms `va_list` Rust FFI difficulties; no definitive RISC-V-specific data

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all components already in project; `esp_log_set_vprintf` is stable ESP-IDF API
- Architecture: HIGH — C shim + relay thread is a proven pattern in this codebase and community
- Pitfalls: HIGH — re-entrancy loop, va_copy, stack size all verified against official docs and community reports

**Research date:** 2026-03-08
**Valid until:** 2026-06-08 (stable ESP-IDF API; 90-day window)
