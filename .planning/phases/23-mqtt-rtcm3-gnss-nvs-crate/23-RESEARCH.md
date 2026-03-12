# Phase 23: MQTT + RTCM3 + gnss-nvs crate - Research

**Researched:** 2026-03-12
**Domain:** Rust async MQTT (Tokio), RTCM3 MSM decoding, no_std flash KV store (sequential-storage), gnss-nvs crate design
**Confidence:** HIGH (core stack verified via official docs and crates.io)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### MQTT client library
- Evaluate all pure-Rust async MQTT client options this phase (benchmark phase). If only one exists, it wins by default; if multiple pure-Rust options exist, benchmark them and select the winner.
- Implementation goes in Phase 24 (this phase is research + benchmarks).
- Async runtime: Tokio — best ecosystem fit for MQTT + HTTP + WebSocket (Phase 25).

#### MQTT server architecture
- Dedicated Tokio supervisor task owns the MQTT EventLoop, broadcasts connection state via a `watch` channel.
- Reconnect with exponential backoff from the supervisor task.
- Other tasks receive decoded data via Tokio channels.

#### Server configuration
- TOML config file (path via `--config` CLI flag) as the base.
- Environment variable override for any value in the TOML — useful for secrets (broker password, credentials) without putting them in the config file.
- Variable substitution syntax in TOML (e.g. `password = "${MQTT_PASSWORD}"`) preferred over a separate env-only layer.

#### Epoch grouping strategy
- MSM messages carry a `gnssEpochTime` field — parse it and use it as the epoch key.
- Epoch boundary detection: when a new `gnssEpochTime` arrives that differs from the currently buffered epoch time, flush the buffered epoch and start a new one.
- No timeout — flush on epoch-change only. Late constellations over MQTT simply join the next epoch.
- Buffer keyed by `epoch_time` only (not per-constellation). All MSMs with the same epoch time accumulate into one output epoch regardless of constellation.
- Log at each epoch boundary: epoch timestamp + constellation + SV count (e.g. `Epoch 2026-03-12T04:23:11.200Z GPS:8 GLO:4 GAL:3 BDS:0`).

#### gnss-nvs trait API
- Trait: `NvsStore` with associated `Error` type — each impl defines its own error; app code wraps with `anyhow`.
- Key type: `namespace: &str` + `key: &str` — mirrors ESP-IDF NVS API, maps cleanly to sequential-storage.
- Sync trait (not async) — flash NVS is fast and blocking; both impls are sync.
- Typed getters/setters via `get<T: DeserializeOwned>` / `set<T: Serialize>` (postcard for serialization).
- Blob support via separate `get_blob(&mut [u8])` / `set_blob(&[u8])` methods — not unified with typed API.

#### gnss-nvs crate design
- Clean-room trait design: the `gnss-nvs` crate has no `esp-idf-*` dependency.
- ESP-IDF impl is feature-gated (or a separate sub-crate) and wraps `NvmStorage` from `esp-idf-svc`.
- sequential-storage impl is the no_std flash-backed implementation (compiles in Phase 23; hardware validation deferred).
- Both impls live in `crates/gnss-nvs/` in the workspace.

#### Crate layout
- All gap crates live under `crates/` in the workspace root: `crates/gnss-nvs/`, `crates/gnss-ota/` (Phase 24), etc.
- Names: keep `gnss-nvs`, `gnss-ota` etc. as-is. Will rename to something generic (e.g. `embedded-nvs`) when/if publishing to crates.io once the trait API stabilises post-Phase 25 hardware validation.
- Publish intent: yes, publish when stable and broadly useful; rename at publish time.

### Claude's Discretion
- Exact postcard vs serde-json choice for typed NvsStore serialization (postcard likely — no_std compatible)
- MqttMessage struct/enum design for internal channel passing in the server
- Specific error variants for the sequential-storage NvsStore impl
- Whether to use `figment` or a simpler custom approach for TOML + env var config loading

### Deferred Ideas (OUT OF SCOPE)
- RINEX file writing — Phase 24
- Web UI / WebSocket push — Phase 25
- gnss-ota crate — Phase 24
- gnss-softap / gnss-dns / gnss-log gap skeletons — Phase 25
- Hardware validation of sequential-storage NvsStore on device FFFEB5 — future milestone (NOSTD-F02)
- Multi-device MQTT subscription — future (SRVR-F01)
- Async NvsStore trait — future if flash drivers go async
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SRVR-01 | Server binary subscribes to MQTT `gnss/{id}/rtcm`, `gnss/{id}/nmea`, and `gnss/{id}/heartbeat` for a configured device ID | rumqttc AsyncClient + subscribe pattern documented; topic format established |
| SRVR-02 | Server reconnects to MQTT broker after disconnect with exponential backoff | rumqttc EventLoop auto-reconnect + tokio-retry ExponentialBackoff; watch channel for state broadcast |
| RTCM-01 | Server decodes RTCM3 MSM4/MSM7 messages for GPS and GLONASS (pseudorange, carrier phase, C/N0) | rtcm-rs Msg1074T/Msg1077T (GPS), Msg1084T/Msg1087T (GLONASS); field names verified |
| RTCM-02 | Server decodes RTCM3 MSM messages for Galileo and BeiDou (best-effort) | rtcm-rs Msg1094T/Msg1097T (Galileo), Msg1124T/Msg1127T (BeiDou); same API pattern |
| RTCM-03 | Server decodes RTCM3 ephemeris messages 1019, 1020, 1046, 1044 | Msg1019T (GPS eph), Msg1020T (GLONASS eph), Msg1046T (Galileo), Msg1044T (BeiDou); all in rtcm-rs |
| RTCM-04 | Server buffers MSM frames within a ~10ms epoch window before emitting an observation epoch | Epoch-change detection via gnssEpochTime field; flush-on-change strategy; no timeout needed |
| NOSTD-02 | `gnss-nvs` crate with NvsStore trait and ESP-IDF NVS backing implementation | EspNvs API: get_blob/set_blob, get_str/set_str, get_u8/set_u8; trait design verified; postcard for typed layer |
| NOSTD-03 | `sequential-storage` backed `NvsStore` implementation started (compiles; hardware validation deferred) | sequential-storage MapStorage API verified; Key+Value/PostcardValue traits documented; embedded-storage NorFlash required |
</phase_requirements>

