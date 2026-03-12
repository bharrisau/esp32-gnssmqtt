# Phase 25: Web UI + Remaining Gap Crate Skeletons - Research

**Researched:** 2026-03-12
**Domain:** Axum WebSocket HTTP server (gnss-server), NMEA GSV parsing, SVG polar skyplot + SNR bar chart in-browser JavaScript, heartbeat JSON parsing, and three nostd gap crate skeletons (gnss-softap, gnss-dns, gnss-log)
**Confidence:** HIGH for axum/WebSocket patterns and NMEA GSV format; HIGH for heartbeat JSON structure (read from firmware source); MEDIUM for gap crate blocker specifics (derived from Phase 22 nostd audit)

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| UI-01 | HTTP server serves static HTML; WebSocket pushes satellite state at ~1 Hz | axum 0.7 + tower-http ServeDir + tokio broadcast channel; pattern confirmed from official axum examples |
| UI-02 | Browser renders polar skyplot SVG from NMEA GSV elevation/azimuth/PRN | GSV sentence format verified from NMEA spec; nmea crate 0.7 `Satellite` accessor methods documented; pure SVG polar plot pattern in browser JS requires no third-party library |
| UI-03 | Browser renders SNR/C/N0 bar chart per satellite from NMEA GSV | GSV SNR field (0–99 dBHz) from nmea crate `Satellite::snr()`; bar chart via SVG `<rect>` elements coloured by constellation |
| UI-04 | Browser shows device health panel from MQTT heartbeat topic; updates within 35s | Heartbeat JSON structure read directly from firmware `src/mqtt.rs`; fields: `uptime_s`, `heap_free`, `fix_type`, `satellites`, `hdop`; server-side subscribed via `MqttMessage::Heartbeat` variant |
| NOSTD-04b | gnss-softap + gnss-dns + gnss-log crates exist with trait definitions and BLOCKER.md | Phase 22 nostd audit documents exact status of each; gnss-ota pattern from Phase 24 is the template |
</phase_requirements>

## Summary

Phase 25 has two independent work streams: (1) Web UI added to gnss-server, and (2) three nostd gap crate skeletons.

The Web UI stream adds an HTTP+WebSocket endpoint to the existing gnss-server binary. The server already receives `MqttMessage::Nmea` and `MqttMessage::Heartbeat` payloads but discards them (they are tagged in mqtt.rs but unprocessed by run_decode_task). This phase wires those payloads into new processing: NMEA sentences are parsed for GSV data and pushed to WebSocket clients; heartbeat JSON is forwarded to WebSocket clients. The HTTP layer uses axum 0.7 with a tokio broadcast channel as the fan-out primitive. The HTML page is embedded as a Rust string literal (no external file serving needed for a single-page app of this size) and serves via a GET `/` route. WebSocket clients connect to `/ws` and receive JSON messages at approximately 1 Hz. The browser renders a polar SVG skyplot and SNR bar chart using vanilla JavaScript — no npm, no bundler, no external CDN required.

The gap crate stream creates three crates following the gnss-ota pattern from Phase 24: trait definition in `#![no_std]` lib.rs, no external deps, and a BLOCKER.md documenting the specific reason a complete nostd implementation cannot be shipped today. The key distinctions: gnss-softap is blocked by absent production-ready no_std HTTP server with multi-field form POST parsing (picoserve exists but form-parsing maturity is unconfirmed); gnss-dns is NOT blocked by an impossibility but by absence of a turnkey crate (embassy-net UDP sockets can do the job, ~50 lines of custom DNS response code required); gnss-log is blocked only by requiring C FFI (`esp_log_set_vprintf()`) to capture C component logs — a pure-Rust-only log hook without any C call is not possible in the esp-hal context.

**Primary recommendation:** Add axum + tower-http + serde_json to gnss-server; add nmea crate for GSV parsing; embed HTML/JS as a Rust string constant. Create three gap crate skeletons following gnss-ota pattern. Both streams are independent and can be developed in parallel.

## Standard Stack

### Core (gnss-server Web UI)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | 0.7 | HTTP + WebSocket server built on hyper/tower | Official tokio-rs web framework; integrates natively with tokio; `ws` feature provides WebSocket upgrade |
| tower-http | 0.5 | `ServeDir`/`ServeFile` middleware for static assets | Required peer for axum 0.7 (axum 0.7 + tower-http must both use http 1.x) |
| tokio::sync::broadcast | (tokio 1.x, already dep) | Fan-out channel — one sender, N WebSocket receiver tasks | Built into tokio; the idiomatic pattern for broadcasting to multiple WebSocket clients |
| serde_json | 1 | Serialize satellite state and heartbeat JSON for WebSocket push | Standard Rust JSON; already transitively available; add explicitly |
| nmea | 0.7 | Parse NMEA GSV sentences to extract satellite PRN, elevation, azimuth, SNR | Provides `parse_gsv` + `Satellite` struct with accessor methods `.prn()`, `.elevation()`, `.azimuth()`, `.snr()` |

