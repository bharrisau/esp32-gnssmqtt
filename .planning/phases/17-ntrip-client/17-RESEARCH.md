# Phase 17: NTRIP Client - Research

**Researched:** 2026-03-08
**Domain:** NTRIP protocol v1, TCP networking (std::net), NVS config storage, heartbeat extension
**Confidence:** HIGH (protocol well-documented; TCP pattern confirmed available in ESP-IDF std; project patterns clear)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| NTRIP-01 | Device connects to configured NTRIP caster and streams RTCM3 corrections to UM980 UART | NTRIP v1 HTTP-over-TCP protocol; std::net::TcpStream available via ESP-IDF lwIP; gnss_cmd_tx already delivers bytes to UART TX thread |
| NTRIP-02 | NTRIP settings (host, port, mountpoint, user, pass) configurable via retained MQTT topic `gnss/{device_id}/ntrip/config` | Existing MQTT callback dispatch pattern (config_tx / ntrip_config_tx channel); NVS persistence with 15-char key limit; JSON parsing pattern already used in config_relay |
| NTRIP-03 | NTRIP client reconnects automatically on connection loss | Reconnect loop inside dedicated thread; TCP read returning Err/0 bytes signals drop; exponential backoff 5s→10s→20s→40s cap |
| NTRIP-04 | NTRIP connection state included in health heartbeat | AtomicU8 NTRIP_STATE global (0=disconnected, 1=connected); heartbeat_loop reads it and includes "ntrip" field in JSON |
</phase_requirements>

---

## Summary

NTRIP (Networked Transport of RTCM via Internet Protocol) v1 is a simple HTTP/1.0-like protocol that runs over a persistent TCP socket. The client sends a single GET request with optional Basic Auth and receives a streaming binary RTCM3 response prefixed with `ICY 200 OK\r\n`. There is no chunked encoding, no keep-alive negotiation — the connection remains open and raw RTCM bytes flow until the TCP socket closes.

ESP-IDF with the Rust std library exposes BSD socket semantics through lwIP, making `std::net::TcpStream` directly usable. The project already uses `std::net` implicitly (MQTT uses TCP underneath). A dedicated NTRIP thread can open a `TcpStream`, write the NTRIP HTTP request, confirm the `ICY 200 OK` response, then loop reading bytes and forwarding them to `gnss_cmd_tx` (the existing `SyncSender<String>` to the GNSS TX thread). Because RTCM3 bytes are binary, they must go through the raw write path in the GNSS TX thread directly, not through `gnss_cmd_tx` which is a String channel. Instead, RTCM correction bytes must be written directly to the UART via a new raw-bytes sender or by reusing the existing `UartDriver` Arc.

The configuration arrives via a new retained MQTT topic `gnss/{device_id}/ntrip/config` carrying JSON with host, port, mountpoint, user, pass. This mirrors the existing `/config` pattern exactly. Settings should be persisted to NVS (namespace `ntrip`, 15-char key limit applies) so the connection can be restarted after reboot without waiting for MQTT.

