# Roadmap: esp32-gnssmqtt

## Milestones

- ✅ **v1.0 Foundation** — Phases 1-3 (shipped 2026-03-04)
- ✅ **v1.1 GNSS Relay** — Phases 4-6 (shipped 2026-03-07)
- ✅ **v1.2 Observations + OTA** — Phases 7-8 (shipped 2026-03-07)
- 🔧 **v1.3 Reliability Hardening** — Phases 9-13 (in progress)

## Phases

<details>
<summary>✅ v1.0 Foundation (Phases 1-3) — SHIPPED 2026-03-04</summary>

- [x] Phase 1: Scaffold (2/2 plans) — completed 2026-03-03
- [x] Phase 2: Connectivity (4/4 plans) — completed 2026-03-04
- [x] Phase 3: Status LED (3/3 plans) — completed 2026-03-04

Archive: `.planning/milestones/v1.0-ROADMAP.md`

</details>

<details>
<summary>✅ v1.1 GNSS Relay (Phases 4-6) — SHIPPED 2026-03-07</summary>

- [x] Phase 4: UART Pipeline (2/2 plans) — completed 2026-03-06
- [x] Phase 5: NMEA Relay (2/2 plans) — completed 2026-03-07
- [x] Phase 6: Remote Config (2/2 plans) — completed 2026-03-07

Archive: `.planning/milestones/v1.1-ROADMAP.md`

</details>

<details>
<summary>✅ v1.2 Observations + OTA (Phases 7-8) — SHIPPED 2026-03-07</summary>

- [x] Phase 7: RTCM Relay (3/3 plans) — completed 2026-03-07
- [x] Phase 8: OTA (3/3 plans) — completed 2026-03-07

</details>

### v1.3 Reliability Hardening

- [x] **Phase 9: Channel + Loop Hardening** — Bound all channels, log UART TX errors, cap all loops and blocking receives (2 plans)
- [x] **Phase 10: Memory + Diagnostics** — Pre-allocate RTCM buffer pool; log stack HWM for all threads at startup (completed 2026-03-07)
- [x] **Phase 11: Thread Watchdog** — Heartbeat counter fed by critical threads; supervisor reboots on missed beats (completed 2026-03-07)
- [x] **Phase 12: Resilience** — Auto-reboot after extended WiFi disconnection or MQTT unavailability (completed 2026-03-07)
- [ ] **Phase 13: Health Telemetry** — Periodic MQTT status publish with uptime, heap, and drop counters

## Phase Details

### Phase 7: RTCM Relay
**Goal**: UM980 RTCM3 correction frames are reliably delivered to MQTT alongside existing NMEA relay, with correct MQTT topic routing for all message types
**Depends on**: Phase 6 (v1.1 complete)
**Requirements**: RTCM-01, RTCM-02, RTCM-03, RTCM-04, RTCM-05
**Success Criteria** (what must be TRUE):
  1. RTCM3 frames appear on `gnss/{device_id}/rtcm/{message_type}` as raw binary at QoS 0 while NMEA topics continue publishing without interruption
  2. Frames with CRC-24Q failures are silently dropped and the byte-stream re-syncs to the next valid 0xD3 preamble or `$` start without operator intervention
  3. MSM7 frames (up to 1029 bytes) are published without truncation or MQTT buffer overflow
  4. A retained `/config` payload is NOT forwarded to the UM980 when an `/ota/trigger` message arrives (topic discrimination fix verifiable by inspection of UM980 UART TX)
**Plans**: 3 plans

Plans:
- [x] 07-01-PLAN.md — Fix mqtt.rs topic routing bug and bump MQTT output buffer to 2048 bytes
- [x] 07-02-PLAN.md — Replace gnss.rs line-buffer with RxState state machine; create rtcm_relay.rs
- [x] 07-03-PLAN.md — Wire rtcm_relay into main.rs; final compile integration

