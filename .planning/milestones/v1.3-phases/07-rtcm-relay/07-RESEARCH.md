# Phase 7: RTCM Relay - Research

**Researched:** 2026-03-07
**Domain:** RTCM3 binary protocol parsing, mixed byte-stream state machines, ESP-IDF MQTT buffer sizing, Rust embedded firmware
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| RTCM-01 | gnss.rs RX thread handles mixed NMEA+RTCM byte stream via `RxState` state machine (Idle / NmeaLine / RtcmHeader / RtcmBody); 1029-byte RTCM frame buffer | State machine design documented; 1029 = 1023 payload + 3 header + 3 CRC |
| RTCM-02 | RTCM3 frames detected by 0xD3 preamble, 10-bit length parsed, CRC-24Q verified; invalid frames trigger resync (scan for next 0xD3/$) | RTCM3 frame format fully documented; CRC-24Q polynomial 0x864CFB confirmed |
| RTCM-03 | Verified RTCM frames delivered via bounded `sync_channel(32)` as `(u16, Vec<u8>)` (message_type, complete_frame) to `rtcm_relay.rs` | Channel pattern matches existing nmea_relay pattern; `Vec<u8>` for heap allocation of variable-length frames |
| RTCM-04 | Raw RTCM frames published to `gnss/{device_id}/rtcm/{message_type}` at QoS 0, retain=false; MQTT `out_buffer_size` bumped to 2048 | `MqttClientConfiguration.out_buffer_size` field confirmed in esp-idf-svc 0.51.0; MSM7 frames up to 1029 bytes fit in 2048 |
| RTCM-05 | `pump_mqtt_events` routes by topic (`/config` vs `/ota/trigger`) — fixes latent bug where all `Received` events route to `config_tx` | `EventPayload::Received { topic, .. }` field confirmed as `Option<&str>`; topic matching pattern documented |
</phase_requirements>

---

## Summary

Phase 7 extends the existing NMEA relay pipeline to handle RTCM3 binary frames that arrive interleaved with NMEA sentences on the UM980 UART. The UM980 outputs a mixed byte stream: NMEA sentences begin with `$` and end with `\n`; RTCM3 frames begin with `0xD3` and are self-delimiting via a 10-bit length field and 24-bit CRC. The two protocols are byte-level distinct and can be discriminated purely by the first byte of each framing unit.

The core change is in `gnss.rs`: the current line-based assembler that recognises only `$`-prefixed NMEA sentences must become a state machine that also recognises `0xD3`-prefixed RTCM3 frames. Once a complete RTCM3 frame is received and CRC-verified, it is forwarded to a new `rtcm_relay.rs` module via a bounded `sync_channel`, which publishes it as raw binary to MQTT. The NMEA relay path is unchanged.

A second change fixes a latent bug in `mqtt.rs`: `pump_mqtt_events` currently routes ALL `EventPayload::Received` events to `config_tx` regardless of topic. This must be corrected to inspect the `topic` field and route `/config` payloads to `config_tx` and ignore (or route separately) `/ota/trigger` payloads. This prevents a retained `/config` message from being forwarded as a UM980 command when an OTA trigger fires.

**Primary recommendation:** Implement a four-state `RxState` enum in gnss.rs, keep the NMEA relay unchanged, add rtcm_relay.rs mirroring nmea_relay.rs, bump `out_buffer_size` to 2048 in mqtt_connect, and fix topic dispatch in pump_mqtt_events.

---

## RTCM3 Protocol Facts

### Frame Structure (HIGH confidence — confirmed by multiple sources)

```
Byte 0:    0xD3           — preamble (fixed, 1 byte)
Bytes 1-2: 0b000000xx xxxxxxxx — 6 reserved bits (must be zero) + 10-bit message length
Bytes 3 to 3+length-1:   message payload
Bytes 3+length to end:   24-bit CRC-24Q (3 bytes, big-endian)
```

Total frame size = 3 (header) + length (payload) + 3 (CRC) = **length + 6 bytes**