**Primary recommendation:** Implement a `ntrip_client` module with a `spawn_ntrip_client` function. The thread receives config updates from a channel, manages its own TCP lifecycle (connect → request → stream → reconnect on error), and writes RTCM bytes directly to the UART using an `Arc<UartDriver>` clone from `spawn_gnss`. NTRIP connection state is exposed via an `AtomicU8` global read by `heartbeat_loop`.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::net::TcpStream` | stdlib | TCP connection to NTRIP caster | Available via ESP-IDF lwIP/BSD sockets; no extra dependency |
| `esp_idf_svc::nvs::EspNvs` | 0.51.0 | Persist NTRIP config across reboots | Already used for MQTT config in provisioning.rs |
| `std::sync::atomic::AtomicU8` | stdlib | Share NTRIP connection state with heartbeat | Pattern established for GNSS counters (NMEA_DROPS, RTCM_DROPS) |
| `std::sync::mpsc::SyncSender` | stdlib | Deliver config updates from MQTT callback to NTRIP thread | Same pattern as config_tx, log_level_tx |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `base64` crate | 0.22 | Encode `user:pass` for NTRIP Basic Auth header | Only when user/pass are non-empty; the crate supports no_std with `alloc` feature |

**Note on base64:** The `base64` crate (version 0.22) with the STANDARD engine can encode credentials. No_std+alloc support is available. However, for this project (ESP32 std), a simpler approach is to encode directly: a minimal custom base64 encoder for the specific credential string avoids adding a crate dependency for ~30 lines of code. Research recommends the latter given the project pattern of avoiding dependencies (the project has no serde, no json crate).

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `std::net::TcpStream` | `esp-idf-svc` EspHttpClient | HTTP client adds overhead; ICY 200 OK is not valid HTTP and will be rejected by compliant parsers |
| Custom base64 | `base64` crate 0.22 | Crate is correct and well-tested; custom is ~30 lines but avoids dependency; either is fine |
| Separate raw bytes sender | Reuse `Arc<UartDriver>` directly | Direct UART write is simpler; avoids touching gnss_cmd_tx (which is String-typed) |

**Installation (if base64 crate is chosen):**
```bash
# Add to Cargo.toml [dependencies]:
# base64 = { version = "0.22", default-features = false, features = ["alloc"] }
```

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── ntrip_client.rs     # New module: spawn_ntrip_client, NTRIP_STATE atomic, NVS load/save
├── mqtt.rs             # Add ntrip_config_tx dispatch + subscriber adds /ntrip/config topic
├── main.rs             # Wire ntrip_config_tx channel + spawn_ntrip_client call
└── config.rs           # Add NTRIP_RECONNECT_* duration constants
```

### Pattern 1: NTRIP Thread with Config Channel

**What:** A single dedicated thread owns the TCP lifecycle. It blocks on the config channel initially, then reconnects whenever config changes or connection drops.

**When to use:** Exactly this scenario — long-lived streaming TCP connection that must restart on config change or error.

**Example:**
```rust
// ntrip_client.rs — conceptual structure (not final code)
pub static NTRIP_STATE: AtomicU8 = AtomicU8::new(0); // 0=disconnected, 1=connected

pub fn spawn_ntrip_client(
    uart: Arc<UartDriver<'static>>,
    ntrip_config_rx: Receiver<Vec<u8>>,
    nvs: EspNvsPartition<NvsDefault>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // Load config from NVS on start (persists across reboots)
            let mut config = load_ntrip_config(&nvs).unwrap_or_default();

            loop {
                if config.host.is_empty() {
                    // No config yet — wait for MQTT delivery
                    match ntrip_config_rx.recv_timeout(SLOW_RECV_TIMEOUT) {
                        Ok(payload) => config = parse_and_save(&payload, &nvs),
                        _ => continue,
                    }
                }

                // Try to connect and stream; returns when connection drops or config arrives
                run_ntrip_session(&config, &uart, &ntrip_config_rx, &nvs, &mut config);
            }
        })
        .expect("ntrip thread spawn failed");
    Ok(())
}
```

### Pattern 2: NTRIP v1 HTTP Request Construction

**What:** NTRIP v1 uses HTTP/1.0 GET with optional Basic Auth. Server responds with `ICY 200 OK\r\n\r\n` then raw RTCM bytes.

**Example:**
```rust
// Source: NTRIP v1 specification (BKG/RTCM SC-104), verified against use-snip.com documentation
fn build_ntrip_request(mountpoint: &str, user: &str, pass: &str) -> String {
    let mut req = format!(
        "GET /{} HTTP/1.0\r\nUser-Agent: NTRIP esp32-gnssmqtt/1.0\r\nAccept: */*\r\n",
        mountpoint
    );
    if !user.is_empty() {
        let credentials = base64_encode(&format!("{}:{}", user, pass));
        req.push_str(&format!("Authorization: Basic {}\r\n", credentials));
    }
    req.push_str("\r\n");
    req
}
```

### Pattern 3: ICY 200 OK Response Parsing

**What:** Read bytes until `\r\n\r\n` (end of headers). Confirm first line is `ICY 200 OK`. Then forward remaining bytes (and all subsequent reads) directly to UART.

**When to use:** After TCP connect, before streaming begins.

