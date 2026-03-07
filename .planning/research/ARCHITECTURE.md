# Architecture Research

**Domain:** Embedded Rust firmware — ESP32-C6 GNSS-to-MQTT bridge (milestone: RTCM relay + OTA)
**Researched:** 2026-03-07
**Confidence:** HIGH for RTCM framing (confirmed against RTKLIB and RTCM standard); HIGH for partition math (confirmed against Espressif docs); MEDIUM for EspOta API (docs.esp-rs.org reachable but ota_http_client.rs example rate-limited; feature gating confirmed from multiple sources)

---

## Context: Existing Architecture (v1.1 Shipped)

This document updates and extends the prior architecture research (2026-03-03) for the specific integration questions posed by the RTCM relay + OTA milestone. Read prior doc for full background on WiFi/MQTT/LED patterns.

### Existing Module Map

```
src/
├── main.rs          — wiring hub; 15-step init sequence (DO NOT reorder)
├── config.rs        — compile-time constants (baud, UART_RX_BUF_SIZE, credentials)
├── device_id.rs     — hardware eFuse MAC → 6-char hex device_id
├── gnss.rs          — UART owner; RX + TX threads; delivers (String,String) via sync_channel(64)
├── nmea_relay.rs    — consumes Receiver<(String,String)>; publishes to gnss/{id}/nmea/{TYPE}
├── config_relay.rs  — consumes Receiver<Vec<u8>> from pump; forwards lines to gnss_cmd_tx
├── mqtt.rs          — mqtt_connect, pump_mqtt_events, subscriber_loop, heartbeat_loop
├── uart_bridge.rs   — stdin → gnss_cmd_tx (espflash monitor debug bridge)
├── led.rs           — Arc<AtomicU8> LED state machine
└── wifi.rs          — wifi_connect, wifi_supervisor
```

### Existing Channel Topology

```
gnss.rs RX thread
    │── sync_channel(64) ──▶ nmea_relay.rs  ──▶ MQTT enqueue
    │
    TX thread ◀── channel() ──┬── config_relay.rs ◀── pump (config_tx)
                              └── uart_bridge.rs  ◀── stdin
                              └── main.rs idle loop (keepalive clone)

pump_mqtt_events
    ├── subscribe_tx  ──▶ subscriber_loop (subscribe on Connected)
    ├── config_tx     ──▶ config_relay.rs
    └── led_state     (Arc<AtomicU8> direct write)
```

---

## System Overview (Post-Milestone)

