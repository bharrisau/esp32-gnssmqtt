# Architecture Research

**Domain:** ESP32-C6 GNSS/MQTT firmware + companion server + nostd audit (milestone v2.1)
**Researched:** 2026-03-12
**Confidence:** HIGH for firmware architecture (all shipped and validated); HIGH for workspace structure (verified against Cargo docs); MEDIUM for rtcm-rs crate capabilities (confirmed via GitHub but not integration-tested); MEDIUM for rinex crate write maturity (navigation writer marked under construction); LOW for embassy gap coverage completeness (ecosystem moving fast, esp-hal 1.0 beta Feb 2025)

---

## Context: What Is Already Shipped (v2.0)

The firmware is complete and validated. This document focuses on what is NEW for v2.1: the companion server binary, the live web UI, and the embassy/nostd audit. The prior architecture is preserved below as a reference baseline.

### v2.0 Module Map (Firmware — Do Not Change)

```
src/
├── main.rs           — wiring hub; strict 18-step init order
├── config.rs         — compile-time constants (gitignored)
├── device_id.rs      — eFuse MAC → 6-char hex device_id
├── gnss.rs           — UART owner; RxState machine; pool-backed RTCM frames
├── gnss_state.rs     — GGA atomics (fix_type, satellites, hdop)
├── nmea_relay.rs     — Receiver<(String,String)> → gnss/{id}/nmea/{TYPE}
├── rtcm_relay.rs     — Receiver<RtcmFrame> → gnss/{id}/rtcm (pool return)
├── config_relay.rs   — MQTT config → UM980 UART; NVS persist; apply_config pub
├── mqtt.rs           — mqtt_connect (13 args); heartbeat; subscriber; cmd relay
├── mqtt_publish.rs   — single publish thread; SyncSender<MqttMessage>; no Arc<Mutex>
├── ntrip_client.rs   — NTRIP v1 TCP+TLS; EspTls; auto-reconnect
├── log_relay.rs      — composite logger; vprintf hook; LOG_REENTERING guard
├── ota.rs            — HTTP OTA; SHA256; EspOta; rollback-safe
├── provisioning.rs   — SoftAP portal; DNS hijack; NVS credentials
├── uart_bridge.rs    — stdin → gnss_cmd_tx (espflash monitor bridge)
├── led.rs            — Arc<AtomicU8> LED state machine
├── resil.rs          — resilience helpers
├── watchdog.rs       — software watchdog; GNSS_RX_HEARTBEAT atomic
└── wifi.rs           — wifi_connect_any; wifi_supervisor; backoff
```

### v2.0 Channel Topology (Firmware — Stable)

```
gnss.rs RX thread (RxState machine)
    │── SyncSender<(String,String)> [128] ──▶ nmea_relay ──▶ mqtt_tx
    │── SyncSender<RtcmFrame>       [32]  ──▶ rtcm_relay ──▶ mqtt_tx
    └── SyncSender<()>              [1]   ──▶ UM980 reboot monitor

mqtt_publish thread (owns EspMqttClient exclusively)
    ◀── SyncSender<MqttMessage> (cloned by all relay threads)

MQTT callback (ESP-IDF C task, via closure)
    ├── subscribe_tx [2]       ──▶ subscriber_loop
    ├── status_tx    [2]       ──▶ heartbeat_loop
    ├── config_tx    [4]       ──▶ config_relay
    ├── ota_tx       [1]       ──▶ ota.rs
    ├── cmd_relay_tx [4]       ──▶ command_relay_task
    ├── log_level_tx [4]       ──▶ log_level_relay_task
    └── ntrip_config_tx [4]   ──▶ ntrip_client

NTRIP client ──▶ Arc<UartDriver> (shared with gnss TX thread)
```

---

## System Overview — v2.1 Architecture

The v2.1 milestone adds three distinct new components:

1. **Server binary** (`server/`) — Tokio async, subscribes to MQTT, decodes RTCM3 MSM, writes RINEX files, serves HTTP+WebSocket
2. **Browser UI** (`server/static/`) — Polar skyplot SVG, SNR bar chart, device health panel via WebSocket
3. **Nostd audit crates** (`crates/gnss-hal-traits/`, `crates/gnss-nvs/`, etc.) — trait definitions + gap crate skeletons

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         MQTT BROKER (external)                           │
│  gnss/{id}/rtcm     gnss/{id}/nmea/{TYPE}     gnss/{id}/heartbeat        │
└──────────────────┬──────────────────┬───────────────────┬───────────────┘
                   │ subscribe         │ subscribe          │ subscribe
                   ▼                  ▼                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    server binary (Tokio, host arch)                      │