### Supporting (gap crates)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none) | — | All three gap crates are trait-only with no external deps | Trait-only skeletons follow gnss-ota pattern |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| axum 0.7 | warp, actix-web | axum is the tokio-rs official framework; already using tokio; most consistent choice |
| axum 0.7 | hyper directly | axum is a thin ergonomic layer over hyper; no cost for the abstraction |
| tokio broadcast | dashmap of senders | broadcast is the idiomatic tokio pattern for this; dashmap adds complexity |
| nmea crate | hand-rolled GSV parser | nmea crate is `no_std`-compatible, well-tested; GSV multi-sentence accumulation is tricky to hand-roll |
| SVG in JS | Chart.js / D3.js | No CDN dependency required; polar SVG and bar chart are ~100 lines of vanilla JS each; simpler, no build step |
| Embedded HTML string | tower-http ServeDir for HTML | For a single-page app, embedding avoids file path dependencies and simplifies deployment |

**Installation (gnss-server):**
```bash
# In gnss-server/Cargo.toml — add these new dependencies:
# axum = { version = "0.7", features = ["ws"] }
# tower-http = { version = "0.5", features = ["fs"] }
# serde_json = "1"
# nmea = "0.7"
```

**No new workspace dependencies for gap crates** — gnss-softap, gnss-dns, gnss-log are trait-only, no external deps.

## Architecture Patterns

### Recommended Project Structure

```
gnss-server/src/
├── main.rs              # updated: spawn web_server task alongside mqtt_supervisor
├── mqtt.rs              # unchanged: MqttMessage::Nmea and Heartbeat already defined
├── web_server.rs        # NEW: axum router, WebSocket handler, broadcast sender
├── nmea_parse.rs        # NEW: GSV sentence accumulator + SatelliteState struct
├── config.rs            # updated: add optional http_port (default 8080)
│
crates/
├── gnss-softap/
│   ├── Cargo.toml
│   ├── src/lib.rs       # SoftApPortal trait
│   └── BLOCKER.md
├── gnss-dns/
│   ├── Cargo.toml
│   ├── src/lib.rs       # CaptiveDnsResponder trait
│   └── BLOCKER.md
└── gnss-log/
    ├── Cargo.toml
    ├── src/lib.rs       # MqttLogHook trait
    └── BLOCKER.md
```

### Pattern 1: axum WebSocket with tokio broadcast

**What:** Each WebSocket client task subscribes to a `broadcast::Sender<String>`. The NMEA/heartbeat processing task publishes to the sender at ~1 Hz. Each client task forwards received strings to its WebSocket sink.

**When to use:** Any server-push pattern with multiple simultaneous browser clients.

```rust
// Source: axum 0.7 official websocket example + tokio broadcast docs
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state.tx.subscribe()))
}

async fn handle_socket(
    mut socket: axum::extract::ws::WebSocket,
    mut rx: broadcast::Receiver<String>,
) {
    while let Ok(msg) = rx.recv().await {
        if socket
            .send(axum::extract::ws::Message::Text(msg.into()))
            .await
            .is_err()
        {
            break; // client disconnected
        }
    }
}
```

### Pattern 2: Wiring MqttMessage into broadcast

**What:** The existing `run_decode_task` in main.rs processes only `MqttMessage::Rtcm`. Phase 25 must also process `MqttMessage::Nmea` and `MqttMessage::Heartbeat`. Add the broadcast sender as a parameter to `run_decode_task`.

```rust
// Source: existing gnss-server/src/main.rs structure + axum broadcast pattern
async fn run_decode_task(
    mut msg_rx: mpsc::Receiver<MqttMessage>,
    output_dir: String,
    station: String,
    ws_tx: broadcast::Sender<String>,  // NEW: WebSocket broadcast sender
) {
    let gps_week = rinex_writer::current_gps_week();
    let mut epoch_buf = epoch::EpochBuffer::new();
    let mut obs_writer = rinex_writer::RinexObsWriter::new(&output_dir, station.clone(), gps_week);
    let mut nav_writer = rinex_writer::RinexNavWriter::new(&output_dir, station, gps_week);
    let mut gsv_acc = nmea_parse::GsvAccumulator::new();  // NEW

    while let Some(msg) = msg_rx.recv().await {
        match msg {
            MqttMessage::Rtcm(payload) => { /* existing code unchanged */ }
            MqttMessage::Nmea(payload) => {
                if let Ok(s) = std::str::from_utf8(&payload) {
                    if let Some(update) = gsv_acc.feed(s) {
                        let json = serde_json::to_string(&update).unwrap_or_default();
                        let _ = ws_tx.send(json);  // lagged receivers dropped silently
                    }
                }
            }
            MqttMessage::Heartbeat(payload) => {
                if let Ok(s) = std::str::from_utf8(&payload) {
                    // Tag with type for browser-side dispatch
                    let tagged = format!("{{\"type\":\"heartbeat\",\"data\":{}}}", s);
                    let _ = ws_tx.send(tagged);
                }
            }
        }
    }
}
```

### Pattern 3: NMEA GSV Accumulation

**What:** GSV sentences arrive in groups (one group per talker ID per cycle, up to 4 sats per sentence). The accumulator collects all sentences in a cycle, then emits a single `SatelliteState` when the last sentence in the group arrives.

