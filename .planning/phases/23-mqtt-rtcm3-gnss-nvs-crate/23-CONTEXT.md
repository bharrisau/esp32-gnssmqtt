# Phase 23: MQTT + RTCM3 + gnss-nvs crate - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Server binary (gnss-server) subscribes to MQTT for RTCM3/NMEA/heartbeat data and decodes all MSM and ephemeris message types into verified observation structs with epoch grouping. Plus the `gnss-nvs` trait crate with ESP-IDF and sequential-storage backing implementations. RINEX file writing is Phase 24; Web UI is Phase 25.

</domain>

<decisions>
## Implementation Decisions

### MQTT client library
- Evaluate all pure-Rust async MQTT client options this phase (benchmark phase). If only one exists, it wins by default; if multiple pure-Rust options exist, benchmark them and select the winner.
- Implementation goes in Phase 24 (this phase is research + benchmarks).
- Async runtime: Tokio — best ecosystem fit for MQTT + HTTP + WebSocket (Phase 25).

### MQTT server architecture
- Dedicated Tokio supervisor task owns the MQTT EventLoop, broadcasts connection state via a `watch` channel.
- Reconnect with exponential backoff from the supervisor task.
- Other tasks receive decoded data via Tokio channels.

### Server configuration
- TOML config file (path via `--config` CLI flag) as the base.
- Environment variable override for any value in the TOML — useful for secrets (broker password, credentials) without putting them in the config file.
- Variable substitution syntax in TOML (e.g. `password = "${MQTT_PASSWORD}"`) preferred over a separate env-only layer.

### Epoch grouping strategy
- MSM messages carry a `gnssEpochTime` field — parse it and use it as the epoch key.
- Epoch boundary detection: when a new `gnssEpochTime` arrives that differs from the currently buffered epoch time, flush the buffered epoch and start a new one.
- No timeout — flush on epoch-change only. Late constellations over MQTT simply join the next epoch.
- Buffer keyed by `epoch_time` only (not per-constellation). All MSMs with the same epoch time accumulate into one output epoch regardless of constellation.
- Log at each epoch boundary: epoch timestamp + constellation + SV count (e.g. `Epoch 2026-03-12T04:23:11.200Z GPS:8 GLO:4 GAL:3 BDS:0`).

### gnss-nvs trait API
- Trait: `NvsStore` with associated `Error` type — each impl defines its own error; app code wraps with `anyhow`.
- Key type: `namespace: &str` + `key: &str` — mirrors ESP-IDF NVS API, maps cleanly to sequential-storage.
- Sync trait (not async) — flash NVS is fast and blocking; both impls are sync.
- Typed getters/setters via `get<T: DeserializeOwned>` / `set<T: Serialize>` (postcard for serialization).
- Blob support via separate `get_blob(&mut [u8])` / `set_blob(&[u8])` methods — not unified with typed API.

### gnss-nvs crate design
- Clean-room trait design: the `gnss-nvs` crate has no `esp-idf-*` dependency.
- ESP-IDF impl is feature-gated (or a separate sub-crate) and wraps `NvmStorage` from `esp-idf-svc`.
- sequential-storage impl is the no_std flash-backed implementation (compiles in Phase 23; hardware validation deferred).
- Both impls live in `crates/gnss-nvs/` in the workspace.

### Crate layout
- All gap crates live under `crates/` in the workspace root: `crates/gnss-nvs/`, `crates/gnss-ota/` (Phase 24), etc.
- Names: keep `gnss-nvs`, `gnss-ota` etc. as-is. Will rename to something generic (e.g. `embedded-nvs`) when/if publishing to crates.io once the trait API stabilises post-Phase 25 hardware validation.
- Publish intent: yes, publish when stable and broadly useful; rename at publish time.

### Claude's Discretion
- Exact postcard vs serde-json choice for typed NvsStore serialization (postcard likely — no_std compatible)
- MqttMessage struct/enum design for internal channel passing in the server
- Specific error variants for the sequential-storage NvsStore impl
- Whether to use `figment` or a simpler custom approach for TOML + env var config loading

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `firmware/src/mqtt.rs` — MQTT event loop and reconnect pattern; server follows similar supervisor task design but with Tokio instead of FreeRTOS tasks
- `firmware/src/rtcm_relay.rs` — RTCM frame routing; server receives these same frames from MQTT topics
- `firmware/src/config_relay.rs` — NVS read/write patterns that gnss-nvs ESP-IDF impl must cover (namespace "gnss", keys like "gnss_config" blob)
- `gnss-server/src/main.rs` — currently a stub; becomes the server binary entry point
- `docs/nostd-audit.md` — complete ESP-IDF usage inventory; gnss-nvs must cover all NVS usage categories listed there

### Established Patterns
- MQTT topic format: `gnss/{device_id}/rtcm`, `gnss/{device_id}/nmea`, `gnss/{device_id}/heartbeat`
- Message-passing via bounded channels (firmware pattern) — server uses Tokio channels with same discipline
- `rtcm-rs 0.11` selected for RTCM3 decode (STATE.md decision)
- GLONASS carrier phase without FCN → `Option::None`, never `0.0` (STATE.md decision)
- `bytes` crate already in firmware deps — server may receive `Bytes` payloads from MQTT

### Integration Points
- `crates/gnss-nvs/` connects to `firmware/` via feature-gated ESP-IDF impl and to future embassy firmware via sequential-storage impl
- `gnss-server/` connects to `crates/gnss-nvs/` only indirectly (server doesn't use NVS; NVS crate is firmware-facing)
- Server output (decoded observations) feeds Phase 24 RINEX writer via internal channel

</code_context>

<specifics>
## Specific Ideas

- Config TOML should support `password = "${MQTT_PASSWORD}"` style env var substitution so secrets stay out of the config file
- Epoch boundary log line format: `Epoch {ISO8601} GPS:{n} GLO:{n} GAL:{n} BDS:{n}` — visible confirmation of epoch grouping working
- gnss-nvs publish plan: keep name for now, rename to generic name at crates.io publish time (post Phase 25)
- The existing `config_relay.rs` NVS usage (namespace "gnss", blob key "gnss_config") is the reference use case for the ESP-IDF impl

</specifics>

<deferred>
## Deferred Ideas

- RINEX file writing — Phase 24
- Web UI / WebSocket push — Phase 25
- gnss-ota crate — Phase 24
- gnss-softap / gnss-dns / gnss-log gap skeletons — Phase 25
- Hardware validation of sequential-storage NvsStore on device FFFEB5 — future milestone (NOSTD-F02)
- Multi-device MQTT subscription — future (SRVR-F01)
- Async NvsStore trait — future if flash drivers go async

</deferred>

---

*Phase: 23-mqtt-rtcm3-gnss-nvs-crate*
*Context gathered: 2026-03-12*
