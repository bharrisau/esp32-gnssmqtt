# Phase 4: UART Pipeline - Research

**Researched:** 2026-03-04
**Domain:** ESP-IDF Rust UART driver, thread architecture, NMEA sentence assembly, mpsc channels
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- New module: `src/gnss.rs` — registered in `main.rs`
- Public function: `spawn_gnss(uart, tx_pin, rx_pin) -> (Sender<String>, Receiver<(String, String)>)`
  - `Sender<String>`: for callers to write command lines to the UM980 UART TX
  - `Receiver<(String, String)>`: stream of `(sentence_type, raw_sentence)` tuples for Phase 5
- `gnss.rs` owns `UartDriver` exclusively — no other module holds a reference to the driver
- **RX thread**: reads UART0, assembles sentences line by line, sends to stdout mirror + unbounded `mpsc::channel::<(String, String)>` → caller (Phase 5)
- **TX thread**: receives `String` lines from an unbounded `mpsc::channel::<String>` → writes each line to UART TX. Callers hold the `Sender<String>` end; gnss.rs drains the receiver.
- Both threads follow the established pattern: `Builder::new().stack_size(8192).spawn()`
- Thread A (UM980 → stdout) **removed** from uart_bridge.rs — gnss.rs RX thread handles stdout mirroring
- Thread B (stdin → UM980) **kept** but refactored: sends complete lines to `Sender<String>` instead of writing to UART directly
- `uart_bridge.rs` becomes TX-only; no longer holds or creates a `UartDriver`
- `spawn_bridge` signature changes to accept `Sender<String>` instead of UART peripherals
- Line buffer approach: accumulate bytes into a fixed-size buffer until `\n` is received
- Strip trailing `\r\n` before processing
- A valid NMEA sentence starts with `$` — any line not starting with `$` (including empty lines) is logged at WARN level and dropped
- No checksum validation — trust UM980 output
- Sentence type extraction: `$GNGGA,123519,...` → type = `"GNGGA"` (strip leading `$`, take up to first `,`)
- Tuple sent to channel: `(String /* type */, String /* full raw sentence including $ */)`
- Unbounded `mpsc::channel` — consistent with existing MQTT mpsc pattern
- Phase 4 sends **no initialization commands** to the UM980 at startup

### Claude's Discretion

- Buffer size for RX line accumulation (suggest 512 or 1024 bytes; NMEA max is 82 chars but proprietary sentences can be longer)
- Exact error handling if UART read returns an error (log + continue vs panic)
- Whether TX thread uses `NON_BLOCK` or blocking read on the mpsc receiver

### Deferred Ideas (OUT OF SCOPE)

- UM980 MODE ROVER and output rate configuration — Phase 6 CONF via retained MQTT topic
- Checksum validation — Phase 4 out of scope; consumers handle if needed
- Bounded NMEA channel with backpressure — not needed at current sentence rates
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UART-01 | Device reads raw bytes from UM980 UART at 115200 baud 8N1 using a dedicated high-priority FreeRTOS task | UartDriver::new() with Config::new().baudrate(Hertz(115_200)) on UART0/GPIO16/17; NON_BLOCK polling loop in dedicated thread |
| UART-02 | Device accumulates bytes into complete NMEA sentences terminated by `\n`, correctly handling fragmented reads across multiple UART read calls | Fixed line buffer with byte-by-byte accumulation; `\n` detection triggers processing; buffer survives across multiple read() calls |
| UART-03 | Device extracts the sentence type from the NMEA prefix (e.g. `$GNGLL` → `GNGLL`) for use in MQTT topic construction | String slice after `$`, truncated at first `,`; yields sentence type as owned String |
| UART-04 | Checksum validation | **OUT OF SCOPE for Phase 4** — deferred per CONTEXT.md decisions |
</phase_requirements>

---

## Summary

Phase 4 is primarily a **refactoring + new module** task. The UartDriver API, thread patterns, and mpsc channel idioms are already established and verified in the codebase. The project uses `esp-idf-svc 0.51.0` / `esp-idf-hal 0.45.2`, which re-exports the HAL through the svc crate. The existing `uart_bridge.rs` is the ground-truth reference for UART initialization (`UartDriver::new()` with `Config::new().baudrate(Hertz(115_200)).rx_fifo_size(4096)`) and the NON_BLOCK polling pattern (read + 10ms sleep on zero bytes).