```
┌───────────────────────────────────────────────────────────────────────┐
│                          EXTERNAL INTERFACES                           │
│  ┌──────────────────┐  ┌─────────────────────┐  ┌──────────────────┐  │
│  │   UM980 GNSS     │  │    MQTT Broker       │  │  HTTP OTA server │  │
│  │   UART 115200    │  │  (Mosquitto/HiveMQ)  │  │  (binary .bin)   │  │
│  └────────┬─────────┘  └──────────┬──────────┘  └────────┬─────────┘  │
└───────────┼────────────────────────┼────────────────────────┼──────────┘
            │ UART RX: mixed         │ TCP/WiFi               │ HTTP GET
            │ NMEA text + RTCM bin   │                        │
┌───────────┼────────────────────────┼────────────────────────┼──────────┐
│                          ESP32-C6 FIRMWARE                             │
│                                                                        │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                        gnss.rs (MODIFIED)                       │   │
│  │  UartDriver owner — Arc<UartDriver>                             │   │
│  │                                                                  │   │
│  │  RX thread: mixed-stream state machine                          │   │
│  │    ┌────────────────────────────────────────────────────────┐   │   │
│  │    │  IDLE ──(0xD3)──▶ RTCM_HDR ──(2 bytes)──▶ RTCM_BODY  │   │   │
│  │    │    │                                          │         │   │   │
│  │    │    └──($)──▶ NMEA_LINE ──(\n)──▶ dispatch    │         │   │   │
│  │    │                                    │          │(+3 CRC) │   │   │
│  │    │         sync_channel(64) ◀──(NMEA)─┘          │         │   │   │
│  │    │         sync_channel(32) ◀──────────(RTCM)────┘         │   │   │
│  │    └────────────────────────────────────────────────────────┘   │   │
│  │                                                                  │   │
│  │  TX thread: unchanged (Sender<String> drain → UART write)       │   │
│  └──────────────────────────────────────┬──────────────────────────┘   │
│                                         │                              │
│            ┌────────────────────────────┴──────────────────┐           │
│            │                                               │           │
│  ┌─────────▼────────────────┐       ┌────────────────────▼─────────┐  │
│  │  nmea_relay.rs (UNCHANGED)│       │   rtcm_relay.rs  (NEW)       │  │
│  │  Receiver<(String,String)>│       │   Receiver<(u16, Vec<u8>)>   │  │
│  │  → gnss/{id}/nmea/{TYPE} │       │   → gnss/{id}/rtcm/{MSG_ID}  │  │
│  └──────────────────────────┘       └──────────────────────────────┘  │
│                                                                        │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  mqtt.rs (MODIFIED — pump routes OTA trigger)                   │   │
│  │  pump_mqtt_events adds ota_tx: Sender<String> routing           │   │
│  │  subscriber_loop adds gnss/{id}/ota/trigger subscription        │   │
│  └───────────────────────────────────────┬─────────────────────────┘   │
│                                          │ ota_tx (URL string)         │
│  ┌───────────────────────────────────────▼─────────────────────────┐   │
│  │  ota.rs (NEW)                                                   │   │
│  │  spawn_ota_listener(ota_rx)                                     │   │
│  │  1. recv URL from ota_rx                                        │   │
│  │  2. HTTP GET → chunk write → SHA256 stream verify               │   │
│  │  3. EspOta::initiate_update() → write chunks → complete()       │   │
│  │  4. esp_restart()                                               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

---

## Pattern 1: Mixed-Stream State Machine in gnss.rs RX Thread

### What Changes

The RX thread currently reads bytes into a flat line buffer and dispatches on `\n`. It must be extended to detect RTCM frames interleaved in the NMEA text stream. The UM980 can output both simultaneously on the same UART.

### RTCM 3.x Frame Structure (HIGH confidence — RTKLIB source + RTCM SC-104 standard)

```
Byte 0:      0xD3          (preamble — unique, never appears in valid NMEA)
Bytes 1-2:   [6 reserved bits = 0][10-bit message length N]
Bytes 3..N+2: payload      (N bytes; first 12 bits = message type number)
Bytes N+3..N+5: CRC-24Q   (3 bytes; covers preamble + header + payload)

Total frame size = 3 (header) + N (payload) + 3 (CRC) = N + 6 bytes
Maximum N = 1023 bytes (10-bit length)
Maximum frame = 1029 bytes
```

The 0xD3 preamble is guaranteed not to appear as the first character of an NMEA sentence (NMEA starts with `$` = 0x24). This makes the two protocols unambiguously distinguishable by first byte.

### State Machine Design

Replace the current `line_buf` / `line_len` local variables with a `RxState` enum. The enum is local to the RX thread closure — no locking required.

```rust
enum RxState {
    // Waiting for either '$' (NMEA) or 0xD3 (RTCM)
    Idle,

    // Accumulating NMEA bytes; line_buf[0..line_len] holds bytes after '$'
    NmeaLine { line_len: usize },

    // Received 0xD3; waiting for 2 header bytes to determine payload length
    RtcmHeader { header_buf: [u8; 2], header_len: usize },

