---
phase: 23-mqtt-rtcm3-gnss-nvs-crate
plan: "02"
subsystem: server
tags: [rust, mqtt, tokio, rumqttc, figment, clap, async, exponential-backoff]

# Dependency graph
requires:
  - phase: 22-workspace-nostd-audit
    provides: workspace Cargo.toml with resolver=2, workspace dependencies (log, bytes)
provides:
  - gnss-server binary that compiles and accepts --config CLI flag
  - ServerConfig/MqttConfig structs loaded from TOML via figment with GNSS_ env var overrides
  - mqtt_supervisor async task with EventLoop ownership, watch channel, exponential backoff reconnect
  - MqttMessage enum (Rtcm/Nmea/Heartbeat) wrapping bytes::Bytes for internal channel passing
affects:
  - 23-03 (RTCM3 decode pipeline will consume MqttMessage via mpsc channel from supervisor)
  - 23-04 (gnss-nvs crate — may need config references)

# Tech tracking
tech-stack:
  added:
    - rumqttc 0.24 (async MQTT client with EventLoop poll model)
    - tokio 1 with full features (async runtime, mpsc, watch, select!, signal)
    - figment 0.10 with toml + env providers (layered config loading)
    - clap 4 with derive feature (CLI argument parsing)
    - tokio-retry 0.3 (available, not yet used — backoff is manual array)
    - serde 1 with derive (struct deserialization)
    - anyhow 1 (error propagation)
    - env_logger 0.11 (structured logging init)
  patterns:
    - figment layered config: Toml::file() base then Env::prefixed("GNSS_").split("__") override
    - rumqttc AsyncClient::new(opts, 64) — fresh client+eventloop per reconnect cycle
    - subscribe before poll loop (enqueued in channel, sent on first poll())
    - backoff array [1,2,5,10,30]s with fail_count.min(len-1) cap
    - watch::channel(bool) for connection state broadcast
    - mpsc::Sender with try_send (non-blocking, drop on full) for message forwarding
    - tokio::select! in main for ctrl_c / msg_rx / state_rx convergence

key-files:
  created:
    - gnss-server/src/config.rs
    - gnss-server/src/mqtt.rs
  modified:
    - gnss-server/Cargo.toml
    - gnss-server/src/main.rs

key-decisions:
  - "Fresh AsyncClient+EventLoop created each reconnect cycle — avoids rumqttc connection state pollution across reconnects"
  - "Backoff implemented as manual [1,2,5,10,30] array with .min(len-1) cap — tokio-retry added to Cargo.toml for future use but not yet needed for simple fixed steps"
  - "subscribe() called before poll loop starts — rumqttc enqueues subscriptions in internal channel; first poll() sends them; no race condition (RESEARCH.md Pitfall 5)"
  - "try_send() for msg_tx — non-blocking to avoid blocking EventLoop poll; drops message if consumer is slow"
  - "#[allow(dead_code)] on MqttMessage enum and MqttConfig struct — payload bytes and credential fields consumed in Phase 23-03"

patterns-established:
  - "Pattern 1: ServerConfig owns device_id at top level and mqtt as sub-struct — matches TOML nesting and GNSS_MQTT__ env prefix"
  - "Pattern 2: mqtt_supervisor is a standalone async fn taking Arc<ServerConfig> + channels — enables tokio::spawn without lifetime issues"
  - "Pattern 3: Inner/outer loop reconnect: outer creates client, inner polls, error breaks inner, backoff then outer repeats"

requirements-completed: [SRVR-01, SRVR-02]

# Metrics
duration: 5min
completed: 2026-03-12
---

# Phase 23 Plan 02: MQTT + RTCM3 + gnss-nvs — Server Foundation Summary

**figment-based TOML+env config loading and rumqttc MQTT supervisor with [1,2,5,10,30]s exponential backoff reconnect and watch channel state broadcast**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-03-12T05:52:34Z
- **Completed:** 2026-03-12T05:57:00Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- `gnss-server` binary compiles clean (zero clippy warnings) with full async tokio runtime
- ServerConfig loads from TOML via `--config` flag; GNSS_ prefixed env vars override any value with __ nesting
- mqtt_supervisor owns the EventLoop, reconnects with [1,2,5,10,30]s backoff, resets on ConnAck, broadcasts state via watch channel
- 10 unit tests pass across config::tests (3) and mqtt::tests (7) including backoff, topic routing, and variant construction

## Task Commits

Each task was committed atomically:

1. **Task 1: Server config loading and CLI entry point** - `1bb06c9` (feat)
2. **Task 2: MQTT supervisor task with exponential backoff and watch channel** - `11e6338` (feat)

## Files Created/Modified
- `gnss-server/Cargo.toml` — added all server dependencies (rumqttc, tokio, figment, clap, tokio-retry, serde, anyhow, log, env_logger, bytes)
- `gnss-server/src/config.rs` — ServerConfig/MqttConfig structs with figment load_config() and 3 unit tests
- `gnss-server/src/mqtt.rs` — MqttMessage enum, mqtt_supervisor async fn, topic_to_message(), BACKOFF_STEPS const, 7 unit tests
- `gnss-server/src/main.rs` — tokio runtime, clap CLI, env_logger, mpsc/watch channel creation, supervisor spawn, select! loop

## Decisions Made
- Fresh AsyncClient+EventLoop per reconnect cycle — avoids rumqttc connection state pollution
- subscribe() before poll loop is correct per rumqttc design (enqueued, not blocking)
- try_send() for message forwarding — non-blocking; slow consumers drop messages rather than stalling EventLoop
- `#[allow(dead_code)]` on MqttMessage and MqttConfig for forward-compatibility (Phase 23-03 will consume these)

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered
- Initial clippy pass found dead_code on MqttConfig credential fields and MqttMessage Bytes tuple fields — resolved with targeted `#[allow(dead_code)]` on the structs/enum rather than field suppressions, preserving public API for Phase 23-03

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- Plan 23-03 (RTCM3 decode pipeline) can consume MqttMessage via the mpsc::Receiver<MqttMessage> that main.rs holds
- ServerConfig fields (device_id, mqtt.*) are all available via Arc<ServerConfig> passed to supervisor
- Watch channel provides connection state to any future consumer (RINEX writer, web UI status endpoint)

---
*Phase: 23-mqtt-rtcm3-gnss-nvs-crate*
*Completed: 2026-03-12*
