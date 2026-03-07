# Phase 4: UART Pipeline - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Read a continuous stream of NMEA bytes from the UM980 over UART0 (GPIO16/17, 115200 baud 8N1),
assemble complete sentences line by line, and deliver them as `(sentence_type, raw_sentence)` tuples
to Phase 5 for MQTT relay. Also provide a TX channel so the stdin debug bridge (uart_bridge.rs)
and future Phase 6 MQTT config can write commands to the UM980 without holding a UART reference.

No NMEA field parsing, no checksum validation, no UM980 initialization commands — firmware relays,
consumers parse downstream.

</domain>

<decisions>
## Implementation Decisions

### Module and public API
- New module: `src/gnss.rs` — registered in `main.rs`
- Public function: `spawn_gnss(uart, tx_pin, rx_pin) -> (Sender<String>, Receiver<(String, String)>)`
  - `Sender<String>`: for callers to write command lines to the UM980 UART TX
  - `Receiver<(String, String)>`: stream of `(sentence_type, raw_sentence)` tuples for Phase 5
- `gnss.rs` owns `UartDriver` exclusively — no other module holds a reference to the driver

### Internal thread architecture
- **RX thread**: reads UART0, assembles sentences line by line (see sentence assembly), sends to:
  - stdout mirror (same behavior as the bridge's former Thread A)
  - unbounded `mpsc::channel::<(String, String)>` → caller (Phase 5)
- **TX thread**: receives `String` lines from an unbounded `mpsc::channel::<String>` → writes each
  line to UART TX. Callers hold the `Sender<String>` end; gnss.rs drains the receiver.
- Both threads follow the established pattern: `Builder::new().stack_size(8192).spawn()`

### uart_bridge.rs restructuring
- Thread A (UM980 → stdout) is **removed** — gnss.rs RX thread handles stdout mirroring
- Thread B (stdin → UM980) is **kept** but refactored: instead of writing to UART directly, it
  sends complete lines to the `Sender<String>` returned by `spawn_gnss`
- `uart_bridge.rs` becomes TX-only; it no longer holds or creates a `UartDriver`
- `spawn_bridge` signature changes to accept `Sender<String>` instead of UART peripherals

### Sentence assembly
- Line buffer approach: accumulate bytes into a fixed-size buffer until `\n` is received
- Strip trailing `\r\n` before processing
- A valid NMEA sentence starts with `$` — any line not starting with `$` (including empty lines)
  is logged at WARN level and dropped (not sent to the NMEA channel)
- No checksum validation — trust UM980 output; consumers validate if they need to

### Sentence type extraction
- Phase 4 extracts the sentence type from the first NMEA field
- e.g. `$GNGGA,123519,...` → type = `"GNGGA"` (strip leading `$`, take up to first `,`)
- Tuple sent to channel: `(String /* type */, String /* full raw sentence including $ */)`

### Pipeline channel
- Unbounded `mpsc::channel::<(String, String)>` — consistent with existing MQTT mpsc pattern
- NMEA sentence rate is low (~1 Hz per type); no backpressure needed

### UM980 mode / initialization
- Phase 4 sends **no initialization commands** to the UM980 at startup
- All UM980 configuration (including `MODE ROVER`, baud, output rate, etc.) is the responsibility
  of Phase 6 CONF — delivered via retained MQTT topic → TX channel → UART TX
- UM980 may be in any mode when Phase 4 starts; it just relays whatever arrives

### Claude's Discretion
- Buffer size for RX line accumulation (suggest 512 or 1024 bytes; NMEA max is 82 chars but
  proprietary sentences can be longer)
- Exact error handling if UART read returns an error (log + continue vs panic)
- Whether TX thread uses `NON_BLOCK` or blocking read on the mpsc receiver

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `uart_bridge.rs` Thread B (stdin line editor with backspace, redraw): keep as-is, redirect
  output to `Sender<String>` instead of `UartDriver::write()`
- `Arc<UartDriver>` thread-sharing pattern: already established (Phase 2) — but Phase 4 does NOT
  use Arc since gnss.rs owns the driver exclusively
- `std::sync::mpsc::channel()` unbounded pattern: used for subscribe_tx in mqtt.rs — same idiom
- `Builder::new().stack_size(8192).spawn()` thread pattern: universal in this codebase
- `NON_BLOCK` + 10ms sleep polling: established in uart_bridge Thread A — use same in RX thread

### Established Patterns
- Module-per-concern: `src/gnss.rs` declared in `main.rs` as `mod gnss;`
- `log::info!/warn!/error!` (never `println!`) — warn for malformed lines fits this pattern
- Initialization order in `main.rs` is MANDATORY; gnss UART init goes at Step 7 (currently
  uart_bridge) — same position, gnss replaces the bridge init

### Integration Points
- `main.rs` Step 7: `uart_bridge::spawn_bridge(...)` → replaced by `gnss::spawn_gnss(peripherals.uart0, peripherals.pins.gpio16, peripherals.pins.gpio17)`
- `main.rs`: `spawn_gnss` returns `(tx_sender, nmea_rx)`:
  - `tx_sender` passed to `uart_bridge::spawn_tx_thread(tx_sender)` (stdin bridge)
  - `tx_sender.clone()` held for Phase 6 MQTT config forwarder
  - `nmea_rx` passed to Phase 5 NMEA publisher (next phase)
- `config::UART_RX_BUF_SIZE` (4096): already defined, reuse for `UartDriver::new()` rx_fifo_size

</code_context>

<specifics>
## Specific Ideas

- `gnss.rs` becomes the UART hub: exclusive driver ownership, two internal threads (RX reader,
  TX writer), clean channel interfaces inward and outward
- The design means Phase 6 only needs a `Sender<String>` clone from main.rs — no UART access needed
- stdout mirroring in the RX thread preserves the dev workflow of watching raw UM980 output via
  `espflash monitor` while also relaying to MQTT

</specifics>

<deferred>
## Deferred Ideas

- UM980 MODE ROVER and output rate configuration — Phase 6 CONF via retained MQTT topic
- Checksum validation — Phase 4 out of scope; consumers handle if needed
- Bounded NMEA channel with backpressure — not needed at current sentence rates

</deferred>

---

*Phase: 04-uart-pipeline*
*Context gathered: 2026-03-04*