The core new work is: (1) move exclusive UartDriver ownership into `gnss.rs`, (2) add a byte-accumulation loop that detects `\n` and emits `(type, raw)` tuples on an mpsc channel, (3) add a TX draining thread consuming `Receiver<String>`, and (4) restructure `uart_bridge.rs` to become TX-only by accepting a `Sender<String>`. All of this maps cleanly to existing patterns already in production.

The only technically novel element is the NMEA sentence assembly state machine — a fixed-size line buffer that survives across fragmented reads. This is straightforward but must handle edge cases: buffer overflow (line too long), lines with no `$` prefix, and empty lines after `\r\n` stripping.

**Primary recommendation:** Implement `gnss.rs` by adapting `uart_bridge.rs` Thread A verbatim into the RX thread, then adding the `\n`-detection accumulator on top. Adapt Thread B into the TX thread by replacing `um980.write()` calls with `mpsc::Receiver::recv()` + `um980.write()`. The bridge restructuring is then a simple signature change — remove UART peripherals, accept `Sender<String>`, send into it instead of writing UART directly.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp-idf-svc` | `=0.51.0` | Re-exports esp-idf-hal; UartDriver accessible as `esp_idf_svc::hal::uart::UartDriver` | Project-pinned; already in use |
| `esp-idf-hal` | `=0.45.2` | UartDriver, Config, NON_BLOCK, Hertz, Peripheral trait | Project-pinned; ground truth in uart_bridge.rs |
| `std::sync::mpsc` | std | Unbounded channels for Sender<String> / Receiver<(String, String)> | Already used in mqtt.rs (subscribe_tx pattern) |
| `std::thread::Builder` | std | `.stack_size(8192).spawn()` | Universal pattern in this codebase |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `log` | `0.4` | `log::info!`, `log::warn!` for NMEA mirror + malformed line warnings | All non-println output |
| `esp_idf_svc::hal::delay::NON_BLOCK` | via svc | Zero-tick timeout for non-blocking UART read | RX polling loop when no bytes available |
| `anyhow` | `1` | `anyhow::Result<>` return type for `spawn_gnss` | Consistent with other spawn functions |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `NON_BLOCK` + 10ms sleep | `BLOCK` with timeout | BLOCK with small timeout (e.g. 10 ticks) avoids the separate sleep call, but NON_BLOCK + sleep is the established project pattern and avoids FreeRTOS watchdog trips |
| `mpsc::channel()` unbounded | `mpsc::sync_channel(64)` | Bounded would add backpressure — decided OUT OF SCOPE for Phase 4 |
| Fixed `[u8; N]` line buffer | `Vec<u8>` | Vec requires heap alloc and would grow unbounded on corrupt input; fixed buffer is safer on embedded |

**Installation:** No new dependencies required. All libraries are already in `Cargo.toml`.

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── gnss.rs          # NEW — exclusive UartDriver owner, RX+TX threads, mpsc channels
├── uart_bridge.rs   # MODIFIED — TX-only stdin bridge, accepts Sender<String>
├── main.rs          # MODIFIED — Step 7 calls spawn_gnss, passes Sender<String> to spawn_bridge
├── mqtt.rs          # UNCHANGED
├── wifi.rs          # UNCHANGED
├── led.rs           # UNCHANGED
├── config.rs        # UNCHANGED
└── device_id.rs     # UNCHANGED
```

### Pattern 1: UartDriver Initialization (from uart_bridge.rs — verified working)

**What:** Create UartDriver with 115200 baud and 4096-byte RX FIFO on UART0/GPIO16/GPIO17.
**When to use:** gnss.rs spawn_gnss function — replaces the identical init in uart_bridge.rs.

```rust
// Source: src/uart_bridge.rs (verified on device FFFEB5)
use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;

let uart = UartDriver::new(
    uart,
    tx_pin,    // GPIO16 — UM980 TX line
    rx_pin,    // GPIO17 — UM980 RX line
    Option::<AnyIOPin>::None,
    Option::<AnyIOPin>::None,
    &Config::new()
        .baudrate(Hertz(115_200))
        .rx_fifo_size(crate::config::UART_RX_BUF_SIZE as usize),
)?;
// NOTE: Do NOT wrap in Arc<>. gnss.rs moves the driver into RX thread directly.
// TX thread gets a clone — UartDriver is NOT Clone, so use split() or a Mutex.
// See Pattern 3 for the correct approach.
```