Payload length field is 10 bits → maximum payload = 1023 bytes → maximum total frame = **1029 bytes**.

### Message Type Field (HIGH confidence)

The first 12 bits of the payload are the message type number. To extract:
```rust
let msg_type: u16 = ((payload[0] as u16) << 4) | ((payload[1] as u16) >> 4);
```

### CRC-24Q Algorithm (HIGH confidence — multiple implementations confirm)

- Polynomial: 0x864CFB
- Initial value: 0x000000
- Input/output reflected: false
- No XOR out

Standard implementation (no external crate needed — 24 lines):

```rust
fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        crc ^= (byte as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x1000000 != 0 {
                crc ^= 0x864CFB;
            }
        }
    }
    crc & 0xFFFFFF
}
```

CRC is computed over **all bytes from 0xD3 preamble through end of payload** (header + payload, not including the CRC bytes themselves).

### MSM7 Frame Sizes (MEDIUM confidence — practical observations)

MSM7 messages (1077 GPS, 1087 GLONASS, 1097 Galileo, 1107 SBAS, 1117 QZSS, 1127 BeiDou) are multi-constellation full-precision messages. Practical sizes:

| Condition | Typical payload size | Total frame |
|-----------|---------------------|-------------|
| Single constellation, few SVs | 50-150 bytes | 56-156 bytes |
| Dual constellation, 20 SVs | 200-400 bytes | 206-406 bytes |
| All constellations, 40+ SVs | 600-1000 bytes | 606-1006 bytes |
| Theoretical maximum | 1023 bytes | 1029 bytes |

Requirements specify "up to 1029 bytes" — the 1029-byte buffer in RTCM-01 is correct and covers the absolute maximum.

### UM980 RTCM Output (MEDIUM confidence — confirmed via community sources)

The UM980 outputs RTCM3 on the same COM port as NMEA when configured via MQTT retained config. Standard RTCM enable commands (forwarded via existing config relay):
- `RTCM1077 1` — GPS MSM7 at 1 Hz
- `RTCM1087 1` — GLONASS MSM7 at 1 Hz
- `RTCM1097 1` — Galileo MSM7 at 1 Hz
- `RTCM1127 1` — BeiDou MSM7 at 1 Hz

At 115200 baud with 1Hz RTCM + standard NMEA, UART load remains well under capacity (RTCM MSM7 multi-constellation at 1Hz adds ~15-25% UART load — within margin).

---

## Existing Code Architecture (from codebase read)

### Current gnss.rs State (lines 82-143)

The existing RX thread is line-oriented: it accumulates bytes until `\n`, then dispatches `$`-prefixed lines as NMEA. Non-`$`, non-empty lines are logged and dropped. It has no concept of binary framing.

**Key constraint:** `0xD3` will never arrive as the first byte of a newline-terminated NMEA sentence. When RTCM is enabled, `0xD3` bytes will appear mid-stream and the current accumulator will absorb them into `line_buf`, logging a "non-NMEA line dropped" warning for any embedded binary that happens to contain `\n`.

**Current buffer:** `line_buf = [0u8; 512]` — too small for RTCM MSM7 (up to 1029 bytes). Must be replaced with the state machine approach.

### Current mqtt.rs Bug (lines 94-99)

```rust
EventPayload::Received { data, .. } => {
    match config_tx.send(data.to_vec()) { ... }
}
```

The `..` ignores `topic`. All received messages — including future `/ota/trigger` — are forwarded to `config_tx`, which feeds `config_relay.rs`, which feeds the UM980 UART. An OTA trigger payload like `{"url":"...","sha256":"..."}` would be sent as a UM980 command. This is the RTCM-05 bug.

### `EventPayload::Received` Field Signature (HIGH confidence — verified in embedded-svc 0.28.1 source)

```rust
EventPayload::Received {
    id: MessageId,
    topic: Option<&'a str>,   // None on chunked subsequent events
    data: &'a [u8],
    details: Details,
}
```

