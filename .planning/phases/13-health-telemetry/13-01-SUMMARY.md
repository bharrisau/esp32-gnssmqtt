---
phase: 13-health-telemetry
plan: "01"
subsystem: health-telemetry
tags: [metrics, mqtt, atomics, gnss, heartbeat]
dependency_graph:
  requires: []
  provides: [METR-01, METR-02]
  affects: [src/gnss.rs, src/mqtt.rs, src/config.example.rs]
tech_stack:
  added: []
  patterns: [cross-module atomic reads via crate::gnss::, manual JSON formatting (no serde)]
key_files:
  created: []
  modified:
    - src/gnss.rs
    - src/mqtt.rs
    - src/config.example.rs
decisions:
  - "config.rs not committed (gitignored) — HEARTBEAT_INTERVAL_SECS added to both config.rs (local) and config.example.rs (committed template)"
  - "Counters are cumulative since boot (no reset on read) — consistent with METR-02 requirement"
  - "JSON built with format!() macro (no serde) — consistent with ota.rs pattern, avoids ~50KB binary cost"
  - "Retained online publish happens once before the loop, not on every tick — correct LWT-clear semantics"
metrics:
  duration_secs: 158
  completed_date: "2026-03-07"
  tasks_completed: 2
  files_modified: 3
---

# Phase 13 Plan 01: Health Telemetry Implementation Summary

**One-liner:** Drop-counter atomics in gnss.rs + JSON health snapshot heartbeat to MQTT /heartbeat with retained online /status on reconnect.

## What Was Built

### Task 1: Drop-counter atomics in gnss.rs

Three changes to `src/gnss.rs`:

1. `static UART_TX_ERRORS` promoted to `pub static` — enables cross-module read from mqtt.rs
2. Added `pub static NMEA_DROPS: AtomicU32` — counts NMEA sentences dropped at `TrySendError::Full` in the NmeaLine arm; `NMEA_DROPS.fetch_add(1, Ordering::Relaxed)` inserted before existing warn log
3. Added `pub static RTCM_DROPS: AtomicU32` — counts RTCM frames dropped at `TrySendError::Full` in the RtcmBody arm; `RTCM_DROPS.fetch_add(1, Ordering::Relaxed)` inserted before existing warn log, alongside the critical `free_pool_tx_clone.try_send(returned_buf)` pool-return call

No new imports needed — `AtomicU32` and `Ordering` were already imported.

### Task 2: Config constant + extended heartbeat_loop

**src/config.example.rs** (and gitignored src/config.rs):
- Added `pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;` with doc comment

**src/mqtt.rs** — `heartbeat_loop` fully rewritten:
- Pre-loop: publishes retained `b"online"` to `gnss/{device_id}/status` at `QoS::AtLeastOnce` with `retain=true` — clears LWT "offline" retained message on reconnect
- Loop body: reads `NMEA_DROPS`, `RTCM_DROPS`, `UART_TX_ERRORS` via `crate::gnss::` full path; reads `uptime_s` via `esp_timer_get_time() / 1_000_000`; reads `heap_free` via `esp_get_free_heap_size()`; builds JSON with `format!()` macro; publishes to `/heartbeat` with `retain=false`
- Sleep uses `crate::config::HEARTBEAT_INTERVAL_SECS` — no hardcoded literal

## Verification Results

```
cargo build --release | grep -E "^error|^warning\[" | head -20
(no output — clean build)
```

All grep verifications passed:
- `pub static UART_TX_ERRORS`, `pub static NMEA_DROPS`, `pub static RTCM_DROPS` present in gnss.rs
- `NMEA_DROPS.fetch_add` at line 219, `RTCM_DROPS.fetch_add` at line 305
- `free_pool_tx_clone.try_send` still present at lines 310, 317, 327
- `HEARTBEAT_INTERVAL_SECS` in config.example.rs at line 64
- Retained `b"online"` publish to status_topic at line 219 (before loop)
- Non-retained JSON publish at line 248 (inside loop)
- No `from_secs(30)` hardcoded literal in heartbeat_loop

## Decisions Made

1. **config.rs not committed** — file is gitignored (project convention). `HEARTBEAT_INTERVAL_SECS` was added to both the local `config.rs` (for this build) and the committed `config.example.rs` template.

2. **Cumulative counters (no reset)** — consistent with METR-02 requirement. Counters accumulate since boot; reads via `load(Ordering::Relaxed)` only.

3. **Manual JSON (no serde)** — consistent with the ota.rs pattern already established in the codebase. Avoids ~50KB binary size increase.

4. **Retained online before loop, not inside loop** — semantically correct: one retained "online" is published at thread start to overwrite the LWT "offline". Publishing it on every tick is unnecessary (retained messages persist at broker). The plan explicitly requires "once per thread lifetime".

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check: PASSED

- src/gnss.rs: FOUND
- src/mqtt.rs: FOUND
- src/config.example.rs: FOUND
- 13-01-SUMMARY.md: FOUND
- commit 3d26005 (Task 1): FOUND
- commit e6ee247 (Task 2): FOUND