### Pattern 2: NON_BLOCK Polling Loop (from uart_bridge.rs — verified working)

**What:** Read available bytes without blocking; sleep 10ms when FIFO is empty. Avoids FreeRTOS watchdog starvation.
**When to use:** gnss.rs RX thread body.

```rust
// Source: src/uart_bridge.rs Thread A (verified on device FFFEB5)
let mut buf = [0u8; 256];
loop {
    match uart_rx.read(&mut buf, NON_BLOCK) {
        Ok(n) if n > 0 => {
            // process bytes
        }
        _ => std::thread::sleep(std::time::Duration::from_millis(10)),
    }
}
```

### Pattern 3: UartDriver Thread Sharing — Exclusive Ownership + Move

**What:** UartDriver is `Send` but not `Clone`. To share between RX and TX threads without Arc, use the driver's `split()` method if available, OR move the whole driver into the RX thread and give TX thread access via `Arc<Mutex<UartDriver>>`. The project has precedent for `Arc<UartDriver>` (Phase 2, CONN-07 verified), but `gnss.rs` decided NOT to use Arc. The correct approach: use `UartDriver` directly — move into RX thread (reads), give TX thread an `Arc<Mutex<UartDriver>>`.

**CRITICAL FINDING (HIGH confidence from project code):** The project used `Arc<UartDriver>` in Phase 2 to share across Thread A (RX) and Thread B (TX) in `uart_bridge.rs` (line 38: `let um980 = Arc::new(um980);`). This worked because UartDriver is Send. Phase 4 context says "gnss.rs owns UartDriver exclusively — no Arc" — this means Arc is avoided for ownership semantics (single owner conceptually), but the implementation still needs both threads to access the driver. The practical solution used by the existing code is `Arc<UartDriver>` with direct `.read()` and `.write()` calls (UartDriver's read/write take `&self`, not `&mut self`).

```rust
// Source: src/uart_bridge.rs (verified working pattern)
let uart = Arc::new(UartDriver::new(/* ... */)?);

// RX thread gets a clone of the Arc
let uart_rx = Arc::clone(&uart);
std::thread::Builder::new().stack_size(8192).spawn(move || {
    // uart_rx.read(...)
});

// TX thread uses the original Arc (or another clone)
std::thread::Builder::new().stack_size(8192).spawn(move || {
    // uart.write(...)
});
```

**Note:** "gnss.rs owns exclusively" means no OTHER MODULE holds a reference, not that Arc is forbidden internally. The existing uart_bridge.rs uses exactly this pattern. Use it.

### Pattern 4: NMEA Sentence Accumulator

**What:** Fixed line buffer accumulates bytes from fragmented reads. On `\n`, process the complete line.
**When to use:** Inside the RX thread, wrapping the NON_BLOCK polling loop.

```rust
// Source: Derived from project patterns + verified NMEA protocol knowledge
// NMEA max standard sentence: 82 chars. Proprietary (e.g. UM980 PVTSLN): up to ~200 bytes.
// Use 512 bytes to be safe — fits UM980 proprietary output.
let mut line_buf = [0u8; 512];
let mut line_len: usize = 0;

// Inside the byte processing loop:
for &byte in &read_buf[..n] {
    if byte == b'\n' {
        // Strip trailing \r if present
        let end = if line_len > 0 && line_buf[line_len - 1] == b'\r' {
            line_len - 1
        } else {
            line_len
        };
        let line = &line_buf[..end];

        // Mirror to stdout (development visibility)
        let _ = std::io::stdout().write_all(line);
        let _ = std::io::stdout().write_all(b"\n");

        // Process if NMEA sentence
        if line.first() == Some(&b'$') && line.len() > 1 {
            if let Ok(s) = std::str::from_utf8(line) {
                // Extract sentence type: everything between '$' and first ','
                let sentence_type = s[1..].split(',').next().unwrap_or("").to_string();
                let raw = s.to_string();
                let _ = nmea_tx.send((sentence_type, raw));
            }
        } else if !line.is_empty() {
            log::warn!("GNSS: non-NMEA line dropped: {:?}", std::str::from_utf8(line));
        }
        line_len = 0; // reset buffer
    } else if line_len < line_buf.len() {
        line_buf[line_len] = byte;
        line_len += 1;
    } else {
        // Buffer overflow — line too long; reset and warn
        log::warn!("GNSS: RX line buffer overflow, discarding {} bytes", line_len);
        line_len = 0;
    }
}
```

### Pattern 5: TX Thread — mpsc Receiver Drain + UART Write

**What:** Drain Receiver<String>, write each line to UART with CRLF appended.
**When to use:** gnss.rs TX thread — handles commands from uart_bridge.rs (stdin) and future Phase 6 MQTT config.

```rust
// Source: Derived from mqtt.rs subscriber_loop pattern + uart_bridge.rs write pattern
// tx_rx is Receiver<String> from the channel returned by spawn_gnss
for line in &tx_rx {
    let _ = uart_tx.write(line.as_bytes());
    let _ = uart_tx.write(b"\r\n");
}
log::error!("GNSS TX channel closed — TX thread exiting");
```

Note: `for x in &receiver` blocks until a message arrives (`recv()` semantics). This is correct for the TX thread — it should sleep until there's work to do. No need for NON_BLOCK here.

### Pattern 6: spawn_gnss Public API

**What:** Public function that initializes UART, spawns threads, returns channel endpoints.
**When to use:** Called from main.rs Step 7.

```rust
// Source: Derived from project conventions (see mqtt.rs, uart_bridge.rs)
use std::sync::mpsc::{self, Receiver, Sender};
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::Uart;

pub fn spawn_gnss(
    uart: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
) -> anyhow::Result<(Sender<String>, Receiver<(String, String)>)> {
    // Initialize UartDriver (exclusive, no Arc at module boundary)
    // Create two mpsc channels
    // Spawn RX thread (moves uart_rx Arc clone)
    // Spawn TX thread (moves uart_tx Arc clone)
    // Return (cmd_tx, nmea_rx)
    //   cmd_tx: callers send command strings → TX thread → UART
    //   nmea_rx: callers recv NMEA tuples from RX thread
}
```

### Pattern 7: uart_bridge.rs Restructuring

**What:** Replace UART peripherals with `Sender<String>` in spawn_bridge. Thread B sends to channel instead of writing UART.
**When to use:** uart_bridge.rs refactor.

```rust
// Source: Derived from existing Thread B in uart_bridge.rs
pub fn spawn_bridge(cmd_tx: std::sync::mpsc::Sender<String>) -> anyhow::Result<()> {
    // Spawn Thread B only (Thread A removed — gnss.rs handles UM980 → stdout)
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // ... same line editor logic ...
            // On Enter (was: um980.write(&line[..line_len]); um980.write(b"\r\n"))
            // Now: cmd_tx.send(String::from_utf8_lossy(&line[..line_len]).into_owned())
            //           .unwrap_or_else(|e| log::warn!("GNSS cmd channel closed: {:?}", e));
        })
        .unwrap();
    Ok(())
}
```

### Pattern 8: main.rs Integration — Step 7 Replacement

```rust
// Source: Derived from existing main.rs Step 7 + project conventions
// Step 7: GNSS pipeline — exclusive UART ownership, RX + TX threads
let (gnss_cmd_tx, nmea_rx) = gnss::spawn_gnss(
    peripherals.uart0,
    peripherals.pins.gpio16,  // TX to UM980 (matches Phase 2 verified wiring)
    peripherals.pins.gpio17,  // RX from UM980
)
.expect("GNSS init failed");
log::info!("GNSS pipeline started");

// stdin bridge: sends typed commands to GNSS TX thread
uart_bridge::spawn_bridge(gnss_cmd_tx.clone())
    .expect("UART bridge init failed");
log::info!("UART bridge started");

// nmea_rx passed to Phase 5 (next phase); gnss_cmd_tx.clone() held for Phase 6
```

### Anti-Patterns to Avoid

- **Calling `Arc<UartDriver>` with `&mut self` methods**: UartDriver.read() and write() take `&self` — no mutex needed for concurrent access. Do not wrap in `Mutex<UartDriver>`.
- **Using `println!` instead of `log::info!`**: Project convention is strict — always use `log::` macros. Only `std::io::stdout().write_all()` for raw byte mirroring.
- **Sending `\r\n` in the NMEA tuple**: Strip CRLF before sending to the channel. The raw sentence sent to Phase 5 should be the clean NMEA string including `$` but without line terminators.
- **Creating `UartDriver` in `uart_bridge.rs`**: After this phase, uart_bridge MUST NOT create or own a UartDriver. UART ownership lives exclusively in gnss.rs.
- **Panicking on UART read errors**: The RX thread runs forever. UART errors (transient, noise) should be logged at WARN and the loop should continue — not panic or exit.
- **Moving `Receiver<(String, String)>` without registering `mod gnss`**: Add `mod gnss;` to main.rs before the module is usable.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Thread-safe UART sharing | Custom locking scheme | `Arc<UartDriver>` (UartDriver is Send; read/write take &self) | Already used and verified in Phase 2 |
| Command queuing | Custom ring buffer | `std::sync::mpsc::channel::<String>()` | Already established mpsc pattern; handles blocking/wakeup correctly |
| Cross-thread byte streaming | Custom shared memory | `std::sync::mpsc::channel::<(String, String)>()` | Same pattern as subscribe_tx in mqtt.rs |
| String from bytes | Manual UTF-8 decode | `std::str::from_utf8()` + `.to_string()` | Handles invalid UTF-8 safely (UM980 ASCII output) |

**Key insight:** All concurrency primitives needed are in `std`. No new crates required. The UART driver already has the correct `&self` API for concurrent access.

---

## Common Pitfalls

### Pitfall 1: UartDriver is NOT Clone — Thread Sharing Requires Arc

**What goes wrong:** Attempting to move UartDriver into two threads without wrapping in Arc results in a compile error: `use of moved value`.
**Why it happens:** UartDriver owns underlying C driver resources and does not implement Clone.
**How to avoid:** Wrap in `Arc<UartDriver>` immediately after creation in `spawn_gnss`. Clone the Arc for each thread. This is identical to what `uart_bridge.rs` does (verified working).
**Warning signs:** Compiler error "use of moved value: `uart`" when spawning second thread.

### Pitfall 2: Line Buffer Overflow on UM980 Proprietary Sentences

**What goes wrong:** UM980 outputs proprietary sentences (e.g., `$PVTSLN`) that are significantly longer than the 82-byte NMEA standard maximum. A 128-byte buffer silently truncates these.
**Why it happens:** NMEA standard says 82 bytes; UM980 extends this with proprietary output.
**How to avoid:** Use 512-byte line buffer (decided as Claude's discretion area). On overflow, log WARN and reset — never silently corrupt.
**Warning signs:** WARN log showing truncated sentences; Phase 5 MQTT receiving malformed payloads.

### Pitfall 3: Empty Lines After CRLF

**What goes wrong:** NMEA output uses `\r\n` terminators. The `\r` becomes the last byte before `\n`. After stripping, if the buffer is empty, logging "non-NMEA line dropped" fills the log with noise.
**Why it happens:** Some tools/devices send double CRLF or blank lines between sentences.
**How to avoid:** Check `if !line.is_empty()` before logging the non-NMEA WARN. Empty lines after stripping = silently skip.
**Warning signs:** Flood of WARN messages for empty lines in espflash monitor.

### Pitfall 4: mpsc Channel Closed — TX Thread Exit

**What goes wrong:** If `Sender<String>` is dropped in main.rs (or all clones dropped), the TX thread's `for line in &rx` exits. The TX thread then exits permanently. Commands can no longer be sent to UM980.
**Why it happens:** Rust mpsc semantics: when all Senders are dropped, Receiver iteration ends.
**How to avoid:** main.rs must hold at least one `Sender<String>` clone alive. The clone passed to uart_bridge must be kept alive (the bridge thread holds it). The clone reserved for Phase 6 should also be stored (even as `_gnss_cmd_tx` in main's idle loop scope).
**Warning signs:** `log::error!("GNSS TX channel closed")` appearing in monitor output.

### Pitfall 5: stdout Mirror Interleaving with espflash monitor

**What goes wrong:** Raw UART bytes written to stdout may interleave with structured log lines (from `log::info!`), producing garbled output in espflash monitor.
**Why it happens:** The log backend and stdout share the same USB-JTAG output stream. Both write bytes concurrently across threads.
**How to avoid:** This is a dev-tooling cosmetic issue, not a functional one. Document as known behavior. For the RX thread, mirror complete lines (`write_all(line); write_all(b"\n")`) rather than individual bytes — reduces the interleave window vs. byte-by-byte writes.
**Warning signs:** Log lines appearing mid-NMEA sentence in monitor. Acceptable; cannot be fully eliminated without a mutex around all stdout output.

### Pitfall 6: `mod gnss` Declaration Missing from main.rs

**What goes wrong:** Compiler cannot find `gnss::spawn_gnss`. Error: `error[E0433]: failed to resolve: could not find gnss in the crate root`.
**Why it happens:** Rust requires explicit `mod gnss;` declaration in main.rs.
**How to avoid:** Add `mod gnss;` to the mod declarations at the top of main.rs alongside `mod uart_bridge;`.
**Warning signs:** Compile error referencing gnss module not found.

---

## Code Examples

Verified patterns from the existing codebase:

### Unbounded mpsc Channel Creation (from mqtt.rs)
```rust
// Source: src/main.rs Step 9 (verified on device FFFEB5)
use std::sync::mpsc::{Receiver, Sender};
let (subscribe_tx, subscribe_rx) = std::sync::mpsc::channel::<()>();
// For gnss:
let (nmea_tx, nmea_rx) = std::sync::mpsc::channel::<(String, String)>();
let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();
```

### Thread Spawn Pattern (from main.rs — verified)
```rust
// Source: src/main.rs (universal pattern, verified on device FFFEB5)
std::thread::Builder::new()
    .stack_size(8192)
    .spawn(move || {
        // thread body — captures moved values
    })
    .expect("thread name spawn failed");
```

### Arc-Based Driver Sharing (from uart_bridge.rs — verified)
```rust
// Source: src/uart_bridge.rs (verified on device FFFEB5)
use std::sync::Arc;
let um980 = Arc::new(UartDriver::new(/* ... */)?);
let um980_rx = Arc::clone(&um980);
// RX thread uses um980_rx
// TX thread uses um980 (original)
```

### NMEA Sentence Type Extraction
```rust
// Source: Derived from NMEA-0183 protocol standard
// Input: "$GNGGA,123519,..." or "$PVTSLN,..."
// Output: "GNGGA" or "PVTSLN"
let sentence_type: String = if s.starts_with('$') {
    s[1..].split(',').next().unwrap_or("UNKNOWN").to_string()
} else {
    "UNKNOWN".to_string()
};
```

### UART Write with CRLF (from uart_bridge.rs — verified)
```rust
// Source: src/uart_bridge.rs Thread B (verified on device FFFEB5)
let _ = uart.write(line.as_bytes());
let _ = uart.write(b"\r\n");
// UM980 requires CRLF line termination for command processing
```

---

## State of the Art

| Old Approach | Current Approach | Notes |
|--------------|------------------|-------|
| uart_bridge.rs owns UART + does both RX and TX | gnss.rs owns UART exclusively; uart_bridge.rs becomes TX-only stdin relay | Phase 4 goal |
| Thread A: raw byte mirror to stdout | RX thread: line-assembled NMEA + stdout mirror + mpsc channel | Adds sentence assembly on top of byte forwarding |
| Thread B: writes directly to UartDriver | Thread B: sends String to Sender<String> → gnss.rs TX thread writes UART | Decouples stdin from UART ownership |

**Not deprecated:**
- NON_BLOCK polling with 10ms sleep — still correct approach for FreeRTOS watchdog avoidance
- `Builder::new().stack_size(8192).spawn()` — still the only way to control thread stack on ESP32
- `Arc<UartDriver>` for inter-thread sharing — still necessary; UartDriver not Clone

---

## Open Questions

1. **Does `UartDriver::read()` take `&self` or `&mut self`?**
   - What we know: The existing `uart_bridge.rs` uses `Arc<UartDriver>` and calls `um980_rx.read(...)` from one thread while the TX thread holds another Arc clone — this implies `&self`. Arc requires inner type to be Sync for shared references across threads.
   - What's unclear: Official docs were unreachable during research. The working code is the authoritative source.
   - Recommendation: Treat as `&self` (confirmed by Arc usage). If compilation fails, switch to `Arc<Mutex<UartDriver>>`.
   - **Confidence: HIGH** — working code in uart_bridge.rs proves the `Arc` (non-Mutex) pattern works.

2. **Line buffer size: 512 bytes vs 1024 bytes?**
   - What we know: NMEA standard max is 82 bytes. UM980 proprietary sentences (PVTSLN, BESTNAV, etc.) are longer but exact max is not documented.
   - What's unclear: UM980 longest possible proprietary sentence length.
   - Recommendation: Use 512 bytes — covers all known UM980 output with room to spare, avoids wasting stack space (buffer lives on stack in thread with 8192-byte stack limit).

3. **TX thread blocking strategy: `recv()` vs `try_recv()` + sleep?**
   - What we know: `for x in &rx` uses blocking `recv()` — thread sleeps until message arrives, no CPU waste.
   - Recommendation: Use `for line in &cmd_rx` (blocking). TX commands are infrequent; blocking is correct. No watchdog concern because the thread is legitimately blocked on a channel, not spinning.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None detected — embedded target; no test runner exists |
| Config file | None |
| Quick run command | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "error\|warning"` |
| Full suite command | Flash + espflash monitor observation on device FFFEB5 |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UART-01 | UartDriver reads bytes from UM980 at 115200 baud | manual-only | N/A — requires hardware | ❌ Wave 0 N/A |
| UART-02 | Fragmented reads assembled into complete sentences | manual-only | N/A — requires hardware | ❌ Wave 0 N/A |
| UART-03 | Sentence type extracted correctly from NMEA prefix | unit (host) | `cargo test --test nmea_type_extraction` (if added) | ❌ Wave 0 gap |
| UART-04 | Checksum validation | OUT OF SCOPE | — | — |

**Note on Rust embedded testing:** `esp-idf-sys` does not support `cargo test` (cross-compiled target, no test harness). Host-side unit tests for pure logic (sentence type extraction) are possible with `#[cfg(test)]` + `cargo test` IF the test code uses no esp-idf imports. The sentence type extraction function is pure Rust string logic and CAN be tested on host.

### Sampling Rate
- **Per task commit:** `cargo build --target riscv32imc-esp-espidf` — verify compile success
- **Per wave merge:** `cargo build` + flash + espflash monitor showing NMEA lines appearing + sentence type in logs
- **Phase gate:** Hardware verification — NMEA tuples visible in logs with correct sentence type extraction, no spurious WARN lines on clean UM980 output

### Wave 0 Gaps
- [ ] Optional: `tests/nmea_type_extraction.rs` — host-side unit test for the sentence type parsing logic (pure Rust, no esp-idf imports). Covers UART-03 automated validation.
- [ ] No test framework install needed — `cargo test` is available for host-side tests if added.

**If no unit tests are added:** "None — phase validation is manual hardware observation (embedded target, no test harness)."

---

## Sources

### Primary (HIGH confidence)
- `src/uart_bridge.rs` (project codebase, verified on device FFFEB5) — UartDriver::new() with Config, NON_BLOCK polling loop, Arc sharing, Thread A/B patterns, write() with CRLF
- `src/main.rs` (project codebase, verified on device FFFEB5) — mpsc::channel() pattern, Builder::new().stack_size(8192).spawn(), initialization order
- `src/mqtt.rs` (project codebase, verified on device FFFEB5) — mpsc Sender/Receiver usage, thread spawn patterns, for-in-receiver blocking pattern

### Secondary (MEDIUM confidence)
- [esp-idf-hal GitHub issue #421](https://github.com/esp-rs/esp-idf-hal/issues/421) — confirmed no built-in line-reading abstraction; manual accumulation required
- [ESP32 Standard Library Embedded Rust: UART Communication (dev.to)](https://dev.to/theembeddedrustacean/esp32-standard-library-embedded-rust-uart-communication-1413) — confirmed UartDriver::new() parameter order, BLOCK/NON_BLOCK as timeout values, read(&mut buf, NON_BLOCK) pattern

### Tertiary (LOW confidence)
- WebSearch results for UartDriver method signatures — could not fetch official docs directly; supplemented by working project code (HIGH confidence)

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries already in Cargo.toml, in-use and verified
- Architecture: HIGH — all patterns derived from existing verified-working project code
- Pitfalls: HIGH for Arc/Clone and channel lifetime; MEDIUM for UM980 proprietary sentence max length (no official size spec found)

**Research date:** 2026-03-04
**Valid until:** 2026-06-04 (90 days — stable embedded HAL, pinned versions, no external dependency changes expected)