**Critical detail:** The UM980 outputs `$GNGSV` (multi-constellation) sentences. The nmea crate recognises talker IDs GP, GL, GA, GB, GN — all handled by `parse_gsv`. The `gnss_type` field in `GsvData` distinguishes the system.

```rust
// Source: nmea crate 0.7 docs.rs/nmea + NMEA 0183 GSV spec
use nmea::sentences::{GsvData, parse_gsv};

pub struct GsvAccumulator {
    satellites: Vec<SatInfo>,
    last_emit: std::time::Instant,
}

#[derive(serde::Serialize)]
pub struct SatInfo {
    pub prn: u32,
    pub elevation: Option<f32>,  // degrees, 0-90
    pub azimuth: Option<f32>,    // degrees, 0-359
    pub snr: Option<f32>,        // dBHz, 0-99; None when not tracking
    pub gnss_type: String,       // "GPS", "GLONASS", "GALILEO", "BEIDOU"
}

#[derive(serde::Serialize)]
pub struct SatelliteState {
    #[serde(rename = "type")]
    pub msg_type: &'static str,  // "satellites"
    pub satellites: Vec<SatInfo>,
}

impl GsvAccumulator {
    pub fn feed(&mut self, sentence: &str) -> Option<SatelliteState> {
        // Only process GSV sentences
        if !sentence.contains("GSV") { return None; }
        let data: &[u8] = sentence.as_bytes();
        let gsv = match parse_gsv(data) {
            Ok(gsv) => gsv,
            Err(_) => return None,
        };
        // Add sats from this sentence
        for sat_opt in &gsv.sats_info {
            if let Some(sat) = sat_opt {
                self.satellites.push(SatInfo {
                    prn: sat.prn(),
                    elevation: sat.elevation(),
                    azimuth: sat.azimuth(),
                    snr: sat.snr(),
                    gnss_type: gnss_type_str(gsv.gnss_type),
                });
            }
        }
        // Emit when the last sentence in the group arrives
        if gsv.sentence_num == gsv.number_of_sentences {
            let state = SatelliteState {
                msg_type: "satellites",
                satellites: std::mem::take(&mut self.satellites),
            };
            self.last_emit = std::time::Instant::now();
            return Some(state);
        }
        None
    }
}
```

**Rate limiting:** The UM980 at 5 Hz emits GSV groups every ~200ms. The broadcast push will fire on each complete GSV group (several times per second). To stay at ~1 Hz for the browser, either: (a) only emit if `last_emit.elapsed() > 900ms`, or (b) emit every complete group but the browser updates its animation frame rate separately. Option (b) is simpler and meets the "approximately 1 Hz" requirement since GSV rate is configurable.

### Pattern 4: Heartbeat JSON Structure (from firmware source)

**Critical: read directly from `firmware/src/mqtt.rs` heartbeat_loop().**

The heartbeat JSON is hand-formatted in firmware and has this exact structure:

```json
{
  "uptime_s": 12345,
  "heap_free": 234560,
  "nmea_drops": 0,
  "rtcm_drops": 0,
  "uart_tx_errors": 0,
  "ntrip": "disconnected",
  "mqtt_enqueue_errors": 0,
  "mqtt_outbox_drops": 0,
  "fix_type": 1,
  "satellites": 14,
  "hdop": 0.9
}
```

**UI-04 requires:** `uptime_s`, `fix_type`, `satellites`, `hdop`, `heap_free`. All five fields are present. `fix_type` and `satellites` are integers (or `null` if no GGA received). `hdop` is a float with one decimal (or `null`).

The server does not need to deserialize this — it forwards the raw JSON string as-is, wrapped in a type tag: `{"type":"heartbeat","data":{...}}`.

### Pattern 5: Polar SVG Skyplot in Browser JavaScript

**What:** Pure SVG skyplot generated from satellite azimuth/elevation. No external libraries. Azimuth maps to angle (0=North, clockwise). Elevation maps to radius (90=centre=zenith, 0=edge=horizon).