**Example:**
```rust
// Read headers — stop at double CRLF
fn read_ntrip_headers(stream: &mut TcpStream) -> Result<bool, std::io::Error> {
    let mut header_buf = [0u8; 512];
    let mut header_len = 0usize;
    let mut buf = [0u8; 1];
    loop {
        match stream.read(&mut buf)? {
            0 => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "header")),
            _ => {
                if header_len < header_buf.len() {
                    header_buf[header_len] = buf[0];
                    header_len += 1;
                }
                if header_len >= 4 && &header_buf[header_len-4..header_len] == b"\r\n\r\n" {
                    break;
                }
            }
        }
    }
    let header_str = std::str::from_utf8(&header_buf[..header_len]).unwrap_or("");
    Ok(header_str.starts_with("ICY 200 OK"))
}
```

### Pattern 4: RTCM Byte Forwarding to UART

**What:** RTCM3 correction bytes are binary — they CANNOT go through `gnss_cmd_tx: SyncSender<String>`. They must be written directly to the UART using `Arc<UartDriver>`.

**Critical:** The GNSS TX thread also writes to this UART. Both write via `UartDriver::write(&self, ...)` which takes `&self` — concurrent writes are safe at the driver level (Arc, no Mutex needed, same pattern as gnss.rs). However, interleaving an NTRIP byte stream with UM980 command strings mid-transmission would corrupt both. The NTRIP stream is continuous; UM980 commands are rare and brief. Practical risk is low, but a Mutex around the write path would be more correct. See Pitfall 3.

**Example:**
```rust
// Forward a chunk of RTCM bytes directly to UART
fn forward_rtcm_bytes(uart: &Arc<UartDriver>, data: &[u8]) {
    if let Err(e) = uart.write(data) {
        log::warn!("NTRIP: UART write error: {:?}", e);
    }
}
```

### Pattern 5: NTRIP Config in MQTT Callback + Subscriber

**What:** New channel `ntrip_config_tx: SyncSender<Vec<u8>>` dispatched from MQTT callback when topic ends with `/ntrip/config`. Subscribed at QoS::AtLeastOnce (retained) in `subscriber_loop`.

**Example (mqtt.rs additions):**
```rust
// In mqtt_connect callback, Received arm:
} else if t.ends_with("/ntrip/config") {
    match ntrip_config_tx.try_send(data.to_vec()) {
        Ok(_) => {}
        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: ntrip config channel full"),
        Err(TrySendError::Disconnected(_)) => {}
    }
}

// In subscriber_loop, add:
let ntrip_config_topic = format!("gnss/{}/ntrip/config", device_id);
match c.subscribe(&ntrip_config_topic, QoS::AtLeastOnce) {
    Ok(_) => log::info!("Subscribed to {}", ntrip_config_topic),
    Err(e) => log::warn!("Subscribe /ntrip/config failed: {:?}", e),
}
```

### Pattern 6: Heartbeat Extension for NTRIP State (NTRIP-04)

**What:** Read `NTRIP_STATE` atomic in `heartbeat_loop` and append `"ntrip":"connected"` or `"ntrip":"disconnected"` to the JSON payload.

**Example (mqtt.rs heartbeat_loop change):**
```rust
let ntrip_state = crate::ntrip_client::NTRIP_STATE.load(Ordering::Relaxed);
let ntrip_str = if ntrip_state == 1 { "connected" } else { "disconnected" };

let json = format!(
    "{{\"uptime_s\":{},\"heap_free\":{},\"nmea_drops\":{},\"rtcm_drops\":{},\
     \"uart_tx_errors\":{},\"ntrip\":\"{}\"}}",
    uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors, ntrip_str
);
```

### Pattern 7: NVS Config Persistence for NTRIP

**What:** Persist host, port, mountpoint, user, pass in NVS namespace `"ntrip"`. All NVS keys must be ≤15 characters.

**Key names (all within 15-char limit):**
- `ntrip_host` (10 chars)
- `ntrip_port_hi` (13 chars)
- `ntrip_port_lo` (13 chars)
- `ntrip_mount` (11 chars)
- `ntrip_user` (10 chars)
- `ntrip_pass` (10 chars)