    // Reading RTCM payload + 3-byte CRC
    // frame_buf: [0xD3, hdr0, hdr1, payload..., crc...]
    // total_needed: payload_len + 6 (full frame including preamble and CRC)
    RtcmBody { frame_buf: Vec<u8>, total_needed: usize },
}
```

### State Transitions

```
byte arrives:
  state = Idle:
    0xD3  → RtcmHeader { header_buf: [0,0], header_len: 0 }
    b'$'  → NmeaLine { line_len: 0 }
    other → Idle (discard with warn if non-whitespace)

  state = NmeaLine { line_len }:
    b'\n' → extract type, try_send to nmea_tx, → Idle
    other → append to line_buf (overflow check → Idle + warn)

  state = RtcmHeader { header_buf, header_len }:
    header_len < 2  → header_buf[header_len] = byte, header_len += 1
    header_len == 2 → extract 10-bit length N from header_buf
                      payload_len = N
                      total_needed = N + 6
                      frame_buf = Vec::with_capacity(total_needed)
                      frame_buf.push(0xD3); frame_buf.extend_from_slice(&header_buf)
                      frame_buf.push(byte)  (this is first payload byte, header_len just hit 2 after push)
                      → RtcmBody { frame_buf, total_needed }

  state = RtcmBody { frame_buf, total_needed }:
    frame_buf.push(byte)
    frame_buf.len() < total_needed → stay in RtcmBody
    frame_buf.len() == total_needed →
        // Verify CRC-24Q over frame_buf[0..total_needed-3]
        // Extract msg_type from bits 24..35 of frame_buf (bytes 3-4)
        msg_type = ((frame_buf[3] as u16) << 4) | ((frame_buf[4] as u16) >> 4)
        try_send to rtcm_tx: (msg_type, frame_buf)
        → Idle
