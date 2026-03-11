# Stack Research

**Domain:** Embedded Rust firmware — ESP32-C6, GNSS/MQTT bridge
**Researched:** 2026-03-07 (milestone 2 update: RTCM relay + OTA); 2026-03-12 (milestone v2.1 update: server binary + embassy/nostd audit)
**Confidence:** HIGH for OTA and HTTP APIs (source code verified in local cargo cache); MEDIUM for RTCM sizing (derived from published spec structure); HIGH for baud rates (multiple sources agree)

---

## Scope of This Document

This document is cumulatively updated. Each milestone section focuses only on new stack decisions.

- **Milestone v1.2 additions** — RTCM binary relay, OTA firmware update
- **Milestone v2.1 additions** — Companion server binary (MQTT + RTCM3 decode + RINEX + HTTP/WS), embassy/nostd crate audit

---

## Milestone v2.1 Stack Additions

### Feature 1: Tokio Server Binary

The companion server is a standard Rust `std` binary compiled for the host (Linux/macOS/Windows). It is a separate Cargo workspace member, not a firmware binary.

#### MQTT Client: rumqttc

**Recommendation: rumqttc 0.24.x (MQTT 3.1.1, AsyncClient)**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rumqttc | 0.24.x | MQTT subscribe/publish on server | Same crate the firmware's broker (rumqttd) is from; tokio-native async; `AsyncClient` is cloneable; integrates cleanly with tokio runtime; MQTT 3.1.1 matches broker |
| tokio | 1.x (1.47+) | Async runtime | Axum requires tokio; single runtime for server; use `tokio = { version = "1", features = ["full"] }` |

**Why not paho-mqtt:** C library with FFI overhead, requires libpaho-mqtt-c installed on the host. For a server-side subscriber that needs clean async ergonomics and zero native deps, rumqttc is the correct choice.

**Why not rumqttc v5 (MQTT 5.0):** The MQTT broker (Mosquitto or HiveMQ) is configured for MQTT 3.1.1, which the firmware also uses. Adding MQTT 5.0 client does not add value and introduces version mismatch risk.

**Configuration note:** `rumqttc::AsyncClient` with `EventLoop` polled in a dedicated tokio task. Subscribe to `gnss/{id}/rtcm/#` and `gnss/{id}/nmea/#` and `gnss/{id}/heartbeat`. Route incoming `Event::Incoming(Packet::Publish)` via `tokio::sync::broadcast` to RINEX writer and WebSocket broadcaster tasks.

#### RTCM3 Decoding: rtcm-rs

**Recommendation: rtcm-rs 0.11.0**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rtcm-rs | 0.11.0 | Decode RTCM3 MSM4/MSM7 and ephemeris messages | Covers all RTCM 3.4 messages including complete MSM coverage: GPS 1074-1077, GLONASS 1084-1087, Galileo 1094-1097, BeiDou 1124-1127, NavIC 1131-1137; `no_std` compatible; `#[forbid(unsafe_code)]`; Serde support; feature flags per message type |

**MSM message coverage (HIGH confidence — verified from GitHub):**
- MSM4 (1074/1084/1094/1124): pseudorange + carrier phase + lock time + half-cycle — sufficient for RINEX 2.x OBS
- MSM5 (1075/1085/1095/1125): adds Doppler — not needed for RINEX obs but available
- MSM7 (1077/1087/1097/1127): full precision — supported

**Ephemeris messages (HIGH confidence):**
- 1019: GPS L1 C/A ephemeris
- 1020: GLONASS ephemeris
- 1044: BeiDou ephemeris
- 1046: Galileo I/NAV ephemeris
All covered by rtcm-rs 0.11.0.

**Decoding pattern:**

```rust
use rtcm_rs::prelude::*;

let msg = decode_msg(rtcm_bytes)?;
if let Some(Msg1074(msm)) = msg.get_message() {
    // access msm.satellite_data, msm.signal_data
}
```

Enable only required feature flags in Cargo.toml to minimize compile time:

```toml
rtcm-rs = { version = "0.11", features = ["1074","1084","1094","1124","1019","1020","1044","1046"] }
```

#### RINEX Writing

**Recommendation: Use the `rinex` crate (nav-solutions, v0.21.x) for observation files; DIY for navigation files**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rinex | 0.21.x | Write RINEX 2.x observation (.yyO) files | Writer for OBS is marked complete; parser supports all revisions automatically; active project under nav-solutions/rinex |