**Port storage:** Same two-u8 pattern as `mqtt_port_hi`/`mqtt_port_lo` (no `set_u16` in EspNvs). Default NTRIP port is 2101.

### Anti-Patterns to Avoid

- **Using gnss_cmd_tx for RTCM bytes:** `SyncSender<String>` sends ASCII lines terminated with `\r\n` by the TX thread. Binary RTCM bytes would be corrupted. Write directly to `Arc<UartDriver>`.
- **Using EspHttpClient for NTRIP:** Standard HTTP parsers reject `ICY 200 OK` as an invalid status line. Use raw `TcpStream`.
- **Not setting read timeout:** Without `set_read_timeout`, a silent TCP connection (caster up but no data) will block the NTRIP thread indefinitely with no reconnect. Set to ~60s.
- **Blocking the MQTT callback:** NTRIP config parsing must happen in the relay thread, not the callback. Callback only does `try_send`.
- **Deduplicating NTRIP config:** Unlike `config_relay.rs`, NTRIP config changes should always trigger a reconnect (even if payload is same hash) because the user may be force-reconnecting after a bad session.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TCP socket | Custom socket wrapper | `std::net::TcpStream` | ESP-IDF provides BSD sockets via lwIP; std::net works directly |
| Base64 encoding | Bit-shift loop | Custom ~30-line encoder or `base64` crate | Credentials are short; simple lookup-table encoder is correct and avoids dependency |
| RTCM byte forwarding | RTCM parser | Direct UART write | UM980 parses RTCM internally; firmware just needs to pipe bytes |

**Key insight:** NTRIP v1 is deliberately simple — the entire protocol is a single HTTP/1.0 GET request and then a raw byte stream. Don't over-engineer it.

---

## Common Pitfalls

### Pitfall 1: ICY 200 OK Not Valid HTTP
**What goes wrong:** Any HTTP client library (including `EspHttpClient`) will reject `ICY 200 OK` because it is not a valid HTTP status line — HTTP requires `HTTP/1.x NNN Reason`. NTRIP v1 inherits the `ICY` response from SHOUTcast.
**Why it happens:** Developers assume they can reuse an existing HTTP client.
**How to avoid:** Use raw `TcpStream`; parse the first response line manually.
**Warning signs:** HTTP client returns a parse error immediately after connect.

### Pitfall 2: TCP Connect Timeout Too Short (lwIP Limitation)
**What goes wrong:** lwIP's TCP connect uses its own internal backing-off (~18s total for 6 retries). `TcpStream::connect_timeout` may not be honoured precisely on ESP-IDF. Attempting to connect to an unreachable host can block the NTRIP thread for up to 18 seconds.
**Why it happens:** lwIP does not implement `SO_CONTIMEO`.
**How to avoid:** Accept the ~18s worst-case connect delay. Do NOT use an extremely short timeout and assume it will work. After connect fails (Err), log and apply the reconnect backoff delay before retrying.
**Warning signs:** NTRIP thread appears to hang for 15-20 seconds on each retry when host is unreachable.

### Pitfall 3: Concurrent UART Writes (NTRIP vs GNSS TX Thread)
**What goes wrong:** The GNSS TX thread and the NTRIP thread both write to the `Arc<UartDriver>`. `UartDriver::write(&self)` is not mutex-protected. If a command is sent to the UM980 mid-stream of an RTCM message, the UM980 sees corrupted bytes.
**Why it happens:** RTCM is a continuous stream; commands are sent infrequently but not atomically interleaved.
**How to avoid:** In practice, UM980 config commands are sent rarely (operator-triggered). Accept the theoretical race and log it. A future hardening could wrap the UART in a Mutex, but that risks priority inversion with the GNSS RX thread. Document the known race.
**Warning signs:** UM980 resets or loses RTK lock after a config command is sent while NTRIP is streaming.