```

### CRC-24Q Verification

CRC-24Q is a standard algorithm. Implement as a pure function — 256-entry lookup table is 768 bytes of ROM, acceptable on ESP32-C6. Verify before dispatching to rtcm_tx. Drop frame with warn on CRC failure.

```rust
// Polynomial: 0x864CFB (CRC-24Q)
fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= (b as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x1000000 != 0 { crc ^= 0x864CFB; }
        }
    }
    crc & 0xFFFFFF
}
```

Verify: `crc24q(&frame_buf[0..total_needed-3]) == u32 from frame_buf[total_needed-3..total_needed]`.

### Stack Size Impact

The RtcmBody state uses `Vec<u8>` on the heap (up to 1029 bytes per frame). The RX thread currently has 8192 bytes of stack. The heap allocation is fine; the stack usage does not increase significantly. Keep 8192 stack. Add `RTCM_RELAY_BUF_SIZE` to config.rs if needed.

---

## Pattern 2: Channel Type for Binary RTCM Frames

### Decision: `SyncSender<(u16, Vec<u8>)>` (RECOMMENDED)

The tuple carries:
- `u16`: RTCM message type number (e.g., 1005, 1074, 1084) — extracted from frame bits 24-35
- `Vec<u8>`: complete raw frame bytes (preamble + header + payload + CRC), ready to publish as-is

**Why `u16` not `u32`:** RTCM message types fit in 12 bits (0-4095). `u16` is sufficient and matches the natural extraction (12-bit field).

**Why include the full frame (not just payload):** RTCM consumers (NTRIP casters, RTK engines) expect the full framed binary including preamble and CRC. Stripping the frame requires the subscriber to re-frame, which adds complexity for no benefit.

**Why `SyncSender` not `Sender`:** Matches the existing pattern. Bounded channel (32 slots) prevents unbounded heap growth if MQTT is backlogged. RTCM frames are larger than NMEA strings; 32 × ~200 bytes = ~6KB max queued — manageable.

**Channel bound:** 32. RTCM corrections typically arrive at 1 Hz per message type. At 5 message types = 5 frames/sec. 32 slots = ~6 seconds of buffer before drops. Acceptable for real-time RTK data where stale corrections are useless anyway.

**Alternative considered: `Sender<Vec<u8>>` (raw frame only).**
Rejected because rtcm_relay.rs needs the message type to build the MQTT topic `gnss/{id}/rtcm/{MSG_ID}`. Computing it again in rtcm_relay.rs duplicates the bit-extraction logic. Including the type in the channel avoids double-parsing.

### spawn_gnss Return Signature Change

Current:
```rust
pub fn spawn_gnss(...) -> anyhow::Result<(Sender<String>, Receiver<(String, String)>)>
```

New:
```rust
pub fn spawn_gnss(...) -> anyhow::Result<(
    Sender<String>,               // cmd_tx (unchanged)
    Receiver<(String, String)>,   // nmea_rx (unchanged)
    Receiver<(u16, Vec<u8>)>,     // rtcm_rx (NEW)
)>
```

This is a breaking change to gnss.rs's public signature. main.rs wiring at Step 7 must be updated to destructure the triple.

---

## Pattern 3: rtcm_relay.rs (New Module)

Mirrors nmea_relay.rs exactly in structure. Consumes `Receiver<(u16, Vec<u8>)>`. Publishes to `gnss/{device_id}/rtcm/{msg_type}` at QoS 0, retain=false.

```
pub fn spawn_rtcm_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    rtcm_rx: Receiver<(u16, Vec<u8>)>,
) -> anyhow::Result<()>
```

Topic format: `gnss/{device_id}/rtcm/1005` (message type as decimal integer). Binary payload = full RTCM frame bytes. MQTT binary payloads are spec-compliant; broker stores and delivers as raw bytes.

Stack size: 8192 (same as nmea_relay). The Vec<u8> is heap-allocated; enqueue() copies into MQTT outbox.

---

## Pattern 4: OTA Architecture

### Partition Table Redesign

**Current layout (4MB flash):**

```
# Name,   Type, SubType, Offset,  Size,     Flags
nvs,      data, nvs,     0x9000,  0x10000,
phy_init, data, phy,     0x19000, 0x1000,
factory,  app,  factory, 0x20000, 0x3E0000,
```

Total: 0x9000 + 0x10000 + 0x1000 + (gap to 0x20000) + 0x3E0000 = 4MB used fully.

**Problem:** No `otadata` partition. No `ota_0`/`ota_1` app partitions. OTA requires the bootloader to read `otadata` to decide which slot to boot. Without `otadata`, `EspOta` will fail or silently overwrite the factory image with no rollback.

**Required new layout (4MB flash):**

```
# Name,    Type, SubType, Offset,  Size,     Flags
nvs,       data, nvs,     0x9000,  0x10000,
otadata,   data, ota,     0x19000, 0x2000,
phy_init,  data, phy,     0x1B000, 0x1000,
ota_0,     app,  ota_0,   0x20000, 0x1E0000,
ota_1,     app,  ota_1,   0x200000, 0x1E0000,
```

**Partition math verification (4MB = 0x400000 bytes):**

| Partition | Start    | Size     | End      |
|-----------|----------|----------|----------|
| nvs       | 0x009000 | 0x010000 | 0x019000 |
| otadata   | 0x019000 | 0x002000 | 0x01B000 |
| phy_init  | 0x01B000 | 0x001000 | 0x01C000 |
| (gap)     | 0x01C000 | 0x004000 | 0x020000 |
| ota_0     | 0x020000 | 0x1E0000 | 0x200000 |
| ota_1     | 0x200000 | 0x1E0000 | 0x3E0000 |
| (spare)   | 0x3E0000 | 0x020000 | 0x400000 |

- NVS preserved at 64KB (0x10000) — existing NVS data format unchanged
- otadata at 8KB (0x2000) — minimum required; Espressif recommends 2 sectors
- phy_init preserved at 4KB
- 16KB gap before ota_0 (aligning to 0x20000 = 128KB boundary — required by ESP-IDF app partition alignment)
- ota_0 and ota_1: 0x1E0000 = 1,966,080 bytes = ~1.875MB each — ample for this firmware
- 128KB spare at end (available for future spiffs/data partition)
- Total: fits exactly in 4MB ✓

**sdkconfig.defaults change required:**

Add `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` to enable rollback if new firmware fails to mark itself valid.

**Critical:** Changing the partition table requires a full erase + reflash (`espflash erase-flash` or `--erase-otadata`). The first OTA flash must be done via USB (espflash), not OTA itself — you cannot OTA from a factory partition to ota_0/ota_1 layout.

### ota.rs Module Design

```
src/ota.rs (NEW)