Topic discrimination pattern:
```rust
EventPayload::Received { topic, data, .. } => {
    let topic_str = topic.unwrap_or("");
    if topic_str.ends_with("/config") {
        let _ = config_tx.send(data.to_vec());
    }
    // /ota/trigger handled in Phase 8; silently ignored for now
}
```

### `MqttClientConfiguration` Buffer Fields (HIGH confidence — verified in esp-idf-svc 0.51.0 source)

```rust
pub struct MqttClientConfiguration<'a> {
    pub buffer_size: usize,       // default 0 (ESP-IDF uses 1024)
    pub out_buffer_size: usize,   // default 0 (falls back to buffer_size)
    ...
}
```

Setting `out_buffer_size: 2048` in `mqtt_connect` covers any RTCM frame up to 1029 bytes plus MQTT fixed header overhead (~5 bytes) and topic string (`gnss/FFFEB5/rtcm/1077` = 22 bytes). 2048 provides a comfortable margin.

---

## Standard Stack

### Core (no new dependencies required)

| Component | Version | Purpose | Why |
|-----------|---------|---------|-----|
| esp-idf-svc | =0.51.0 (pinned) | MQTT client, UART | Already in project |
| esp-idf-hal | =0.45.2 (pinned) | UartDriver | Already in project |
| embedded-svc | =0.28.1 (pinned) | EventPayload, QoS types | Already in project |
| std::sync::mpsc | stdlib | sync_channel for RTCM relay | Already used for NMEA |

**No new Cargo dependencies.** CRC-24Q is 24 lines of pure Rust with no external crate needed.

### Alternatives Considered

| Instead of | Could Use | Why Not |
|------------|-----------|---------|
| Hand-rolled CRC-24Q | `crc` crate | No dependency needed; algorithm is simple and well-specified; avoids dependency management overhead |
| `sync_channel(32)` for RTCM | `sync_channel(8)` | RTCM frames at 1Hz with 4 constellations = 4 frames/sec max; 32 slots matches NMEA channel; consistent |
| `Vec<u8>` per RTCM frame | Fixed array | Frame size is variable (6-1029 bytes); heap allocation acceptable for 1-4 frames/sec |

---

## Architecture Patterns

### RxState State Machine Design

Replace the current line-buffer-based loop in gnss.rs with a four-state machine:

```
Idle         — waiting for frame start
NmeaLine     — accumulating bytes after '$' until '\n'
RtcmHeader   — accumulating 3 header bytes (preamble + 2 length bytes)
RtcmBody     — accumulating payload + 3 CRC bytes
```

```rust
enum RxState {
    Idle,
    NmeaLine { buf: [u8; 512], len: usize },
    RtcmHeader { buf: [u8; 3], len: usize },
    RtcmBody { buf: [u8; 1029], len: usize, expected: usize },
}
```

State transitions:

```
Idle:
  byte == b'$'   → NmeaLine { buf[0]='$', len=1 }
  byte == 0xD3   → RtcmHeader { buf[0]=0xD3, len=1 }
  any other      → stay Idle (resync)

NmeaLine:
  byte == b'\n'  → dispatch NMEA, → Idle
  len >= 512     → overflow, warn, → Idle (resync)
  other          → buf[len]=byte, len+=1

RtcmHeader:
  len < 3        → buf[len]=byte, len+=1
  len == 3       → parse 10-bit length, → RtcmBody { expected = length + 6 }
                    if length > 1023, resync → Idle

RtcmBody:
  buf[len]=byte, len+=1
  len == expected  → verify CRC-24Q over buf[0..expected-3]
                     CRC ok: extract message_type, send (type, Vec::from(&buf[..expected]))
                     CRC fail: warn, → Idle
                     either way: → Idle
```

**Key invariant:** buf in RtcmBody must be 1029 bytes (3 header + 1023 max payload + 3 CRC).

### Thread Architecture (unchanged from v1.1)