│                                                                          │
│  ┌────────────────────────────┐   ┌──────────────────────────────────┐  │
│  │  rtcm_decoder task         │   │  nmea_parser task                 │  │
│  │  rtcm-rs crate             │   │  nmea-parser or hand-rolled       │  │
│  │  MSM4/MSM7 → observations  │   │  GSV → sat elevation/azimuth/snr  │  │
│  │  1019/1020/1044/1045 → eph │   │  GGA → fix quality (live)         │  │
│  └────────────┬───────────────┘   └─────────────┬────────────────────┘  │
│               │                                  │                       │
│               ▼                                  ▼                       │
│  ┌────────────────────────────┐   ┌──────────────────────────────────┐  │
│  │  rinex_writer task         │   │  satellite_state: Arc<RwLock<>>   │  │
│  │  rinex crate (obs write)   │   │  SatState per GNSS system         │  │
│  │  hourly rotation           │   │  updated by both tasks above       │  │
│  │  .26O / .26P files         │   └─────────────┬────────────────────┘  │
│  └────────────────────────────┘                 │                       │
│                                                  ▼                       │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  axum HTTP + WebSocket server                                      │  │
│  │  GET /          → static HTML/JS                                   │  │
│  │  GET /ws        → WebSocket upgrade                                │  │
│  │  broadcast::Sender<SkyplotUpdate> → all WS connections            │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                    ESP32-C6 firmware (unchanged from v2.0)               │
│  publishes RTCM3, NMEA, heartbeat to MQTT broker                        │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                    nostd gap crates (new library crates)                  │
│  crates/gnss-hal-traits/  — trait definitions for NVS, OTA, SoftAP…    │
│  crates/gnss-nvs/         — priority gap: NVS implementation skeleton   │
│  (future crates per gap found in audit)                                  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Question 1: Workspace Structure — Same Repo or Separate?

### Recommendation: Single Cargo Workspace, Same Repo

Keep everything in `esp32-gnssmqtt/`. Add a Cargo workspace root that makes the firmware crate and server crate siblings, plus shared library crates.

**Rationale:**
- The RTCM3 parser crate (`crates/rtcm-proto/` or a thin wrapper around `rtcm-rs`) is shared between the server and the future nostd firmware. A single workspace guarantees they always compile against the same version.
- Git history stays unified — firmware changes that affect the server's topic format are visible in the same commit.
- The `rtcm-rs` crate is already `no_std` compatible. A thin re-export crate in the workspace can expose it under the project's own name for future vendoring flexibility.
- Workspace-level `Cargo.lock` keeps all dep versions pinned together (important: the firmware already uses `=` version pins).

**Counterargument considered (separate repo):** Avoids the per-target build complexity. Rejected because shared crates are the main value of the workspace — splitting them into two repos forces manual version sync, which is error-prone.

### Workspace Layout

```
esp32-gnssmqtt/           ← workspace root (new Cargo.toml added here)
├── Cargo.toml            ← [workspace] members = ["firmware", "server", "crates/*"]
├── Cargo.lock            ← single lock file for everything
│
├── firmware/             ← moved from root; ESP32-C6 target
│   ├── Cargo.toml        ← package name "esp32-gnssmqtt-firmware"
│   ├── .cargo/config.toml ← [build] target = "riscv32imac-esp-espidf" (firmware-local)
│   ├── build.rs
│   ├── sdkconfig.defaults
│   ├── partitions.csv
│   └── src/
│       └── (all existing src/*.rs files)
│
├── server/               ← new server binary; host target
│   ├── Cargo.toml        ← package name "gnss-server"; tokio, axum, rumqttc, etc.
│   └── src/
│       ├── main.rs
│       ├── rtcm_decoder.rs
│       ├── nmea_parser.rs
│       ├── rinex_writer.rs
│       ├── satellite_state.rs
│       └── web_server.rs
│
└── crates/               ← shared/gap library crates
    ├── gnss-hal-traits/  ← no_std trait definitions (NVS, OTA, SoftAP, NTRIP TLS…)
    │   └── Cargo.toml    ← #![no_std]; no deps
    └── gnss-nvs/         ← priority gap crate; NVS abstraction
        └── Cargo.toml    ← no_std; optional std feature for test harness
```

### Per-Member Build Target: The Key Problem and Solution

Cargo's `.cargo/config.toml` applies globally to the workspace when placed at the root. The firmware needs `target = "riscv32imac-esp-espidf"`; the server and library crates need the host target.

**Solution:** Place the firmware's `.cargo/config.toml` inside `firmware/` (not the workspace root). Cargo searches upward from the package being built, so `firmware/.cargo/config.toml` applies only when building that member.

The workspace root should have NO `[build] target` in its `.cargo/config.toml` — leave target selection to individual members.

**Build commands:**
```bash
# Build firmware (from workspace root or firmware/)
cargo build -p esp32-gnssmqtt-firmware --release

# Build server (from workspace root)
cargo build -p gnss-server --release

# Build all library crates (host target, for tests)
cargo test -p gnss-hal-traits
cargo test -p gnss-nvs
```