pub fn spawn_ota_listener(
    ota_rx: Receiver<String>,   // URL strings from MQTT trigger
) -> anyhow::Result<()>
```

The thread blocks on `ota_rx`. When a URL arrives:

1. Log "OTA triggered: {url}"
2. Open `EspHttpConnection` (from `esp_idf_svc::http::client::EspHttpConnection`)
3. HTTP GET the URL; read in chunks (20KB recommended to avoid heap pressure)
4. Stream chunks into `EspOta::initiate_update()` → `update.write(&chunk)`
5. Stream-hash with SHA256 (computed over received bytes)
6. After final chunk: compare SHA256 against expected hash
7. Call `update.complete()` on success; `update.abort()` on hash mismatch
8. If complete: `unsafe { esp_idf_svc::sys::esp_restart() }`

**SHA256 source:** The MQTT trigger message should carry both URL and expected hash, e.g. JSON `{"url":"http://...","sha256":"abc..."}`. The ota_rx channel type can be `String` (JSON) or a struct. Recommended: parse JSON in ota.rs using simple string matching (matching the existing no-serde pattern from config_relay.rs).

**Cargo.toml additions for OTA:**

```toml
esp-idf-svc = { version = "=0.51.0", features = ["ota"] }
```

The `ota` feature gates the `esp_idf_svc::ota` module. Without it, `EspOta` is not visible (confirmed: multiple sources state OTA is feature-gated in esp-idf-svc; MEDIUM confidence on exact feature name — verify against esp-idf-svc 0.51 Cargo.toml before implementing).

**sdkconfig.defaults addition:**

```
CONFIG_ESP_HTTPS_OTA_ALLOW_HTTP=y
```

Required if using plain HTTP OTA URLs (no TLS). Without this, ESP-IDF rejects non-HTTPS OTA.

### OTA MQTT Integration

**New topic:** `gnss/{device_id}/ota/trigger` (subscribed, not retained — OTA is a one-shot action)

**pump_mqtt_events change:** Add `ota_tx: Option<Sender<String>>`. Route `Received` events where `topic` ends with `/ota/trigger` to `ota_tx`. Keep `config_tx` routing for `/config` topic. The current pump routes ALL `Received` events to `config_tx` regardless of topic — this must be fixed in the RTCM milestone anyway (or at OTA time at latest).

**Current pump bug:** `pump_mqtt_events` sends every `Received` event to `config_tx` without checking the topic. When OTA subscription is added, RTCM and OTA payloads would incorrectly flow to config_relay. The pump needs topic discrimination:

```rust
EventPayload::Received { topic, data, .. } => {
    let topic = topic.unwrap_or("");
    if topic.ends_with("/config") {
        let _ = config_tx.send(data.to_vec());
    } else if topic.ends_with("/ota/trigger") {
        if let Ok(s) = std::str::from_utf8(data) {
            let _ = ota_tx.send(s.to_string());
        }
    }
    // else: unrouted topic — log warn, drop
}
```

**subscriber_loop change:** Add `gnss/{device_id}/ota/trigger` subscription alongside the config subscription.

---

## Modified vs New Files

| File | Status | Change Summary |
|------|--------|----------------|
| `src/gnss.rs` | MODIFIED | RX thread: flat line assembler → RxState machine; add rtcm_tx SyncSender; return triple from spawn_gnss |
| `src/main.rs` | MODIFIED | Step 7: destructure triple from spawn_gnss; Step 14b: wire rtcm_rx to spawn_rtcm_relay; Step 9c: add ota_tx/rx channel; Step 15b: spawn_ota_listener |
| `src/mqtt.rs` | MODIFIED | pump_mqtt_events: add ota_tx param + topic discrimination in Received arm; subscriber_loop: add OTA trigger subscription |
| `src/rtcm_relay.rs` | NEW | Mirror of nmea_relay.rs; consumes Receiver<(u16,Vec<u8>)>; publishes binary to gnss/{id}/rtcm/{type} |
| `src/ota.rs` | NEW | OTA listener thread; HTTP download; SHA256 verification; EspOta write; reboot |
| `partitions.csv` | MODIFIED | Replace factory-only with otadata + ota_0 + ota_1 layout |
| `sdkconfig.defaults` | MODIFIED | Add CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y; CONFIG_ESP_HTTPS_OTA_ALLOW_HTTP=y |
| `Cargo.toml` | MODIFIED | Add `ota` feature to esp-idf-svc |

---

## Data Flow

### RTCM Relay Flow

```
UM980 UART TX (binary RTCM bytes mixed with NMEA text)
    │
    ▼
