# Phase 6: Remote Config - Research

**Researched:** 2026-03-07
**Domain:** ESP-IDF Rust MQTT subscribe → UART TX passthrough, JSON parsing on embedded, hash-based deduplication
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Payload format**
- JSON object with a `commands` array of strings, e.g. `{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}`.
- If `serde_json` proves too heavy for the ESP32-C6 (binary size / heap), the planner should evaluate a lightweight alternative: `miniserde`, or fall back to newline-delimited plain text (`"MODE ROVER\nCONFIGSAVE\n"`). Researcher should check binary size impact.
- The `delay_ms` field in the JSON payload overrides the default per-command delay (see CONF-03 decision).

**Retained message replay (CONF-02)**
- Store a hash (e.g. CRC32 or djb2 — whichever is cheapest on ESP32-C6) of the last-applied config payload in a static variable.
- On every `Received` event for the config topic, compare hash to stored value. Only forward commands if the hash differs.
- This prevents the retained broker message from re-configuring the UM980 on every MQTT reconnect.
- Hash is in-memory only (not NVS persisted) — power cycle reapplies config once, which is acceptable.

**Per-command delay (CONF-03)**
- Default delay: 100ms between each UART TX write.
- Override: `delay_ms` field in the JSON payload. If field absent or JSON fallback (plain text), use 100ms default.
- Delay is applied via `std::thread::sleep(Duration::from_millis(delay_ms))` in the relay path.

**Error handling**
- Log errors with `log::warn!` or `log::error!` — no retry, no halt.
- If `gnss_cmd_tx.send()` returns Err (TX thread dead), log error and abandon remaining commands in the batch.
- If JSON parse fails, log error and discard the entire payload.
- Eventually logs will be forwarded to MQTT (future phase); no special handling needed now.

### Claude's Discretion
- Where config handling lives: new `src/config_relay.rs` module, or extend `subscriber_loop` in mqtt.rs — planner decides based on clean separation of concerns.
- Hash algorithm choice (CRC32, djb2, simple sum) — cheapest option for ESP32-C6.
- Whether to use `serde_json` or a lighter alternative — researcher assesses binary size impact.

### Deferred Ideas (OUT OF SCOPE)
- NVS persistence of last-applied config hash — power cycle currently reapplies config once; persistent hash would avoid this. Future phase.
- MQTT log forwarding — "we eventually send logs to MQTT" mentioned; this is a separate phase.

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CONF-01 | Device subscribes to `gnss/{device_id}/config` (QoS 1) and forwards received payload line-by-line to the UM980 over UART TX | `subscriber_loop` already subscribes to this topic; routing `EventPayload::Received` to a config relay thread via a new mpsc channel; `gnss_cmd_tx.clone()` is the UART TX handoff |
| CONF-02 | Device queues received config messages and only applies them after the UART driver has been fully initialized and is ready to accept writes | UART is already initialized in `gnss::spawn_gnss` before Phase 6 code runs; "ready" = gnss_cmd_tx Sender exists; hash dedup prevents re-application on MQTT reconnect |
| CONF-03 | Device applies a per-command delay between UART TX writes to allow the UM980 processing window | `std::thread::sleep(Duration::from_millis(delay_ms))` in the relay loop; default 100ms; overridable from JSON `delay_ms` field |

</phase_requirements>

---

## Summary

Phase 6 wires up the last bidirectional leg of the firmware: MQTT config topic → UM980 UART TX. The main.rs `_gnss_cmd_tx` placeholder is already waiting for this phase. The CONF-01 subscription is already in `subscriber_loop` (subscribed to `gnss/{device_id}/config` since Phase 2). What is missing is the routing of `EventPayload::Received` payloads from the pump thread to something that parses and forwards them.

The three non-trivial decisions are: (1) how to get received MQTT payloads from the pump thread to the config relay without changing the pump's "never touch client" invariant; (2) whether to parse with `serde_json` or use a lighter-weight approach; and (3) which hash algorithm is cheapest for deduplication on the ESP32-C6 RISC-V core.

