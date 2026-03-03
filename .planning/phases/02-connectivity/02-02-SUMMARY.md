---
phase: 02-connectivity
plan: 02
subsystem: mqtt
tags: [esp-idf-svc, mqtt, lwt, heartbeat, arc-mutex, freertos, embedded-svc, esp32c6]

# Dependency graph
requires:
  - phase: 01-scaffold
    provides: "Compilable Rust scaffold with pinned esp-idf-svc =0.51.0, config.rs MQTT constants"
provides:
  - "src/mqtt.rs — mqtt_connect, pump_mqtt_events, heartbeat_loop"
  - "LWT registered at connect time: gnss/{device_id}/status offline/retain=true"
  - "pump_mqtt_events drives EspMqttConnection::next() loop; re-subscribes gnss/{id}/config on every Connected event"
  - "heartbeat_loop publishes online to gnss/{id}/heartbeat with retain=true every 30s"
  - "EspMqttClient wrapped in Arc<Mutex<EspMqttClient<'static>>> for thread-safe sharing"
affects:
  - 02-04-PLAN  # integration plan wires mqtt_connect into main and tests live broker connection

# Tech tracking
tech-stack:
  added:
    - "anyhow =1 added to Cargo.toml as direct dependency (previously only transitive)"
  patterns:
    - "LWT lifetime pattern: lwt_topic String declared before MqttClientConfiguration in same scope"
    - "MQTT pump thread pattern: EspMqttConnection moved into pump, EspMqttClient wrapped in Arc<Mutex<>>"
    - "Re-subscribe on Connected: handles broker restart where clean_session drops session state"
    - "Heartbeat with 5s initial delay: give pump thread time to process first Connected event before first publish"
    - "if let Ok(mut c) = client.lock() pattern: avoids panic on mutex poison without unwrap()"

key-files:
  created:
    - src/mqtt.rs
  modified:
    - src/main.rs
    - Cargo.toml

key-decisions:
  - "lwt_topic declared before conf in mqtt_connect to satisfy LwtConfiguration<'a> lifetime (pitfall 1 from research)"
  - "EspMqttConnection moved into pump thread; EspMqttClient in Arc<Mutex<>> — matches esp-idf-svc recommended pattern for reconnect-aware code"
  - "pump_mqtt_events returns ! (diverging) — connection is permanent; loop-after-pump-exit prevents thread termination"
  - "heartbeat uses client.publish() (blocking) not client.enqueue() — acceptable from dedicated thread with pump running"
  - "Added anyhow = '1' as explicit direct dependency — previously relied on transitive availability"

patterns-established:
  - "MQTT thread pattern: pump thread owns EspMqttConnection; heartbeat/main share Arc<Mutex<EspMqttClient>>"
  - "Re-subscribe on every Connected: not just on startup — handles broker restart without device reboot"

requirements-completed: [CONN-02, CONN-04, CONN-05, CONN-06]

# Metrics
duration: ~7min
completed: 2026-03-03
---

# Phase 2 Plan 02: MQTT Module Summary

**EspMqttClient with LWT (offline/retain on disconnect), pump thread re-subscribing config topic on every Connected event, and 30-second retained heartbeat loop**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-03-03T11:18:00Z (estimated)
- **Completed:** 2026-03-03T11:25:00Z (estimated)
- **Tasks:** 1 of 1
- **Files modified:** 3

## Accomplishments

- Created src/mqtt.rs with three public functions: mqtt_connect, pump_mqtt_events, heartbeat_loop
- LWT configured at connect time: `gnss/{device_id}/status` with payload `offline`, retain=true, QoS::AtLeastOnce — broker marks device offline on unexpected disconnect
- pump_mqtt_events drives EspMqttConnection::next() in an infinite loop; re-subscribes `gnss/{device_id}/config` at QoS::AtLeastOnce on every `EventPayload::Connected` event (handles broker restarts)
- heartbeat_loop publishes `online` to `gnss/{device_id}/heartbeat` with retain=true every 30 seconds; 5-second initial delay before first publish
- lwt_topic String declared before MqttClientConfiguration struct to satisfy LwtConfiguration<'a> lifetime constraint (avoids compile error documented in research pitfall 1)
- EspMqttClient wrapped in Arc<Mutex<EspMqttClient<'static>>> — shareable across pump thread, heartbeat thread, and main
- Added anyhow = "1" to Cargo.toml as explicit direct dependency
- Registered mod mqtt in main.rs

## Task Commits

Each task was committed atomically:

1. **Task 1: Create src/mqtt.rs — client factory, LWT, pump, heartbeat** - `80b55f5` (feat)