gnss.rs RX thread — RxState machine
    │ detects 0xD3 preamble → accumulates frame → CRC verify
    ▼
SyncSender<(u16, Vec<u8>)>  [bound: 32 frames]
    │
    ▼
rtcm_relay.rs thread
    │ topic = format!("gnss/{}/rtcm/{}", device_id, msg_type)
    │ payload = raw frame bytes (binary)
    ▼
EspMqttClient::enqueue(topic, QoS0, retain=false, &frame)
    │
    ▼
MQTT Broker → RTK subscribers (NTRIP, base station software)
```

### OTA Flow

```
Operator publishes to gnss/{device_id}/ota/trigger:
  payload: {"url":"http://10.86.32.41:8080/firmware.bin","sha256":"abc123..."}
    │
    ▼
pump_mqtt_events → topic discrimination → ota_tx.send(payload_string)
    │
    ▼
ota.rs listener thread
    │ HTTP GET url → stream chunks
    │ write chunks to EspOta + stream SHA256
    │ verify SHA256 → complete() or abort()
    ▼
ESP-IDF bootloader sets ota_0 (or ota_1) as boot partition
    │
    ▼
esp_restart() → boots new firmware
    │ new firmware marks itself valid (app_desc check or explicit mark)
    ▼
Rollback: if new firmware fails to mark valid before watchdog → auto-rollback to prior slot
```

---

## Build Order for Phases

### Recommendation: RTCM Relay Before OTA

**Rationale:**

1. **RTCM relay has zero partition risk.** It requires no partition table changes. OTA requires a partition table change that involves a full erase — if OTA partition work goes wrong, you lose the ability to test RTCM relay on the device without USB recovery.

2. **gnss.rs state machine is a shared dependency.** The state machine refactor in gnss.rs must be done before either RTCM relay or OTA can be tested. Isolating that change in Phase A makes debugging cleaner.

3. **mqtt.rs topic discrimination is a prerequisite for OTA.** The pump currently routes all `Received` events to config_tx. OTA needs the pump to discriminate topics. This fix is best done as part of RTCM relay work (where you also want clean routing), not deferred to OTA.

4. **OTA requires hardware re-flash to change partitions.** That's a harder reset of the device. Complete RTCM relay while the current partition table is intact.

**Recommended phase order:**

```
Phase A — gnss.rs state machine + RTCM relay (no partition change)
  A1: RxState machine in gnss.rs; extend spawn_gnss to return triple
  A2: rtcm_relay.rs (mirrors nmea_relay.rs); wire into main.rs
  A3: mqtt.rs topic discrimination fix
  Hardware verify: RTCM frames appear on gnss/{id}/rtcm/NNNN topics

Phase B — OTA
  B1: partitions.csv redesign; sdkconfig.defaults updates; full erase + reflash
  B2: ota.rs module; Cargo.toml feature addition
  B3: mqtt.rs OTA trigger routing; subscriber_loop OTA subscription
  B4: Wire ota.rs into main.rs
  Hardware verify: trigger OTA from MQTT, new firmware boots, rollback works
