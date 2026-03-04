# Phase 3: Status LED - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Drive the single yellow user LED on GPIO15 to reflect WiFi+MQTT connectivity state.
Three distinct blink patterns: connecting (fast blink), connected (steady), error (rapid burst).
No other behavior — no button input, no brightness control, no other LEDs.

</domain>

<decisions>
## Implementation Decisions

### Blink patterns
- **Connecting** (LED-01): 200ms on / 200ms off — fast blink, clearly "working on it"
- **Connected** (LED-02): steady on — visually distinct from all blink states; unambiguous
- **Error** (LED-03): 3× rapid pulse (100ms on / 100ms off) then 700ms off, repeating — distinct from connecting rate

### Connected definition
- LED-02 (steady on) requires BOTH WiFi and MQTT to be connected
- If either drops: immediately revert to LED-01 (connecting pattern)
- Rationale: the device is only "operational" when the full stack is up

### Error threshold
- Error state (LED-03) triggers after WiFi reconnect backoff has reached max (60s cap) AND at least 3 consecutive failures at max backoff — roughly 3+ minutes of failed connecting
- Resets back to connecting pattern on the next successful connect attempt

### State model
- Three states: `Connecting`, `Connected`, `Error`
- Shared `Arc<AtomicU8>` (or `Arc<Mutex<LedState>>`) updated by wifi_supervisor and pump thread
- LED thread polls state every 50ms and adjusts blink timing accordingly
- Initial state on boot: `Connecting`

### Claude's Discretion
- Exact Rust type for shared state (`AtomicU8` vs `Arc<Mutex<LedState>>` — pick whichever is cleaner)
- GPIO driver API (`PinDriver::output` from esp-idf-hal)
- LED thread stack size (follow 8192 pattern from other threads)
- Whether to express blink timing as a state machine or simple sleep loop
- Exact counter implementation for error threshold tracking

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Arc<Mutex<T>>` pattern from `src/mqtt.rs` — established pattern for shared mutable state
- `std::sync::mpsc` channels — available if push-based signaling preferred over polling
- `std::thread::Builder::new().stack_size(8192).spawn()` — standard thread launch pattern

### Established Patterns
- Module-per-concern: new code goes in `src/led.rs`, declared in `main.rs`
- All threads spawned in `main()` with explicit stack size
- `log::info!/warn!/error!` for all logging (never println!)
- Active-low GPIO: LOW signal = LED on, HIGH = LED off

### Integration Points
- `wifi_supervisor` in `src/wifi.rs` — the natural place to set `Connecting`/`Error` based on reconnect outcomes; receives the shared state Arc
- `pump_mqtt_events` in `src/mqtt.rs` — sets `Connected` on `EventPayload::Connected`, `Connecting` on `EventPayload::Disconnected`; already uses mpsc channel for subscribe_tx, same pattern applies
- `main.rs` step 14 (idle loop) — LED thread spawned here, same as WiFi supervisor
- GPIO15 available via `peripherals.pins.gpio15` in `main()`

</code_context>

<specifics>
## Specific Ideas

- Active-low: `gpio15.set_low()` = LED on, `gpio15.set_high()` = LED off
- The LED thread holds exclusive ownership of the GPIO pin — no sharing needed
- State is shared write-only from wifi/mqtt threads into the LED thread's read path

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-status-led*
*Context gathered: 2026-03-04*
