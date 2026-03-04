# Phase 3: Status LED - Research

**Researched:** 2026-03-04
**Domain:** ESP32-C6 GPIO output, shared atomic state, FreeRTOS threading in Rust (esp-idf-hal 0.45.2)
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Phase boundary:** Drive GPIO15 yellow LED to reflect WiFi+MQTT connectivity state. No button input, no brightness control, no other LEDs.

**Blink patterns:**
- Connecting (LED-01): 200ms on / 200ms off — fast blink
- Connected (LED-02): steady on
- Error (LED-03): 3x rapid pulse (100ms on / 100ms off) then 700ms off, repeating

**Connected definition:** LED-02 requires BOTH WiFi AND MQTT connected. If either drops, immediately revert to LED-01.

**Error threshold:** Triggers after WiFi reconnect backoff has reached max (60s cap) AND at least 3 consecutive failures at max backoff. Resets to connecting on next successful connect attempt.

**State model:** Three states: `Connecting`, `Connected`, `Error`. Shared `Arc<AtomicU8>` (or `Arc<Mutex<LedState>>`) updated by wifi_supervisor and pump thread. LED thread polls state every 50ms.

**Initial state on boot:** `Connecting`.

**Active-low GPIO:** `gpio15.set_low()` = LED on, `gpio15.set_high()` = LED off.

**LED thread holds exclusive GPIO ownership** — no sharing of the pin driver.

### Claude's Discretion

- Exact Rust type for shared state (`AtomicU8` vs `Arc<Mutex<LedState>>` — pick whichever is cleaner)
- GPIO driver API (`PinDriver::output` from esp-idf-hal)
- LED thread stack size (follow 8192 pattern from other threads)
- Whether to express blink timing as a state machine or simple sleep loop
- Exact counter implementation for error threshold tracking

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LED-01 | LED shows a distinct blink pattern while the device is attempting to connect to WiFi or MQTT | 200ms on/off fast blink; `AtomicU8` polling every 50ms; `PinDriver::output` on GPIO15 |
| LED-02 | LED shows a steady-on (or slow blink) pattern when WiFi and MQTT are both connected | `Connecting`→`Connected` transition set by pump thread on `EventPayload::Connected` AND wifi_supervisor on reconnect success; single atomic write |
| LED-03 | LED shows an error pattern when connectivity cannot be established after repeated retries | Error state triggered in wifi_supervisor after 3 consecutive failures at 60s backoff cap; 3x rapid burst + 700ms off cycle |
</phase_requirements>

---

## Summary

Phase 3 adds a status LED to communicate device connectivity state without a serial monitor. The hardware is a single yellow LED on GPIO15, active-low. The implementation requires: (1) a shared state value owned by the LED thread and written from the wifi_supervisor and mqtt pump threads, (2) a dedicated LED thread that owns the GPIO pin driver and drives blink timing based on the current state.

The ESP-IDF HAL (`esp-idf-hal 0.45.2`) provides `PinDriver::output` which takes ownership of the GPIO peripheral, implements `Send`, and exposes `set_low()`/`set_high()` for direct output control. `PinDriver` can be safely moved into a spawned thread. The shared-state problem is cleanly solved with `Arc<AtomicU8>` — three states fit in 3 values, loads use `Relaxed` ordering (state is only directional/visual, no happens-before required), and the type is available in full Rust std on the `riscv32imac-esp-espidf` target.

The error threshold counter (3 consecutive max-backoff failures) lives entirely within `wifi_supervisor` as a local variable — it does not need to be shared. Only the resolved `LedState` value crosses the thread boundary via the atomic. The mqtt pump thread needs only one write: `Connected` on `EventPayload::Connected` and `Connecting` on `EventPayload::Disconnected`.