## Summary

Phase 23 has two distinct work streams that can be developed in parallel: the `gnss-server` binary (MQTT subscription + RTCM3 decode + epoch grouping) and the `crates/gnss-nvs/` crate (NvsStore trait + two backing implementations). Both streams have well-understood library choices with verified APIs.

The MQTT client side uses `rumqttc` (most widely adopted pure-Rust async MQTT client, Tokio-native). The server benchmark required by CONTEXT.md is straightforward because no other production-grade pure-Rust async std MQTT option with Tokio exists at comparable maturity — minimq and rust-mqtt target no_std/embassy. RTCM3 decoding uses `rtcm-rs 0.11`, already locked in STATE.md. The epoch-time field names differ per constellation: `gps_epoch_time_ms` for GPS, `glo_epoch_time_ms` for GLONASS, `gal_epoch_time_ms` for Galileo, `bds_epoch_time_ms` for BeiDou — the planner must handle all four variants when mapping to a common u32 epoch key.

For gnss-nvs: the ESP-IDF impl wraps `EspNvs` (which has direct `get_blob`/`set_blob`/`get_str`/`set_str`/`get_u8`/`set_u8` methods — confirmed by firmware code using these methods). The sequential-storage impl wraps `MapStorage` using the `Key` and `PostcardValue` traits. The clean-room trait (`NvsStore`) lives in the crate root with no ESP-IDF dependency; the two impls are feature-gated. Server config uses `figment` with `Toml::file()` + `Env::prefixed()` merge — figment does NOT natively support `${VAR}` substitution in TOML, so CONTEXT.md's preferred approach requires a custom pre-processing step or an alternative strategy (see Open Questions).

**Primary recommendation:** Implement in this order: (1) crates/gnss-nvs/ NvsStore trait + ESP-IDF impl, (2) sequential-storage impl skeleton, (3) gnss-server MQTT supervisor task, (4) RTCM3 decode pipeline, (5) server config loading.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rtcm-rs | 0.11 | RTCM3 MSM + ephemeris decode | Locked in STATE.md; no_std compatible, all message types supported, forbids unsafe |
| rumqttc | 0.24+ | Async MQTT 3.1.1 client (server binary) | Only production-grade pure-Rust async MQTT for Tokio; 274k downloads/month |
| tokio | 1.x | Async runtime (server) | Locked in CONTEXT.md; ecosystem fit for MQTT + HTTP + WebSocket |
| sequential-storage | latest | Flash KV map for sequential-storage NvsStore impl | Purpose-built no_std flash KV; uses embedded-storage traits; PostcardValue integration |
| postcard | 1.x | Binary serialization for NvsStore typed layer | no_std compatible, stable wire format since 1.0, serde derive support |
| figment | 0.10 | TOML + env var config loading | Standard Rocket ecosystem config library; TOML + Env provider merge |
| serde | 1.x | Derive for config structs and NvsStore typed values | Universal |
| anyhow | 1.x | Error handling in server binary | Already in workspace |
| bytes | 1.x | MQTT payload handling (zero-copy) | Already in workspace; firmware uses Bytes for RTCM relay |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio-retry | 0.3 | Exponential backoff strategy | Used in MQTT supervisor task for reconnect delay |
| clap | 4.x | CLI `--config` flag parsing | Server binary entry point |
| embedded-storage | 0.3 | NorFlash trait for sequential-storage | Required by sequential-storage; not pulled into esp-idf build |
| log | 0.4 | Logging (already workspace dep) | Universal; server uses env_logger or similar |
| env_logger | 0.11 | Log output for server binary | Standard for std binaries |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rumqttc | minimq | minimq is no_std/MQTT5; not suitable for std Tokio server |
| rumqttc | rust-mqtt | Less mature, fewer downloads, no clear Tokio integration |
| figment | config-rs | config-rs 0.13 is popular but figment has cleaner layered API |
| postcard | serde_json | serde_json requires std; postcard works in no_std sequential-storage impl |
| sequential-storage | ekv | ekv is LSM-tree based (embassy-rs project); sequential-storage has simpler linear flash model matching NVS semantics |

