# Phase 13: Health Telemetry - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Publish a periodic MQTT health message containing uptime, free heap, and message drop counters so operators can observe device health remotely. Also clears the LWT "offline" retained message on reconnect by publishing retained "online" to the status topic. The heartbeat payload changes from a static "online" string to a structured JSON health snapshot.

</domain>

<decisions>
## Implementation Decisions

### Status topic / LWT interaction
- On every reconnect (at heartbeat thread start), publish retained `"online"` to `gnss/{device_id}/status` — this overwrites the LWT "offline" retained message
- This retained "online" publish happens once per reconnect, NOT repeated on every heartbeat tick
- Health JSON goes to `gnss/{device_id}/heartbeat` (replaces the existing `b"online"` payload in `heartbeat_loop`)

### Heartbeat cadence
- Make interval configurable in `src/config.rs` as a named constant (e.g., `HEARTBEAT_INTERVAL_SECS`)
- Default value: 30 seconds (the existing cadence — 60s was a spec placeholder)

### Payload fields
- Include ALL available metrics — the METR-01 4-field spec is an example, not a rigid contract
- Fields: `uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`
- Counters are cumulative since last boot (no reset on publish) — METR-02 spec
- `GNSS_RX_HEARTBEAT` excluded — watchdog mechanism, not a meaningful health metric for operators

### retain flag for health JSON
- Health JSON published to `/heartbeat` with `retain=false` (ephemeral)
- LWT on `/status` handles offline indication; no stale health data needed

### Claude's Discretion
- Exact JSON serialization approach (manual format! string consistent with existing codebase pattern — no serde dependency)
- How uptime is measured (esp_timer_get_time() / 1_000_000 or similar ESP-IDF call)
- How heap_free is obtained (esp_get_free_heap_size() via esp-idf-svc::sys)
- Whether to add new atomics in gnss.rs or in a new telemetry.rs module

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `UART_TX_ERRORS: AtomicU32` in `src/gnss.rs:60` — already exists, purpose-built for Phase 13 (comment says "Will be read by the health telemetry subsystem (Phase 13)")
- `heartbeat_loop()` in `src/mqtt.rs:191` — existing thread to extend; currently publishes `b"online"` to `/heartbeat` every 30s with retain=true
- `Arc<Mutex<EspMqttClient>>` pattern in `src/ota.rs` — established pattern for publishing from non-MQTT threads

### Established Patterns
- `std::thread::Builder::new().stack_size(N).spawn(...)` — thread spawn pattern in `src/main.rs`
- `format!()` for manual JSON construction — used in ota.rs (`{"state":"...","reason":"..."}`) — no serde dependency
- `AtomicU32::new(0)` statics — established in gnss.rs and watchdog.rs
- Config constants in `src/config.example.rs` with `config.rs` gitignored

### Integration Points
- `src/gnss.rs` — two new `AtomicU32` statics needed: `NMEA_DROPS` and `RTCM_DROPS`; incremented at `TrySendError::Full` sites (lines ~209 and ~294)
- `src/mqtt.rs:heartbeat_loop()` — payload changes from `b"online"` to health JSON; add one-time retained "online" publish to `/status` before the loop
- `src/config.example.rs` — add `HEARTBEAT_INTERVAL_SECS` constant

</code_context>

<specifics>
## Specific Ideas

- "Include any/all metrics we have — the spec is just an example, not to be rigidly followed"
- "60s spec was a placeholder" — use 30s default, make it configurable
- Retained "online" to /status clears LWT; this is a missing feature the device should already have

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 13-health-telemetry*
*Context gathered: 2026-03-07*