**Primary recommendation:** Use `Arc<AtomicU8>` with a three-value `LedState` enum (`Connecting=0`, `Connected=1`, `Error=2`). LED thread polls every 50ms and drives blink timing with a simple elapsed-time counter. This is the cleanest approach: no mutex, no blocking, no additional channels.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-idf-hal | =0.45.2 | GPIO output via `PinDriver` | Already in Cargo.toml; only GPIO crate available for this target |
| std::sync::atomic::AtomicU8 | std | Shared LED state across threads | Lock-free, `Send + Sync`, full std available on espidf target |
| std::sync::Arc | std | Shared ownership of the atomic | Established pattern in this codebase (see mqtt.rs) |
| std::thread | std | Spawn LED blink thread | Established pattern; all subsystems use dedicated threads |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std::time::Duration | std | Sleep durations in blink loop | All sleeps need this; already used throughout codebase |
| std::sync::atomic::Ordering | std | Load/store ordering on AtomicU8 | `Relaxed` sufficient for visual-only LED state |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `AtomicU8` | `Arc<Mutex<LedState>>` | Mutex is heavier but makes the enum type explicit; AtomicU8 requires `repr(u8)` enum + `from_u8` conversion; either works, AtomicU8 is lighter |
| Poll every 50ms | `mpsc::channel` push model | Channel push is cleaner for instantaneous response but adds complexity; 50ms polling is imperceptible visually and simpler |

**Installation:** No new dependencies — all required types are in existing crates (`esp-idf-hal` and `std`).

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── led.rs           # New: LedState enum, led_task() function, spawn_led()
├── main.rs          # Updated: spawn LED thread, pass Arc<AtomicU8> to wifi/mqtt
├── wifi.rs          # Updated: accept Arc<AtomicU8>, write state, track error counter
├── mqtt.rs          # Updated: accept Arc<AtomicU8>, write Connected/Connecting
├── config.rs        # Unchanged
├── device_id.rs     # Unchanged
└── uart_bridge.rs   # Unchanged
```

### Pattern 1: GPIO Output via PinDriver (VERIFIED)

**What:** `PinDriver::output(pin)` creates an owned, `Send` output driver. Call `set_low()` to illuminate (active-low), `set_high()` to extinguish.

**When to use:** Any time you need to drive a GPIO output pin from a single thread.

**Example:**
```rust
// Source: esp-idf-hal-0.45.2/src/gpio.rs (cargo registry, verified)
use esp_idf_hal::gpio::PinDriver;

// In main(), after Peripherals::take():
let led_pin = PinDriver::output(peripherals.pins.gpio15)?;
// led_pin is Send — move it into the LED thread
std::thread::Builder::new()
    .stack_size(8192)
    .spawn(move || led::led_task(led_pin, led_state.clone()))
    .expect("LED thread spawn failed");
```

Active-low mapping:
```rust
// Source: active-low hardware (3.3V → 1.5kΩ → LED → GPIO15)
led_pin.set_low()?;   // GPIO low  → LED ON
led_pin.set_high()?;  // GPIO high → LED OFF
```

### Pattern 2: Shared State via Arc<AtomicU8> (VERIFIED)

**What:** Three-state enum encoded as `u8`, stored in `Arc<AtomicU8>`. Writers (wifi_supervisor, pump thread) call `store(..., Relaxed)`. Reader (LED thread) calls `load(Relaxed)` every 50ms.

**When to use:** When multiple threads write a simple enumerated value and one thread reads it for non-critical (visual) output.

**Example:**
```rust
// Source: std::sync::atomic (Rust std, available on riscv32imac-esp-espidf)
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub enum LedState {
    Connecting = 0,
    Connected  = 1,
    Error      = 2,
}

impl LedState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => LedState::Connected,
            2 => LedState::Error,
            _ => LedState::Connecting,
        }
    }
}

// In main():
let led_state: Arc<AtomicU8> = Arc::new(AtomicU8::new(LedState::Connecting as u8));

// Writer (any thread):
led_state.store(LedState::Connected as u8, Ordering::Relaxed);