**Installation (gnss-server):**
```bash
cargo add rumqttc tokio --features tokio/full
cargo add rtcm-rs
cargo add figment --features toml,env
cargo add clap --features derive
cargo add tokio-retry
cargo add serde --features derive
cargo add anyhow log env_logger bytes
```

**Installation (crates/gnss-nvs):**
```bash
cargo add postcard --no-default-features --features alloc
cargo add serde --no-default-features --features derive
# For sequential-storage impl (feature-gated):
cargo add sequential-storage embedded-storage
# For ESP-IDF impl (feature-gated):
cargo add esp-idf-svc
```

## Architecture Patterns

### Recommended Project Structure

```
gnss-server/
├── src/
│   ├── main.rs          # CLI parsing, config load, Tokio runtime, supervisor spawn
│   ├── config.rs        # ServerConfig struct, figment load, env var pre-process
│   ├── mqtt.rs          # MQTT supervisor task, watch channel, reconnect loop
│   ├── rtcm_decode.rs   # RTCM3 frame decode, MSM observation structs
│   ├── epoch.rs         # Epoch buffer, flush-on-change logic, log line
│   └── observation.rs   # Observation and EpochGroup output types

crates/gnss-nvs/
├── Cargo.toml           # no esp-idf-svc dep in default; features = ["esp-idf", "sequential"]
├── src/
│   ├── lib.rs           # NvsStore trait definition only
│   ├── esp_idf.rs       # #[cfg(feature = "esp-idf")] EspNvsStore impl
│   └── sequential.rs    # #[cfg(feature = "sequential")] SeqNvsStore impl
```

### Pattern 1: MQTT Supervisor Task with watch Channel

**What:** Dedicated Tokio task owns EventLoop. On connection events, broadcasts state via `tokio::sync::watch`. On disconnect, applies exponential backoff before reconnecting.

**When to use:** Any time other tasks need to know broker connection state without owning the EventLoop.

**Example:**
```rust
// Source: rumqttc docs + STATE.md supervisor pattern
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::watch;

pub async fn mqtt_supervisor(
    opts: MqttOptions,
    topics: Vec<String>,
    tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
    state_tx: watch::Sender<bool>,
) {
    let backoff_steps = [1u64, 2, 5, 10, 30];
    let mut fail_count = 0usize;

    loop {
        let (client, mut eventloop) = AsyncClient::new(opts.clone(), 64);
        for topic in &topics {
            let _ = client.subscribe(topic, QoS::AtMostOnce).await;
        }
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    let _ = state_tx.send(true);
                    fail_count = 0;
                }
                Ok(Event::Incoming(Packet::Publish(pub_msg))) => {
                    let _ = tx.send(pub_msg.payload.into()).await;
                }
                Err(e) => {
                    log::warn!("MQTT error: {:?}", e);
                    let _ = state_tx.send(false);
                    break;
                }
                _ => {}
            }
        }
        let delay = backoff_steps[fail_count.min(backoff_steps.len() - 1)];
        fail_count += 1;
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
    }
}
```

### Pattern 2: RTCM3 Decode with rtcm-rs

**What:** `next_msg_frame` extracts a MessageFrame from a byte slice; `get_message()` returns the typed Message enum. Match on MSM variants to extract observation data.

**When to use:** Any raw RTCM3 byte payload from the MQTT topic.

**Example:**
```rust
// Source: rtcm-rs docs.rs + GitHub README
use rtcm_rs::prelude::*;

fn decode_rtcm(payload: &[u8]) -> Option<Message> {
    let (_, frame) = next_msg_frame(payload)?; // returns (bytes_consumed, Option<MessageFrame>)
    let frame = frame?;
    Some(frame.get_message())
}

// MSM7 GPS — epoch key and satellite/signal data
match msg {
    Message::Msg1077(m) => {
        let epoch_ms = m.gps_epoch_time_ms;  // u32
        for sig in &m.data_segment.signal_data {
            let pseudorange = sig.gnss_signal_fine_pseudorange_ext_ms; // Option<f64>
            let phase = sig.gnss_signal_fine_phaserange_ext_ms;        // Option<f64>
            let cnr = sig.gnss_signal_cnr_ext_dbhz;                    // Option<f64>
        }
    }
    // GLONASS — note different epoch field name
    Message::Msg1087(m) => {
        let epoch_ms = m.glo_epoch_time_ms;  // u32 — different field name!
        // ... same signal struct pattern
    }
    _ => {}
}
```

### Pattern 3: Epoch Grouping (flush-on-change)

**What:** Buffer all signals from one constellation epoch until the epoch key changes. Emit the accumulated epoch when a new epoch key arrives.

**When to use:** MSM processing loop; one global epoch buffer per device ID subscription.

**Example:**
```rust
// Source: CONTEXT.md epoch strategy decisions
use std::collections::HashMap;

struct EpochBuffer {
    epoch_key: u32,            // current epoch_ms value
    observations: Vec<Obs>,   // accumulated signals
}

impl EpochBuffer {
    fn push(&mut self, epoch_ms: u32, obs: Vec<Obs>) -> Option<EpochGroup> {
        if self.epoch_key != 0 && epoch_ms != self.epoch_key {
            // Flush previous epoch — new epoch started
            let group = self.flush();
            self.epoch_key = epoch_ms;
            self.observations = obs;
            return Some(group);
        }
        self.epoch_key = epoch_ms;
        self.observations.extend(obs);
        None
    }
}
```