**Status (MEDIUM confidence — verified from GitHub documentation):**
- OBS writer: available and marked stable
- NAV writer: marked work-in-progress (construction emoji in feature matrix)
- The crate claims "all revisions supported without compilation options" including RINEX V2

**Risk:** The rinex crate is focused on data processing/parsing rather than file production. The OBS writer may produce RINEX 3.x format by default even when targeting 2.x output. Verify at integration time by checking the `prod` module and `Rinex::to_file()` or equivalent API. If RINEX 2.x compliance is mandatory for downstream software, a DIY writer may be required.

**DIY RINEX 2.x writer rationale (fallback):** RINEX 2.11 observation files are fixed-width text format. The header is ~20 lines of fixed-format records; each epoch record is a timestamp line plus satellite observation lines. The format is well-documented at the IGS. A minimal implementation writing GPS+GLONASS+Galileo+BeiDou pseudorange + carrier phase + SNR (3 observables per signal) is approximately 200-300 lines of Rust with no external crate. If `rinex` writer is found insufficient at integration time, switch to DIY.

**Hourly file rotation pattern:**

```
// Filename convention: {doy}{hour_letter}{minute}0.{yy}O
// Example: 0710a00.26O for day 071 hour 00 year 2026
// Rotate at each UTC hour boundary; finalize/flush current file, open new
```

Use `tokio::time::sleep_until` targeting the next UTC hour to trigger rotation. File naming uses the standard RINEX 2 naming convention (doy + hour letter a-x).

#### HTTP + WebSocket Server: axum

**Recommendation: axum 0.8.x**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| axum | 0.8.x | HTTP routes + WebSocket handler | Tokio-native; ergonomic extractors; built-in WebSocket via `axum::extract::ws`; tower middleware compatible; actively maintained by tokio-rs; no tungstenite dependency required in your code |
| tokio-tungstenite | (indirect) | WebSocket protocol | axum uses tungstenite internally; do NOT add as direct dep |
| tower-http | 0.6.x | Static file serving (embedded UI assets) | Pairs with axum for `ServeDir` or embedding HTML/JS |

**Pattern for live data push:**

```
MQTT EventLoop task
    ↓ tokio::sync::broadcast::Sender<SatState>
WebSocket handler (one per client connection)
    → subscribe to broadcast, send JSON frame per tick
```

Each new WebSocket connection receives its own `broadcast::Receiver`. The MQTT task decodes incoming packets and sends to the broadcast channel. If no clients are connected, the `broadcast::Sender` drops lagged frames (set `.capacity()` appropriately, e.g. 64).

**Why not actix-web:** actix uses its own async runtime (actix-rt/System) separate from tokio, which creates friction when the MQTT loop (rumqttc, tokio-native) runs alongside. axum runs directly on tokio with no runtime conflict.

**Why not raw hyper:** axum is the recommended ergonomic layer over hyper 1.x. Raw hyper requires much more boilerplate for routing and WebSocket upgrade.

**Browser UI:** Render SVG skyplot and SNR chart client-side using vanilla JavaScript. The server pushes JSON events over WebSocket: `{"type":"sat_state","sats":[{"id":"G01","az":45,"el":30,"snr":42},...]}`. The browser updates the SVG using DOM manipulation. No frontend build step required (no webpack/vite). Embed `index.html` as a `&'static str` via `include_str!`.

#### Serialization: serde + serde_json

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| serde | 1.x | Derive serialization | Standard; already likely in workspace |
| serde_json | 1.x | JSON for WebSocket messages | WebSocket API sends text frames; JSON is simplest for browser parsing |