### Phase 8: OTA
**Goal**: An operator can remotely update firmware by publishing a URL to an MQTT topic; the device downloads, flashes, and reboots into new firmware with automatic rollback if the new firmware fails to confirm itself
**Depends on**: Phase 7 (topic discrimination fix required for OTA trigger routing)
**Requirements**: OTA-01, OTA-02, OTA-03, OTA-04, OTA-05, OTA-06
**Success Criteria** (what must be TRUE):
  1. After publishing `{"url":"..."}` to `gnss/{device_id}/ota/trigger`, the device publishes `{"state":"downloading","progress":N}` updates and eventually `{"state":"complete"}` to `gnss/{device_id}/ota/status`, then reboots into new firmware
  2. MQTT heartbeat continues publishing during OTA download (pump event loop is not blocked by the HTTP transfer)
  3. When a new firmware boot completes without calling `mark_running_slot_valid()` within the watchdog window, the device reboots back into the previous firmware slot on the next boot
  4. OTA can be triggered a second time immediately after a successful update (trigger topic cleared; no retained re-trigger on reconnect)
**Plans**: 3 plans

Plans:
- [x] 08-01-PLAN.md — Redesign partitions.csv for dual-slot OTA; add rollback + watchdog sdkconfig; add sha2 dependency
- [x] 08-02-PLAN.md — Implement src/ota.rs: HTTP download, SHA-256 verify, EspOta flash, status publish, restart
- [x] 08-03-PLAN.md — Wire ota into main.rs and mqtt.rs: mark_valid call, ota channel, trigger routing, subscription

### Phase 9: Channel + Loop Hardening
**Goal**: All inter-thread communication channels have explicit, documented bounds; all loops and blocking calls have finite timeouts so the firmware cannot silently hang or spin forever
**Depends on**: Phase 8 (v1.2 complete)
**Requirements**: HARD-01, HARD-02, HARD-05, HARD-06
**Success Criteria** (what must be TRUE):
  1. Every `sync_channel` call in the codebase has a capacity value accompanied by a comment explaining the chosen size; no unbounded channels exist
  2. A UART TX write failure emits a log message and increments a per-failure error counter rather than being silently discarded via `let _ = ...`
  3. Every retry or init-sequence loop contains an explicit maximum iteration count or deadline; exceeding the limit logs an error and exits the loop cleanly rather than spinning indefinitely
  4. Every blocking channel receive uses `recv_timeout()` with a documented duration; no unbounded `recv()` or `lock()` call exists on any hot-path thread
**Plans**: 2 plans

Plans:
- [x] 09-01-PLAN.md — Convert 4 unbounded channels to sync_channel; log UART TX write failures (HARD-01, HARD-02)
- [x] 09-02-PLAN.md — Convert 6 blocking recv() calls to recv_timeout(); add MAX_WIFI_RECONNECT_ATTEMPTS constant (HARD-05, HARD-06)

### Phase 10: Memory + Diagnostics
**Goal**: RTCM frame delivery uses a pre-allocated buffer pool with zero per-frame heap allocation in steady state, and stack headroom for every thread is visible at startup
**Depends on**: Phase 9
**Requirements**: HARD-03, HARD-04
**Success Criteria** (what must be TRUE):
  1. At startup, the log shows a stack high-water mark (HWM) line for each spawned thread (GNSS RX, MQTT pump, NMEA relay, RTCM relay, config relay, watchdog supervisor, status publisher)
  2. RTCM frame buffers are allocated once at init into a fixed-size pool; no `Vec::new()` or heap allocation occurs per received RTCM frame during normal relay operation
  3. The pool exhaustion path (all buffers in use) drops the incoming frame and logs a warning rather than allocating dynamically or panicking
**Plans**: 2 plans

Plans:
- [ ] 10-01-PLAN.md — Add uxTaskGetStackHighWaterMark log at entry of all 11 spawned threads (HARD-04)
- [ ] 10-02-PLAN.md — Replace Vec<u8> RTCM channel with Box pool; update gnss.rs, rtcm_relay.rs, main.rs (HARD-03)