Epoch boundary log line format (CONTEXT.md requirement):
```
Epoch 2026-03-12T04:23:11.200Z GPS:8 GLO:4 GAL:3 BDS:0
```

### Pattern 4: NvsStore Trait Design

**What:** Clean-room trait with associated Error type. No esp-idf-svc dependency at trait level.

```rust
// Source: CONTEXT.md trait decisions
pub trait NvsStore {
    type Error: std::fmt::Debug;

    fn get<T: serde::de::DeserializeOwned>(
        &self, namespace: &str, key: &str,
    ) -> Result<Option<T>, Self::Error>;

    fn set<T: serde::Serialize>(
        &mut self, namespace: &str, key: &str, value: &T,
    ) -> Result<(), Self::Error>;

    fn get_blob<'a>(
        &self, namespace: &str, key: &str, buf: &'a mut [u8],
    ) -> Result<Option<&'a [u8]>, Self::Error>;

    fn set_blob(
        &mut self, namespace: &str, key: &str, data: &[u8],
    ) -> Result<(), Self::Error>;
}
```

### Pattern 5: Server Config Loading (figment)

**What:** figment merges TOML base config with environment variable overrides. TOML `${VAR}` substitution is NOT native in figment — use `Env::prefixed()` for secrets instead.

**Decision on CONTEXT.md's preferred `${VAR}` syntax:** figment does not support it natively. The practical alternative is using `Env::prefixed("GNSS_")` which overrides any matching key. E.g., `GNSS_MQTT__PASSWORD=secret` overrides `mqtt.password` in TOML. This is strictly better for secrets (secrets never appear in TOML at all). Recommend adopting `Env::prefixed("GNSS_")` with double-underscore nesting separator.

```rust
// Source: figment docs.rs
use figment::{Figment, providers::{Toml, Env, Format}};

#[derive(serde::Deserialize)]
struct ServerConfig {
    mqtt: MqttConfig,
    device_id: String,
}

fn load_config(path: &str) -> anyhow::Result<ServerConfig> {
    let config: ServerConfig = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("GNSS_").split("__"))
        // GNSS_MQTT__PASSWORD overrides mqtt.password
        // GNSS_DEVICE_ID overrides device_id
        .extract()?;
    Ok(config)
}
```

### Pattern 6: ESP-IDF NvsStore Implementation

The existing firmware uses these EspNvs methods directly (confirmed by code grep):
- `nvs.get_blob("key", &mut buf)` → `Result<Option<&[u8]>, EspError>`
- `nvs.set_blob("key", &[u8])` → `Result<(), EspError>`
- `nvs.get_str("key", &mut buf)` → `Result<Option<&str>, EspError>`
- `nvs.set_str("key", &str)` → `Result<(), EspError>`
- `nvs.get_u8("key")` → `Result<Option<u8>, EspError>`
- `nvs.set_u8("key", u8)` → `Result<(), EspError>`

The `EspNvsImpl` for `NvsStore` maps `namespace` by opening a new `EspNvs::new(partition, namespace, true)` handle per call (matching firmware pattern in `config_relay.rs`). The typed `get<T>/set<T>` layer serializes via postcard into a stack-allocated buffer before calling `set_blob`.

### Pattern 7: sequential-storage NvsStore Implementation

```rust
// Source: sequential-storage docs.rs MapStorage API
use sequential_storage::map::{MapStorage, Key, PostcardValue};
use embedded_storage::nor_flash::NorFlash;

// Key type: (namespace, key) pair serialized as postcard
// Value type: implements PostcardValue (= Serialize + Deserialize<'a>)

// MapStorage::fetch_item signature:
// pub async fn fetch_item<'d, V: Value<'d>>(
//     &mut self, data_buffer: &'d mut [u8], search_key: &K,
// ) -> Result<Option<V>, Error<S::Error>>

// MapStorage::store_item signature:
// pub async fn store_item<'d, V: Value<'d>>(
//     &mut self, data_buffer: &mut [u8], key: &K, item: &V,
// ) -> Result<(), Error<S::Error>>
```

**Note:** MapStorage is async (takes `&mut self` async fn). The NvsStore trait is sync. The sequential-storage impl wraps this by using `block_on` from a no_std executor or using the `embedded-storage` sync (non-async) flash driver path. Check if sequential-storage has a sync API variant; if not, the impl can use a minimal `futures_executor::block_on` or embassy-futures `block_on` wrapper.

### Anti-Patterns to Avoid