**Note on nightly per-package-target:** The unstable `per-package-target` Cargo feature exists but requires nightly. The firmware already uses stable Rust. The directory-local `.cargo/config.toml` approach works on stable and is the correct production-grade solution. (MEDIUM confidence — confirmed via Rust Users forum discussion; no official Cargo documentation explicitly endorses this pattern, but it is widely used in embedded Rust multi-target projects.)

---

## Question 2: Shared RTCM3 Parser Crate — no_std Design

### Use `rtcm-rs` Directly in the Server; Wrap for Future Firmware Use

The `rtcm-rs` crate (version 0.11.0, April 2024) is:
- `no_std` compatible (`default-features = false` disables std)
- 100% safe Rust (`#[forbid(unsafe_code)]`)
- Supports all RTCM 3.4 messages including MSM4 and MSM7 for GPS/GLONASS/Galileo/BeiDou/NavIC
- Serde support via feature flag

**For the server binary:** Add `rtcm-rs` directly to `server/Cargo.toml`. No wrapper needed for the server — std is available, use it freely.

**For future nostd firmware:** Do not add `rtcm-rs` to the firmware crate now. The firmware currently relays raw RTCM3 frames without decoding them (by design — the UM980 outputs complete frames already framed with CRC). If RTCM decoding is ever needed on-device, add `rtcm-rs` with `default-features = false` to the firmware crate at that time.

**For shared library crates:** If a `gnss-hal-traits` crate needs to define types for RTCM3 observations (e.g., for a future nostd decoder), define minimal newtypes there. Do not re-export `rtcm-rs` types in the traits crate — that would force every consumer to depend on `rtcm-rs` even when they don't decode RTCM.

### Shared Crate `no_std` Declaration Pattern

Every crate in `crates/` that targets embedded must declare `no_std` at the crate root:

```rust
// crates/gnss-hal-traits/src/lib.rs
#![no_std]
// If alloc is needed (e.g., for String types in error messages):
// extern crate alloc;

// Trait definitions using only core:: types
pub trait NvsStorage {
    type Error;
    fn get_blob<'a>(&self, key: &str, buf: &'a mut [u8]) -> Result<Option<&'a [u8]>, Self::Error>;
    fn set_blob(&mut self, key: &str, data: &[u8]) -> Result<(), Self::Error>;
}
```

The `Cargo.toml` for each gap crate should expose a `std` feature that unlocks `std`-dependent impls (useful for test harnesses and the server if it reuses the traits):

```toml
[features]
default = []
std = []

[dependencies]
# no mandatory std deps
```

This is the standard embedded Rust pattern: `#![no_std]` at the top, `cfg(feature = "std")` gates for anything that uses `std::`. The `core` and `alloc` crates are always available in no_std contexts; only `std` is absent.

---

## Question 3: RTCM3 MSM Decode Architecture for the Server

### Context: Frames Arrive Pre-Framed from MQTT

The MQTT topic `gnss/{id}/rtcm` carries complete raw RTCM3 frames including preamble (0xD3), header, payload, and CRC-24Q. The firmware's `rtcm_relay.rs` publishes the full frame from the pool buffer after CRC verification in `gnss.rs`. This means the server receives already-framed and already-CRC-verified data.

**No frame accumulation state machine is needed in the server.** Each MQTT message payload IS a complete RTCM3 frame. The server can pass the raw bytes directly to `rtcm-rs` for decoding.

### Recommended Decode Architecture

```
MQTT message arrives on gnss/{id}/rtcm
    │ payload: complete raw RTCM3 frame bytes (Vec<u8>)
    ▼
rtcm_decoder task (tokio::spawn)
    │
    │ step 1: extract message type
    │   msg_type = ((payload[3] as u16) << 4) | ((payload[4] as u16) >> 4)
    │
    │ step 2: dispatch by message type range
    │   1071-1077 → GPS MSM (MSM1-MSM7)
    │   1081-1087 → GLONASS MSM
    │   1091-1097 → Galileo MSM
    │   1121-1127 → BeiDou MSM
    │   1019      → GPS ephemeris
    │   1020      → GLONASS ephemeris
    │   1044      → BeiDou ephemeris
    │   1045/1046 → Galileo ephemeris
    │   1005/1006 → reference station position
    │   other     → log and discard
    │
    │ step 3: for MSM4/MSM7, call rtcm-rs decoder
    │   use rtcm_rs::msg::{Msg, MsmMsg}
    │   decode MSM header: satellite mask, signal mask, cell mask
    │   extract per-satellite observations: pseudorange, carrier phase, SNR
    │
    │ step 4: update satellite_state
    │   satellite_state.write().update_from_msm(constellation, observations)
    │
    │ step 5: for ephemeris, decode orbital parameters
    │   store in EphemerisStore (HashMap<SvId, EphemerisData>)
    │   needed for RINEX navigation files
    ▼
SkyplotUpdate sent to broadcast::Sender<SkyplotUpdate>
```