```javascript
// Source: NMEA polar plot geometry (standard GNSS display convention)
// Coordinate system: azimuth 0=North, clockwise. Elevation 0=horizon, 90=zenith.
// SVG coordinates: (cx, cy) = centre; radius r = cx.
// r_sat = r * (1 - elevation/90)  — zenith at centre, horizon at edge

function azel_to_xy(az_deg, el_deg, cx, cy, r) {
    const az_rad = (az_deg - 90) * Math.PI / 180; // rotate so 0=North maps to SVG up
    const r_sat = r * (1.0 - el_deg / 90.0);
    const x = cx + r_sat * Math.cos(az_rad);
    const y = cy + r_sat * Math.sin(az_rad);
    return [x, y];
}

function drawSkyplot(svgEl, satellites) {
    svgEl.innerHTML = '';
    const cx = 150, cy = 150, r = 140;
    // Draw concentric rings at 0, 30, 60, 90 deg elevation
    for (const elRing of [0, 30, 60, 90]) {
        const rRing = r * (1 - elRing / 90);
        const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        circle.setAttribute('cx', cx); circle.setAttribute('cy', cy);
        circle.setAttribute('r', rRing); circle.setAttribute('fill', 'none');
        circle.setAttribute('stroke', '#ccc'); circle.setAttribute('stroke-width', '1');
        svgEl.appendChild(circle);
    }
    // Draw cardinal directions
    for (const [label, ax, ay] of [['N',cx,cy-r-12],['S',cx,cy+r+12],['E',cx+r+12,cy],['W',cx-r-12,cy]]) {
        const t = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        t.setAttribute('x', ax); t.setAttribute('y', ay);
        t.setAttribute('text-anchor', 'middle'); t.setAttribute('font-size', '12');
        t.textContent = label;
        svgEl.appendChild(t);
    }
    // Draw satellites
    for (const sat of satellites) {
        if (sat.elevation == null || sat.azimuth == null) continue;
        const [x, y] = azel_to_xy(sat.azimuth, sat.elevation, cx, cy, r);
        const dot = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        dot.setAttribute('cx', x); dot.setAttribute('cy', y);
        dot.setAttribute('r', 8); dot.setAttribute('fill', gnssColor(sat.gnss_type));
        svgEl.appendChild(dot);
        const lbl = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        lbl.setAttribute('x', x); lbl.setAttribute('y', y - 10);
        lbl.setAttribute('text-anchor', 'middle'); lbl.setAttribute('font-size', '9');
        lbl.textContent = sat.prn;
        svgEl.appendChild(lbl);
    }
}
```

### Pattern 6: SNR Bar Chart in Browser JavaScript

**What:** One SVG `<rect>` per satellite, coloured by constellation, height proportional to SNR (0–60 dBHz typical range).

```javascript
// Source: SVG bar chart convention + GNSS SNR range (0-99 dBHz per NMEA spec)
function drawSnrChart(svgEl, satellites) {
    svgEl.innerHTML = '';
    const w = 300, h = 120, barW = Math.max(4, Math.floor(w / (satellites.length + 1)));
    for (let i = 0; i < satellites.length; i++) {
        const sat = satellites[i];
        const snr = sat.snr || 0;
        const barH = Math.round((snr / 60) * h);  // 60 dBHz = full height
        const x = i * (barW + 2) + 2;
        const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('x', x); rect.setAttribute('y', h - barH);
        rect.setAttribute('width', barW); rect.setAttribute('height', barH);
        rect.setAttribute('fill', gnssColor(sat.gnss_type));
        svgEl.appendChild(rect);
        // PRN label below bar
        const lbl = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        lbl.setAttribute('x', x + barW/2); lbl.setAttribute('y', h + 12);
        lbl.setAttribute('text-anchor', 'middle'); lbl.setAttribute('font-size', '8');
        lbl.textContent = sat.prn;
        svgEl.appendChild(lbl);
    }
}

function gnssColor(gnssType) {
    return { GPS: '#3498db', GLONASS: '#e74c3c', GALILEO: '#2ecc71',
             BEIDOU: '#f39c12' }[gnssType] || '#95a5a6';
}
```

### Pattern 7: Gap Crate Skeleton (following gnss-ota template)

All three crates follow the exact pattern established by `crates/gnss-ota/`:
- `#![no_std]` in lib.rs
- No external dependencies in Cargo.toml
- Comments in Cargo.toml listing what an implementation would need
- BLOCKER.md documenting specifically what prevents a nostd implementation today

**gnss-softap trait concept:**
```rust
#![no_std]
// Source: firmware/src/provisioning.rs API analysis

/// Runs a SoftAP captive portal for initial device provisioning.
///
/// Implementations:
/// - ESP-IDF: `provisioning.rs` using EspHttpServer + BlockingWifi
/// - nostd: blocked — see BLOCKER.md
pub trait SoftApPortal {
    type Error: core::fmt::Debug;

    /// Start the SoftAP with the given SSID and WPA2-PSK password.
    fn start(&mut self, ssid: &str, password: &str) -> Result<(), Self::Error>;

    /// Block until a valid credential set is submitted via the web form.
    /// Returns collected credentials.
    fn wait_for_credentials(&mut self) -> Result<ProvisioningCredentials, Self::Error>;

    /// Stop the SoftAP and release resources.
    fn stop(&mut self) -> Result<(), Self::Error>;
}

pub struct ProvisioningCredentials {
    pub wifi_networks: heapless::Vec<WifiNetwork, 3>,
    pub mqtt_host: heapless::String<64>,
    pub mqtt_port: u16,
    // ... other fields
}
```

**gnss-dns trait concept:**
```rust
#![no_std]
// Source: firmware/src/provisioning.rs DNS hijack (UdpSocket port 53)

/// Captive portal DNS responder — replies to all DNS queries with a fixed IP.
///
/// Implementations:
/// - ESP-IDF: `std::net::UdpSocket` bound to port 53 (in provisioning.rs)
/// - nostd: SOLVABLE but no turnkey crate — see BLOCKER.md
pub trait CaptiveDnsResponder {
    type Error: core::fmt::Debug;

    /// Start listening on UDP port 53 and responding with `portal_ip` for all A queries.
    fn start(&mut self, portal_ip: [u8; 4]) -> Result<(), Self::Error>;

    /// Process one pending DNS query. Returns `Ok(true)` if a query was answered.
    /// Call in a loop while the captive portal is active.
    fn poll(&mut self) -> Result<bool, Self::Error>;

    /// Stop listening.
    fn stop(&mut self) -> Result<(), Self::Error>;
}
```