- **Matching on `_` for epoch time field:** Each constellation has a different field name (`gps_epoch_time_ms`, `glo_epoch_time_ms`, `gal_epoch_time_ms`, `bds_epoch_time_ms`). Must match each constellation explicitly — do NOT assume a single field name.
- **Using 0.0 for missing GLONASS carrier phase:** STATE.md decision is `Option::None`, never `0.0`. The `gnss_signal_fine_phaserange_ext_ms` field is already `Option<f64>` in rtcm-rs — pass it through as-is.
- **Opening EspNvs once and sharing across namespaces:** EspNvs is opened per-namespace. The NvsStore impl must open a fresh handle for each namespace string passed in.
- **Adding ESP-IDF deps to crate root:** `crates/gnss-nvs/` must have NO `esp-idf-svc` dependency at the crate root — only under a `[target.'cfg(...)'.dependencies]` or feature gate.
- **Figment `${VAR}` substitution:** figment does not support this natively. Use `Env::prefixed("GNSS_").split("__")` instead; document the override naming convention in config.toml comments.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| RTCM3 MSM cell mask and pseudorange scaling | Custom MSM decoder | rtcm-rs | MSM cell mask parsing has O(n*m) satellite×signal cell logic; pseudorange reconstruction requires modulo-1ms addition of rough+fine ranges; hand-rolling introduces well-known bugs (RTKLIB has had multiple MSM bugs) |
| MQTT reconnect + keep-alive | Custom TCP reconnect loop | rumqttc EventLoop | rumqttc handles TCP half-open detection, PINGREQ/PINGRESP, QoS state machine |
| Flash wear leveling for NVS | Append-then-erase flash driver | sequential-storage | Flash page erase lifetime management; sequential-storage tracks page state correctly |
| Hierarchical config merging | Custom TOML + env var parser | figment | Precedence chains, type coercion, TOML parsing edge cases |
| Postcard serialization for KV values | Custom binary format | postcard | Stable wire format since 1.0; handles all serde-derivable types including enums, Options |

**Key insight:** The RTCM3 MSM decode is the most dangerous area for hand-rolling. MSM4 vs MSM7 differ in whether fine pseudorange uses the 15-bit (MSM4) or 20-bit extended (MSM7) field. The GLONASS FCN absence from MSM signal structs (FCN is not in Msg1087Sig) means carrier phase cannot be reconstructed to a frequency-domain value without an external FCN source — this validates the Option::None decision.

## Common Pitfalls

### Pitfall 1: GLONASS Epoch Time Wraps at 86,400,000 ms (day boundary)

**What goes wrong:** `glo_epoch_time_ms` resets to 0 at GLONASS day boundary (~UTC+3 midnight). If the server is running across a boundary, epoch-change detection triggers a spurious flush.

**Why it happens:** GLONASS epoch time is time-of-day in the GLONASS time zone, not GPS week time.

**How to avoid:** Epoch grouping is per-epoch-time value, not cross-constellation synchronization. A boundary flush is harmless — it emits a partial epoch, then starts a new one. Document this as known behavior; do not add cross-constellation clock alignment in Phase 23.

**Warning signs:** Epoch boundary log shows a very small GLO count after a `glo_epoch_time_ms == 0` entry.

### Pitfall 2: Epoch Field Name per Constellation

**What goes wrong:** Attempting to use a single `epoch_time` field across all MSM message types — there is no common field name.

**Why it happens:** RTCM spec assigns constellation-specific epoch time fields. rtcm-rs models this faithfully.

**How to avoid:** Define a helper function or trait that extracts epoch_ms from each MSM message variant:
- GPS (1074/1077): `m.gps_epoch_time_ms`
- GLONASS (1084/1087): `m.glo_epoch_time_ms`
- Galileo (1094/1097): `m.gal_epoch_time_ms` (verify field name)
- BeiDou (1124/1127): `m.bds_epoch_time_ms` (verify field name)

**Warning signs:** Compile error when accessing `.epoch_time` — each Message variant has a different field.

### Pitfall 3: sequential-storage MapStorage is async; NvsStore is sync

**What goes wrong:** Calling `block_on(map.fetch_item(...))` inside a sync NvsStore impl when called from a Tokio async context panics if Tokio's current-thread runtime is in scope.

**Why it happens:** Tokio's `block_on` cannot be called from within an existing Tokio executor.

**How to avoid:** The sequential-storage NvsStore impl is only used in embedded (no Tokio) contexts. The firmware embedded target never uses Tokio — `block_on` with embassy-futures or a simple spin executor is safe. For the ESP-IDF impl (std context), use the synchronous EspNvs API directly. The two impls never share a runtime context.

**Warning signs:** `thread 'main' panicked at 'Cannot start a runtime from within a runtime'`

### Pitfall 4: EspNvs namespace is opened read-write for all ops

**What goes wrong:** `EspNvs::new(partition, ns, true)` opens the namespace in read-write mode. Opening with `true` (mutable) on a partition already opened by another handle can cause corruption.

**Why it happens:** NVS handles are per-namespace; multiple handles to the same namespace from different threads are safe per ESP-IDF docs only if they use different partitions.

**How to avoid:** In the ESP-IDF NvsStore impl, hold the `EspNvsPartition` handle and open a new `EspNvs` per call (matching the firmware pattern). Do not cache `EspNvs` handles between calls. The firmware pattern in `config_relay.rs` opens a fresh handle per save — follow this.

### Pitfall 5: rumqttc EventLoop must be polled continuously

**What goes wrong:** Subscribing after `AsyncClient::new` without starting the poll loop causes the subscription to never reach the broker — the EventLoop is not polled.

**Why it happens:** rumqttc is poll-driven; `client.subscribe()` enqueues the packet in the channel, but the EventLoop sends it only when `poll()` is called.