// Reader (LED thread every 50ms):
let state = LedState::from_u8(led_state.load(Ordering::Relaxed));
```

### Pattern 3: Blink Loop with Elapsed Counter

**What:** Instead of sleeping for each on/off phase (which would block state checks), the LED thread polls every 50ms and accumulates elapsed time to decide when to flip the LED.

**When to use:** When blink timing needs to be responsive to state changes — if you slept 200ms between flips, state changes would be delayed up to 200ms.

**Example:**
```rust
// Source: pattern derived from timing requirements in 03-CONTEXT.md
pub fn led_task(mut pin: PinDriver<'static, Gpio15, Output>, state: Arc<AtomicU8>) -> ! {
    let poll_ms: u64 = 50;
    let mut elapsed_ms: u64 = 0;
    let mut led_on = false;

    loop {
        let current = LedState::from_u8(state.load(Ordering::Relaxed));

        match current {
            LedState::Connected => {
                // Steady on — set once, reset elapsed
                if !led_on {
                    pin.set_low().ok();  // LED ON (active-low)
                    led_on = true;
                    elapsed_ms = 0;
                }
            }
            LedState::Connecting => {
                // 200ms on / 200ms off
                let period_ms = 200u64;
                elapsed_ms += poll_ms;
                let phase = (elapsed_ms / period_ms) % 2;
                let want_on = phase == 0;
                if want_on != led_on {
                    if want_on { pin.set_low().ok(); } else { pin.set_high().ok(); }
                    led_on = want_on;
                }
            }
            LedState::Error => {
                // 3x rapid pulse (100ms on/off) then 700ms off
                // Total cycle: 600ms pulses + 700ms off = 1300ms
                elapsed_ms += poll_ms;
                let cycle_ms = 1300u64;
                let pos = elapsed_ms % cycle_ms;
                let want_on = pos < 600 && (pos % 200) < 100;
                if want_on != led_on {
                    if want_on { pin.set_low().ok(); } else { pin.set_high().ok(); }
                    led_on = want_on;
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}
```

### Pattern 4: Error Threshold Counter in wifi_supervisor

**What:** Local counter in `wifi_supervisor` tracks consecutive failures at max backoff. The counter is NOT shared — only the resolved `LedState` is written to the Arc.

**When to use:** Error threshold detection that doesn't need to be visible from other threads.

**Example:**
```rust
// Source: derived from CONTEXT.md error threshold spec
pub fn wifi_supervisor(
    mut wifi: BlockingWifi<EspWifi<'static>>,
    led_state: Arc<AtomicU8>,
) -> ! {
    let mut backoff_secs: u64 = 1;
    let mut max_backoff_failures: u32 = 0;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let connected = wifi.is_connected().unwrap_or(false);
        led_state.store(
            if connected { LedState::Connected as u8 } else { LedState::Connecting as u8 },
            Ordering::Relaxed,
        );

        if !connected {
            std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
            match wifi.connect().and_then(|_| wifi.wait_netif_up()) {
                Ok(_) => {
                    backoff_secs = 1;
                    max_backoff_failures = 0;
                    led_state.store(LedState::Connected as u8, Ordering::Relaxed);
                }
                Err(e) => {
                    log::error!("WiFi reconnect failed: {:?}", e);
                    if backoff_secs >= 60 {
                        max_backoff_failures += 1;
                        if max_backoff_failures >= 3 {
                            led_state.store(LedState::Error as u8, Ordering::Relaxed);
                        }
                    }
                    backoff_secs = (backoff_secs * 2).min(60);
                }
            }
        }
    }
}
```

**IMPORTANT NOTE on wifi_supervisor signature change:** The existing `wifi_supervisor(wifi)` in `wifi.rs` takes one argument. The LED integration adds a second: `Arc<AtomicU8>`. This requires updating both the function signature in `wifi.rs` AND the spawn call in `main.rs`.

### Pattern 5: MQTT pump LED integration

**What:** `pump_mqtt_events` currently receives `EspMqttConnection` and `Sender<()>`. Add `Arc<AtomicU8>` as a third parameter. Write `Connected` on `EventPayload::Connected`, `Connecting` on `EventPayload::Disconnected`.

**IMPORTANT CONSTRAINT:** The pump thread NEVER calls any client method (established deadlock prevention pattern). Atomic stores are NOT client method calls — they are safe from the pump thread.

```rust
// Source: existing pump_mqtt_events pattern in src/mqtt.rs + LED addition
pub fn pump_mqtt_events(
    mut connection: EspMqttConnection,
    subscribe_tx: Sender<()>,
    led_state: Arc<AtomicU8>,
) -> ! {
    while let Ok(event) = connection.next() {
        match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("MQTT connected");
                led_state.store(LedState::Connected as u8, Ordering::Relaxed);
                let _ = subscribe_tx.send(());
            }
            EventPayload::Disconnected => {
                log::warn!("MQTT disconnected");
                led_state.store(LedState::Connecting as u8, Ordering::Relaxed);
            }
            // ...
        }
    }
    // ...
}
```

### Anti-Patterns to Avoid

- **Sharing PinDriver across threads:** PinDriver is `Send` but NOT `Sync`. It cannot be in an `Arc<Mutex<PinDriver>>` because `PinDriver` requires `&mut` for all output operations — which is fine, but the LED thread must own it exclusively. Do NOT attempt to share the pin driver; move it into the LED thread.
- **Sleeping for the full blink period:** Sleeping 200ms between LED flips means state changes take up to 200ms to register. Use the 50ms polling loop with elapsed-time tracking instead.
- **Writing `Connected` from wifi_supervisor without checking MQTT:** `wifi_supervisor` seeing WiFi connected does NOT mean MQTT is connected. `Connected` state should only be written by the MQTT pump on `EventPayload::Connected`. The wifi_supervisor should write `Connecting` when WiFi drops, but NEVER write `Connected` — that's the pump's job.
- **Resetting error state from wifi_supervisor on WiFi reconnect:** The CONTEXT.md says error resets "on the next successful connect attempt." WiFi reconnect is necessary but not sufficient — MQTT must also reconnect. Reset max_backoff_failures on WiFi success, but don't store `Connected` — let the pump thread do that when MQTT fires `EventPayload::Connected`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GPIO output | Custom ESP-IDF `gpio_set_level` unsafe wrappers | `PinDriver::output()` from esp-idf-hal | PinDriver handles mode config, reset-on-drop, type safety, Send impl |
| Thread-safe shared value | Custom spinlock or unsafe static | `Arc<AtomicU8>` from std | Correct memory ordering, no UB, already proven in Rust std |
| Blink timing | Hardware timer or LEDC PWM peripheral | Simple sleep loop with elapsed counter | Hardware timers add complexity; 50ms polling is imperceptible |

**Key insight:** The LED implementation is simple enough that no additional crates are needed. All required primitives (`PinDriver`, `AtomicU8`, `Arc`, `thread::sleep`) are already available in the project's existing dependencies and Rust std.

---

## Common Pitfalls

### Pitfall 1: Writing `Connected` from Both wifi_supervisor AND pump Thread

**What goes wrong:** If wifi_supervisor writes `Connected` when WiFi reconnects (before MQTT reconnects), the LED goes steady while the device is actually still waiting for MQTT. Operator sees "connected" but MQTT is not up.

**Why it happens:** WiFi success is the obvious place to set connected state, but MQTT connection is the real completion signal.

**How to avoid:** `Connected` state is written ONLY by the MQTT pump thread on `EventPayload::Connected`. wifi_supervisor writes `Connecting` (on disconnect) and manages the `Error` threshold, but NEVER writes `Connected`.

**Warning signs:** LED goes steady immediately after WiFi reconnect before MQTT heartbeats resume.

### Pitfall 2: PinDriver Lifetime When Moved to Thread

**What goes wrong:** `PinDriver<'d, Gpio15, Output>` has a lifetime parameter `'d` tied to the `Gpio15` peripheral. If the peripheral is borrowed rather than consumed, the `'d` lifetime won't be `'static` and the thread closure won't compile (threads require `'static` bounds).

**Why it happens:** `PinDriver::output(pin)` consumes `pin` via `impl Peripheral<P = T> + 'd`. Passing `peripherals.pins.gpio15` directly (consuming from `Peripherals`) produces `PinDriver<'static, Gpio15, Output>` because `Peripherals::take()` gives owned values with static lifetime.

**How to avoid:** Pass `peripherals.pins.gpio15` directly to `PinDriver::output()`. Do NOT borrow it. The resulting `PinDriver<'static, Gpio15, Output>` (or `PinDriver<'_, AnyOutputPin, Output>` with `.downgrade_output()`) is `'static` and moves into the thread cleanly.

**Warning signs:** Compiler error "does not satisfy `'static`" on thread spawn.

### Pitfall 3: Error State Not Resetting Correctly

**What goes wrong:** Error state persists even after WiFi/MQTT recovers if `max_backoff_failures` counter is not reset and `Connected` never gets written.

**Why it happens:** The pump thread writes `Connected` on `EventPayload::Connected`, which overrides `Error`. But if the pump thread isn't running (e.g., pump loop exited) the error state could be stuck.

**How to avoid:** Ensure the pump's `EventPayload::Connected` branch unconditionally stores `Connected` — this naturally overrides any prior `Error` value. Also, reset `max_backoff_failures` in wifi_supervisor on any successful WiFi reconnect.

**Warning signs:** After recovery, LED still shows error pattern instead of steady-on.

### Pitfall 4: Initial State Mismatch

**What goes wrong:** LED starts in `Connecting` state (correct per spec), but if `wifi_connect()` in main.rs succeeds synchronously before the LED thread is spawned and the MQTT pump fires `Connected`, the first state transition may be missed.

**Why it happens:** The LED thread is spawned after WiFi and MQTT initialization. If MQTT connects immediately, the pump fires `Connected` before the LED thread is reading the atomic.

**How to avoid:** This is NOT a real problem for `AtomicU8` — the atomic store happens before the LED thread reads it, and the LED thread will pick up the current value on its first poll. The atomic is a current-state store, not an event stream. No race condition exists.

**Warning signs:** None — this pitfall is a false alarm when using polling-based AtomicU8 design.

### Pitfall 5: set_low/set_high Error Handling

**What goes wrong:** `set_low()` and `set_high()` return `Result<(), EspError>`. Ignoring errors with `.ok()` is fine for LED (non-critical), but panicking on error would crash the LED thread.

**Why it happens:** Defensive code that unwraps GPIO errors.

**How to avoid:** Use `.ok()` or `let _ =` to discard GPIO errors. Log with `log::warn!` if you want visibility. Never `.unwrap()` GPIO operations in the LED thread.

---

## Code Examples

### Creating GPIO15 Output Driver
```rust
// Source: esp-idf-hal-0.45.2/src/gpio.rs (verified from cargo registry)
use esp_idf_hal::gpio::PinDriver;

// peripherals.pins.gpio15 is Gpio15 which implements OutputPin on ESP32-C6
let led_pin = PinDriver::output(peripherals.pins.gpio15)
    .expect("GPIO15 PinDriver init failed");
// led_pin: PinDriver<'static, Gpio15, Output> — implements Send
```

### Active-Low LED Control
```rust
// Source: hardware spec (active-low: 3.3V → 1.5kΩ → LED → GPIO15)
led_pin.set_low().ok();   // GPIO15 driven LOW  → LED illuminated
led_pin.set_high().ok();  // GPIO15 driven HIGH → LED extinguished
```

### AtomicU8 State Share (main.rs)
```rust
// Source: std::sync::atomic, std::sync::Arc
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

// Create shared state — initial value = Connecting (0)
let led_state = Arc::new(AtomicU8::new(0u8));

// Clone before moving into each thread that needs write access
let led_state_wifi = led_state.clone();
let led_state_mqtt = led_state.clone();
// led_state itself moves into led_task

std::thread::Builder::new()
    .stack_size(8192)
    .spawn(move || led::led_task(led_pin, led_state))
    .expect("LED thread spawn failed");
```

### Spawn Order in main.rs

The LED thread should be spawned BEFORE wifi_supervisor and pump threads so that it is reading by the time those threads start writing. Suggested position: after GPIO pin is obtained (Step 3), before WiFi connect (Step 6), or at minimum before Step 10 (pump spawn).

Revised init order:
```
Step 3:  Peripherals::take()
Step 3b: Create led_state Arc<AtomicU8>
Step 3c: Create PinDriver for GPIO15
Step 3d: Spawn LED thread (moves pin + led_state clone)
Step 4:  EspSystemEventLoop
Step 5:  NVS
Step 6:  wifi_connect (pass led_state clone for supervisor)
...
Step 10: Spawn pump thread (pass led_state clone)
Step 13: Spawn wifi_supervisor (pass led_state clone)
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| LEDC (hardware PWM) for blink | Software sleep loop | N/A for this use case | Software loop is simpler and adequate for 100-200ms periods |
| `embedded-hal` `OutputPin` trait object | Direct `PinDriver` concrete type | esp-idf-hal 0.45 | Concrete type is simpler; trait objects needed only for hardware abstraction |

**Deprecated/outdated:**
- `gpio_set_level()` via raw `esp-idf-sys`: Wrapped by `PinDriver::set_low/set_high`. Do not call directly.
- `esp_idf_hal::gpio::Gpio15::downgrade()`: Not needed — `peripherals.pins.gpio15` can be passed directly to `PinDriver::output`.

---

## Open Questions

1. **`PinDriver` lifetime annotation in thread signature**
   - What we know: `PinDriver<'static, Gpio15, Output>` is the expected type when consuming `peripherals.pins.gpio15` directly
   - What's unclear: Whether the compiler will require explicit `'static` annotation in the `led_task` function signature or infer it
   - Recommendation: Define `led_task` as `fn led_task(pin: PinDriver<'static, Gpio15, Output>, ...)` to be explicit; if generic over pin type is preferred, bound as `pin: PinDriver<'static, impl OutputPin, Output>`

2. **`Gpio15` type import path**
   - What we know: The type exists as `esp_idf_hal::gpio::Gpio15` (verified in cargo registry source)
   - What's unclear: Whether the function signature needs the concrete `Gpio15` type or can use `AnyOutputPin`
   - Recommendation: Use `PinDriver<'static, Gpio15, Output>` for clarity in led.rs; alternatively `.downgrade_output()` converts to `AnyOutputPin` which may be more portable

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — no test infrastructure exists in this project |
| Config file | None |
| Quick run command | `cargo build` (compile-check only; cross-compile target cannot run on host) |
| Full suite command | `cargo build` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LED-01 | Connecting blink 200ms on/off | manual-only | N/A — requires hardware observation | N/A |
| LED-02 | Steady on when WiFi+MQTT both up | manual-only | N/A — requires hardware observation | N/A |
| LED-03 | Error burst pattern after 3+ max-backoff failures | manual-only | N/A — requires hardware observation | N/A |

**Manual-only justification:** This is embedded firmware targeting `riscv32imac-esp-espidf`. There is no host-side test runner for the compiled binary. LED behavior requires physical hardware observation. Unit tests could test the `LedState::from_u8` conversion function and blink timing math in isolation (with `cargo test --target x86_64` if the led.rs module is written with no esp-idf dependencies in pure logic), but the project currently has no test infrastructure and this is out of scope for Phase 3 planning.

### Sampling Rate
- **Per task commit:** `cargo build` — verifies the firmware compiles for target
- **Per wave merge:** `cargo build` — same
- **Phase gate:** Hardware flash and manual verification of all three LED patterns before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] No test files needed — manual verification is the gate for LED behavior
- [ ] `cargo build` already works — no framework install needed

*(LED behavioral tests require on-device observation; compile-time verification via `cargo build` is the automated gate)*

---

## Sources

### Primary (HIGH confidence)
- `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-hal-0.45.2/src/gpio.rs` — `PinDriver` struct, `output()` constructor, `set_low()`, `set_high()`, `Send` impl, GPIO15 pin definition for ESP32-C6
- Rust std `std::sync::atomic::AtomicU8` — available in full std; `riscv32imac-esp-espidf` target uses full std (confirmed by `build-std = ["std", "panic_abort"]` in `.cargo/config.toml`)
- `src/mqtt.rs` (project source) — established `Arc<Mutex<>>` and `mpsc` channel patterns; pump thread deadlock constraints
- `src/wifi.rs` (project source) — wifi_supervisor backoff logic; integration point for error counter and LED state writes
- `src/main.rs` (project source) — thread spawn pattern (`Builder::new().stack_size(8192).spawn()`), init order, peripheral ownership model
- `.planning/phases/03-status-led/03-CONTEXT.md` — locked blink patterns, state model, error threshold, active-low spec

### Secondary (MEDIUM confidence)
- ESP32-C6 technical reference: GPIO15 is a general IO pin, not subject to special restrictions (verified by `pin!(Gpio15:15, IO, ...)` in esp-idf-hal source)
- RISC-V A extension: ESP32-C6 uses `riscv32imac` which includes the A (atomic) extension; `AtomicU8` uses CAS-based emulation for sub-word types on RISC-V (acceptable for this use case)

### Tertiary (LOW confidence)
- None — all critical claims verified from primary sources

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all types verified in local cargo registry source
- Architecture: HIGH — patterns derived directly from existing project code + verified GPIO API
- Pitfalls: HIGH — derived from concrete API analysis and existing project patterns (MQTT deadlock memory)

**Research date:** 2026-03-04
**Valid until:** 2026-06-04 (stable library; esp-idf-hal 0.45.2 is pinned with `=`)