**gnss-log trait concept:**
```rust
#![no_std]
// Source: firmware/src/log_relay.rs MqttLogger analysis

/// Hook for capturing all log output (Rust + C components) for remote relay.
///
/// Implementations:
/// - ESP-IDF: MqttLogger composite log::Log + esp_log_set_vprintf() C FFI hook
/// - nostd: PARTIAL — pure-Rust log::Log side works; C component capture requires C FFI — see BLOCKER.md
pub trait LogHook {
    type Error: core::fmt::Debug;

    /// Install this hook as the global log handler.
    ///
    /// After this call, all Rust `log::` output is forwarded through `on_log`.
    fn install(self) -> Result<(), Self::Error>;
}

pub trait LogSink {
    /// Called for each log message. Implementation must be non-blocking.
    fn on_log(&self, level: LogLevel, message: &str);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel { Error, Warn, Info, Debug, Trace }
```

### Anti-Patterns to Avoid

- **Parsing heartbeat JSON with serde on the server side:** Not needed. The server forwards the raw payload bytes as UTF-8 string, wrapped in a type tag. No deserialization required server-side.
- **Using axum 0.6 patterns with axum 0.7:** axum 0.7 upgraded from http 0.2 to http 1.x. tower-http must be 0.5.x (not 0.4.x) to match. Mixing versions causes opaque type errors.
- **tokio broadcast with large capacity:** A capacity of 16 is sufficient for ~1 Hz messages. Lagged receivers (slow browsers) receive `RecvError::Lagged` and should reconnect — do not use unbounded channels.
- **GSV accumulation across talker IDs:** GP, GL, GN, GA, GB sentences are separate GSV groups. Each group has its own `number_of_sentences`. Do not mix satellites from different talker IDs in the same accumulation cycle.
- **Serializing gnss-softap/dns/log crates to no_std with external deps:** All three crates must remain dep-free in their default configuration. Adding heapless or similar requires a feature flag, not a mandatory dep.
- **BLOCKER.md claiming DNS hijack is impossible:** It is NOT impossible — embassy-net UDP sockets can implement it. The BLOCKER.md for gnss-dns should document "no turnkey crate exists; requires ~50 lines of custom DNS query parser" not "impossible."

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GSV sentence parsing | Custom split/parse | nmea 0.7 `parse_gsv` | Multi-sentence accumulation, checksum verification, talker ID normalization are all handled; hand-roll gets sentence boundaries wrong |
| WebSocket upgrade | Raw hyper WebSocket | axum `WebSocketUpgrade` extractor | Axum handles HTTP → WebSocket upgrade negotiation (101 Switching Protocols), ping/pong frames, close handshake |
| JSON serialization | Manual string formatting | serde_json | Avoids escaping bugs in satellite PRN/type strings; already a transitive dep in the workspace |
| Broadcast fan-out | Vec<mpsc::Sender> behind Mutex | tokio broadcast | tokio broadcast is O(1) send; Vec<Sender> is O(N) and requires locking |

**Key insight:** The SVG polar skyplot and bar chart are genuinely simple to implement in browser JS without libraries (~150 total lines). The complexity is in getting GSV accumulation right on the server side — use the nmea crate to avoid subtle multi-sentence edge cases.

## Common Pitfalls

### Pitfall 1: axum/tower-http Version Mismatch
**What goes wrong:** Adding `axum = "0.7"` and `tower-http = "0.4"` causes compile errors like "the trait `Service<http::Request<Body>>` is not satisfied".
**Why it happens:** axum 0.7 upgraded its http dependency from 0.2 to 1.x. tower-http 0.4 still uses http 0.2. The types are incompatible even though they have the same name.
**How to avoid:** Use `tower-http = "0.5"`. If in doubt, check that `axum` and `tower-http` both appear in `Cargo.lock` referencing `http = "1.x"`.
**Warning signs:** "type mismatch" or "trait not implemented" errors mentioning `http::Request` or `http::Response`.

### Pitfall 2: GSV Sentence Accumulation Reset
**What goes wrong:** Satellites from a previous cycle appear in the next emission because the accumulator is not cleared at the start of a new GSV group.
**Why it happens:** GSV sentences arrive as: sentence 1 of N, sentence 2 of N, ..., sentence N of N. A new group starts when `sentence_num == 1`. If the accumulator is only cleared when emitting (last sentence), leftover sats from an aborted cycle (e.g., fewer GSV sentences than expected) remain.
**How to avoid:** Clear `self.satellites` when `gsv.sentence_num == 1` (start of new group). Emit and clear on `gsv.sentence_num == gsv.number_of_sentences`.
**Warning signs:** Satellite count grows unboundedly; duplicate PRNs in skyplot.