### MSM Message Structure (for reference)

The MSM header contains three bitmasks:
- **Satellite mask** (64 bits): which of 64 possible SVs are present
- **Signal mask** (32 bits): which of 32 possible signal types are present
- **Cell mask** (`nsat × nsig` bits): which satellite/signal combinations have data

The `rtcm-rs` crate handles this decoding automatically. The server code should call into `rtcm-rs` and work with its decoded types, not re-implement the bit parsing.

### MSM4 vs MSM7: Which to Prefer

The UM980 configured with `RTCM 1074` (GPS MSM4) outputs pseudorange and carrier phase. `RTCM 1077` (GPS MSM7) adds Doppler, extended carrier phase, and CNR (carrier-to-noise ratio). For RINEX observation files, MSM7 provides the richest data. Accept both — write whatever the firmware sends.

### Ephemeris State Machine

Unlike MSM (which is stateless per frame), ephemeris messages for GLONASS (1020) carry orbital parameters that must be accumulated over multiple frames before a complete ephemeris is available. Use a simple accumulation map:

```rust
// Per-SV ephemeris accumulator; no complex state needed
// rtcm-rs provides decoded ephemeris structs directly
struct EphemerisStore {
    gps: HashMap<u8, GpsEphemeris>,    // SV PRN → decoded eph
    glonass: HashMap<u8, GloEphemeris>,
    galileo: HashMap<u8, GalEphemeris>,
    beidou: HashMap<u8, BdEphemeris>,
}
```

GLONASS ephemeris: each 1020 message is complete for one SV — no multi-frame accumulation needed at this protocol level. The term "accumulate" in the question likely refers to collecting ephemeris for all visible SVs before writing the first navigation file. Simply maintain the HashMap and write the navigation file at the hourly rotation with whatever SVs have been received.

---

## Question 4: WebSocket Architecture with Axum

### Recommended: tokio::sync::broadcast + AppState

The canonical pattern for pushing satellite state to multiple browser connections is:

```rust
#[derive(Clone)]
struct AppState {
    satellite_state: Arc<RwLock<SatelliteState>>,
    ws_tx: broadcast::Sender<SkyplotUpdate>,
}
```

The `ws_tx` is a `tokio::sync::broadcast::Sender<SkyplotUpdate>` stored in `AppState`. Each WebSocket connection handler calls `ws_tx.subscribe()` at the start of the handler to get its own `Receiver<SkyplotUpdate>`. The update producer (the NMEA/RTCM decoder tasks) calls `ws_tx.send(update)` once; all connected clients receive a clone.

```rust
// Producer side (nmea_parser task)
app_state.ws_tx.send(SkyplotUpdate::from(&sat_state))?;

// Consumer side (per-connection WebSocket handler)
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state.ws_tx.subscribe()))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<SkyplotUpdate>,
) {
    loop {
        match rx.recv().await {
            Ok(update) => {
                let json = serde_json::to_string(&update).unwrap();
                if socket.send(Message::Text(json)).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                // Client fell behind; skip n messages and continue
                log::warn!("WS client lagged, skipped {} updates", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
```

**Broadcast channel capacity:** Use `broadcast::channel(64)`. At 5 Hz NMEA rate, 64 slots gives ~12 seconds of buffer before the slowest client is forced to lag. A lagged client receives a `RecvError::Lagged` and continues — it does not crash or block the producer.

**Update rate:** Do not send a WebSocket update per NMEA sentence (40 msg/s is too fast for a browser to render). Aggregate into a 1 Hz tick:

```rust
// Update aggregator
let mut interval = tokio::time::interval(Duration::from_secs(1));
loop {
    interval.tick().await;
    let snap = satellite_state.read().await.snapshot();
    let _ = ws_tx.send(snap); // ignore if no receivers
}
```

**Initial state on connect:** When a new WebSocket client connects, send the current snapshot immediately before subscribing to the broadcast, so the client doesn't wait up to 1 second for the first update.

### SkyplotUpdate Structure

```rust
#[derive(Clone, serde::Serialize)]
struct SkyplotUpdate {
    timestamp: u64,                   // Unix seconds
    satellites: Vec<SatelliteInfo>,
    fix_type: u8,
    hdop: Option<f32>,
    device_health: Option<HeartbeatData>,
}

#[derive(Clone, serde::Serialize)]
struct SatelliteInfo {
    system: &'static str,  // "GPS", "GLO", "GAL", "BDS"
    prn: u8,
    elevation: i8,         // degrees; -1 if unknown
    azimuth: u16,          // degrees 0-359
    snr: Vec<u8>,          // dB-Hz per signal; from MSM CNR or NMEA GSV
    used: bool,            // in fix solution
}
```