### Pitfall 4: GGA Required by VRS Mountpoints
**What goes wrong:** Some NTRIP mountpoints (VRS/MAC network corrections) require the client to send a GGA sentence immediately after receiving `ICY 200 OK`, before RTCM data flows. Without it, the caster stream is silent.
**Why it happens:** VRS casters calculate a virtual reference station near the rover — they need the rover's approximate position.
**How to avoid:** The Phase 17 design streams from single-base mountpoints only (GGA not required). Document that VRS mountpoints requiring GGA are a future enhancement. The initial NTRIP connection should NOT send a GGA in Phase 17 (Phase 18 parses GGA from UM980 output, enabling a future follow-on). If a user configures a VRS mountpoint, the caster will accept the TCP connection but send no RTCM — the device will sit at `ntrip:connected` with 0 RTCM bytes flowing. This is acceptable behaviour for Phase 17.
**Warning signs:** Connection shows `connected` but UM980 never achieves RTK Float/Fix.

### Pitfall 5: NVS Key Length > 15 Characters
**What goes wrong:** NVS silently truncates or rejects keys longer than 15 characters. This was already hit with MQTT port (`mqtt_port_hi`, `mqtt_port_lo` pattern in provisioning.rs).
**Why it happens:** ESP-IDF NVS limit is exactly 15 ASCII chars for key names.
**How to avoid:** All proposed NTRIP NVS key names above are within the limit. Double-check before committing.

### Pitfall 6: NTRIP Config Payload Deduplication Should NOT Be Applied
**What goes wrong:** `config_relay.rs` deduplicates by djb2 hash — identical payload on reconnect is skipped. For NTRIP config, this is wrong: the user may want to force a reconnect by re-publishing the same config after a network failure.
**Why it happens:** Pattern copied from config_relay.
**How to avoid:** The NTRIP relay thread should always process received config payloads and always trigger a reconnect, even if the content is unchanged.

### Pitfall 7: Read Timeout Must Be Set
**What goes wrong:** NTRIP casters can keep a TCP connection alive but stop sending RTCM data (e.g., caster overloaded, mountpoint exhausted). Without a read timeout, the NTRIP thread blocks indefinitely.
**Why it happens:** `TcpStream::read` blocks until data arrives or connection closes.
**How to avoid:** Call `stream.set_read_timeout(Some(Duration::from_secs(60)))` immediately after connect. On `WouldBlock` or `TimedOut` error, treat as a connection drop and trigger reconnect.

---

## Code Examples

### Minimal NTRIP v1 Session (Conceptual)
```rust
// Source: NTRIP v1 specification (BKG), verified against use-snip.com NTRIP Rev1 format docs
use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;

fn run_ntrip_session(host: &str, port: u16, mountpoint: &str,
                     user: &str, pass: &str,
                     uart: &Arc<UartDriver>) -> Result<(), std::io::Error> {
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    // Send NTRIP v1 GET request
    let request = build_ntrip_request(mountpoint, user, pass);
    stream.write_all(request.as_bytes())?;

    // Read and validate response headers
    if !read_ntrip_headers(&mut stream)? {
        log::warn!("NTRIP: unexpected response (not ICY 200 OK)");
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "bad response"));
    }

    NTRIP_STATE.store(1, Ordering::Relaxed);
    log::info!("NTRIP: connected, streaming RTCM to UART");

    // Forward RTCM bytes to UART
    let mut buf = [0u8; 512];
    loop {
        match stream.read(&mut buf)? {
            0 => {
                log::warn!("NTRIP: connection closed by caster");
                break;
            }
            n => {
                if let Err(e) = uart.write(&buf[..n]) {
                    log::warn!("NTRIP: UART write error: {:?}", e);
                }
            }
        }
    }

    NTRIP_STATE.store(0, Ordering::Relaxed);
    Ok(())
}
```