```
gnss.rs RX thread
  ├── nmea_tx: SyncSender<(String, String)>  → nmea_relay.rs thread
  └── rtcm_tx: SyncSender<(u16, Vec<u8>)>   → rtcm_relay.rs thread (NEW)

gnss.rs TX thread (unchanged)
  └── cmd_rx: Receiver<String>

mqtt.rs pump_mqtt_events (modified)
  └── EventPayload::Received { topic, data }
        topic ends_with("/config")  → config_tx
        topic ends_with("/ota/trigger")  → ignored (Phase 8)
```

### spawn_gnss Return Signature Change (RTCM-01, RTCM-03)

Current: `(Sender<String>, Receiver<(String, String)>)`
New:     `(Sender<String>, Receiver<(String, String)>, Receiver<(u16, Vec<u8>)>)`

`main.rs` must be updated to receive the third element and pass it to `rtcm_relay::spawn_relay`.

### rtcm_relay.rs Design (mirrors nmea_relay.rs)

```rust
pub fn spawn_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    rtcm_rx: Receiver<(u16, Vec<u8>)>,
) -> anyhow::Result<()>
```

Topic format: `gnss/{device_id}/rtcm/{message_type}` where `message_type` is the decimal u16 (e.g. `1077`).
Publish: `QoS::AtMostOnce`, `retain=false`, payload = raw frame bytes (complete RTCM3 frame including header and CRC).

### Anti-Patterns to Avoid

- **Stripping header/CRC before publishing:** Publish the complete raw frame (header + payload + CRC). Receivers need the complete frame to parse correctly.
- **Using `String` topic with format!() in hot loop:** Pre-format topic or use a small stack string — but at 1-4 frames/sec this is not a concern.
- **Holding Mutex across sleep or multi-step logic:** Same rule as nmea_relay — acquire Mutex per frame, release immediately after enqueue.
- **Blocking on UART read while waiting for RTCM body:** Use `NON_BLOCK` throughout (same as current RX thread). State machine preserves partial state across reads.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CRC-24Q | Complex polynomial table | 24-line inline function | Algorithm is simple, fixed; no crate needed |
| Topic-based routing | Complex dispatcher | `topic.ends_with()` string match | Only 2 topics to discriminate; simple is correct |
| Frame buffering | Ring buffer / circular buffer | Fixed `[u8; 1029]` array in enum variant | Frame is assembled once; complete frame dispatched atomically |
| MQTT publish | Custom framing | `client.enqueue()` (existing pattern) | Same as nmea_relay; non-blocking; pump drains outbox |

**Key insight:** The state machine is the only novel logic. Everything else reuses established patterns from the existing codebase.

---

## Common Pitfalls

### Pitfall 1: RtcmBody Buffer on Stack in Enum

**What goes wrong:** `RxState::RtcmBody { buf: [u8; 1029], ... }` is 1029+ bytes. Rust places enum variants on stack. The RX thread has `stack_size(8192)`. A 1029-byte array in the enum plus existing stack frame (read_buf 256 bytes + other locals) risks stack overflow.

**Why it happens:** ESP-IDF FreeRTOS threads have explicit, fixed stack sizes. Stack overflow detection (canary method) is enabled, but may not trigger before corruption on RISC-V.

**How to avoid:** Either:
1. Reduce stack allocation: use `Box<[u8; 1029]>` in the enum (heap allocate once at thread start), OR
2. Increase thread stack from 8192 to 12288 for the GNSS RX thread.

**Recommendation:** Increase RX thread stack to 12288. Heap allocation adds complexity; stack increase is the simpler fix. 8192 - 1029 - 256 (read_buf) - ~500 (other frame) = ~6407 remaining, which is marginal on RISC-V with alignment padding. 12288 is safe.

**Warning signs:** FreeRTOS stack overflow error in logs, or hard fault / corrupted variables.

### Pitfall 2: CRC Computed Over Wrong Bytes