---

## Question 5: Build Order — RINEX Before or After RTCM3 Decode

### Recommendation: RTCM3 Decode First, then RINEX Writing

**Rationale:**

1. **RTCM3 decode is the data source for RINEX.** You cannot write valid RINEX observation data without first having decoded pseudoranges, carrier phases, and CNR from MSM messages. RINEX writing depends on the decode output, not the reverse.

2. **RINEX navigation files depend on ephemeris decode.** Ephemeris messages (1019/1020/1044/1045) must be decoded before you know what to write in the `.26P` navigation file. Navigation writing is blocked on ephemeris decode being correct.

3. **The `rinex` crate's navigation writer is under construction.** The crate documentation marks navigation file writing as `🚧`. Build RTCM3 decode and RINEX observation writing first (observation writer is stable). Add navigation writing in a subsequent phase once the decode is validated against known-good data.

4. **Testing is easier with decode validated first.** You can verify decoded MSM observations against RTKLIB or RTKPost before committing to a RINEX file format. A phase that combines decode + write makes it harder to isolate bugs.

### Suggested Phase Order for v2.1 Server Work

```
Phase A — Workspace restructure
  A1: Add workspace Cargo.toml; move firmware to firmware/
  A2: Create server/ skeleton with rumqttc + tokio
  A3: Verify firmware still builds; verify server compiles for host
  A4: Create crates/gnss-hal-traits/ skeleton

Phase B — MQTT subscriber + RTCM3 decode
  B1: rumqttc client subscribes to gnss/{id}/rtcm, /nmea, /heartbeat
  B2: rtcm-rs integration; MSM4/MSM7 decode to observation structs
  B3: Ephemeris decode (1019/1020/1044/1045)
  B4: NMEA GSV parsing for elevation/azimuth (satellite_state)
  Verify: decoded observations match known RTCM3 test vectors

Phase C — RINEX observation writing
  C1: rinex crate integration; hourly file rotation logic
  C2: Observation file (.26O) from decoded MSM data
  C3: Validate output against RTKLIB or RTKPost
  Defer: Navigation file (.26P) until rinex nav writer is stable

Phase D — HTTP + WebSocket server
  D1: axum server with broadcast channel; static file serving
  D2: WebSocket handler; JSON SkyplotUpdate messages
  D3: Browser skyplot SVG (polar plot, elevation/azimuth dots)
  D4: SNR bar chart; device health panel

Phase E — Nostd audit + gap crates
  E1: Enumerate all esp-idf-svc/hal/sys usages in firmware by category
  E2: Map each to esp-hal/embassy equivalent or "no equivalent found"
  E3: Create gap crate skeletons for each missing capability
  E4: Begin NVS gap crate implementation (priority)
```

**Why RINEX before WebSocket (Phase C before D):** The RINEX writer validates that the decoded data is correct and complete. If the WebSocket UI is built first, display bugs and decode bugs are tangled. Validating decode via RINEX files (which can be compared against reference data) gives confidence before building the browser UI.

**Why nostd audit last (Phase E):** The audit is read-only — no firmware changes, no server changes. It can run in parallel with Phase D but has no external dependencies. Running it last means the final picture of what the firmware actually uses (post all v2.0 work) is captured, not an interim state.

---

## Question 3b: RTCM3 State Machine Clarification

The question asks about "accumulating RTCM3 frames." Since the firmware publishes complete pre-framed RTCM3 messages to MQTT, there is no byte-stream reassembly needed in the server. Each MQTT payload is one complete RTCM3 message.

If the server were consuming a raw UART stream (not MQTT), it would need the four-state machine already implemented in `gnss.rs` (`Idle → RtcmHeader → RtcmBody`). That design is documented in full in the prior version of this file (reproduced at the end of this document for reference).

The MSM decoder inside `rtcm-rs` does have internal state for processing the bitmask structure (satellite mask → signal mask → cell mask iteration), but this is handled internally by the crate — the server code just calls `decode()` on a complete frame buffer.

---

## Embassy / nostd Audit: Key Gaps Found

### esp-hal 1.0 Beta Status (February 2025)

Espressif released the esp-hal 1.0 beta in February 2025. The stabilized scope is deliberately limited:
- Stable: GPIO, UART, SPI, I2C initialization and drivers
- Unstable (behind `unstable` feature): everything else, including WiFi, async executors, timers

esp-idf-svc (which the firmware uses) is now **community-maintained** — Espressif no longer puts paid developer time into it. The official direction is esp-hal + embassy. However, esp-hal does not yet have equivalents for:

| Firmware Capability | esp-idf-svc API | esp-hal / embassy equivalent | Gap Status |
|---------------------|-----------------|------------------------------|------------|
| NVS credential storage | `EspNvs` | None — raw flash only | **GAP — priority** |
| OTA dual-slot update | `EspOta` | `esp-ota` crate (community) | Partial — needs validation |
| SoftAP provisioning portal | `EspWifi` AP mode + `EspNetif` | `esp-wifi` has basic AP | Partial — no captive portal/DNS hijack |
| MQTT client | `EspMqttClient` | rumqttc (no_std possible) | Community crates exist |
| SNTP time sync | `EspSntp` | No equivalent | GAP |
| HTTP client (for OTA) | `EspHttpConnection` | `reqwless` (no_std) | Community crates exist |
| TLS (for NTRIP) | `EspTls` (mbedTLS) | `embedded-tls` | Community; maturity lower |
| DNS hijack in SoftAP | ESP-IDF DHCP server option | No equivalent | GAP |
| GNSS fix heartbeat GGA atomics | `AtomicU32` (std) | `AtomicU32` (core) | No gap |
| Log relay (vprintf hook) | ESP-IDF log hook | No equivalent | GAP |
| FreeRTOS thread watchdog | `uxTaskGetStackHighWaterMark` | No equivalent | GAP |

**Gap crates to create (priority order):**
1. `gnss-nvs` — NVS-compatible key/value storage trait + implementation sketch
2. `gnss-ota-trait` — OTA update trait: initiate, write, complete, abort, rollback
3. `gnss-softap-trait` — SoftAP + captive portal trait
4. `gnss-ntrip-tls-trait` — NTRIP TCP/TLS stream trait

**Conclusion on nostd port feasibility:** A full embassy/nostd port is a substantial undertaking. The NVS gap alone (persistent credential storage) has no drop-in replacement. The audit phase should document every gap precisely, create trait definitions that an implementation could satisfy, and begin NVS implementation. A complete port is out of scope for v2.1.

---

## New vs Modified Components (v2.1)

| Component | Status | Notes |
|-----------|--------|-------|
| `firmware/` directory | EXISTING (moved) | All src/*.rs unchanged; just relocated in workspace |
| `firmware/Cargo.toml` | MODIFIED | Package rename; workspace inheritance for shared deps |
| `firmware/.cargo/config.toml` | MODIFIED | Moved from root to firmware/; same content |
| `server/` | NEW | Tokio server binary |
| `server/src/rtcm_decoder.rs` | NEW | rtcm-rs integration; MSM decode |
| `server/src/nmea_parser.rs` | NEW | GSV/GGA parsing; satellite_state update |
| `server/src/rinex_writer.rs` | NEW | hourly RINEX observation file rotation |
| `server/src/web_server.rs` | NEW | axum HTTP + WebSocket; broadcast channel |
| `server/src/satellite_state.rs` | NEW | Arc<RwLock<SatelliteState>> shared state |
| `server/static/` | NEW | HTML/JS browser UI (skyplot, SNR, health) |
| `crates/gnss-hal-traits/` | NEW | no_std trait definitions for all gap capabilities |
| `crates/gnss-nvs/` | NEW | NVS gap crate (partial implementation) |
| `Cargo.toml` (workspace root) | NEW | Workspace manifest |

---

## Data Flow

### RTCM3 Observation Flow (Server)

```
MQTT broker
    │ gnss/{id}/rtcm payload = complete RTCM3 frame (bytes)
    ▼
rumqttc async client (tokio task)
    │ mpsc::Sender<Bytes>
    ▼
rtcm_decoder task
    │ rtcm-rs::decode(frame_bytes) → Msg::Msm7 { header, cells, .. }
    │ extract per-SV pseudorange, carrier_phase, cnr
    ▼
satellite_state: Arc<RwLock<SatelliteState>>   ←──── nmea_parser task
    │                                                  (elevation, azimuth from GSV)
    ▼
tokio::time::interval(1s) tick
    │ snapshot = satellite_state.read().snapshot()
    │ ws_tx.send(SkyplotUpdate) → all WebSocket subscribers
    ▼
browser: polar SVG redraws, SNR bars update
```

### RINEX Hourly Rotation Flow

```
wall clock crosses :00:00
    │
rinex_writer task (tokio::select! on interval + observation channel)
    │ close current .26O file
    │ open new file: SSSS{doy}{session}.{yy}O
    │   where SSSS = 4-char station name, doy = day of year, yy = 2-digit year
    │ write RINEX 2.11 observation file header
    ▼
continue writing epoch records as MSM observations arrive
```

### nostd Audit Flow

```
audit tool (grep/rg on firmware/src/**/*.rs)
    │ find all uses of: esp_idf_svc::, esp_idf_hal::, esp_idf_sys::
    │ categorize by: WiFi, MQTT, NVS, OTA, HTTP, TLS, GPIO, UART, log, time
    ▼
audit table: capability → esp-idf API → esp-hal equivalent → gap status
    ▼
for each GAP: create crates/{name}/ with trait definition
    ▼