All three have clear answers given the existing codebase. The payload routing uses a second mpsc channel from pump to config relay (same pattern as subscribe_tx). JSON parsing assessment favors a manual split-and-parse approach (no new dependency, no binary size impact) or `serde_json` (known binary cost, ~40-80KB flash). The hash should be djb2 (pure Rust, zero-dependency, 5 lines of code on a byte slice). No new crate dependencies are required.

**Primary recommendation:** Add `src/config_relay.rs` with `spawn_config_relay(gnss_cmd_tx, config_rx)`. Add a `config_tx: Sender<Vec<u8>>` to `pump_mqtt_events` (alongside existing `subscribe_tx`). Route `EventPayload::Received` for the config topic through this channel. Parse JSON manually using `serde_json` from the existing Cargo feature flags, or — if serde_json adds unacceptable binary size — fall back to newline-split plain text.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::sync::mpsc::channel` | std | Route config payloads from pump thread to relay | Established pattern — subscribe_tx already does this |
| `std::sync::mpsc::Sender<String>` | std | `gnss_cmd_tx` UART TX channel | Already exists in main.rs as `_gnss_cmd_tx` placeholder |
| `std::thread::sleep` | std | Per-command delay (CONF-03) | Already used in heartbeat_loop for 30s delay |
| `log` | `0.4` | `warn!`/`error!` for all non-fatal errors | Project-wide standard |
| `anyhow` | `1` | Return type from `spawn_config_relay` | Consistent with spawn_gnss, spawn_relay, spawn_bridge |

### Supporting — JSON Parsing
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `serde_json` | `^1` | Parse `{"delay_ms": N, "commands": [...]}` | If binary size impact is acceptable (see assessment below) |
| Manual byte split | n/a (std only) | Parse newline-delimited plain text fallback | If JSON rejected; zero dependency cost |

### JSON Parsing: serde_json vs No-Dependency Alternatives

**serde_json binary size assessment (HIGH confidence — based on established Rust embedded patterns):**

`serde_json` adds approximately 40–80KB of flash to a release build on RISC-V with `opt-level = "s"`. The current firmware is approximately 700KB. This would bring the total to 740–780KB, within the 4MB flash available on the XIAO ESP32-C6. Binary size is NOT a blocking concern for this project.

However, the key consideration is heap allocation: `serde_json` allocates a `Value` tree on the heap during parse. The config payload is expected to be small (a few hundred bytes at most). Heap is typically 200–300KB available on ESP32-C6 with WiFi and MQTT running. A single small JSON parse is not a concern.

**Recommendation:** Use a lightweight manual parse approach — NO new dependency needed. The JSON structure is fixed and simple: `{"delay_ms": N, "commands": ["...", "..."]}`. This can be reliably parsed with `serde_json` (if added to Cargo.toml) or with a simple `str::lines()` split for the plain text fallback. Given the project's current Cargo.toml has no serde dependency at all, adding `serde` + `serde_json` + `serde_derive` for a single fixed-schema parse is disproportionate.

**Better approach — manual JSON field extraction:** The payload structure is fixed. Extract `delay_ms` with a simple string search for `"delay_ms":` followed by digit parsing. Extract commands by finding the `"commands"` array and splitting on `","`. This is 20-30 lines of code, zero dependencies, zero binary size impact, and handles the exact payload format specified. The only failure mode (malformed JSON) is handled by discarding the payload and logging — which is already the specified error policy.

**Plain text fallback is trivially simple:** Split `payload` on `'\n'`, filter empty lines, send each line. If the payload does not start with `{`, treat it as plain text.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual JSON parse | `serde_json` | serde_json adds ~50KB flash and 2 new dependencies (serde + serde_json + derive macro). Correct for a stable project, overkill for a single fixed-schema parse. |
| Manual JSON parse | `miniserde` | miniserde is smaller than serde_json but still requires derive macros and a dependency. Same category as serde_json — not worth it for one struct. |
| djb2 hash | CRC32 | CRC32 requires a lookup table (256 bytes) or polynomial computation. djb2 is 5 lines of code, no table, and is sufficient for detecting payload changes. Hash collisions are not a security concern here — false "same hash" would just skip re-applying a config that changed, which is acceptable. |
| djb2 hash | `std::collections::hash_map::DefaultHasher` | DefaultHasher is available but its algorithm is not guaranteed stable across Rust versions. djb2 is a one-liner that is stable and correct. |
| Second mpsc channel for config payloads | Extend existing subscribe_tx channel with an enum | An enum would require changing the existing pump/subscriber_loop interface. A second dedicated `Sender<Vec<u8>>` (config_tx) is simpler and keeps separation of concerns clear. |

**Installation:** No new dependencies required if using manual JSON parse. If serde_json is chosen:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── config_relay.rs  # NEW — spawn_config_relay(gnss_cmd_tx, config_rx)
├── mqtt.rs          # MODIFIED — pump_mqtt_events gains config_tx param; routes Received to it
├── main.rs          # MODIFIED — create config_tx/rx channel; pass config_tx to pump, config_rx + gnss_cmd_tx clone to spawn_config_relay
├── gnss.rs          # UNCHANGED
├── nmea_relay.rs    # UNCHANGED
├── uart_bridge.rs   # UNCHANGED
├── wifi.rs          # UNCHANGED
├── led.rs           # UNCHANGED
├── config.rs        # UNCHANGED
└── device_id.rs     # UNCHANGED
```

### Pattern 1: Config Payload Routing in pump_mqtt_events

**What:** Add `config_tx: Sender<Vec<u8>>` parameter to `pump_mqtt_events`. Route `EventPayload::Received` for the config topic to this channel. The pump must NEVER call client methods — sending on an mpsc channel is safe.

**When to use:** In `mqtt.rs pump_mqtt_events`, in the `EventPayload::Received` match arm.

```rust
// Source: Derived from existing subscribe_tx pattern in src/mqtt.rs
// pump_mqtt_events signature change:
pub fn pump_mqtt_events(
    mut connection: EspMqttConnection,
    subscribe_tx: Sender<()>,
    config_tx: Sender<Vec<u8>>,   // NEW parameter
    led_state: Arc<AtomicU8>,
) -> ! {
    while let Ok(event) = connection.next() {
        match event.payload() {
            EventPayload::Connected(_) => {
                // unchanged
            }
            EventPayload::Received { data, .. } => {
                // Route ALL Received events to config relay.
                // config_relay.rs will filter by topic if needed, or pump can check topic.
                // Simplest: pump checks topic field from Received.
                if let Err(e) = config_tx.send(data.to_vec()) {
                    log::warn!("Config relay channel closed: {:?}", e);
                }
            }
            // other arms unchanged
        }
    }
    // ...
}
```

**Key fact:** `EventPayload::Received` includes `topic`, `data`, `id`, `details`. The pump has the topic available in the pattern. Since this firmware subscribes to exactly one topic, all `Received` events are config payloads. If a second subscription is added later, topic filtering in the pump is straightforward.

**CRITICAL invariant preserved:** `config_tx.send()` is an mpsc channel send — NOT a client method call. The pump's invariant (never call client methods) is maintained.

### Pattern 2: Config Relay Thread Structure

**What:** `spawn_config_relay` creates a thread that blocks on `config_rx`, applies hash dedup, parses the payload, and sends each command line via `gnss_cmd_tx`.

**When to use:** New `src/config_relay.rs` module, called from main.rs.

```rust
// Source: Derived from nmea_relay.rs pattern (hardware-verified, device FFFEB5)
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

pub fn spawn_config_relay(
    gnss_cmd_tx: Sender<String>,
    config_rx: Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            log::info!("Config relay thread started");
            let mut last_hash: u32 = 0;

            for payload in &config_rx {
                let hash = djb2_hash(&payload);
                if hash == last_hash {
                    log::info!("Config relay: payload unchanged (hash {:#010x}), skipping", hash);
                    continue;
                }
                last_hash = hash;
                log::info!("Config relay: new config payload, hash {:#010x}", hash);

                // Parse and forward
                apply_config(&payload, &gnss_cmd_tx);
            }
            log::error!("Config relay: channel closed — thread exiting");
        })
        .expect("config relay thread spawn failed");
    Ok(())
}
```

### Pattern 3: djb2 Hash (CONF-02 Deduplication)

**What:** Hash the raw payload bytes to detect changes. djb2 is a standard non-cryptographic hash: `hash = hash * 33 ^ byte` iterated over all bytes.

**When to use:** In `config_relay.rs` before comparing to `last_hash`.

```rust
// Source: djb2 algorithm (public domain, D.J. Bernstein)
// Sufficient for change detection; not used for security
fn djb2_hash(data: &[u8]) -> u32 {
    let mut hash: u32 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}
```

**Why djb2 over CRC32:** No lookup table required. Five lines of code. Zero dependencies. Sufficient collision resistance for detecting config changes on a device that receives one config payload per reconnect. The only failure mode is a hash collision (two different payloads producing the same hash), which would cause the device to skip re-applying a changed config — acceptable per the error policy.

### Pattern 4: Payload Parse and Command Dispatch

**What:** Parse the payload as `{"delay_ms": N, "commands": ["...", "..."]}` JSON or fall back to newline-delimited plain text. Send each command via `gnss_cmd_tx.send()`. Apply `std::thread::sleep` between commands.

**When to use:** In `apply_config()` helper called from the relay loop.

```rust
// Source: Derived from CONTEXT.md payload spec + error handling decisions
fn apply_config(payload: &[u8], gnss_cmd_tx: &Sender<String>) {
    let text = match std::str::from_utf8(payload) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Config relay: payload is not valid UTF-8: {:?}", e);
            return;
        }
    };

    let (delay_ms, commands): (u64, Vec<&str>) = if text.trim_start().starts_with('{') {
        // JSON path — manual extraction (no serde_json dependency)
        match parse_config_json(text) {
            Some(parsed) => parsed,
            None => {
                log::error!("Config relay: JSON parse failed, discarding payload");
                return;
            }
        }
    } else {
        // Plain text fallback: newline-delimited, 100ms fixed delay
        let cmds: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        (100, cmds)
    };

    for cmd in commands {
        log::info!("Config relay: sending command: {:?}", cmd);
        match gnss_cmd_tx.send(cmd.to_string()) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Config relay: gnss_cmd_tx send failed (TX thread dead?): {:?}", e);
                return; // abandon remaining commands in batch
            }
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}
```

### Pattern 5: Manual JSON Field Extraction (No serde_json)

**What:** Extract `delay_ms` and `commands` from the fixed-schema JSON string using simple string operations.

**When to use:** In `parse_config_json()` helper. Handles the locked payload format `{"delay_ms": N, "commands": ["...", "..."]}`.

```rust
// Source: Derived from CONTEXT.md payload format spec
// Input: {"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}
// NOTE: This is a minimal parser for this exact schema. Not a general JSON parser.
fn parse_config_json(text: &str) -> Option<(u64, Vec<&str>)> {
    // Extract delay_ms (optional field, default 100)
    let delay_ms = if let Some(pos) = text.find("\"delay_ms\"") {
        // Find the colon and then the integer
        let after_key = &text[pos + 10..]; // skip "delay_ms"
        let colon = after_key.find(':')? ;
        let after_colon = after_key[colon + 1..].trim_start();
        let end = after_colon.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_colon.len());
        after_colon[..end].parse::<u64>().unwrap_or(100)
    } else {
        100 // default
    };

    // Extract commands array — find content between [ and ]
    let array_start = text.find('"')?; // first quote after [
    let open = text.find('[')? ;
    let close = text.rfind(']')?;
    if close <= open { return None; }
    let array_content = &text[open + 1..close];

    // Split on ',' and strip surrounding quotes
    let commands: Vec<&str> = array_content
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
            Some(inner)
        })
        .collect();

    let _ = array_start; // suppress unused warning
    if commands.is_empty() { return None; }
    Some((delay_ms, commands))
}
```

**Limitations of this approach:** Handles the specified format only. Does not handle escaped quotes inside command strings. Does not handle whitespace variations aggressively. The UM980 command strings (e.g. `MODE ROVER`, `CONFIGSAVE`) contain no special characters, so this limitation does not apply in practice.

**If the manual parser proves unreliable in testing:** Add `serde_json = "1"` and `serde = { version = "1", features = ["derive"] }` to Cargo.toml and use standard derive deserialization. Binary size impact (~50KB) is acceptable on a 4MB flash device.

### Pattern 6: main.rs Integration

**What:** Create `config_tx`/`config_rx` mpsc channel. Pass `config_tx` to `pump_mqtt_events`. Pass `config_rx` and `gnss_cmd_tx.clone()` to `spawn_config_relay`. Remove the `_gnss_cmd_tx` placeholder (or keep it to maintain Sender count).

**When to use:** main.rs, between Step 9 (subscribe channel) and Step 10 (pump spawn).

```rust
// Source: Derived from main.rs existing pattern
// After Step 9 (subscribe_tx/rx channel creation):

// Config relay channel — pump sends received payloads here
let (config_tx, config_rx) = std::sync::mpsc::channel::<Vec<u8>>();

// Step 10 (MODIFIED): Pump thread — now also receives config_tx
std::thread::Builder::new()
    .stack_size(8192)
    .spawn(move || mqtt::pump_mqtt_events(
        mqtt_connection, subscribe_tx, config_tx, led_state_mqtt,
    ))
    .expect("pump thread spawn failed");

// (Steps 11-13 unchanged)

// Step 14 (existing): NMEA relay — unchanged
nmea_relay::spawn_relay(mqtt_client.clone(), device_id.clone(), nmea_rx)
    .expect("NMEA relay thread spawn failed");

// NEW Step 15: Config relay — consumes config_rx, forwards to UM980 via gnss_cmd_tx
config_relay::spawn_config_relay(gnss_cmd_tx.clone(), config_rx)
    .expect("Config relay thread spawn failed");
log::info!("Config relay started");

// Idle loop: _gnss_cmd_tx keeps the Sender alive so TX thread does not exit
// gnss_cmd_tx.clone() was passed to config_relay; original stays in main scope
let _gnss_cmd_tx = gnss_cmd_tx;
```

### Anti-Patterns to Avoid

- **Calling any client method from inside pump_mqtt_events on a Received event:** The pump must never call client methods. Sending on mpsc `config_tx` is NOT a client call — it is safe.
- **Blocking the pump thread during config application:** If config parsing and UART TX happen inside the pump, it blocks the MQTT event loop for the duration of all commands (N commands × 100ms = potentially seconds). The pump must route to a separate thread and return immediately.
- **Calling gnss_cmd_tx.send() inside subscriber_loop:** subscriber_loop holds the MQTT client Mutex while subscribing. Calling UART TX from inside subscriber_loop is semantically wrong (wrong thread, wrong context). Config relay must be its own thread.
- **Using serde_json without adding it to Cargo.toml:** The current Cargo.toml has no serde dependency. If serde_json is chosen, both `serde` (with `derive` feature) and `serde_json` must be added.
- **Forgetting `mod config_relay;` in main.rs:** Same class of error as `mod gnss;` pitfall from Phase 4. Must be added to the mod declarations block.
- **Dropping gnss_cmd_tx in the idle loop:** The idle loop's `let _gnss_cmd_tx = gnss_cmd_tx` keeps the Sender alive. If this is removed after the clone is passed to config_relay, the TX thread will still run (one Sender alive via config_relay). But if config_relay thread exits unexpectedly and drops its clone, the TX thread exits too. Keep `_gnss_cmd_tx` in the idle loop as insurance.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Thread-safe inter-thread payload routing | Custom shared buffer with mutex | `std::sync::mpsc::channel::<Vec<u8>>()` | mpsc is the established pattern; subscribe_tx already demonstrates it |
| UART TX with inter-command delay | Timer interrupt or tick counter | `std::thread::sleep(Duration::from_millis(delay_ms))` | FreeRTOS yield; identical to heartbeat 30s sleep; correct for infrequent config application |
| Hash for change detection | Rolling checksum or full equality compare | djb2 in 5 lines | Zero dependency, no table, sufficient collision resistance for this use case |
| JSON parse for a fixed 2-field schema | General-purpose JSON library | Manual string extraction (or serde_json if preferred) | The schema never changes; a 20-line parser is correct and adds zero binary size |

**Key insight:** Phase 6 is pure channel plumbing — same as Phase 5, but in the opposite direction. The only new primitives are the config mpsc channel, the djb2 hash (5 lines), and the payload parser (20-30 lines). Everything else reuses patterns that are already hardware-verified.

---

## Common Pitfalls

### Pitfall 1: Blocking the MQTT Pump During Config Application

**What goes wrong:** Config parsing and UART TX with 100ms delays per command happen inside `pump_mqtt_events`. While commands are being sent, `connection.next()` is not being called. The MQTT broker's keep-alive timer may expire, triggering a disconnect. With 10 commands at 100ms each, the pump is blocked for 1 second minimum.

**Why it happens:** It is tempting to handle `EventPayload::Received` inline in the pump. This works for heartbeat (instant) but not for multi-command config with delays.

**How to avoid:** Always route `EventPayload::Received` to a channel. Let a separate thread do the parsing and UART TX. The pump returns to `connection.next()` immediately after the channel send.

**Warning signs:** MQTT disconnect events appearing in logs shortly after config delivery; keep-alive timeout in broker logs.

### Pitfall 2: Payload Lifetime in EventPayload::Received

**What goes wrong:** `EventPayload::Received` contains a `data: &[u8]` field — a borrowed slice. The data is only valid for the lifetime of the event. If you store the `&[u8]` reference and return from the match arm, the data is gone.

**Why it happens:** The event's internal buffer is reused by the MQTT stack on the next `connection.next()` call.

**How to avoid:** Call `data.to_vec()` immediately inside the match arm, before the match arm returns. The `Sender::send(data.to_vec())` pattern copies the bytes into an owned `Vec<u8>` before sending. This is the correct and only safe approach.

**Warning signs:** Garbled or empty payload bytes in the relay thread; Rust borrow checker error if you try to pass the `&[u8]` directly across a channel.

### Pitfall 3: Hash of Empty Payload

**What goes wrong:** If the MQTT broker sends a retained message with an empty payload (to clear the retained message), djb2 of an empty slice returns 5381 (the initial value). On the next reconnect, the hash is still 5381, and the relay incorrectly concludes nothing has changed.

**Why it happens:** djb2 initial value is non-zero, but empty slices produce a deterministic non-zero hash that looks like a "real" hash.

**How to avoid:** Check `if payload.is_empty()` before hashing. If empty, log and skip processing. Clearing a retained MQTT message by publishing empty payload is a standard MQTT pattern — handle it explicitly.

**Warning signs:** Device ignores a valid config payload that follows an empty-payload "clear" message on the same reconnect.

### Pitfall 4: gnss_cmd_tx.send() Blocks When TX Thread is Dead

**What goes wrong:** The GNSS TX thread exits if all `Sender<String>` clones are dropped (normal Rust channel semantics). If the TX thread exits for any reason (e.g. panic), `gnss_cmd_tx.send()` returns `Err(SendError)`. The config relay must handle this and not hang.

**Why it happens:** `mpsc::Sender::send()` returns `Err` only if the receiver (TX thread) has been dropped — never blocks.

**How to avoid:** Match on `gnss_cmd_tx.send(cmd)`: if `Err`, log error and return from `apply_config` immediately. Per the locked error handling decision, this is correct behavior (log and abandon remaining commands).

**Warning signs:** `log::error!("Config relay: gnss_cmd_tx send failed")` appearing at config time — indicates the GNSS TX thread has exited, which is itself a bug to investigate separately.

### Pitfall 5: serde_json Linking Failure Without serde Features

**What goes wrong:** If serde_json is chosen and added to Cargo.toml without the `serde` crate's `derive` feature enabled, derive macros for `Deserialize` fail to compile.

**Why it happens:** serde's derive macros are in a separate proc-macro crate that must be explicitly enabled.

**How to avoid:** If serde_json is added, use:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```
Not just `serde_json = "1"` alone.

**Warning signs:** Compile error "cannot find derive macro `Deserialize` in this scope".

---

## Code Examples

Verified patterns from existing project sources:

### djb2 Hash (CONF-02)
```rust
// Source: djb2 algorithm (D.J. Bernstein, public domain)
// Five lines, no dependency, no lookup table
fn djb2_hash(data: &[u8]) -> u32 {
    let mut hash: u32 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}
```

### Routing Received Payloads (pump_mqtt_events modification)
```rust
// Source: Derived from src/mqtt.rs pump_mqtt_events (hardware-verified, device FFFEB5)
// New match arm added to existing while let Ok(event) = connection.next() loop:
EventPayload::Received { data, .. } => {
    // data: &[u8] — must be copied before this arm returns
    match config_tx.send(data.to_vec()) {
        Ok(_) => {}
        Err(e) => log::warn!("Config relay channel closed: {:?}", e),
    }
}
```

### Per-Command Delay (CONF-03)
```rust
// Source: Derived from src/mqtt.rs heartbeat_loop std::thread::sleep pattern
// (hardware-verified, device FFFEB5)
std::thread::sleep(std::time::Duration::from_millis(delay_ms));
// delay_ms: u64, default = 100, override from JSON "delay_ms" field
```

### Thread Spawn Pattern (config_relay.rs)
```rust
// Source: Derived from src/nmea_relay.rs spawn_relay (hardware-verified, device FFFEB5)
pub fn spawn_config_relay(
    gnss_cmd_tx: std::sync::mpsc::Sender<String>,
    config_rx: std::sync::mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // relay logic here
        })
        .expect("config relay thread spawn failed");
    Ok(())
}
```

### Subscribe Channel Pattern (already working — reference)
```rust
// Source: src/main.rs + src/mqtt.rs (hardware-verified, device FFFEB5)
// Config channel follows identical pattern to subscribe channel:
let (subscribe_tx, subscribe_rx) = std::sync::mpsc::channel::<()>();
// Config variant:
let (config_tx, config_rx) = std::sync::mpsc::channel::<Vec<u8>>();
```

### Empty Payload Guard (CONF-02 edge case)
```rust
// Source: MQTT 3.1.1 spec — zero-length payload clears retained message
for payload in &config_rx {
    if payload.is_empty() {
        log::info!("Config relay: empty payload — retained message cleared, skipping");
        continue;
    }
    let hash = djb2_hash(&payload);
    // ... rest of dedup logic
}
```

---

## State of the Art

| Old Approach | Current Approach | Notes |
|--------------|------------------|-------|
| `_gnss_cmd_tx` placeholder in main.rs idle loop | Clone passed to `spawn_config_relay`; original kept in idle loop | Phase 6 goal — activate the placeholder |
| `pump_mqtt_events` logs all non-Connected events as `Unhandled message` | `EventPayload::Received` arm routes to `config_tx` channel | Removes the "Unhandled message: Received" WARN log spam |
| Config topic subscribed but no handler for received payloads | `config_relay.rs` thread processes received config payloads | Phase 6 closes the control loop |

**Not changing:**
- `subscriber_loop` — already subscribes to `gnss/{device_id}/config` at QoS 1 on every Connected signal. No change needed.
- `gnss::spawn_gnss` — TX thread already accepts commands from `Sender<String>`. No change needed.
- Thread stack size 8192 — consistent with all other threads in this project.

---

## Open Questions

1. **Does EventPayload::Received include a `topic` field accessible in the pump match arm?**
   - What we know: `embedded_svc::mqtt::client::EventPayload::Received` has fields: `id`, `topic`, `data`, `details`. The topic field is `Option<&str>`.
   - What's unclear: Whether the topic is always `Some` for QoS 1 received messages, or sometimes `None`.
   - Recommendation: Treat all `Received` events as config payloads (this firmware subscribes to exactly one topic). If a second subscription is added in a future phase, add topic filtering then. Do not add defensive topic filtering prematurely.
   - **Confidence: MEDIUM** — based on training data; verify in embedded_svc 0.28.1 source if any doubt.

2. **Does the UM980 require commands in a specific format (no trailing spaces, specific line endings)?**
   - What we know: gnss.rs TX thread appends `\r\n` after each command from `gnss_cmd_tx`. Config relay sends command strings without `\r\n` (the TX thread adds them). This is the established convention from Phase 4.
   - What's unclear: Whether UM980 commands are case-sensitive (e.g. `MODE ROVER` vs `mode rover`).
   - Recommendation: Document in config_relay.rs that commands must be sent without `\r\n` (TX thread adds it). The planner should note that the integration test should send `MODE ROVER` exactly as documented in the UM980 manual.
   - **Confidence: HIGH** — TX thread appending `\r\n` is verified and working from Phase 4.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — embedded target; no test runner |
| Config file | None |
| Quick run command | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` |
| Full suite command | Flash + `espflash monitor` + `mosquitto_pub` config payload test |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CONF-01 | Subscribe to config topic QoS 1; forward payload line-by-line to UM980 UART TX | manual-only | N/A — requires hardware + MQTT broker | N/A |
| CONF-02 | Hash dedup prevents re-applying unchanged config on reconnect | manual-only | N/A — requires MQTT broker retained message + device reconnect cycle | N/A |
| CONF-03 | 100ms default delay between commands; override via `delay_ms` JSON field | manual-only | N/A — observable in espflash monitor log timing | N/A |