#### Installation (server binary Cargo.toml additions)

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
rumqttc = "0.24"
rtcm-rs = { version = "0.11", features = ["1074","1084","1094","1124","1019","1020","1044","1046"] }
rinex = "0.21"
axum = { version = "0.8", features = ["ws"] }
tower-http = { version = "0.6", features = ["fs"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }  # UTC time for RINEX filenames and epoch records
```

---

### Feature 2: Embassy/nostd Crate Audit for ESP32-C6

This section maps every ESP-IDF capability used in the v2.0 firmware to its embassy/nostd equivalent, noting where gaps exist.

#### Core HAL: esp-hal

**Recommendation: esp-hal 1.0.0 (stable) with `unstable` feature for embassy/wifi**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| esp-hal | 1.0.0 | Hardware abstraction (GPIO, UART, SPI, I2C, timers) | First stable release (October 2025); ESP32-C6 is explicitly supported; RISC-V riscv32imac; 1.0 API stability commitment |
| esp-hal-embassy | (bundled in esp-hal 1.0) | Embassy executor integration | Provides `embassy_time` implementation using ESP32 SYSTIMER; required for `embassy-executor` |

**ESP-IDF crate vs esp-hal mapping:**

| ESP-IDF (current) | esp-hal equivalent | Status | Notes |
|-------------------|--------------------|--------|-------|
| `esp_idf_hal::uart::UartDriver` | `esp_hal::uart::Uart` | Available | Async read via embassy |
| `esp_idf_hal::gpio::PinDriver` | `esp_hal::gpio::Output/Input` | Available | |
| `esp_idf_hal::delay::FreeRtos` | `embassy_time::Timer` | Available | Use embassy executor |
| `esp_idf_hal::task::thread` | `embassy_executor::task` | Available | Replaces std::thread |
| `esp_idf_svc::wifi::EspWifi` | `esp_radio` (was esp-wifi) 0.15.x | Available (STA only) | SoftAP open-only in esp-wifi 0.15.x; password-protected SoftAP missing |
| `esp_idf_svc::mqtt::client::EspMqttClient` | `minimq 0.10` (MQTT5) or `rust-mqtt` | PARTIAL | minimq: MQTT5 only; rust-mqtt: MQTT3.1.1 + no_std but less battle-tested |
| `esp_idf_svc::nvs::EspNvs` | NO EQUIVALENT | GAP | Largest gap; see NVS section below |
| `esp_idf_svc::ota` | `esp-ota 0.2.2` | PARTIAL | esp-ota wraps ESP-IDF OTA API; still requires esp-idf-sys linkage; true nostd OTA via esp-ota-nostd (experimental) |
| `esp_idf_svc::tls::EspTls` | `embedded-tls 0.18.0` | PARTIAL | TLS 1.3 only (mbedTLS in ESP-IDF supports 1.2+); cipher suite coverage incomplete; marked "work in progress" |
| `esp_idf_svc::sntp` | `embassy-time` + NTP client | PARTIAL | No ready-made nostd SNTP crate; requires DIY UDP NTP packet decode over smoltcp |
| DNS server (port 53 UDP) for captive portal | smoltcp raw UDP socket | GAP | No DNS server crate for nostd; would require DIY |
| `EspHttpServer` (captive portal HTTP) | picoserve or DIY | PARTIAL | `picoserve` is a nostd HTTP server for embassy; missing form POST handling |

#### WiFi (Station + SoftAP): esp-radio

**Recommendation: esp-radio (formerly esp-wifi) 0.15.x with `unstable` feature**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| esp-radio | 0.15.x | WiFi station mode + embassy-net integration | Renamed from esp-wifi; supports ESP32-C6; integrates with embassy-net via `embassy-net-driver`; STA mode is functional |
| embassy-net | 0.6.x | TCP/IP stack (wraps smoltcp) | Required by esp-radio for IP networking; provides async sockets |
| smoltcp | 0.11.x | Underlying TCP/IP stack | Used by embassy-net internally |

**SoftAP gap (confirmed LOW confidence → MEDIUM after verification):** esp-wifi 0.15.x documentation explicitly lists "Support for non-open SoftAP" under "Missing / To be done." Only open (passwordless) SoftAP exists. The v2.0 firmware uses WPA2 SoftAP with a password for the provisioning portal. This is a **critical gap** for the nostd port.

#### MQTT (nostd): minimq vs rust-mqtt

**Recommendation: minimq 0.10.0 for nostd firmware MQTT (with broker MQTT5 support required)**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| minimq | 0.10.0 | MQTT client for nostd firmware | Actively maintained (January 2025 release); `no_std + no_alloc`; uses `embedded-nal` TCP abstraction |

**Critical protocol mismatch:** minimq supports MQTT 5.0 **only**. The v2.0 firmware uses MQTT 3.1.1. To use minimq, the MQTT broker must be configured for MQTT 5.0. Mosquitto 2.0+ supports MQTT 5.0. This is a **configuration change** on the broker side, not just a firmware change.

**Alternative — rust-mqtt:** Supports both MQTT 3.1.1 and MQTT 5.0 in no_std mode. Less mature than minimq. Use if MQTT 3.1.1 broker compatibility must be preserved.

#### TLS (nostd): embedded-tls

**Recommendation: embedded-tls 0.18.0 (NTRIP TLS replacement)**

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| embedded-tls | 0.18.0 | TLS 1.3 for NTRIP client in nostd firmware | Async via `embedded-io-async`; integrates with embassy-net TCP sockets; latest release January 2026 |

**Constraint:** TLS 1.3 only. AUSCORS (the NTRIP caster used in v2.0) must support TLS 1.3. Most modern servers do; verify at integration time.

#### NVS (nostd): GAP — Requires Gap Crate

**The NVS gap is the most significant blocker for the nostd port.**

The current firmware uses `EspNvs` (from esp-idf-svc) to persist:
- WiFi credentials (3 SSIDs + passwords)
- MQTT credentials
- NTRIP credentials (host, port, mount, user, pass, TLS flag)
- GNSS config blob
- `config_ver` schema version field

**Available nostd flash options:**
- `esp-storage` (archived, now in esp-hal): implements `embedded-storage` traits for raw flash access on ESP32-C6. Provides byte-level read/write to flash partitions — **not** key-value store.
- `sequential-storage` crate: key-value store built on `embedded-storage` traits. Pure Rust, no_std. Provides wear-levelled key-value map over raw flash. This is the closest nostd equivalent to ESP-IDF NVS.
- `esp-nvs` crate: claims ESP-IDF compatible bare-metal NVS. Uses ESP ROM CRC32 and Platform trait. Unverified compatibility with ESP32-C6 without ESP-IDF linkage.

**Recommendation for gap crate:**

Create a `gnss-nvs` gap crate that:
1. Defines a `NvsStore` trait (get/set for typed keys, erase)
2. Provides two implementations: `EspNvsStore` (wraps `esp_idf_svc::nvs`) and `SequentialStore` (wraps `sequential-storage` + `esp-storage`)
3. The `SequentialStore` implementation becomes the nostd path

**Gap crate stack:**

| Technology | Version | Purpose |
|------------|---------|---------|
| sequential-storage | 0.8.x | Wear-levelled key-value on raw flash |
| embedded-storage | 0.3.x | Flash trait interface (implemented by esp-storage/esp-hal flash driver) |

#### OTA (nostd): esp-ota-nostd

**Recommendation: Evaluate esp-ota-nostd (experimental)**

| Technology | Version | Purpose | Notes |
|------------|---------|---------|-------|
| esp-ota-nostd | 0.x | OTA update without ESP-IDF | From-scratch OTA compatible with default ESP32 bootloader; avoids esp-idf-sys linkage |

**Note:** `esp-ota 0.2.2` (faern/esp-ota) appears to still require esp-idf-sys linkage. True nostd OTA requires `esp-ota-nostd`. Both are experimental. **Low confidence** — verify at implementation time.

#### Embassy Executor Integration

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| embassy-executor | 0.7.x | Async task executor for embedded | Required for embassy async tasks on bare metal |
| embassy-time | 0.4.x | Timers, delays | Backed by ESP32 SYSTIMER via esp-hal-embassy |
| embassy-sync | 0.6.x | Channel, mutex, signal primitives | `embassy_sync::channel::Channel` replaces `std::sync::mpsc` |

**Rust toolchain requirement:** esp-hal 1.0.0 requires minimum Rust 1.88.0 (for `unstable`/esp-wifi features). This is newer than the current firmware's `rust-version = "1.77"`. The nostd port requires a toolchain bump.

---

## Gap Crate Skeleton Priorities

Ordered by blocking impact on nostd port:

| Priority | Gap | Blocker | Approach |
|----------|-----|---------|----------|
| 1 | NVS key-value storage | All config persistence | `sequential-storage` + `esp-hal` flash driver |
| 2 | Password-protected SoftAP | Provisioning portal | Wait for esp-radio or DIY WiFi config via BLE |
| 3 | MQTT 3.1.1 nostd | Broker compat | `rust-mqtt` or accept MQTT5 via `minimq` |
| 4 | NTRIP TLS 1.3 | AUSCORS corrections | `embedded-tls` (TLS1.3 only) |
| 5 | SNTP time sync | RINEX epoch timestamps | DIY UDP NTP client over embassy-net |
| 6 | HTTP server (captive portal) | Provisioning UX | `picoserve` for basic GET/POST |
| 7 | DNS server (captive portal) | Captive detection | DIY UDP over embassy-net |
| 8 | OTA nostd | Remote firmware update | `esp-ota-nostd` (experimental) |

---

## Milestone v1.2 Stack (Prior — Unchanged)

### Feature 1: RTCM3 Binary Frame Relay

**Stay at 115200 baud. No baud-rate change needed.**

**Bandwidth math:**

RTCM3 MSM4 frame structure:
- Frame overhead: 3 bytes (0xD3 preamble + 6 reserved bits + 10-bit length) + 3 bytes CRC = **6 bytes per frame**
- MSM4 fixed header: 169 bits
- Per-satellite delta: 10 bits (satellite mask cell)
- Per-signal cell: 42 bits (15-bit pseudorange mod 1ms + 22-bit carrier phase + 4-bit lock time + 1-bit half-cycle ambiguity)
- Formula: total bits = 169 + nSat×10 + nSig×42

Typical open-sky scenario (conservative estimates, 1 Hz):

| Constellation | Msg ID | Visible Sats | Signals | Payload bits | Frame bytes |
|---------------|--------|--------------|---------|-------------|-------------|
| GPS           | 1074   | 10           | 10      | 169+100+420=689 | ceil(689/8)+6 = **92** |
| GLONASS       | 1084   | 8            | 8       | 169+80+336=585  | ceil(585/8)+6 = **80** |
| Galileo       | 1094   | 10           | 10      | 169+100+420=689 | ceil(689/8)+6 = **92** |
| BeiDou        | 1124   | 12           | 12      | 169+120+504=793 | ceil(793/8)+6 = **106** |
| **Total**     |        |              |         |             | **~370 bytes/s** |

NMEA at 115200 baud with typical UM980 output (GGA, RMC, GSA, GSV×4, VTG = ~8 sentences × ~80 bytes = ~640 bytes/s):

Total UART load: ~370 + 640 = **~1,010 bytes/s**

At 115200 baud: 115,200 / 10 = **11,520 bytes/s** capacity.

RTCM + NMEA combined is **< 9% of 115200 baud capacity**. No baud rate change required.

**UM980 supported baud rates (COMCONFIG command):** 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600. Factory default is 115200.

**If RTCM MSM7 (higher precision, 58-bit signal cells instead of 42) were used**, the same constellation set would produce ~500 bytes/s — still well within 115200 budget.

### RTCM3 Frame Detection in Byte Stream

RTCM3 frames start with byte `0xD3`. NMEA sentences start with `$` (0x24). The byte stream is unambiguous: a `0xD3` byte starts an RTCM frame; `$` starts an NMEA sentence.

Frame parsing state machine (pure Rust, no external crate needed):

```
State: Idle
  0xD3  → read 2 more bytes (6-bit reserved + 10-bit length) → State: InRtcm(length)
  '$'   → accumulate until '\n' → emit NMEA sentence
  other → discard (log warn)

State: InRtcm(n)
  read n bytes (payload) + 3 bytes (CRC-24Q) → emit RTCM frame as &[u8]
  → State: Idle
```

Maximum RTCM frame size: 10-bit length field = max 1023 bytes payload + 6 bytes overhead = **1029 bytes maximum**.

**No external crate required.** The detection and framing logic is a simple state machine that fits in ~50 lines of Rust.

### MQTT Binary Payload Publishing

**API:** `EspMqttClient::enqueue(topic, QoS, retain, payload: &[u8])` — already used in this codebase. The `payload` parameter is `&[u8]`, so binary data works identically to text. No API change needed.

**Size limit:** The ESP-IDF MQTT client's internal outbox buffer defaults to 1024 bytes. RTCM frames up to 1029 bytes will exceed this.

**Recommendation:** Increase `out_buffer_size` to 2048 in `MqttClientConfiguration`.

**Topic pattern:** `gnss/{device_id}/rtcm/{msg_type}` where `msg_type` is the 4-digit RTCM message number (e.g., `1074`).

---

### Feature 2: OTA Firmware Update

All OTA functionality is in `esp_idf_svc::ota` — **no extra crate, no feature flag**.

**Key types (all in `esp_idf_svc::ota`):**

| Type | Purpose |
|------|---------|
| `EspOta` | Singleton OTA manager |
| `EspOtaUpdate<'a>` | Write handle; implements `io::Write` |
| `EspOtaUpdateFinished<'a>` | Returned by `update.finish()`; call `.activate()` |

**OTA sequence:**

```rust
let mut ota = EspOta::new()?;
let mut update = ota.initiate_update()?;
update.write(&chunk)?;  // repeated as HTTP data arrives
update.complete()?;
esp_idf_svc::hal::reset::restart();
// On next boot:
ota.mark_running_slot_valid()?;
```

**HTTP client:** `esp_idf_svc::http::client::EspHttpConnection` — already in the crate, no feature flag.

**Partition table change:** Replace single-factory layout with `otadata + ota_0 + ota_1` layout. `erase-flash` required after changing partition table.

---

## Core Technologies (Existing Firmware — Unchanged)

| Technology | Version | Purpose | Status |
|------------|---------|---------|--------|
| `esp-idf-svc` | =0.51.0 | WiFi, MQTT, HTTP client, OTA | Already pinned |
| `esp-idf-hal` | =0.45.2 | UART, GPIO | Already pinned |
| `esp-idf-sys` | =0.36.1 | ESP-IDF C bindings | Already pinned |
| `embedded-svc` | =0.28.1 | Trait definitions (MQTT, HTTP, OTA) | Already pinned |
| ESP-IDF | v5.3.3 | Underlying C framework | Already in .embuild/ |

---

## What NOT to Add

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| paho-mqtt (server) | C FFI, native lib dep, async story requires callbacks | rumqttc 0.24 AsyncClient |
| actix-web (server) | Uses actix-rt runtime, conflicts with tokio used by rumqttc | axum 0.8 (tokio-native) |
| tokio-tungstenite direct dep | axum uses it internally; direct dep causes version conflicts | axum::extract::ws |
| Custom RTCM parser on server | rtcm-rs already covers all RTCM 3.4 message types including MSM | rtcm-rs 0.11 |
| DIY RINEX nav writer (rush) | rinex crate has OBS writer working; start there, fall back to DIY only if 2.x compliance fails | rinex 0.21 for OBS; evaluate NAV separately |
| esp-storage directly (nostd) | Archived; moved into esp-hal; raw flash only, no key-value | sequential-storage over esp-hal flash driver |
| esp-ota (IDF-linked) for nostd | Still links esp-idf-sys; not truly bare metal | esp-ota-nostd (experimental) or deferred |
| embassy for firmware in v2.1 | The firmware is working on ESP-IDF; migration is the audit/gap work, not the port | Audit + gap skeletons in v2.1; port is a future milestone |

---

## Alternatives Considered

| Category | Recommended | Alternative | When Alternative Fits |
|----------|-------------|-------------|-----------------------|
| Server MQTT | rumqttc 0.24 | paho-mqtt | If MQTT 5.0 features (topic aliases, subscriptions IDs) are needed |
| Server HTTP | axum 0.8 | actix-web | If team has actix experience and no tokio MQTT client is used |
| RTCM decode | rtcm-rs 0.11 | DIY bit parser | If only 1-2 message types needed and no serde requirement |
| RINEX write | rinex 0.21 | DIY fixed-width writer | If RINEX 2.x format compliance is critical and rinex produces 3.x output |
| nostd MQTT | minimq 0.10 | rust-mqtt | If MQTT 3.1.1 broker compat must be preserved (no broker upgrade) |
| nostd TLS | embedded-tls 0.18 | mbedTLS via esp-idf | If TLS 1.2 is needed (AUSCORS caster TLS version audit required) |
| nostd KV store | sequential-storage | esp-nvs crate | If esp-nvs ESP32-C6 nostd compat is verified |

---

## Version Compatibility

| Package | Version | Notes |
|---------|---------|-------|
| esp-hal | 1.0.0 | Requires Rust 1.88.0 minimum (for unstable/wifi) — firmware currently targets 1.77 |
| esp-radio | 0.15.x | Requires `unstable` feature on esp-hal; renamed from esp-wifi |
| embassy-executor | 0.7.x | Must match version expected by esp-hal-embassy |
| rumqttc | 0.24.x | tokio 1.x required; avoid 0.25+ until stability verified |
| axum | 0.8.x | tokio 1.x; hyper 1.x internally |
| rtcm-rs | 0.11.0 | Last release April 2024; no breaking changes expected |
| rinex | 0.21.x | Active project; API may change; pin with `=` if stability required |
| embedded-tls | 0.18.0 | embedded-io-async 0.7 required; TLS 1.3 only |
| minimq | 0.10.0 | MQTT5 only; broker must support MQTT5 |
| sequential-storage | 0.8.x | embedded-storage 0.3 trait required |

---

## Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| rumqttc server usage | HIGH | Official crate from bytebeamio; tokio-native; widely used in Rust MQTT ecosystem |
| rtcm-rs MSM coverage | HIGH | GitHub README explicitly lists all RTCM 3.4 messages; version 0.11.0 confirmed |
| axum 0.8 WebSocket | HIGH | Official tokio-rs project; announced January 2025; WebSocket via `extract::ws` confirmed |
| rinex OBS writer | MEDIUM | GitHub docs show writer available for OBS; NAV writer unfinished; 2.x vs 3.x output format unverified without running code |
| esp-hal 1.0 ESP32-C6 | HIGH | Version 1.0.0 released October 2025; ESP32-C6 explicitly listed |
| esp-radio SoftAP gap | HIGH | esp-wifi 0.15.x docs explicitly list "non-open SoftAP" as missing |
| minimq MQTT5 only | HIGH | Documentation unambiguous; 15 releases all targeting MQTT5 |
| embedded-tls TLS1.3 only | HIGH | Documentation unambiguous; architecture by design |
| sequential-storage for NVS | MEDIUM | Crate is real and functional; compatibility with ESP32-C6 esp-hal flash driver unverified without building |
| esp-ota-nostd | LOW | Exists as crate; no verified integration on ESP32-C6 without ESP-IDF |

---

## Sources

**v2.1 additions:**
- [GitHub: bytebeamio/rumqtt](https://github.com/bytebeamio/rumqtt) — rumqttc 0.24/0.25 latest version, AsyncClient API. MEDIUM confidence (WebSearch; crates.io unavailable during research).
- [GitHub: martinhakansson/rtcm-rs](https://github.com/martinhakansson/rtcm-rs) — version 0.11.0 confirmed, all RTCM 3.4 MSM messages confirmed. HIGH confidence.
- [GitHub: nav-solutions/rinex](https://github.com/nav-solutions/rinex) — v0.21.1 latest; OBS writer available; NAV writer incomplete. MEDIUM confidence.
- [Tokio blog: Announcing axum 0.8.0](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) — axum 0.8 released January 2025. HIGH confidence.
- [GitHub: tokio-rs/axum releases](https://github.com/tokio-rs/axum/releases) — v0.8.8 latest stable. HIGH confidence.
- [GitHub: esp-rs/esp-hal](https://github.com/esp-rs/esp-hal) — 1.0.0 stable released October 2025; ESP32-C6 supported. HIGH confidence.
- [docs.rs: esp-wifi 0.15.1](https://docs.rs/crate/esp-wifi/latest) — SoftAP (non-open) listed as missing; renamed to esp-radio. HIGH confidence.
- [lib.rs: minimq](https://lib.rs/crates/minimq) — version 0.10.0 January 2025; MQTT5 only. HIGH confidence.
- [lib.rs: embedded-tls](https://lib.rs/crates/embedded-tls) — version 0.18.0 January 2026; TLS 1.3; embedded-io-async 0.7; work in progress. HIGH confidence.
- [GitHub: esp-rs/esp-storage](https://github.com/esp-rs/esp-storage) — raw flash only; archived (moved to esp-hal); ESP32-C6 supported. HIGH confidence.
- [WebSearch: esp-ota-nostd](https://lib.rs/crates/esp-ota-nostd) — exists; from-scratch bootloader-compatible OTA. LOW confidence (not verified by doc fetch).

**v1.2 additions (prior milestone):**
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/ota.rs` — EspOta API. HIGH confidence (direct source read).
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/lib.rs` — OTA module gate conditions. HIGH confidence.
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/http/client.rs` — EspHttpConnection::new. HIGH confidence.

---

*Stack research for: RTCM binary relay + OTA (v1.2); Tokio server binary + embassy/nostd audit (v2.1), ESP32-C6 Rust firmware*
*Researched: 2026-03-07 (v1.2), 2026-03-12 (v2.1)*