**What goes wrong:** CRC-24Q must be computed over the header bytes (0xD3 + 2 length bytes) plus the payload — NOT just the payload, and NOT including the CRC bytes themselves.

**Why it happens:** Ambiguous phrasing in informal sources. Some say "CRC over the message", which can mean just the payload.

**How to avoid:** CRC input = `buf[0..expected-3]` where expected = header(3) + payload + crc(3). The CRC covers everything except the trailing 3 CRC bytes.

**Warning signs:** All RTCM frames fail CRC check and resync endlessly.

### Pitfall 3: Length Field Extraction Off-by-One

**What goes wrong:** The 10-bit length field occupies bits 14-5 of the 3-byte header (big-endian). Extracting it incorrectly yields wrong payload length → wrong buffer fill point → CRC mismatch.

**Why it happens:** Bit manipulation with masked bytes is error-prone.

**How to avoid:** Verified extraction:
```rust
// header[0] = 0xD3 (preamble)
// header[1] = bits [7:6] reserved (0), bits [5:0] = length[9:4]
// header[2] = bits [7:4] = length[3:0], bits [3:0] = reserved (0)...
// Wait — per spec: 6 reserved bits then 10-bit length
// Byte 1: bits [7:2] = 6 reserved, bits [1:0] = length[9:8]
// Byte 2: bits [7:0] = length[7:0]
let length: usize = (((header[1] & 0x03) as usize) << 8) | (header[2] as usize);
```
Total frame = `length + 6`.

**Warning signs:** Frames consistently parse with wrong lengths; resync immediately after header.

### Pitfall 4: `pump_mqtt_events` Topic is `Option<&str>`

**What goes wrong:** `EventPayload::Received { topic, .. }` — `topic` is `Option<&'a str>`, not `&str`. On chunked messages (large payloads split across multiple events), subsequent chunks have `topic = None`. If code panics on `unwrap()` or routes `None` incorrectly, it crashes or misroutes.

**Why it happens:** The MQTT stack only provides the topic on the first chunk of a received message.

**How to avoid:** Use `topic.unwrap_or("")` for simple topic matching. Config and OTA payloads are small enough to arrive as single chunks (Details::Complete), so `topic` will be `Some(...)` for all relevant messages.

**Warning signs:** Panic on topic unwrap, or OTA trigger payload arriving with `topic = None`.

### Pitfall 5: RTCM Frame Sent to NMEA Channel or Vice Versa

**What goes wrong:** After refactoring gnss.rs to return two receivers, if main.rs wires them incorrectly (nmea_rx passed to rtcm_relay, rtcm_rx passed to nmea_relay), NMEA relay tries to publish binary as UTF-8 topics and RTCM relay tries to publish ASCII as raw binary.

**Why it happens:** Both channels are `Receiver<(_, Vec<u8>)>` — type system does not distinguish them.

**How to avoid:** Name variables clearly: `nmea_rx` and `rtcm_rx`. Add a comment at spawn site.

---

## Code Examples

### CRC-24Q Implementation

```rust
// Source: RTCM SC-104, confirmed in RTKLIB (tomojitakasu/RTKLIB/src/rtcm.c)
fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        crc ^= (byte as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x1000000 != 0 {
                crc ^= 0x864CFB;
            }
        }
    }
    crc & 0xFFFFFF
}
```

### Extracting 10-bit Length from RTCM3 Header

```rust
// header: [u8; 3] where header[0] == 0xD3
// Bits 15-10 of the 16-bit word formed by bytes 1-2 are reserved (zero).
// Bits 9-0 are the payload length.
fn rtcm3_payload_length(header: &[u8; 3]) -> usize {
    (((header[1] & 0x03) as usize) << 8) | (header[2] as usize)
}
// Total frame size = rtcm3_payload_length(header) + 6
```

### Extracting Message Type from RTCM3 Payload

```rust
// First 12 bits of payload are the message type number
fn rtcm3_message_type(payload: &[u8]) -> u16 {
    ((payload[0] as u16) << 4) | ((payload[1] as u16) >> 4)
}
```