**How to avoid:** Spawn the EventLoop poll loop as the first thing in the supervisor task. All `client.*()` calls work concurrently via the internal channel once polling is running.

### Pitfall 6: rtcm-rs feature flags required for MSM messages

**What goes wrong:** By default, rtcm-rs compiles all messages. If adding `default-features = false` for binary size, MSM message types (1074, 1077, 1084, etc.) must be enabled individually.

**Why it happens:** rtcm-rs has per-message-type feature flags for embedded size optimization.

**How to avoid:** For the server binary, use `rtcm-rs = { version = "0.11" }` with default features (all messages). For embedded use (not Phase 23), selectively enable only needed message types.

## Code Examples

Verified patterns from official sources:

### Decode RTCM3 Frame from MQTT Payload

```rust
// Source: rtcm-rs docs.rs (next_msg_frame + get_message pattern)
use rtcm_rs::prelude::*;

fn process_rtcm_payload(payload: &[u8]) {
    let mut remaining = payload;
    loop {
        match next_msg_frame(remaining) {
            (consumed, Some(frame)) => {
                remaining = &remaining[consumed..];
                match frame.get_message() {
                    Message::Msg1077(m) => handle_gps_msm7(&m),
                    Message::Msg1087(m) => handle_glo_msm7(&m),
                    Message::Msg1097(m) => handle_gal_msm7(&m),
                    Message::Msg1127(m) => handle_bds_msm7(&m),
                    Message::Msg1074(m) => handle_gps_msm4(&m),
                    Message::Msg1084(m) => handle_glo_msm4(&m),
                    Message::Msg1019(m) => handle_gps_eph(&m),
                    Message::Msg1020(m) => handle_glo_eph(&m),
                    _ => {}
                }
            }
            (0, None) => break, // no more frames
            (consumed, None) => { remaining = &remaining[consumed..]; }
        }
    }
}
```

### GPS MSM7 Signal Data Extraction

```rust
// Source: rtcm-rs Msg1077T / Msg1077Data / Msm57Sat / Msg1077Sig field docs
fn handle_gps_msm7(m: &Msg1077T) {
    let epoch_ms = m.gps_epoch_time_ms;  // u32
    for sig in &m.data_segment.signal_data {
        // satellite_id: u8, signal_id: SigId
        let sv = sig.satellite_id;
        // All fine measurements are Option<f64>
        let pseudorange_ms = sig.gnss_signal_fine_pseudorange_ext_ms;
        let phase_ms = sig.gnss_signal_fine_phaserange_ext_ms;
        let cnr_dbhz = sig.gnss_signal_cnr_ext_dbhz;
    }
    for sat in &m.data_segment.satellite_data {
        // rough range components for full pseudorange reconstruction
        let rough_int = sat.gnss_satellite_rough_range_integer_ms;   // Option<u8>
        let rough_mod = sat.gnss_satellite_rough_range_mod1ms_ms;    // f64
    }
}
```

### GLONASS MSM7 — Option::None for Phase (no FCN)

```rust
// Source: rtcm-rs Msg1087Sig fields + STATE.md GLONASS decision
fn handle_glo_msm7(m: &Msg1087T) {
    let epoch_ms = m.glo_epoch_time_ms;  // u32, day-of-week time in ms
    for sig in &m.data_segment.signal_data {
        // gnss_signal_fine_phaserange_ext_ms is Option<f64>
        // Without FCN, carrier phase cannot be converted to cycles — emit None
        // FCN is NOT present in Msg1087Sig — the rtcm-rs field is already Option
        let phase: Option<f64> = sig.gnss_signal_fine_phaserange_ext_ms;
        // In output observation, carrier_phase = None when phase.is_none() OR when FCN unknown
    }
}
```

### NvsStore Trait + ESP-IDF Implementation Skeleton

```rust
// crates/gnss-nvs/src/lib.rs
pub trait NvsStore {
    type Error: core::fmt::Debug;

    fn get<T: serde::de::DeserializeOwned>(
        &self, namespace: &str, key: &str,
    ) -> Result<Option<T>, Self::Error>;

    fn set<T: serde::Serialize>(
        &mut self, namespace: &str, key: &str, value: &T,
    ) -> Result<(), Self::Error>;

    fn get_blob<'a>(
        &self, namespace: &str, key: &str, buf: &'a mut [u8],
    ) -> Result<Option<&'a [u8]>, Self::Error>;

    fn set_blob(
        &mut self, namespace: &str, key: &str, data: &[u8],
    ) -> Result<(), Self::Error>;
}

// crates/gnss-nvs/src/esp_idf.rs  (feature = "esp-idf")
#[cfg(feature = "esp-idf")]
pub struct EspNvsStore {
    partition: esp_idf_svc::nvs::EspNvsPartition<esp_idf_svc::nvs::NvsDefault>,
}

#[cfg(feature = "esp-idf")]
impl NvsStore for EspNvsStore {
    type Error = esp_idf_svc::sys::EspError;

    fn set_blob(&mut self, namespace: &str, key: &str, data: &[u8]) -> Result<(), Self::Error> {
        // Pattern from firmware/src/config_relay.rs save_gnss_config()
        let mut nvs = esp_idf_svc::nvs::EspNvs::new(self.partition.clone(), namespace, true)?;
        nvs.set_blob(key, data)
    }
    // ... get_blob, get<T>, set<T> via postcard
}
```