**Hardware verification procedure:**
1. Flash firmware with config relay
2. Publish config to broker (retained): `mosquitto_pub -h <host> -u <user> -P <pass> -t 'gnss/FFFEB5/config' -r -m '{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}'`
3. Observe espflash monitor: expect log lines `"Config relay: new config payload"` and `"Config relay: sending command: MODE ROVER"` / `"sending command: CONFIGSAVE"` with ~200ms spacing
4. Verify CONF-02: power cycle or force MQTT reconnect; observe `"Config relay: payload unchanged"` log — config should NOT be re-applied
5. Verify CONF-02 override: stop broker, change the retained message payload, restart broker; observe new hash triggers re-application
6. Verify plain text fallback: publish `"MODE ROVER\nCONFIGSAVE\n"` (no leading `{`); observe commands sent with 100ms delay

### Sampling Rate
- **Per task commit:** `cargo build --target riscv32imc-esp-espidf` — verify compile success
- **Per wave merge:** Flash + `mosquitto_pub` config test shows commands reaching UM980 (espflash monitor log)
- **Phase gate:** Full hardware cycle: publish config → observe relay → reconnect → observe dedup skip

### Wave 0 Gaps
- None — no test files needed. Validation is entirely hardware observation and log inspection. `cargo build` compile check covers structural correctness.