### CRC Verification

```rust
fn verify_rtcm3_frame(frame: &[u8]) -> bool {
    // frame: complete RTCM3 frame including preamble, header, payload, CRC
    if frame.len() < 6 { return false; }
    let payload_plus_header_len = frame.len() - 3;
    let computed = crc24q(&frame[..payload_plus_header_len]);
    let stored = ((frame[frame.len()-3] as u32) << 16)
               | ((frame[frame.len()-2] as u32) << 8)
               | (frame[frame.len()-1] as u32);
    computed == stored
}
```

### Topic Discrimination in pump_mqtt_events (RTCM-05 fix)

```rust
EventPayload::Received { topic, data, .. } => {
    let t = topic.unwrap_or("");
    if t.ends_with("/config") {
        let _ = config_tx.send(data.to_vec());
    }
    // /ota/trigger: ignored here; Phase 8 adds ota_tx channel
}
```

### MqttClientConfiguration with out_buffer_size (RTCM-04)

```rust
let conf = MqttClientConfiguration {
    // ... existing fields ...
    out_buffer_size: 2048,  // covers 1029-byte RTCM MSM7 + MQTT overhead
    ..Default::default()    // or existing struct expression with this added
};
```

---

## Files to Modify

| File | Change |
|------|--------|
| `src/gnss.rs` | Replace line-buffer loop with `RxState` state machine; add `rtcm_tx: SyncSender<(u16, Vec<u8>)>`; return third element from `spawn_gnss` |
| `src/main.rs` | Receive `rtcm_rx` from `spawn_gnss`; call `rtcm_relay::spawn_relay`; add `mod rtcm_relay` |
| `src/mqtt.rs` | Fix `pump_mqtt_events` topic dispatch; add `out_buffer_size: 2048` to `mqtt_connect` |
| `src/rtcm_relay.rs` | New file — mirrors `nmea_relay.rs`; publishes `(u16, Vec<u8>)` to `gnss/{device_id}/rtcm/{type}` |

---

## Validation Architecture

> nyquist_validation key absent from config.json — treated as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | None detected — embedded firmware, no host-side test runner configured |
| Config file | None — see Wave 0 |
| Quick run command | `cargo build --release 2>&1 \| grep -E "error\|warning"` (compile-only validation) |
| Full suite command | `cargo build --release` (full compile; hardware flash for runtime validation) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | Notes |
|--------|----------|-----------|-------------------|-------|
| RTCM-01 | RxState state machine compiles with all four states | compile | `cargo build --release` | Compiler enforces exhaustive match |
| RTCM-01 | 1029-byte RTCM buffer fits in thread stack | manual | Flash + monitor for stack overflow log | FreeRTOS canary detection |
| RTCM-02 | CRC-24Q function returns correct value for known frame | unit | See Wave 0 — `cargo test --lib` if host tests added | Known-good RTCM frame bytes from RTKLIB test vectors |
| RTCM-02 | Bad CRC causes resync (next 0xD3/$ found) | manual | Inject corrupt 0xD3 byte via monitor; verify NMEA continues | Requires hardware |
| RTCM-03 | RTCM frames arrive on rtcm_rx channel | manual | Flash + Mosquitto subscriber on rtcm/# topic | Requires UM980 RTCM output enabled |
| RTCM-04 | Frames published without truncation | manual | `mosquitto_sub -t 'gnss/+/rtcm/+'` + hexdump verify size | Requires hardware + MQTT |
| RTCM-05 | /config payload not forwarded on OTA trigger | compile+manual | Inspect Received arm — topic check visible in code review; runtime: send OTA trigger, verify UM980 UART TX silent | |

### Wave 0 Gaps

- [ ] `src/gnss.rs` — delete existing line-buffer loop; introduce `RxState` enum (new code, not existing file)
- [ ] `src/rtcm_relay.rs` — new file, does not exist yet
- [ ] No host-side test harness exists; CRC-24Q correctness validated by code review against known polynomial + by hardware: if CRC is wrong, all RTCM frames are silently dropped and nothing appears on `rtcm/#`