### sequential-storage NvsStore Skeleton

```rust
// crates/gnss-nvs/src/sequential.rs  (feature = "sequential")
// NOTE: MapStorage is async — see Pitfall 3; this impl is only for no_std embedded targets
use sequential_storage::map::{MapStorage, PostcardValue};
use embedded_storage::nor_flash::NorFlash;

pub struct SeqNvsStore<S: NorFlash> {
    storage: MapStorage<NsKey, S, sequential_storage::map::NoCache>,
    data_buf: [u8; 256],  // must fit longest key+value serialized
}

// NsKey implements Key: serializes as postcard bytes of (namespace, key) strings
```

### figment Config Loading

```rust
// Source: figment docs.rs Toml + Env::prefixed pattern
use figment::{Figment, providers::{Toml, Env, Format}};

pub fn load_server_config(config_path: &str) -> anyhow::Result<ServerConfig> {
    let config = Figment::new()
        .merge(Toml::file(config_path))
        .merge(Env::prefixed("GNSS_").split("__"))
        .extract::<ServerConfig>()?;
    Ok(config)
}
// GNSS_MQTT__BROKER overrides mqtt.broker
// GNSS_MQTT__PASSWORD overrides mqtt.password
// GNSS_DEVICE_ID overrides device_id
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| rumqttc 0.10 sync client | rumqttc 0.24+ AsyncClient + EventLoop | 2023+ | EventLoop poll loop is the idiomatic pattern; sync API still works but less ergonomic with Tokio |
| rtcm-rs had redundant length fields | rtcm-rs 0.10+ removed redundant fields, standardized MSM naming | 0.10 release | Field names are now consistent across MSM types; old 0.9 code needs update |
| esp-storage standalone crate | esp-storage merged into esp-hal | May 2024 | Use esp-hal for flash storage in no_std ESP targets; esp-storage repo archived |
| EspNvs implements StorageBase/RawStorage | EspNvs has direct typed methods (get_u8, set_str, get_blob) | current esp-idf-svc 0.51+ | Firmware code confirms direct method use; RawStorage trait is an additional abstraction layer |
| figment without env prefix conventions | figment Env::prefixed("X_").split("__") | standard | Double-underscore nesting is the idiomatic convention for hierarchical env vars |

**Deprecated/outdated:**
- `rumqttc` sync `Connection::iter()` loop: still works but EventLoop::poll().await is idiomatic for Tokio
- `esp-wifi` crate: replaced by `esp-radio`; not relevant to Phase 23 but noted from audit
- `minimq` for std servers: minimq targets no_std; not a real alternative for the gnss-server binary

## Open Questions

1. **Galileo and BeiDou epoch time field names in rtcm-rs**
   - What we know: GPS uses `gps_epoch_time_ms`, GLONASS uses `glo_epoch_time_ms`
   - What's unclear: Galileo (`gal_epoch_time_ms`?) and BeiDou (`bds_epoch_time_ms`?) field names not verified from docs — 404 on individual struct pages
   - Recommendation: Verify by checking `docs.rs/rtcm-rs/latest/rtcm_rs/msg/struct.Msg1094T.html` at implementation time; names follow the pattern established by GPS/GLONASS

2. **sequential-storage sync vs async API**
   - What we know: MapStorage methods are `async fn` (documented signatures use `async`)
   - What's unclear: Is there a sync (non-async) path in sequential-storage? The `embedded-storage` sync `NorFlash` trait exists — does sequential-storage expose a blocking variant?
   - Recommendation: Check `sequential_storage::map` for a `fetch_item_blocking` or similar; if absent, use `embassy-futures::block_on` in the SeqNvsStore impl (safe in no_std, not in Tokio context)

3. **figment `${VAR}` substitution in TOML**
   - What we know: figment does NOT support this natively
   - What's unclear: CONTEXT.md preferred this syntax; the Env::prefixed approach is strictly better for secrets (secrets never in TOML)
   - Recommendation: Use `Env::prefixed("GNSS_").split("__")` and document the naming convention; drop `${VAR}` approach from implementation

4. **rumqttc benchmark scope**
   - What we know: rumqttc is the only production-grade std/Tokio MQTT client; minimq and rust-mqtt target no_std
   - What's unclear: CONTEXT.md requires benchmark; with only one viable option, the benchmark verifies connection, reconnect, and throughput characteristics rather than comparing alternatives
   - Recommendation: Benchmark rumqttc connection time, reconnect latency, and publish throughput at realistic RTCM rates (~4 messages/sec, ~500 bytes each); document results as the Phase 23 benchmark artifact

5. **GLONASS FCN source for carrier phase reconstruction**
   - What we know: FCN is absent from Msg1087Sig; carrier phase in MHz requires FCN
   - What's unclear: Is FCN available from any RTCM message (e.g., 1230 GLONASS Code-Phase Bias)?
   - Recommendation: For Phase 23, emit carrier phase as the raw `gnss_signal_fine_phaserange_ext_ms` value (milliseconds, not cycles) with a flag indicating FCN is unknown. Full cycle-domain conversion deferred; RINEX writer in Phase 24 can handle this.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test (`cargo test`) |
| Config file | none — `#[cfg(test)]` modules inline |
| Quick run command | `cargo test -p gnss-nvs` |
| Full suite command | `cargo test --workspace --exclude esp32-gnssmqtt-firmware` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SRVR-01 | MQTT subscription to correct topics | integration | Manual — requires running broker | N/A |
| SRVR-02 | Reconnect with exponential backoff | unit | `cargo test -p gnss-server -- mqtt::tests` | ❌ Wave 0 |
| RTCM-01 | GPS/GLONASS MSM4/MSM7 decode | unit | `cargo test -p gnss-server -- rtcm_decode::tests` | ❌ Wave 0 |
| RTCM-02 | Galileo/BeiDou MSM decode | unit | `cargo test -p gnss-server -- rtcm_decode::tests::gal_bds` | ❌ Wave 0 |
| RTCM-03 | Ephemeris 1019/1020/1044/1046 decode | unit | `cargo test -p gnss-server -- rtcm_decode::tests::eph` | ❌ Wave 0 |
| RTCM-04 | Epoch grouping flush-on-change | unit | `cargo test -p gnss-server -- epoch::tests` | ❌ Wave 0 |
| NOSTD-02 | NvsStore trait + ESP-IDF impl compiles | build | `cargo check -p gnss-nvs --features esp-idf` | ❌ Wave 0 |
| NOSTD-03 | sequential-storage impl compiles | build | `cargo check -p gnss-nvs --features sequential` | ❌ Wave 0 |

