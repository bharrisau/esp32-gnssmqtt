---
phase: 17-ntrip-client
plan: "03"
subsystem: log-quality-resilience
tags: [log-relay, mqtt, gnss, um980, ansi, reboot-detection]
dependency_graph:
  requires: [17-02]
  provides: [log-quality-fixes, um980-reboot-detection]
  affects: [src/log_relay.rs, src/mqtt.rs, src/gnss.rs, src/main.rs]
tech_stack:
  added: []
  patterns: [sync-channel-capacity, ansi-strip-no-regex, reboot-signal-channel]
key_files:
  created: []
  modified:
    - src/log_relay.rs
    - src/mqtt.rs
    - src/gnss.rs
    - src/main.rs
decisions:
  - "strip_ansi uses byte scan (no regex crate) matching ESC [ digits/semicolons m pattern"
  - "UM980 reboot monitor uses warning fallback — NVS-backed gnss config re-apply deferred (config_relay reads from MQTT channel, not NVS)"
  - "reboot_tx channel bounded to 1 to coalesce rapid consecutive reboot signals"
  - "sentence_type cloned before nmea_tx.try_send so reboot check can borrow it after send"
metrics:
  duration_minutes: 3
  completed_date: "2026-03-08"
  tasks_completed: 2
  files_modified: 4
---

# Phase 17 Plan 03: Log Quality and UM980 Resilience Summary

**One-liner:** ANSI strip on C-path vprintf log output, sync_channel capacity 32→128 for boot burst, explicit MQTT ACK event handling, and UM980 reboot banner detection with signal channel.

## What Was Built

Four targeted fixes to log quality and UM980 resilience issues deferred from phase 16 testing.

### Task 1: Log channel capacity, ANSI strip, MQTT ACK events (src/log_relay.rs, src/mqtt.rs)

**Change A — Channel capacity 32→128 (log_relay.rs):**
`sync_channel::<String>(128)` replaces capacity 32. Boot produces 30+ log messages in a 30ms burst before the relay thread drains; capacity 32 caused silent drops of legitimate boot diagnostics.

**Change B — ANSI strip on C vprintf path (log_relay.rs):**
`strip_ansi(s: String) -> String` added as a private function. Uses a byte-scan approach (no regex crate) to remove `ESC [ digits/semicolons m` sequences (e.g. `\x1b[0;32m`, `\x1b[1;33m`). Applied to every string received via `rust_log_try_send` before `LOG_TX.try_send`. The Rust `MqttLogger` path does not need stripping — EspLogger formats Rust log records without ANSI codes.

**Change C — MQTT ACK events (mqtt.rs):**
`EventPayload::Subscribed(_) | EventPayload::Published(_)` handled explicitly before the `m @ _` catch-all `warn!`. These are normal lifecycle ACKs from subscribe operations and QoS≥1 publishes; they no longer generate warning noise on the MQTT log topic.

### Task 2: UM980 reboot detection and config re-apply signal (src/gnss.rs, src/main.rs)

**spawn_gnss signature extended (gnss.rs):**
New final parameter `reboot_tx: std::sync::mpsc::SyncSender<()>`. The RX thread closure captures `reboot_tx` and fires it on detection.

**NmeaLine completion arm detection (gnss.rs):**
After `nmea_tx.try_send(...)`, the `sentence_type` string is checked: `if sentence_type == "devicename"` triggers `reboot_tx.try_send(())` with full error logging for Full and Disconnected cases.

**main.rs channel + monitor thread:**
`um980_reboot_rx/tx` created with capacity 1 (coalesces rapid signals). `spawn_gnss` receives `um980_reboot_tx`. A dedicated monitor thread (`stack_size(4096)`) loops on `recv_timeout`, waits 500ms on signal, then logs a prominent warning directing the operator to re-send UM980 config via MQTT `/config` topic. Full automatic re-apply (NVS-backed config reload) is deferred because `config_relay` reads config from the MQTT channel at runtime, not from persistent NVS storage.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] sentence_type ownership for reboot check**
- **Found during:** Task 2 — implementing reboot detection after nmea_tx.try_send
- **Issue:** `nmea_tx.try_send((sentence_type, s.to_string()))` moves `sentence_type` into the tuple; subsequent `if sentence_type == "devicename"` check requires the value still be accessible
- **Fix:** Changed to `nmea_tx.try_send((sentence_type.clone(), s.to_string()))` so the original `sentence_type` binding remains for the reboot check
- **Files modified:** src/gnss.rs
- **Commit:** 6f22fd2

**2. [Rule 3 - Deviation] UM980 reboot monitor uses warning fallback instead of NVS re-apply**
- **Found during:** Task 2 — checking config_relay.rs for re-callable function
- **Issue:** Plan assumed NVS-backed `apply_stored_config` existed; actual config_relay receives config from MQTT channel at runtime with no NVS persistence of gnss commands
- **Fix:** Used plan's documented fallback: log prominent warning directing operator to re-send config via MQTT. Detection signal path (the primary value) is fully implemented
- **Files modified:** src/main.rs
- **Commit:** 6f22fd2

## Decisions Made

1. `strip_ansi` uses byte scan (no regex crate) matching `ESC [ digits/semicolons m` — as specified in plan
2. UM980 reboot monitor uses warning fallback — NVS-backed gnss config re-apply deferred (config_relay reads from MQTT channel, not NVS)
3. `reboot_tx` channel bounded to 1 to coalesce rapid consecutive reboot signals
4. `sentence_type` cloned before `nmea_tx.try_send` so reboot check can use the value after move

## Verification

- `cargo build --release` exits 0 with no new errors
- `grep "sync_channel::<String>(128)" src/log_relay.rs` matches line 179
- `grep "strip_ansi" src/log_relay.rs` matches function definition (line 139) and call site (line 128)
- `grep "EventPayload::Subscribed" src/mqtt.rs` matches explicit handler before catch-all (line 148)
- `grep "devicename" src/gnss.rs` matches reboot detection block (line 255)
- `grep "um980_reboot" src/main.rs` matches channel creation (line 159) and monitor thread (line 290+)
- `cargo clippy -- -D warnings 2>&1 | grep "error\["` returns empty

## Self-Check: PASSED