### Pitfall 3: WebSocket Message Type Dispatch in Browser
**What goes wrong:** Browser receives both `{"type":"satellites",...}` and `{"type":"heartbeat",...}` on the same WebSocket. Without type dispatch, one handler overwrites the other.
**How to avoid:** On message receipt in JS: `const msg = JSON.parse(e.data); if (msg.type === 'satellites') updateSkyplot(msg); else if (msg.type === 'heartbeat') updateHealth(msg.data);`
**Warning signs:** Health panel or skyplot intermittently blank.

### Pitfall 4: run_decode_task Currently Handles Only Rtcm
**What goes wrong:** `MqttMessage::Nmea` and `MqttMessage::Heartbeat` exist in `mqtt.rs` and are sent on the channel, but `run_decode_task` only matches `MqttMessage::Rtcm`. All other variants are silently dropped.
**Why it happens:** Phases 23 and 24 only needed RTCM processing. The match arm `if let MqttMessage::Rtcm(payload) = msg` discards everything else.
**How to avoid:** Change to a full `match msg { MqttMessage::Rtcm(p) => {...}, MqttMessage::Nmea(p) => {...}, MqttMessage::Heartbeat(p) => {...} }`.
**Warning signs:** WebSocket client connects but never receives any satellite data.

### Pitfall 5: tokio broadcast Lagged Error
**What goes wrong:** A slow WebSocket client causes its `broadcast::Receiver` to lag. When it tries to receive, it gets `RecvError::Lagged(n)`. If unhandled, the WebSocket task panics.
**How to avoid:** Match `Err(broadcast::error::RecvError::Lagged(_))` and either continue (skip missed messages) or close the WebSocket and let the browser reconnect.
**Warning signs:** Server task panics with "RecvError::Lagged" in logs; client disconnects unexpectedly.

### Pitfall 6: gnss-dns BLOCKER.md Must Not Claim Impossibility
**What goes wrong:** BLOCKER.md for gnss-dns says "DNS hijack not possible in nostd" when Phase 22 audit explicitly identified it as SOLVABLE.
**Why it happens:** Confusion between "no turnkey crate" and "impossible".
**How to avoid:** BLOCKER.md for gnss-dns documents: "No turnkey captive-portal DNS crate exists for embassy-net. A complete implementation requires ~50 lines of custom DNS query parser (extract QNAME, respond with A record for portal IP). This is a maturity gap (missing crate), not a fundamental impossibility. See Phase 22 nostd-audit.md §9."
**Warning signs:** BLOCKER.md says "DNS not supported in embassy" — incorrect; flag for correction.

### Pitfall 7: gnss-log BLOCKER.md Scope
**What goes wrong:** BLOCKER.md for gnss-log says "logging not possible in nostd" when actually only the C component capture path requires C FFI.
**How to avoid:** BLOCKER.md clearly distinguishes: (1) Rust `log::Log` trait hook — fully portable, no C required, implementable in no_std; (2) C component log capture via `esp_log_set_vprintf()` — requires one C FFI call; the C function is available in ROM in any esp-hal build and is NOT an ESP-IDF-only API. The blocker is "pure-Rust-only C component log capture is not possible without C FFI" — not a hard impossibility.

## Code Examples

Verified patterns from official sources and codebase analysis:

### axum 0.7 Router Setup (gnss-server)

```rust
// Source: axum 0.7 official example + tokio broadcast pattern
use axum::{
    extract::{State, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use tokio::sync::broadcast;

const INDEX_HTML: &str = include_str!("../static/index.html");
// OR: const INDEX_HTML: &str = "<!DOCTYPE html>..."; // embedded literal

pub async fn run_web_server(
    port: u16,
    ws_tx: broadcast::Sender<String>,
) -> anyhow::Result<()> {
    let state = AppState { ws_tx };
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.ws_tx.subscribe()))
}
```

### WebSocket Per-Client Handler

```rust
// Source: axum 0.7 websockets example (github.com/tokio-rs/axum/examples/websockets)
use axum::extract::ws::{Message, WebSocket};
use tokio::sync::broadcast;

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<String>,
) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break; // client disconnected
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue, // skip missed
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Optional: handle incoming messages from browser (ping, close)
            msg = socket.recv() => {
                if msg.is_none() { break; } // client closed
            }
        }
    }
}
```

### gnss-server/Cargo.toml additions

```toml
# Source: axum 0.7 release notes + tower-http 0.5 compatibility note
[dependencies]
# existing deps...
axum = { version = "0.7", features = ["ws"] }
tower-http = { version = "0.5", features = ["fs"] }
serde_json = "1"
nmea = "0.7"
```

### gnss-softap Cargo.toml template

```toml
[package]
name = "gnss-softap"
version = "0.1.0"
edition = "2021"
description = "SoftAP captive portal trait for embedded GNSS firmware — nostd gap crate"

[dependencies]
# No external dependencies — trait-only skeleton
# ESP-IDF implementation would add: esp-idf-svc, embedded-svc, heapless
# nostd implementation blocked — see BLOCKER.md
```

### Heartbeat JSON Tag Pattern