RTCM decode tests should use real binary RTCM3 frames captured from the UM980 output (available via `gnss.log` at workspace root).

### Sampling Rate
- **Per task commit:** `cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **Per wave merge:** `cargo clippy --workspace --exclude esp32-gnssmqtt-firmware -- -D warnings && cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `gnss-server/src/rtcm_decode.rs` — covers RTCM-01, RTCM-02, RTCM-03 with `#[cfg(test)]` module using real RTCM frames
- [ ] `gnss-server/src/epoch.rs` — covers RTCM-04 with unit tests for flush-on-change logic
- [ ] `gnss-server/src/mqtt.rs` — covers SRVR-02 reconnect state machine unit tests
- [ ] `crates/gnss-nvs/Cargo.toml` and `crates/gnss-nvs/src/` — covers NOSTD-02, NOSTD-03 (crate doesn't exist yet)
- [ ] Test fixtures: extract RTCM3 frames from `gnss.log` (workspace root) as `gnss-server/tests/fixtures/rtcm_sample.bin`

## Sources

### Primary (HIGH confidence)
- docs.rs/rtcm-rs/latest — Msg1074T, Msg1077T, Msg1084T, Msg1087T field names; Msm57Sat fields; Msg1077Sig fields; Msg1087Sig fields; next_msg_frame API
- docs.rs/rumqttc/latest — AsyncClient, EventLoop, MqttOptions API; Event enum; reconnect pattern
- docs.rs/sequential-storage/latest — MapStorage::fetch_item and store_item signatures; Key trait; PostcardValue trait; Value trait
- docs.rs/figment/latest — Toml + Env::prefixed pattern; variable substitution absence confirmed
- docs.rs/postcard — no_std serde compatibility; stable wire format; PostcardValue blanket impl
- Firmware source grep — EspNvs method names (get_blob, set_blob, get_str, set_str, get_u8, set_u8) confirmed by reading firmware/src/config_relay.rs, provisioning.rs, ntrip_client.rs

### Secondary (MEDIUM confidence)
- github.com/martinhakansson/rtcm-rs — MSM struct naming conventions; supported message range (all RTCM 3.4 messages); 0.10 breaking change (removed redundant length fields)
- github.com/tweedegolf/sequential-storage — Flash KV map description; PostcardValue purpose; embedded-storage dependency
- github.com/bytebeamio/rumqtt — rumqttc architecture; EventLoop ownership model; auto-reconnect behavior
- docs.rs/Msg1084T + Msg1087T — GLONASS epoch field name `glo_epoch_time_ms` confirmed; FCN absence from Msg1087Sig confirmed

### Tertiary (LOW confidence)
- Galileo (Msg1094T/Msg1097T) and BeiDou (Msg1124T/Msg1127T) epoch field names — assumed from GPS/GLONASS pattern; 404 on individual struct pages; verify at implementation time
- sequential-storage sync/blocking API variant — docs showed async signatures; sync path existence not confirmed

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries verified via docs.rs or crates.io
- Architecture: HIGH — patterns derived from verified APIs and existing firmware
- RTCM MSM field names: HIGH for GPS/GLONASS (verified from docs.rs); MEDIUM for Galileo/BeiDou (pattern inference)
- Epoch field names: HIGH for GPS/GLONASS; MEDIUM for Galileo/BeiDou
- figment `${VAR}` limitation: HIGH — explicitly confirmed absent from docs
- sequential-storage async-only: MEDIUM — async signatures observed; sync path not confirmed

**Research date:** 2026-03-12
**Valid until:** 2026-06-12 (stable libraries; rtcm-rs 0.x API may change before 1.0)