### Reconnect Loop with Backoff
```rust
// Reconnect strategy: exponential backoff capped at 40s
// Source: rtkdata.com NTRIP connection flow recommendations
const NTRIP_BACKOFF_STEPS: &[u64] = &[5, 10, 20, 40];

fn ntrip_reconnect_loop(config: &NtripConfig, uart: &Arc<UartDriver>,
                         config_rx: &Receiver<Vec<u8>>) {
    let mut backoff_idx = 0usize;
    loop {
        match run_ntrip_session(&config.host, config.port, &config.mountpoint,
                                 &config.user, &config.pass, uart) {
            Ok(()) => { backoff_idx = 0; } // clean close, reset backoff
            Err(e) => {
                log::warn!("NTRIP: session error: {:?} — reconnecting in {}s",
                    e, NTRIP_BACKOFF_STEPS[backoff_idx]);
            }
        }
        NTRIP_STATE.store(0, Ordering::Relaxed);
        let delay = NTRIP_BACKOFF_STEPS[backoff_idx];
        backoff_idx = (backoff_idx + 1).min(NTRIP_BACKOFF_STEPS.len() - 1);
        // During backoff, check for config updates
        match config_rx.recv_timeout(Duration::from_secs(delay)) {
            Ok(payload) => { /* apply new config, reset backoff */ }
            Err(RecvTimeoutError::Timeout) => {} // continue with reconnect
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}
```

### Custom Base64 Encoder (No Dependency)
```rust
// Minimal base64 encoder for NTRIP Basic Auth — credentials are always short ASCII
// Source: RFC 4648 §4 standard base64 alphabet
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(ALPHABET[(b0 >> 2)] as char);
        out.push(ALPHABET[((b0 & 0x3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((b1 & 0xf) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[b2 & 0x3f] as char } else { '=' });
    }
    out
}
```

### NVS Config Struct Pattern
```rust
#[derive(Default)]
struct NtripConfig {
    host:       String,   // NVS key: "ntrip_host"     (10 chars)
    port:       u16,      // NVS keys: "ntrip_port_hi" + "ntrip_port_lo" (13 chars each)
    mountpoint: String,   // NVS key: "ntrip_mount"    (11 chars)
    user:       String,   // NVS key: "ntrip_user"     (10 chars)
    pass:       String,   // NVS key: "ntrip_pass"     (10 chars)
}
// Default port: 2101 (standard NTRIP TCP port)
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| NTRIP v1 only (ICY response) | NTRIP v2 (standard HTTP/1.1 200 OK) | ~2009 (RTCM SC-104) | v2 is compatible with standard HTTP parsers; most public casters support both |
| Single-base corrections | VRS/MAC network corrections | ~2005+ | VRS requires GGA; Phase 17 targets single-base (no GGA needed) |
| Unauthenticated streams | Basic Auth mandatory for most public casters | Ongoing | Always include Authorization header path |

**Deprecated/outdated:**
- NTRIP v1 GET without `\r\n` line endings: some older docs show `\n` only — use `\r\n` (CRLF) per spec.

---

## Open Questions

1. **GGA Requirement for Target Caster**
   - What we know: Single-base mountpoints don't require GGA; VRS does.
   - What's unclear: Whether the specific caster the user will configure uses single-base or VRS.
   - Recommendation: Document in firmware log that VRS mountpoints require a future GGA enhancement; proceed with no-GGA implementation.

2. **Concurrent UART Write Race**
   - What we know: `UartDriver::write(&self)` is not mutex-protected; both GNSS TX and NTRIP thread can write concurrently.
   - What's unclear: Whether lwIP/ESP-IDF UartDriver serialises internally at the driver level.
   - Recommendation: Accept the known race for Phase 17. Commands are rare; NTRIP bytes are continuous. If corruption observed in testing, wrap write path in a `Mutex<()>` guard.

3. **Stack Size for NTRIP Thread**
   - What we know: Other relay threads use 8192 bytes. NTRIP session has a 512-byte read buffer on stack plus TcpStream internals.
   - What's unclear: Exact stack usage of `std::net::TcpStream` on ESP-IDF.
   - Recommendation: Start with 8192, log HWM at thread entry (project standard), increase to 12288 if HWM < 20% free.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None detected — embedded firmware, no test runner found |
| Config file | None |
| Quick run command | `cargo build --release` (compile check) |
| Full suite command | `cargo build --release && cargo clippy` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| NTRIP-01 | NTRIP TCP connect + RTCM stream forwarding to UART | manual-only | Flash device, configure valid caster, verify UM980 RTK status | N/A |
| NTRIP-02 | Config via retained MQTT topic; NVS persistence | manual-only | Publish JSON to `/ntrip/config`; reboot; verify reconnection | N/A |
| NTRIP-03 | Automatic reconnect on TCP drop | manual-only | Kill caster TCP connection; verify reconnect within backoff window | N/A |
| NTRIP-04 | `ntrip` field in heartbeat JSON | manual-only | Subscribe to `/heartbeat`; verify field present and correct | N/A |

**Note:** All NTRIP requirements require hardware validation — an NTRIP caster endpoint and UM980 receiver. Compile-time checking via `cargo build --release` is the only automated gate. The project has no unit test infrastructure (no test files exist in src/).

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release && cargo clippy -- -D warnings`
- **Phase gate:** Hardware test — device achieves RTK Float/Fix with valid NTRIP source