```rust
// Source: firmware/src/mqtt.rs heartbeat_loop() — exact field names verified
// Wraps raw heartbeat JSON with a type discriminator for browser dispatch
MqttMessage::Heartbeat(payload) => {
    if let Ok(s) = std::str::from_utf8(&payload) {
        // s is: {"uptime_s":N,"heap_free":N,"fix_type":N,"satellites":N,"hdop":N.N,...}
        let tagged = format!(r#"{{"type":"heartbeat","data":{}}}"#, s);
        let _ = ws_tx.send(tagged);
    }
}
```

### Browser WebSocket Dispatcher

```javascript
// Source: Web API standard (WebSocket, JSON.parse)
const ws = new WebSocket('ws://' + location.host + '/ws');
ws.onmessage = function(e) {
    const msg = JSON.parse(e.data);
    if (msg.type === 'satellites') {
        updateSkyplot(msg.satellites);
        updateSnrChart(msg.satellites);
    } else if (msg.type === 'heartbeat') {
        updateHealth(msg.data);
    }
};

function updateHealth(d) {
    document.getElementById('uptime').textContent =
        d.uptime_s != null ? Math.floor(d.uptime_s / 3600) + 'h ' + (Math.floor(d.uptime_s/60)%60) + 'm' : '--';
    document.getElementById('fix_type').textContent =
        {0:'No fix',1:'SPS',2:'DGPS',4:'RTK Fixed',5:'RTK Float'}[d.fix_type] ?? '--';
    document.getElementById('satellites').textContent = d.satellites ?? '--';
    document.getElementById('hdop').textContent = d.hdop ?? '--';
    document.getElementById('heap').textContent =
        d.heap_free != null ? Math.round(d.heap_free/1024) + ' KB' : '--';
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| axum 0.6 + tower-http 0.4 (http 0.2) | axum 0.7 + tower-http 0.5 (http 1.x) | Nov 2023 | Breaking: types are different even with same names; must use matching versions |
| esp-wifi (renamed) | esp-radio (replaces esp-wifi) | esp-hal 1.0-rc.1 Oct 2024 | SoftAP WPA2 password blocker from STATE.md is now RESOLVED in esp-radio |
| SoftAP password was a hard blocker | esp-radio `AccessPointConfig::with_password().with_auth_method(Wpa2Personal)` | esp-radio 0.16.x | SoftAP gap crate BLOCKER.md should note this is now resolved in esp-radio but no_std HTTP server with form parsing remains the gap |

**Deprecated/outdated:**
- `axum 0.6` patterns: `Router::new().route(...).layer(Extension(...))` — replaced by `Router::new().route(...).with_state(...)`
- `tower-http 0.4`: incompatible with axum 0.7 due to http crate version split

## Open Questions

1. **GSV 1 Hz rate limiting**
   - What we know: UM980 at 5 Hz outputs GSV groups 5×/sec; the broadcast push will fire 5×/sec
   - What's unclear: Does the browser handle 5 updates/sec gracefully, or should the server throttle?
   - Recommendation: Throttle on server side with `last_emit.elapsed() >= Duration::from_millis(900)` guard in GsvAccumulator. This satisfies the "approximately 1 Hz" requirement.

2. **HTML embedding vs file serving**
   - What we know: gnss-server is a single binary deployed without a web root directory
   - What's unclear: Should the HTML be `include_str!("../static/index.html")` (file at compile time) or a string literal inline in web_server.rs?
   - Recommendation: Use `include_str!` with `gnss-server/static/index.html`. This keeps HTML/JS in a separate file for readability while still embedding it at compile time. No runtime file I/O needed.

3. **port configuration**
   - What we know: ServerConfig currently has no HTTP port field
   - What's unclear: Should http_port be required or default to 8080?
   - Recommendation: Add `#[serde(default = "default_http_port")] pub http_port: u16` defaulting to 8080. Environment variable `GNSS_HTTP_PORT` overrides.

4. **gnss-dns blocker precision**
   - What we know: Phase 22 audit calls DNS hijack SOLVABLE; the gap is "no turnkey crate"
   - What's unclear: Whether BLOCKER.md should say "solvable, pending crate" or just document what you need to write
   - Recommendation: BLOCKER.md documents: the specific code pattern needed (~50 lines), which embassy-net APIs to use, and why there is no turnkey solution. This is more actionable than "impossible" and correctly reflects the audit finding.

## Validation Architecture

`workflow.nyquist_validation` key is absent from config.json — treat as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test (`cargo test`) |
| Config file | none — `#[cfg(test)]` modules inline |
| Quick run command | `cargo test -p gnss-server -- nmea_parse web` |
| Full suite command | `cargo test --workspace --exclude esp32-gnssmqtt-firmware` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| UI-01 | HTTP GET `/` returns HTML; `/ws` upgrades to WebSocket | smoke | `cargo test -p gnss-server -- web_server::tests` | ❌ Wave 0 |
| UI-02 | GSV accumulator emits correct elevation/azimuth/PRN from a sample GSV string | unit | `cargo test -p gnss-server -- nmea_parse::tests::gsv_accumulator` | ❌ Wave 0 |
| UI-03 | GSV accumulator captures SNR field; None for untracked satellites | unit | `cargo test -p gnss-server -- nmea_parse::tests::gsv_snr_null` | ❌ Wave 0 |
| UI-04 | Heartbeat JSON forwarding: raw payload wrapped with type tag | unit | `cargo test -p gnss-server -- nmea_parse::tests::heartbeat_tag` | ❌ Wave 0 |
| NOSTD-04b | gnss-softap/dns/log crates compile in no_std context | build | `cargo check --target thumbv7em-none-eabihf -p gnss-softap -p gnss-dns -p gnss-log` | ❌ Wave 0 |

