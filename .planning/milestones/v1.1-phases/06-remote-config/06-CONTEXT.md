# Phase 6: Remote Config - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Subscribe to `gnss/{device_id}/config` (QoS 1), parse the payload, and forward each command line-by-line to the UM980 over UART TX via the existing `gnss_cmd_tx: Sender<String>` channel. A per-command delay is applied between writes. Config is only applied when it changes (hash-based deduplication). Error handling is log-only.

</domain>

<decisions>
## Implementation Decisions

### Payload format
- JSON object with a `commands` array of strings, e.g. `{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}`.
- If `serde_json` proves too heavy for the ESP32-C6 (binary size / heap), the planner should evaluate a lightweight alternative: `miniserde`, or fall back to newline-delimited plain text (`"MODE ROVER\nCONFIGSAVE\n"`). Researcher should check binary size impact.
- The `delay_ms` field in the JSON payload overrides the default per-command delay (see CONF-03 decision).

### Retained message replay (CONF-02)
- Store a hash (e.g. CRC32 or djb2 — whichever is cheapest on ESP32-C6) of the last-applied config payload in a static variable.
- On every `Received` event for the config topic, compare hash to stored value. Only forward commands if the hash differs.
- This prevents the retained broker message from re-configuring the UM980 on every MQTT reconnect.
- Hash is in-memory only (not NVS persisted) — power cycle reapplies config once, which is acceptable.

### Per-command delay (CONF-03)
- Default delay: 100ms between each UART TX write.
- Override: `delay_ms` field in the JSON payload. If field absent or JSON fallback (plain text), use 100ms default.
- Delay is applied via `std::thread::sleep(Duration::from_millis(delay_ms))` in the relay path.

### Error handling
- Log errors with `log::warn!` or `log::error!` — no retry, no halt.
- If `gnss_cmd_tx.send()` returns Err (TX thread dead), log error and abandon remaining commands in the batch.
- If JSON parse fails, log error and discard the entire payload.
- Eventually logs will be forwarded to MQTT (future phase); no special handling needed now.

### Claude's Discretion
- Where config handling lives: new `src/config_relay.rs` module, or extend `subscriber_loop` in mqtt.rs — planner decides based on clean separation of concerns.
- Hash algorithm choice (CRC32, djb2, simple sum) — cheapest option for ESP32-C6.
- Whether to use `serde_json` or a lighter alternative — researcher assesses binary size impact.

</decisions>

<specifics>
## Specific Ideas

- "If JSON parsing too heavy for device suggest alternative" — researcher should benchmark or estimate serde_json binary size delta on this target (currently ~700KB firmware).
- Config payload example: `{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}`
- Plain text fallback format if JSON rejected: newline-delimited commands, 100ms fixed delay.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `gnss_cmd_tx: Sender<String>` — already wired in main.rs Step 14 as `_gnss_cmd_tx` placeholder. Phase 6 clones it for the config relay.
- `mqtt::subscriber_loop` — currently subscribes to one topic on Connected signal. Config topic (`gnss/{device_id}/config`) must be added to this subscription set.
- `mqtt::pump_mqtt_events` — currently routes `EventPayload::Received` to subscriber via mpsc. Config payloads need to reach the config relay somehow — either via a second mpsc channel or by extending the existing routing.

### Established Patterns
- Thread-per-concern: heartbeat, subscriber, pump each run in their own thread. Config relay will follow the same pattern.
- `Arc<Mutex<EspMqttClient>>` — client shared across threads via clone. Subscriber has `sub_client` clone already.
- `log::warn!` / `log::error!` for non-fatal errors — consistent with gnss.rs `try_send` drop handling.
- `std::thread::sleep` for delays — used in heartbeat; same approach for per-command delay.

### Integration Points
- `main.rs` Step 14: `_gnss_cmd_tx` placeholder is the handoff point — clone it and pass to config relay thread.
- `mqtt::subscriber_loop`: needs a second topic added, and a way to pass received payloads to config relay (mpsc channel).
- `mqtt.rs` pump: `EventPayload::Received` currently only logged — needs routing to config relay channel.

</code_context>

<deferred>
## Deferred Ideas

- NVS persistence of last-applied config hash — power cycle currently reapplies config once; persistent hash would avoid this. Future phase.
- MQTT log forwarding — "we eventually send logs to MQTT" mentioned; this is a separate phase.

</deferred>

---

*Phase: 06-remote-config*
*Context gathered: 2026-03-07*