### Phase 11: Thread Watchdog
**Goal**: Critical threads are supervised so that a silent hang — a thread that stops progressing without panicking — triggers an automatic device reboot
**Depends on**: Phase 10
**Requirements**: WDT-01, WDT-02
**Success Criteria** (what must be TRUE):
  1. GNSS RX thread and MQTT pump thread each update a shared atomic counter (or equivalent heartbeat) at intervals no greater than 5 seconds during normal operation
  2. A watchdog supervisor thread detects when any critical thread has failed to update its heartbeat for 3 consecutive check intervals and calls `esp_restart()`
  3. If the watchdog supervisor thread itself stops (e.g., due to a bug), the hardware watchdog timer (already configured in sdkconfig) eventually reboots the device
**Plans**: 2 plans

Plans:
- [ ] 11-01-PLAN.md — Create watchdog.rs module with AtomicU32 counters + supervisor loop; add WDT config constants (WDT-01, WDT-02)
- [ ] 11-02-PLAN.md — Wire heartbeat fetch_add into GNSS RX loop and MQTT pump; spawn supervisor in main.rs; enable TWDT panic (WDT-01, WDT-02)

### Phase 12: Resilience
**Goal**: The device recovers from extended connectivity loss without manual intervention by rebooting itself after configurable disconnection timeouts
**Depends on**: Phase 11
**Requirements**: RESIL-01, RESIL-02
**Success Criteria** (what must be TRUE):
  1. If WiFi remains disconnected for 10 minutes (configurable constant), `wifi_supervisor` calls `esp_restart()`; the reboot is logged before it occurs
  2. If WiFi is connected but MQTT remains disconnected for 5 minutes (configurable constant), the MQTT pump signals a reboot; the reboot is logged before it occurs
  3. After a reboot triggered by either timeout, the device reconnects normally — demonstrating the restart resolved the stuck state rather than making it permanent
**Plans**: 2 plans

Plans:
- [ ] 12-01-PLAN.md — Create resil.rs module; add timeout constants; extend wifi_supervisor with RESIL-01 and RESIL-02 checks (RESIL-01, RESIL-02)
- [ ] 12-02-PLAN.md — Wire MQTT_DISCONNECTED_AT writes into MQTT callback; human verification checkpoint (RESIL-02)

### Phase 13: Health Telemetry
**Goal**: Operators can observe device health remotely via a periodic MQTT status message containing uptime, free heap, and message drop counters
**Depends on**: Phase 12
**Requirements**: METR-01, METR-02
**Success Criteria** (what must be TRUE):
  1. Every 60 seconds, the device publishes a JSON payload `{"uptime_s":N,"heap_free":N,"nmea_drops":N,"rtcm_drops":N}` to `gnss/{device_id}/status` at QoS 0
  2. The `nmea_drops` and `rtcm_drops` counters are backed by atomics that are incremented at each `TrySendError::Full` site in gnss.rs; the values in the status message reflect all drops since last boot
  3. The status publisher does not interfere with NMEA/RTCM relay throughput — publishing occurs on its own thread or timer, not inline in the relay hot path
**Plans**: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffold | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. Connectivity | v1.0 | 4/4 | Complete | 2026-03-04 |
| 3. Status LED | v1.0 | 3/3 | Complete | 2026-03-04 |
| 4. UART Pipeline | v1.1 | 2/2 | Complete | 2026-03-06 |
| 5. NMEA Relay | v1.1 | 2/2 | Complete | 2026-03-07 |
| 6. Remote Config | v1.1 | 2/2 | Complete | 2026-03-07 |
| 7. RTCM Relay | v1.2 | 3/3 | Complete | 2026-03-07 |
| 8. OTA | v1.2 | 3/3 | Complete | 2026-03-07 |
| 9. Channel + Loop Hardening | v1.3 | 2/2 | Complete | 2026-03-07 |
| 10. Memory + Diagnostics | 2/2 | Complete    | 2026-03-07 | - |
| 11. Thread Watchdog | 2/2 | Complete    | 2026-03-07 | - |
| 12. Resilience | 2/2 | Complete    | 2026-03-07 | - |
| 13. Health Telemetry | v1.3 | 0/? | Not started | - |