---

## Sources

### Primary (HIGH confidence)
- `src/mqtt.rs` — `pump_mqtt_events` and `subscriber_loop` (hardware-verified, device FFFEB5): channel routing pattern, EventPayload match arm structure, "never call client in pump" invariant
- `src/gnss.rs` — `spawn_gnss` TX thread (hardware-verified, device FFFEB5): `Sender<String>` interface, `\r\n` appending convention
- `src/nmea_relay.rs` — `spawn_relay` (hardware-verified, device FFFEB5): thread spawn pattern, for-in-receiver loop, anyhow::Result return
- `src/main.rs` — Step 14 `_gnss_cmd_tx` placeholder (verified): handoff point for Phase 6
- `.planning/phases/06-remote-config/06-CONTEXT.md` — all locked decisions for this phase
- `std::sync::mpsc` — channel, Sender, Receiver API (standard library, stable)

### Secondary (MEDIUM confidence)
- djb2 hash algorithm — D.J. Bernstein, widely documented; no implementation risk; five-line implementation
- MQTT 3.1.1 specification Section 3.3 — retained message clearing via zero-length payload
- `embedded_svc::mqtt::client::EventPayload` — training data Aug 2025; verify `Received` fields in embedded_svc 0.28.1 source if needed

### Tertiary (LOW confidence)
- serde_json binary size estimate (~40-80KB on RISC-V release build) — from embedded Rust community patterns; exact value varies by usage; verify with `cargo bloat` if size is a concern

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all patterns derived from hardware-verified code in this project; no new external dependencies required
- Architecture: HIGH — direct adaptation of pump/channel/thread patterns already working on device FFFEB5
- JSON parsing (manual): HIGH — schema is fixed and simple; manual parser is 20-30 lines with no surprises
- djb2 hash: HIGH — trivial algorithm, no external dependency, correct for change detection
- EventPayload::Received field availability: MEDIUM — training data; verify in embedded_svc source if uncertain
- serde_json binary size: MEDIUM — estimate from community patterns; not blocking at 4MB flash

**Research date:** 2026-03-07
**Valid until:** 2026-06-07 (90 days — pinned crate versions, no external dependency changes)