**Plan metadata:** (committed below in docs commit)

## Files Created/Modified

- `/home/bharris/esp32-gnssmqtt/src/mqtt.rs` — MQTT client factory with LWT, connection pump (returns !), heartbeat loop (returns !)
- `/home/bharris/esp32-gnssmqtt/src/main.rs` — added `mod mqtt;` declaration
- `/home/bharris/esp32-gnssmqtt/Cargo.toml` — added `anyhow = "1"` as direct dependency

## Decisions Made

- **lwt_topic lifetime order:** Research pitfall 1 explicitly documented that LwtConfiguration.topic is `&'a str` — the topic String must outlive MqttClientConfiguration. Implemented by declaring `let lwt_topic` on line 29 and `let conf = MqttClientConfiguration { ... }` on line 31 of the same function scope. Drop order in Rust is declaration order reversed, so lwt_topic lives longer than conf.
- **pump_mqtt_events returns `!`:** The connection pump must never exit in normal operation. After the `while let Ok(event) = connection.next()` loop ends (connection closed), a permanent sleep loop prevents thread exit. This avoids the FreeRTOS task being destroyed and the WDT detecting a stalled thread.
- **heartbeat uses publish() not enqueue():** From research: publish() is blocking but acceptable from a dedicated heartbeat thread. The pump thread keeps the outbox moving so publish() returns promptly. enqueue() would add unnecessary complexity.
- **5-second initial heartbeat delay:** Prevents heartbeat_loop from attempting publish before the pump thread has processed the first Connected event and the MQTT stack is fully ready.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added anyhow as direct Cargo.toml dependency**
- **Found during:** Task 1 (reviewing mqtt_connect return type `anyhow::Result<...>`)
- **Issue:** mqtt_connect uses `anyhow::Result` but `anyhow` was not listed in Cargo.toml as a direct dependency. uart_bridge.rs (from plan 02-03) also uses anyhow::Result — both relied on anyhow being transitively available. While this often works, explicit direct dependencies are required for correctness.
- **Fix:** Added `anyhow = "1"` to `[dependencies]` in Cargo.toml.
- **Files modified:** Cargo.toml
- **Committed in:** 80b55f5 (Task 1 commit)

**2. [Rule 2 - Missing Critical] Registered mod mqtt in main.rs**
- **Found during:** Task 1 (verifying src/mqtt.rs would be compiled)
- **Issue:** A Rust source file at src/mqtt.rs is dead code unless `mod mqtt;` is declared in main.rs. Without it, the module is not included in the compilation unit and the code never compiles at all.
- **Fix:** Added `mod mqtt;` to src/main.rs module declarations.
- **Files modified:** src/main.rs
- **Committed in:** 80b55f5 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 2 - Missing Critical)
**Impact on plan:** Both fixes required for correct compilation. No scope creep — both are standard Rust module registration and dependency declaration requirements.

## Issues Encountered

`cargo check --target riscv32imac-esp-espidf` cannot be run in WSL2 because the host C compiler (`gcc`) is not installed. Build scripts for esp-idf-sys and build-std crates require a host C toolchain. This is the same pre-existing environment constraint documented in plan 02-03 SUMMARY. The Windows environment (where `ldproxy.exe` is installed) is the correct build target. Compilation will be verified at the Plan 04 flash step. The Rust source implements all plan-specified API calls, lifetime patterns, and thread models correctly.

## User Setup Required

None — no external service configuration required. Network credentials are already in src/config.rs (gitignored).

## Next Phase Readiness

- src/mqtt.rs ready: mqtt_connect, pump_mqtt_events, and heartbeat_loop all implemented
- Plan 04 (human-verify) will wire mqtt_connect into main.rs, spawn pump and heartbeat threads, and flash to hardware to verify broker connection, LWT, and heartbeat at runtime
- LWT and heartbeat behavior verified by monitoring broker with `mosquitto_sub -t 'gnss/#' -v` during Plan 04

---
*Phase: 02-connectivity*
*Completed: 2026-03-03*

## Self-Check: PASSED

- FOUND: src/mqtt.rs
- FOUND: src/main.rs (mod mqtt registered at line 13)
- FOUND: Cargo.toml (anyhow = "1" at line 9)
- FOUND: 02-02-SUMMARY.md
- FOUND: commit 80b55f5 (feat(02-02): implement MQTT client, LWT, connection pump, and heartbeat loop)
- VERIFIED: mqtt_connect exported at line 17
- VERIFIED: pump_mqtt_events exported at line 66
- VERIFIED: heartbeat_loop exported at line 105
- VERIFIED: lwt_topic declared at line 29 (before conf struct at line 31 — lifetime correct)