**UI-01 full browser render is manual-only** — verifying that the SVG skyplot actually renders visually requires a browser. The automated test validates the server endpoint structure only.

### Sampling Rate

- **Per task commit:** `cargo test -p gnss-server -- nmea_parse && cargo clippy -p gnss-softap -p gnss-dns -p gnss-log -- -D warnings`
- **Per wave merge:** `cargo clippy --workspace --exclude esp32-gnssmqtt-firmware -- -D warnings && cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **Phase gate:** Full suite green before `/gsd:verify-work`; UI manual validation (browser rendering) documented in VALIDATION.md

### Wave 0 Gaps

- [ ] `gnss-server/src/nmea_parse.rs` — covers UI-02, UI-03 with `#[cfg(test)]` module
- [ ] `gnss-server/src/web_server.rs` — covers UI-01, UI-04
- [ ] `gnss-server/static/index.html` — HTML + JS for skyplot, bar chart, health panel
- [ ] `crates/gnss-softap/Cargo.toml`, `src/lib.rs`, `BLOCKER.md` — covers NOSTD-04b
- [ ] `crates/gnss-dns/Cargo.toml`, `src/lib.rs`, `BLOCKER.md` — covers NOSTD-04b
- [ ] `crates/gnss-log/Cargo.toml`, `src/lib.rs`, `BLOCKER.md` — covers NOSTD-04b

## Sources

### Primary (HIGH confidence)

- `firmware/src/mqtt.rs` — Heartbeat JSON structure verified from source: `heartbeat_loop()` at line 438; exact fields: `uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`, `ntrip`, `mqtt_enqueue_errors`, `mqtt_outbox_drops`, `fix_type`, `satellites`, `hdop`
- `gnss-server/src/mqtt.rs` — `MqttMessage` enum: `Rtcm`, `Nmea`, `Heartbeat` variants confirmed; `run_decode_task` only processes `Rtcm` today
- `docs/nostd-audit.md` — Phase 22 nostd audit: gnss-softap gap (HTTP server form parsing), gnss-dns gap (no turnkey crate, SOLVABLE), gnss-log gap (C FFI required for C component capture); all statuses verified
- `crates/gnss-ota/src/lib.rs` and `crates/gnss-ota/BLOCKER.md` — Template for gap crate structure; trait-only, no_std, no external deps
- `docs.rs/nmea` v0.7 — `Satellite` struct accessor methods: `.prn() -> u32`, `.elevation() -> Option<f32>`, `.azimuth() -> Option<f32>`, `.snr() -> Option<f32>`; `GsvData.gnss_type` for constellation identification
- receiverhelp.trimble.com/alloy-gnss/en-us/NMEA-0183messages_GSV.html — GSV sentence format: field layout (PRN, elevation, azimuth, SNR), up to 4 sats per sentence, multiple sentences per cycle

### Secondary (MEDIUM confidence)

- WebSearch: axum 0.7 + tower-http 0.5 version pairing confirmed (http 1.x requirement); `tokio::sync::broadcast` as idiomatic WebSocket fan-out pattern confirmed from multiple sources including official axum examples discussion
- WebSearch: axum WebSocket handler pattern (`WebSocketUpgrade` extractor, per-client task, `broadcast::Receiver`) confirmed from axum 0.7 websocket example reference
- `.planning/phases/22-workspace-nostd-audit/22-RESEARCH.md` — picoserve as primary no_std HTTP server candidate; nanofish as alternative; form parsing maturity unconfirmed

### Tertiary (LOW confidence)

- WebSearch (SVG polar skyplot): azimuth-to-SVG coordinate mapping (`az_rad = (az - 90) * pi/180` for North-up convention); elevation-to-radius mapping (`r_sat = r * (1 - el/90)`) — standard GNSS display convention, unverified against a reference implementation

## Metadata

**Confidence breakdown:**

- Web UI stack (axum + broadcast + nmea): HIGH — library versions and patterns confirmed from official axum examples and nmea docs.rs
- Heartbeat JSON fields: HIGH — read directly from firmware source
- GSV accumulation pattern: HIGH — NMEA GSV spec verified; nmea crate accessor methods confirmed from docs.rs
- SVG polar/bar chart geometry: MEDIUM — standard convention; not verified against a reference library
- Gap crate blocker specifics: MEDIUM — derived from Phase 22 nostd-audit.md (written 2026-03-12 with human review)
- picoserve form-parsing maturity: LOW — mentioned as candidate in audit but not independently verified for multi-field POST parsing

**Research date:** 2026-03-12
**Valid until:** 2026-06-12 (axum 0.7 is stable; nmea crate 0.7 is stable; gap crate ecosystem moves faster — re-check picoserve form parsing status before writing gnss-softap BLOCKER.md)