*(If host tests are desired: `#[cfg(test)] mod tests` in `src/gnss.rs` with a known-good RTCM3 frame byte sequence can test the state machine and CRC without hardware.)*

---

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| Line-based NMEA parser (gnss.rs) | Four-state `RxState` machine (gnss.rs) | RTCM binary frames no longer corrupt NMEA parsing |
| All Received → config_tx | Topic-discriminated routing | OTA trigger no longer sent to UM980 as a command |
| Default MQTT buffer (1024 bytes) | out_buffer_size: 2048 | MSM7 frames up to 1029 bytes publish without truncation |

---

## Open Questions

1. **Stack size for GNSS RX thread**
   - What we know: Current 8192 bytes; adding `[u8; 1029]` to enum variant + existing 256-byte read_buf puts pressure on stack
   - What's unclear: Exact frame overhead on ESP32-C6 RISC-V; alignment padding unknown
   - Recommendation: Increase to 12288 bytes (50% increase, well within available memory); verify with FreeRTOS HWM logging

2. **UM980 RTCM output already configured vs. needs config relay**
   - What we know: RTCM output is enabled via retained MQTT `/config` payload using `RTCM1077 1` style commands
   - What's unclear: Whether the existing device config already sends RTCM commands
   - Recommendation: Phase 7 firmware change is independent; operator enables RTCM via MQTT config as before; no firmware change needed for UM980 configuration

3. **Resync from partial RTCM frame at startup**
   - What we know: If device boots mid-frame, first bytes will be mid-payload
   - What's unclear: How quickly resync occurs
   - Recommendation: State machine starts in Idle; any byte that is not `$` or `0xD3` is dropped. In the worst case, a 1029-byte frame takes ~90ms at 115200 baud to pass through the Idle state before the next frame begins. No special handling required.

---

## Sources

### Primary (HIGH confidence)
- RTCM3 frame structure: verified against RTKLIB source (github.com/tomojitakasu/RTKLIB/blob/master/src/rtcm.c) and multiple secondary sources (kernelsat.com, emlid docs)
- CRC-24Q polynomial 0x864CFB: confirmed in RTKLIB and GPSD documentation
- `MqttClientConfiguration` fields: read directly from `/home/ben/.cargo/registry/src/.../esp-idf-svc-0.51.0/src/mqtt/client.rs` lines 52-89 (buffer_size, out_buffer_size confirmed as usize fields)
- `EventPayload::Received { topic: Option<&str>, data: &[u8], ... }`: read directly from `/home/ben/.cargo/registry/src/.../embedded-svc-0.28.1/src/mqtt/client.rs` lines 71-78
- Existing codebase: all 11 source files read; gnss.rs, mqtt.rs, nmea_relay.rs, config_relay.rs, main.rs fully analysed

### Secondary (MEDIUM confidence)
- MSM7 message sizes: community.emlid.com forum, SNIP documentation, practical experience reported by users
- UM980 RTCM command syntax: SparkFun UM980 docs, Unicore reference commands manual (indirect)

### Tertiary (LOW confidence)
- UM980 UART load estimate at 115200 with RTCM MSM7 1Hz: calculated from typical frame sizes; not directly measured

---

## Metadata

**Confidence breakdown:**
- RTCM3 protocol (frame format, CRC): HIGH — multiple cross-verified sources, RTKLIB reference implementation
- State machine design: HIGH — derived from protocol facts + existing code patterns
- esp-idf-svc API fields: HIGH — read directly from pinned library source in local cargo registry
- MSM7 frame sizes: MEDIUM — practical community observations, theoretical maximum is exact
- UM980 configuration: MEDIUM — community and vendor docs

**Research date:** 2026-03-07
**Valid until:** 2026-09-07 (stable embedded protocol specs; esp-idf-svc pinned at =0.51.0 so API won't drift)
