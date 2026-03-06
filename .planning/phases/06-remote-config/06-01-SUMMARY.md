---
phase: 06-remote-config
plan: 01
subsystem: config-relay
tags: [mqtt, gnss, config, uart, rust, embedded]
dependency_graph:
  requires: [05-nmea-relay]
  provides: [config_relay.rs, extended-pump-signature]
  affects: [src/main.rs (Plan 02 caller fix)]
tech_stack:
  added: []
  patterns: [mpsc-channel-routing, djb2-hash-dedup, no-std-json-parsing]
key_files:
  created:
    - src/config_relay.rs
  modified:
    - src/mqtt.rs
key_decisions:
  - djb2 hash chosen for payload deduplication — non-cryptographic, adequate for retained MQTT messages
  - 100ms default per-command delay (overridable via delay_ms JSON field)
  - gnss_cmd_tx.send() failure triggers log + abandon (no panic, no retry)
  - Empty payload guard skips retained-message-cleared events
  - No-serde JSON parsing for fixed schema — UM980 commands have no special characters
  - EventPayload::Received arm placed before catch-all m@_ in pump
metrics:
  duration: "~2 min"
  completed: "2026-03-07"
  tasks: 2
  files: 2
---

# Phase 06 Plan 01: Config Relay Implementation Summary

Config relay thread and MQTT pump extension providing MQTT-to-UART command path with djb2 hash deduplication, no-serde JSON parsing, and 100ms per-command delay.

## What Was Built

### Task 1: src/config_relay.rs (new file)

Four functions implementing the MQTT-to-UART control path:

- **`pub fn spawn_config_relay`** — entry point, spawns 8192-byte stack thread consuming `config_rx: Receiver<Vec<u8>>`
- **`fn djb2_hash`** — computes DJB2 hash for payload deduplication; unchanged payloads (same hash as previous) are skipped
- **`fn apply_config`** — decodes UTF-8, dispatches to JSON or plain-text path, sends each command to `gnss_cmd_tx` with per-command sleep
- **`fn parse_config_json`** — no-serde parser for `{"delay_ms": N, "commands": ["CMD1", ...]}` schema

Key correctness properties:
- Empty payload guard prevents processing retained-message-cleared events
- Hash initialized to 0; first non-empty payload always passes through
- `gnss_cmd_tx.send()` failure logs error and returns immediately — no panic, remaining commands abandoned
- Commands sent WITHOUT `\r\n` — gnss.rs TX thread appends terminator

### Task 2: src/mqtt.rs (modified)

`pump_mqtt_events` signature extended with `config_tx: Sender<Vec<u8>>` after `subscribe_tx`. New `EventPayload::Received` arm added before the catch-all:

```rust
EventPayload::Received { data, .. } => {
    match config_tx.send(data.to_vec()) {
        Ok(_) => {}
        Err(e) => log::warn!("Config relay channel closed: {:?}", e),
    }
}
```

Critical invariant preserved: `config_tx.send()` is an mpsc channel send, NOT a client method call — no deadlock risk.

## Build Status

- `src/config_relay.rs` — compiles cleanly (module not yet declared in main.rs)
- `src/mqtt.rs` — compiles cleanly
- `src/main.rs` — one expected compile error: `pump_mqtt_events` called with 3 args, now requires 4. Will be fixed in Plan 02 Task 1.

## Deviations from Plan

None — plan executed exactly as written.

The plan noted the build target in the verification command (`riscv32imc-esp-espidf`) incorrectly; actual target is `riscv32imac-esp-espidf` per `.cargo/config.toml`. This was a documentation discrepancy in the PLAN.md, not a deviation — the correct target was used.

## Self-Check: PASSED

- src/config_relay.rs: FOUND
- src/mqtt.rs: FOUND
- Commit 938ee8e (Task 1 — config_relay.rs): FOUND
- Commit c2fc72f (Task 2 — mqtt.rs extension): FOUND