priority: gnss-nvs — sketch partial implementation using embedded-storage trait
```

---

## Anti-Patterns

### Anti-Pattern 1: Putting firmware `.cargo/config.toml` at Workspace Root

**What people do:** Place a single `.cargo/config.toml` at the workspace root with `[build] target = "riscv32imac-esp-espidf"` thinking it will only affect the firmware.

**Why it's wrong:** Cargo applies the config to ALL workspace members. The server binary and library crates will attempt to compile for the ESP32 RISC-V target and fail immediately with linker errors.

**Do this instead:** Place `.cargo/config.toml` inside `firmware/`. Cargo searches upward from the build root of each package, so `firmware/.cargo/config.toml` applies only to that package.

### Anti-Pattern 2: Sharing `esp-idf-svc` Types in Library Crates

**What people do:** Define shared structs in `crates/gnss-hal-traits/` that use `esp_idf_svc::nvs::EspNvs` in their trait bounds or associated types.

**Why it's wrong:** Library crates intended for nostd use cannot depend on `esp-idf-svc` — it requires ESP-IDF std. Any consumer of the trait (future embassy firmware, tests on host, server) would have to pull in ESP-IDF.

**Do this instead:** Define traits using only `core::` types. Use associated error types (`type Error`). Implementations of the trait (in `firmware/` or a platform-specific crate) can use ESP-IDF types internally without exposing them.

### Anti-Pattern 3: Per-Connection Satellite State Clone at WebSocket Connect

**What people do:** When a new WebSocket connection arrives, clone the entire current satellite state and send it as a JSON blob, then start the broadcast subscription.

**Why it's wrong (slightly):** Not wrong per se, but the implementation order matters. If you subscribe to the broadcast BEFORE sending the snapshot, you may miss an update that fires between snapshot time and subscribe time. If you subscribe AFTER the snapshot, you get the snapshot plus all future updates correctly.

**Do this instead:**
1. Subscribe to broadcast channel first: `let mut rx = state.ws_tx.subscribe();`
2. Send current snapshot immediately after subscribe
3. Then enter the receive loop

This guarantees no updates are missed between snapshot and subscription.

### Anti-Pattern 4: Decoding RTCM3 in the MQTT Callback

**What people do:** Put the `rtcm-rs` decode call inside the rumqttc event loop handler, blocking the event processing while decoding.

**Why it's wrong:** The MQTT event loop must remain responsive to heartbeat, NMEA, and keep-alive events. A slow decode (or a malformed frame causing retries) stalls the connection.

**Do this instead:** The MQTT callback sends the raw frame bytes through a `tokio::sync::mpsc::channel` to a dedicated decoder task. The callback is non-blocking (`try_send` or async send with `send` in an async context).

### Anti-Pattern 5: Writing RINEX Navigation Files Before Validating Ephemeris Decode

**What people do:** Implement navigation file writing in parallel with ephemeris decode, writing partial or incorrect orbital parameters.

**Why it's wrong:** RINEX navigation files are consumed by RTK engines that assume correct orbital parameters. A file with wrong ephemeris silently produces wrong position solutions — hard to diagnose.

**Do this instead:** Validate ephemeris decode against a known reference (e.g., compare against RTKLIB's decode of the same raw data, or against a known broadcast ephemeris from a reference station) before implementing the RINEX writer for navigation files. The `rinex` crate's navigation writer is also marked as under construction — defer until the crate stabilizes.

### Anti-Pattern 6: Single tokio::Mutex for satellite_state

**What people do:** Use `tokio::sync::Mutex<SatelliteState>` for the shared satellite state.

**Why it's wrong:** The satellite state is updated by the decoder task and read by the WebSocket aggregator every second. `tokio::sync::Mutex` blocks the async executor while held. For a struct that is written infrequently (once per second per constellation) and read frequently (every WebSocket tick), `RwLock` is the correct primitive.

**Do this instead:** Use `tokio::sync::RwLock<SatelliteState>`. Writers (`rtcm_decoder`, `nmea_parser`) acquire a write lock briefly; the WebSocket aggregator acquires a read lock for the snapshot. This allows concurrent reads from multiple aggregator ticks without blocking.

---

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| MQTT broker | `rumqttc` async client; subscribe to device topics | Use `QoS::AtMostOnce` to match firmware publish; no need for persistent sessions |
| RTCM3 frames | `rtcm-rs` crate; pass complete frame bytes directly | No framing state machine needed — MQTT payload is one complete frame |
| RINEX files | `rinex` crate writer; hourly rotation | Observation writing stable; navigation writing under construction |
| Browser clients | axum WebSocket + broadcast channel | 1 Hz update rate; JSON serialized SkyplotUpdate |

### Internal Boundaries (Server)

| Boundary | Communication | Notes |
|----------|---------------|-------|
| MQTT task → rtcm_decoder | `mpsc::channel<Bytes>` (tokio, bounded 64) | Non-blocking send in MQTT handler |
| MQTT task → nmea_parser | `mpsc::channel<String>` (tokio, bounded 128) | NMEA sentences as strings |
| rtcm_decoder/nmea_parser → satellite_state | `Arc<RwLock<SatelliteState>>` | Direct write; no channel |
| satellite_state → WS aggregator | `tokio::time::interval` poll + RwLock read | 1 Hz; aggregator owns the interval |
| WS aggregator → WS connections | `broadcast::channel<SkyplotUpdate>` capacity 64 | Per-connection subscribe() at connect |
| rtcm_decoder → rinex_writer | `mpsc::channel<Observation>` (tokio, bounded 128) | Decoded observations forwarded |

### Firmware → Server Contract (MQTT Topics)

| Topic | Payload | Notes |
|-------|---------|-------|
| `gnss/{id}/rtcm` | Binary RTCM3 frame (complete: preamble + header + payload + CRC) | CRC already verified by firmware |
| `gnss/{id}/nmea/{TYPE}` | Raw NMEA sentence string (UTF-8) | e.g., TYPE = GNGGA, GPGSV |
| `gnss/{id}/heartbeat` | JSON: uptime_s, heap_free, nmea_drops, rtcm_drops, fix_type, satellites, hdop | Published every 30s |
| `gnss/{id}/status` | "online" (retained) or "offline" (LWT) | String |

---

## Concurrency Budget (Server)

| Task | Type | Notes |
|------|------|-------|
| rumqttc event loop | tokio::spawn | Single task; routes to decoder/parser channels |
| rtcm_decoder | tokio::spawn | Receives from MQTT; writes satellite_state; forwards to rinex_writer |
| nmea_parser | tokio::spawn | Receives NMEA sentences; writes elevation/azimuth to satellite_state |
| rinex_writer | tokio::spawn | Hourly file rotation; writes observation epochs |
| ws_aggregator | tokio::spawn | 1 Hz interval; reads satellite_state; broadcasts SkyplotUpdate |
| axum server | tokio::spawn | HTTP + WebSocket; per-connection handler tasks |

Total: ~6 root tasks + N per-connection handlers. All async, no blocking. Single-threaded tokio runtime is sufficient for this load; use `tokio::runtime::Builder::new_current_thread()` for determinism during development, switch to multi-thread for production.

---

## Sources

- `rtcm-rs` crate: [GitHub — martinhakansson/rtcm-rs](https://github.com/martinhakansson/rtcm-rs) — no_std, RTCM 3.4, MSM4/MSM7 support, v0.11.0 (April 2024) — MEDIUM confidence (GitHub confirmed, not integration-tested)
- `rinex` crate: [docs.rs/rinex](https://docs.rs/rinex/latest/rinex/) — observation writer stable, navigation writer under construction — MEDIUM confidence
- axum WebSocket broadcast: [tokio-rs/axum discussion #1335](https://github.com/tokio-rs/axum/discussions/1335) — `broadcast::channel` is the canonical pattern — MEDIUM confidence
- Cargo workspace per-member targets: [Rust Users Forum](https://users.rust-lang.org/t/cargo-workspace-members-with-different-target-architectures/122464) — directory-local `.cargo/config.toml` is the stable solution — MEDIUM confidence
- esp-hal 1.0 beta: [Espressif Developer Portal, Feb 2025](https://developer.espressif.com/blog/2025/02/rust-esp-hal-beta/) — stabilized scope confirmed; NVS/OTA/SoftAP not in stable scope — HIGH confidence
- esp-rs organization gaps: [HackMD esp-rs org overview](https://hackmd.io/@Mabez/rkUh6KAoj) — gap analysis between std and no_std stacks — MEDIUM confidence (document from early 2023; ecosystem has evolved)
- ESP-IDF std crates community status: [esp-hal GitHub discussions](https://github.com/esp-rs/esp-hal/discussions/744) — Espressif confirmed std crates are community-maintained — HIGH confidence
- RTCM3 MSM message structure: [SNIP decoding MSM](https://www.use-snip.com/kb/knowledge-base/decoding-msm-messages/) — satellite/signal/cell mask structure — HIGH confidence (matches RTKLIB source)
- RTKLIB reference implementation: [RTKLIB rtcm3.c](https://github.com/tomojitakasu/RTKLIB/blob/master/src/rtcm3.c) — authoritative RTCM3 decode reference — HIGH confidence
- rumqttc crate: [docs.rs/rumqttc](https://docs.rs/rumqttc/latest/rumqttc/) — async MQTT client; no_std support exists — MEDIUM confidence (no_std feature not validated for this use case)

---

*Architecture research for: esp32-gnssmqtt v2.1 — server + nostd foundation*
*Researched: 2026-03-12*