### Wave 0 Gaps
- None for test infrastructure (no test runner in this project by design)
- Hardware prerequisites: access to a public or private NTRIP caster with valid credentials

---

## Wire-Up Changes to main.rs

For the planner's reference, the initialization sequence additions needed:

```
After Step 8 (channel creation):
  - Create (ntrip_config_tx, ntrip_config_rx) SyncSender<Vec<u8>>(4)

After Step 9 (mqtt_connect):
  - Pass ntrip_config_tx into mqtt_connect (new parameter)

After Step 9 (subscriber_loop):
  - Pass ntrip_config_topic subscription into subscriber_loop (new topic)

New Step 15c (after config relay, before RTCM relay):
  - ntrip_client::spawn_ntrip_client(uart_arc_clone, ntrip_config_rx, nvs.clone())

Heartbeat modification:
  - Read NTRIP_STATE atomic in heartbeat_loop JSON construction
```

The `uart` Arc must be extractable from `spawn_gnss` or the planner must restructure to pass an `Arc<UartDriver>` out of the GNSS init. Currently `spawn_gnss` consumes the peripheral and returns channels only — the UART Arc is internal. **This is the most significant structural change in Phase 17.** The planner should account for returning the `Arc<UartDriver>` from `spawn_gnss` (or creating a separate raw-bytes channel `SyncSender<Vec<u8>>` alongside the existing String channel).

---

## Sources

### Primary (HIGH confidence)
- [NTRIP v1 Specification PDF (BKG/ESA)](https://gssc.esa.int/wp-content/uploads/2018/07/NtripDocumentation.pdf) — protocol structure, ICY 200 OK, request format
- [NTRIP Rev1 vs Rev2 Formats (use-snip.com)](https://www.use-snip.com/kb/knowledge-base/ntrip-rev1-versus-rev2-formats/) — exact request/response format with headers
- [ESP-IDF lwIP documentation](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/lwip.html) — BSD socket availability, connect timeout limitation
- [NVS Flash documentation](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/storage/nvs_flash.html) — 15-char key/namespace limit
- Project source: `gnss.rs`, `mqtt.rs`, `config_relay.rs`, `provisioning.rs` — established patterns

### Secondary (MEDIUM confidence)
- [rtkdata.com NTRIP Connection Flow](https://rtkdata.com/blog/ntrip-connection-flow-explained/) — reconnect backoff recommendations (5s→10s→20s→40s)
- [use-snip.com GGA subtleties](https://www.use-snip.com/kb/knowledge-base/subtle-issues-with-using-ntrip-client-nmea-183-strings/) — VRS GGA requirement
- [esp-rs GitHub issue #350](https://github.com/esp-rs/esp-idf-svc/issues/350) — std::net::TcpStream availability confirmed via lwIP BSD sockets

### Tertiary (LOW confidence)
- [TcpStream connect_timeout lwIP issue](https://github.com/espressif/esp-idf/issues/8296) — SO_CONTIMEO not implemented; ~18s worst-case connect delay; flagged LOW as it's an older issue and lwIP may have changed

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — std::net::TcpStream confirmed via ESP-IDF lwIP/BSD; NVS patterns identical to provisioning.rs
- Architecture: HIGH — NTRIP v1 protocol is simple and well-documented; project patterns are clear
- Pitfalls: HIGH for ICY/EspHttpClient, GGA, NVS key length (verified); MEDIUM for concurrent UART write race (plausible, not hardware-confirmed)

**Research date:** 2026-03-08
**Valid until:** 2026-09-08 (stable — NTRIP v1 protocol is frozen; ESP-IDF socket API is stable)