```

---

## Baud Rate Change Impact (If Moving to 230400+)

The PROJECT.md records "UM980 fixed at 115200 baud, 8N1" as a constraint. If this changes:

1. `config.rs` `UART_RX_BUF_SIZE` should be increased proportionally (currently 4096; at 230400 the byte rate doubles — consider 8192).
2. `UartDriver::new()` baudrate parameter change in gnss.rs.
3. No impact on the state machine design — the byte-by-byte processing is rate-agnostic.
4. RX thread sleep-on-empty is currently 10ms. At 230400, a 10ms gap delivers 230 bytes into the FIFO. The 4096-byte ring buffer provides adequate cushion; no overflow risk.
5. The UM980 must be commanded to change baud before the ESP32 changes — send the baud-change command, then reinitialize UartDriver. This is a tricky sequencing problem best handled in a dedicated plan.

Current constraint stands: **do not change baud rate without an explicit plan and hardware validation.**

---

## Anti-Patterns

### Anti-Pattern 1: Using a Single Channel for Mixed NMEA+RTCM Output

**What people do:** Add a Rust enum to the existing NMEA channel:
```rust
enum GnssFrame { Nmea(String, String), Rtcm(u16, Vec<u8>) }
SyncSender<GnssFrame>
```

**Why it's wrong:** nmea_relay.rs and rtcm_relay.rs are independent consumers. A single channel can only have one consumer. You would need to fan-out in a new dispatcher thread, adding latency and a thread just for routing. Two separate channels are simpler, cheaper, and match the existing pattern exactly.

**Do this instead:** Two separate typed channels from gnss.rs: one `SyncSender<(String,String)>` for NMEA (unchanged), one `SyncSender<(u16,Vec<u8>)>` for RTCM (new).

### Anti-Pattern 2: Buffering RTCM Frames Across MQTT Disconnections

**What people do:** Use an unbounded channel or large bounded channel for RTCM to avoid dropping frames during broker outages.

**Why it's wrong:** RTCM corrections are time-sensitive. A correction more than 10-30 seconds old is useless for RTK positioning. Buffering stale corrections wastes memory and confuses RTK engines. The UM980 will generate fresh corrections when the broker reconnects.

**Do this instead:** Use `try_send` with drop-on-full (same as NMEA). Log a warn. The bounded channel (32 slots) provides a reasonable buffer for transient hiccups.

### Anti-Pattern 3: Verifying CRC in rtcm_relay.rs Instead of gnss.rs

**What people do:** Pass raw unverified bytes from the state machine and verify CRC in the relay thread.

**Why it's wrong:** gnss.rs is the UART owner and the framing authority. Passing corrupted frames downstream creates a contract violation — consumers can't know whether frames are verified. gnss.rs should only emit verified frames.

**Do this instead:** Verify CRC-24Q in gnss.rs before calling try_send. Drop the frame and log warn on failure. This mirrors how gnss.rs already validates NMEA (`first() == Some(&b'$')`).

### Anti-Pattern 4: Triggering OTA From the MQTT Pump Thread Directly

**What people do:** Perform the HTTP download and OTA write inside `pump_mqtt_events` when the trigger arrives.

**Why it's wrong:** `pump_mqtt_events` must call `connection.next()` continuously. Blocking for a multi-second HTTP download inside the pump stops the MQTT event loop, causing the broker to disconnect (keepalive timeout), and any MQTT state written by the pump (LED, subscribe signals) is frozen.

**Do this instead:** pump sends URL string to `ota_tx` channel. A dedicated `ota.rs` thread blocks on the channel and performs the download. The pump remains unblocked.

### Anti-Pattern 5: Writing New Firmware to the Running Partition

**What people do:** Try to write OTA data to the currently-active partition slot (ota_0 while running from ota_0).

**Why it's wrong:** ESP-IDF's `EspOta::initiate_update()` automatically selects the inactive slot. If you attempt to write the active slot, it fails. Do not attempt to select the partition manually.

**Do this instead:** Let `EspOta::initiate_update()` choose. It always targets the inactive slot.

### Anti-Pattern 6: Omitting otadata Partition

**What people do:** Add ota_0 and ota_1 to partitions.csv without adding the `otadata` partition.

**Why it's wrong:** The ESP-IDF bootloader reads `otadata` (subtype `ota`) to determine which app partition to boot. Without it, the bootloader cannot track OTA state and will always boot from the first app partition. OTA writes will succeed but `esp_restart()` will boot the old image.

**Do this instead:** Always include `otadata, data, ota, <offset>, 0x2000` in the partition table when using OTA.

---

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| UM980 GNSS | UART full-duplex, 115200 8N1, mixed NMEA+RTCM stream | RX: RxState machine handles both protocols by first byte; TX: unchanged |
| MQTT Broker | EspMqttClient binary publish for RTCM frames | MQTT supports binary payloads natively; broker stores and delivers as raw bytes |
| OTA HTTP server | EspHttpConnection GET, chunked read | Can be any HTTP server (python -m http.server, nginx); HTTPS requires certificate bundle |

### Internal Boundaries (Updated)

| Boundary | Communication | Notes |
|----------|---------------|-------|
| gnss.rs RX → nmea_relay.rs | `SyncSender<(String,String)>` bound 64 — UNCHANGED | Same as v1.1 |
| gnss.rs RX → rtcm_relay.rs | `SyncSender<(u16,Vec<u8>)>` bound 32 — NEW | Binary frames, heap-allocated Vec per frame |
| pump → config_relay | `Sender<Vec<u8>>` — UNCHANGED routing | pump must now discriminate topic before sending |
| pump → ota.rs | `Sender<String>` — NEW | URL+hash JSON string; unbounded OK (OTA triggers are rare) |
| ota.rs → EspOta | Direct in-thread — no channel | OTA thread owns the EspOta handle; blocking calls acceptable |

---

## Concurrency Budget (Updated)

| Thread | Stack | Role | Status |
|--------|-------|------|--------|
| main (idle loop) | 8KB | keepalive, holds gnss_cmd_tx | unchanged |
| gnss RX | 8KB | state machine, UART read | MODIFIED |
| gnss TX | 8KB | UART write | unchanged |
| uart_bridge | 8KB | stdin → gnss_cmd_tx | unchanged |
| led_task | 8KB | GPIO LED | unchanged |
| pump | 8KB | MQTT event loop | MODIFIED (minor) |
| subscriber_loop | 8KB | subscribe on Connected | MODIFIED (add OTA topic) |
| heartbeat_loop | 8KB | periodic heartbeat | unchanged |
| wifi_supervisor | 8KB | reconnect logic | unchanged |
| nmea_relay | 8KB | NMEA MQTT publish | unchanged |
| rtcm_relay | 8KB | RTCM MQTT publish | NEW |
| ota_listener | 16KB | HTTP download + OTA write | NEW (16KB: HTTP client stack is larger) |

Total new threads: 2 (rtcm_relay + ota_listener). ESP32-C6 has adequate task capacity. Total approximate stack: ~13 threads × ~8KB = ~104KB + WiFi/MQTT internal = within budget.

---

## Sources

- RTCM 3.x frame structure: [RTKLIB/src/rtcm.c](https://github.com/tomojitakasu/RTKLIB/blob/master/src/rtcm.c) (HIGH confidence — reference implementation)
- RTCM SC-104 Wikipedia overview: [RTCM SC-104](https://en.wikipedia.org/wiki/RTCM_SC-104) (MEDIUM confidence — correct on preamble/length/CRC structure)
- An RTCM 3 message cheat sheet: [SNIP Support](https://www.use-snip.com/kb/knowledge-base/an-rtcm-message-cheat-sheet/) (MEDIUM confidence)
- EspOta API: [docs.esp-rs.org](https://docs.esp-rs.org/esp-idf-svc/esp_idf_svc/ota/struct.EspOta.html) (MEDIUM — site reachable but full API read rate-limited)
- OTA example: [esp-idf-svc/examples/ota_http_client.rs](https://github.com/esp-rs/esp-idf-svc/blob/master/examples/ota_http_client.rs) (MEDIUM — confirmed to exist)
- OTA practical walkthrough: [Programming ESP32 with Rust: OTA firmware update](https://quan.hoabinh.vn/post/2024/3/programming-esp32-with-rust-ota-firmware-update) (MEDIUM — 2024 article, 0.51 not specifically tested)
- ESP-IDF Partition Tables: [Espressif docs](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/partition-tables.html) (HIGH — official)
- ESP-IDF OTA: [Espressif OTA docs](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/ota.html) (HIGH — official)

---

*Architecture research for: ESP32-C6 GNSS-to-MQTT bridge — RTCM relay + OTA milestone*
*Researched: 2026-03-07*
