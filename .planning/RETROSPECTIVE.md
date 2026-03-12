# Retrospective: esp32-gnssmqtt

## Milestone: v1.0 — Foundation

**Shipped:** 2026-03-04
**Phases:** 3 | **Plans:** 9 | **Duration:** 2 days

### What Was Built

- ESP32-C6 Rust firmware scaffold with pinned crates, ESP-IDF v5.3.3, hardware device ID
- WiFi connect + exponential backoff reconnect supervisor (1s→60s cap)
- MQTT client: LWT, pump thread, heartbeat loop, re-subscribe on broker restart
- Bidirectional USB-serial ↔ UM980 UART bridge (UART0, GPIO16/17, 115200 baud)
- Status LED (GPIO15 active-low): three-state blink patterns via `Arc<AtomicU8>` + dedicated thread
- All modules wired and hardware-verified on device FFFEB5

### What Worked

- **Plan-then-execute discipline** — CONTEXT.md → RESEARCH.md → PLAN.md → execute kept scope tight
- **Executor agents** handled all Rust compilation errors independently, including a non-trivial MQTT deadlock fix
- **Hardware checkpoints** with clear serial log criteria made verification fast
- `Arc<AtomicU8>` for LED state was the right call — lock-free, zero contention, Relaxed ordering sufficient

### What Was Inefficient

- UART bridge plan used wrong pins (UART1/GPIO20-21 vs actual UART0/GPIO16-17) — required correction during execution
- LED-03 error state not hardware-verified (requires ~4 min deliberate WiFi failure); accepted on code inspection
- discuss-phase 03 UI non-functional in session (AskUserQuestion returned empty); worked around with "skip"

### Patterns Established

- MQTT deadlock prevention: pump thread never calls client methods; `mpsc::channel` to `subscriber_loop` handles subscribe
- LWT lifetime pattern: `lwt_topic` String must be declared before `MqttClientConfiguration` in same scope
- `disable_clean_session: true` for subscription persistence across MQTT reconnects (not broker restarts)
- LED state ownership: WiFi supervisor writes Connecting/Error; MQTT pump writes Connected; LED thread reads only
- Thread spawn template: `std::thread::Builder::new().stack_size(8192).spawn(move || ...)`

### Key Lessons

- Executor agents are reliable for Rust embedded work — they handle borrow/lifetime errors well
- Verify hardware wiring against plan pin assignments before executing UART plans
- For error states requiring deliberate failure conditions, code inspection + acceptance is pragmatic
- `PinDriver::output()` GPIO log showing `OutputEn: 0` is the reset state before configuration — not a bug

### Cost Observations

- Profile: sonnet throughout
- Sessions: 2 (context reset between plan-phase and execute-phase)
- Notable: Phase 2 MQTT deadlock solved by executor without escalation; added subscriber_loop pattern

## Milestone: v1.1 — GNSS Relay

**Shipped:** 2026-03-07
**Phases:** 3 (04-06) | **Plans:** 6 | **Duration:** 3 days

### What Was Built

- gnss.rs: exclusive UartDriver owner with RX sentence-assembly thread (512-byte buf, NON_BLOCK polling) and TX command thread; delivers (type, sentence) tuples via sync_channel(64)
- uart_bridge.rs refactored to TX-only (Sender<String>); UART peripheral ownership consolidated in gnss.rs
- nmea_relay.rs: spawn_relay() consumes Receiver, publishes raw NMEA to gnss/FFFEB5/nmea/{TYPE} at QoS 0; bounded backpressure via try_send drop
- config_relay.rs: spawn_config_relay() with djb2 hash dedup, JSON/plain-text payload parser, 100ms per-command delay
- All wired into main.rs; hardware-verified on device FFFEB5 at 10 msg/sec NMEA throughput

### What Worked

- **mpsc channel boundaries** between subsystems made each phase independently testable and easy to wire
- **sync_channel(64) + try_send** pattern correctly handles NMEA backpressure without blocking UART reads
- **djb2 hash dedup** elegantly prevents re-applying retained MQTT configs on reconnect — no external crate needed
- **Hardware verification** per phase (not just at end) caught issues immediately
- **UM980 behavioral notes** captured in STATE.md (RESET reboot delay, UNLOG vs CONFIGSAVE) will save time next milestone

### What Was Inefficient

- ROADMAP.md progress table had Phase 5 marked "1/2 In Progress" throughout Phase 6 — stale tracking not caught until milestone close
- gsd-tools `summary-extract --fields one_liner` returned null for all summaries (frontmatter uses different key names) — MILESTONES.md accomplishments required manual entry
- No REQUIREMENTS.md was created for this milestone; requirements lived only in PROJECT.md Active section

### Patterns Established

- UART exclusive ownership in one module (gnss.rs); other subsystems receive channel endpoints only
- `gnss_cmd_tx.clone()` to relay threads; original retained in main.rs idle loop to keep Sender alive (prevents TX thread exit)
- `Arc<Mutex<EspMqttClient>>` per-sentence lock acquisition released each iteration — prevents heartbeat/subscriber starvation at 10+ Hz
- Empty payload guard in config relay skips retained-message-cleared MQTT events
- UM980 init via retained MQTT config topic (not CONFIGSAVE) — avoids NVM wear, enables remote reconfiguration without reflash

### Key Lessons

- Verify ROADMAP.md progress table entries when completing a phase, not just at milestone close
- For embedded UART modules: consolidate peripheral ownership in one module, expose only channel endpoints
- `sync_channel(N)` + `try_send` is the right pattern for real-time sensor data with slow consumers
- UM980: use UNLOG to silence outputs cleanly, not CONFIGSAVE; wait after RESET before sending more commands

### Cost Observations

- Profile: sonnet throughout
- Sessions: ~4 (context resets between phases)
- Notable: All 6 plans executed without escalation; hardware verification passed on first attempt for all phases

## Milestone: v1.3 — Reliability Hardening

**Shipped:** 2026-03-08
**Phases:** 7 (07-13) | **Plans:** 15 | **Duration:** 2 days

### What Was Built

- Phase 7: Four-state UART RX state machine (NMEA/RTCM/FreeLine/HashLine), CRC-24Q verification, RTCM3 relay to MQTT
- Phase 8: Dual-slot OTA with rollback, HTTP streaming + SHA-256 verify, MQTT-triggered, mark_valid on successful boot
- Phase 9: All channels bounded with `sync_channel`; UART TX error counter; `recv_timeout` on all 6 blocking receives
- Phase 10: Pre-allocated RTCM buffer pool (4 × 1029 bytes); FreeRTOS HWM logged at entry of all 11 threads
- Phase 11: Software watchdog (AtomicU32 heartbeats, 15s detection) + hardware TWDT backstop (30s)
- Phase 12: Auto-reboot after 10min WiFi disconnect or 5min MQTT disconnect while WiFi up
- Phase 13: JSON health heartbeat every 30s; retained "online" on every MQTT reconnect; UM980 query response routing

### What Worked

- **Reliability-first sequencing** — channel hardening → memory → watchdog → resilience → telemetry built each layer on the previous; no rework required
- **AtomicU32 pattern for cross-thread signalling** — used for watchdog heartbeats, drop counters, UART TX errors, MQTT disconnect timestamp; consistent and lock-free
- **Pre-allocated pool pattern** — RTCM buffer pool completely eliminated per-frame heap allocation; pool exhaustion handled gracefully without panic
- **recv_timeout everywhere** — exposed that several threads had no liveness guarantee; systematic conversion was low-risk and high-value
- **Post-phase improvements** — MQTT reconnect "online" bug and UM980 free-text handling were caught during live hardware testing and fixed cleanly without plan rework

### What Was Inefficient

- `gsd-tools summary-extract --fields one_liner` still returns null for all summaries — MILESTONES.md accomplishments required manual entry again (same issue as v1.1)
- METR-01 in REQUIREMENTS.md had stale draft text (wrong topic `/status` vs actual `/heartbeat`, wrong interval 60s vs 30s, wrong field count) — not caught until verifier ran
- v1.2 (Phases 7-8) was not formally declared as a milestone at the time; retrospective coverage starts at v1.3

### Patterns Established

- Separate `status_tx` channel in MQTT callback for heartbeat reconnect signalling — pattern for any logic that needs to react to Connected events without sharing `subscribe_tx`
- `RxState` enum with `FreeLine`/`HashLine` arms — clean way to handle heterogeneous UART protocols without silently discarding bytes; reuse this for future UM980 response parsing
- Pool buffer pattern: `sync_channel(N)` pre-filled at init; `try_recv()` at frame start; `try_send` to return on drop/error — zero dynamic allocation, bounded memory
- Watchdog via two `AtomicU32` counters + supervisor thread checking deltas — simpler than FreeRTOS task handles, works across Rust thread abstraction

### Key Lessons

- Always verify REQUIREMENTS.md text matches the actual implementation spec before writing code — stale draft text in METR-01 caused a documentation inconsistency that verifier caught
- Post-phase hardware testing is valuable even when automated checks pass — the MQTT reconnect "online" bug was invisible to static analysis
- `recv_timeout` conversion is a low-effort, high-value hardening pass; do it early in any embedded project with threads
- UM980 UART protocol has four distinct line types (`$`, `0xD3`, `#`, other) — model them all from the start rather than adding states incrementally

### Cost Observations

- Profile: sonnet throughout
- Sessions: 2 (one for execution, one for milestone completion)
- Notable: All 15 plans executed without escalation; two post-phase fixes (MQTT reconnect, UM980 RX) landed cleanly in the same session

## Milestone: v2.0 — Field Deployment

**Shipped:** 2026-03-12
**Phases:** 8 (14-21) | **Plans:** 24 | **Duration:** 4 days

### What Was Built

- Phase 14: SNTP wall-clock timestamps; UM980 command relay topic; remote reboot trigger
- Phase 15: SoftAP web provisioning portal — WiFi (3 SSIDs), MQTT, stored in NVS; multi-AP failover; GPIO9 button entry
- Phase 16: ESP-IDF vprintf hook → Rust relay → MQTT log topic; re-entrancy guard prevents feedback loops; runtime level config
- Phase 17: NTRIP v1 TCP client streams RTCM3 to UM980 UART; captive portal DNS hijack for all OS probes; NTRIP state in heartbeat
- Phase 18: GGA parsing into atomics (fix_type, satellites, HDOP); heartbeat extended; OTA hardware-validated on FFFEB5; project README
- Phase 19: DHCP DNS fix unblocking Android captive detection; NVS config_ver + TLS default fix; GPIO9 3-phase state machine
- Phase 20: Windows/iOS captive portal probes; NMEA channel 64→128; UM980 config NVS persistence + auto-reapply; TLS NTRIP (EspTls)
- Phase 21: Arc<Mutex<EspMqttClient>> eliminated; single publish thread; SyncSender<MqttMessage> across all relay threads; bytes crate; outbox observability

### What Worked

- **Field testing feedback loop** — deploying on real hardware (FFFEB5) and immediately creating phases for discovered bugs (phases 19, 20) kept the firmware improving rapidly
- **Captive portal DNS hijack** — implementing a full DNS server on port 53 UDP was more involved than expected but solved all OS captive detection in one pass
- **EspTls for NTRIP TLS** — ESP-IDF's bundled CA certificates handled AUSCORS certificate chain without any manual cert management
- **SyncSender publish thread** — replacing Arc<Mutex<EspMqttClient>> with a dedicated publish thread simplified ownership across 6+ relay threads simultaneously
- **bytes crate** — zero-copy Bytes type fit naturally into the MqttMessage enum for RTCM frames; no per-publish allocation on the hot path

### What Was Inefficient

- **Phases 19-21 were not in the original v2.0 roadmap** — field testing exposed captive portal, MQTT throughput, and contention issues that required 3 additional phases; roadmap should have anticipated at least one "field fixes" phase
- **`gsd-tools summary-extract --fields one_liner` returns null** for all summaries — third milestone with this issue; accomplishments still require manual entry
- **v2.0 ROADMAP.md archived copy captured state at phase 18**, not the full 14-21 scope; milestone archive was created before phases 19-21 were added

### Patterns Established

- Field deployment phase at milestone end — explicitly budget one phase for real-hardware bugs after major feature work
- NVS schema versioning (`config_ver` u8) — any NVS schema change needs a version field + migration path for OTA-upgraded devices
- `EspNetif::new_with_conf` for DNS in SoftAP — configure DNS at netif creation, not post-start
- `SyncSender<MqttMessage>` publish bus — single-publisher thread owning MQTT client is the right pattern for embedded; avoids all mutex contention on the publish path

### Key Lessons

- Plan a "field fixes" phase in any milestone that involves hardware deployment — real-world conditions always reveal issues that bench testing misses
- NVS TLS/bool defaults must be explicitly written on save — reading an unwritten key returns an error, not `false`; always write on first save with a version field
- Captive portal requires DNS + multiple OS-specific probe URLs — Android, iOS, and Windows all use different probes; test all three before closing
- The publish thread pattern is strictly better than Arc<Mutex<Client>> for MQTT in embedded Rust — apply this from the start in future projects

### Cost Observations

- Profile: sonnet throughout
- Sessions: ~6 (context resets between phases; field testing loop added 2 extra sessions)
- Notable: Phase 20 field fixes and Phase 21 refactor were both planned and executed in single sessions without escalation

## Milestone: v2.1 — Server and nostd Foundation

**Shipped:** 2026-03-12
**Phases:** 4 (22-25) | **Plans:** 11 | **Duration:** 1 day

### What Was Built

- Cargo workspace restructured with resolver=2; firmware/ + gnss-server/ + crates/* members; panic=abort via rustflag scoped to embedded target
- Complete ESP-IDF nostd audit — 27 usages across 12 categories, gap priority ranking, implementation notes
- gnss-nvs crate: NvsStore trait + ESP-IDF impl + sequential-storage skeleton (first gap crate with actual implementation)
- RTCM3 MSM4/MSM7 decode pipeline using rtcm-rs 0.11; EpochBuffer flush-on-change; GPS/GLONASS/Galileo/BeiDou + ephemeris 1019/1020/1046/1042
- RINEX 2.11 observation (.26O) + navigation (.26P) writers with hourly rotation, D19.12 Fortran formatter, GPS week tracking
- axum HTTP + WebSocket server: polar skyplot SVG, SNR bar chart, device health panel; GsvAccumulator multi-sentence state machine
- 5 gap crate skeletons: gnss-ota, gnss-softap, gnss-dns, gnss-log — trait definitions + BLOCKER.md documenting exactly what blocks nostd implementation

### What Worked

- **Single-day execution** — all 4 phases and 11 plans completed in one day; clean parallel dependency graph (Phase 23 → Phase 24 + Phase 25 in parallel) enabled efficient ordering
- **rtcm-rs 0.11 was the right choice** — avoids hand-rolled MSM cell mask and pseudorange bugs; private module paths forced inline signal extraction which turned out cleaner
- **TDD discipline throughout** — RED tests before GREEN implementation caught plan errors (BeiDou msg1042 vs 1044, D19.12 zero-case, nav header label) before they became integration bugs
- **Gap crates as trait-only skeletons** — captures exactly what is needed for the embassy port without blocking delivery; BLOCKER.md format proved concise and actionable
- **Workspace resolver=2** — prevents std feature unification into no_std crates; the pattern was well-understood from planning and executed without surprises

### What Was Inefficient

- **GN-talker functional gap** — nmea 0.7 doesn't support $GN combined-constellation talker (UM980 default); discovered during TDD, worked around in tests with GP talker, but means skyplot/SNR chart won't update with real device data; should be addressed in v2.2 (configure UM980 per-constellation talkers or upgrade nmea crate)
- **gnss-nvs not wired into firmware** — crate exists with good coverage but firmware still calls EspNvs directly; intentionally deferred but creates a gap between the crate and its actual use
- **Nyquist VALIDATION.md files not updated post-execution** — all 4 phases have VALIDATION.md with nyquist_compliant: false; planning artifacts created but never updated; run /gsd:validate-phase 22-25 to close

### Patterns Established

- Workspace Cargo.toml resolver=2 with no build.target; embedded target exclusively in firmware/.cargo/config.toml — copy this verbatim for any future Rust embedded + server workspace
- RTCM3 epoch buffer pattern: EpochBuffer::push() accumulates; epoch_key=0 as sentinel; flush-on-change returns EpochGroup for downstream writers
- Gap crate template: trait-only, no external deps, `BLOCKER.md` with specific crate/issue references — repeat for all future gap crates
- broadcast::channel with `_discard` receiver in main() — keeps channel open when no WebSocket clients are connected

### Key Lessons

- Check NMEA crate talker support early when targeting UM980 — it emits $GN by default; this caused test friction and a known production gap
- Plan RINEX spec-checking tests with the exact column widths before implementing; the D19.12 zero case and nav header label were both caught by TDD in the GREEN phase, not during planning
- Gap crates should be validated against hardware before marking NOSTD requirements as fully satisfied — gnss-nvs sequential-storage impl is code-complete but not device-tested

### Cost Observations

- Profile: sonnet (balanced profile throughout)
- Sessions: 1 (full milestone in single context)
- Notable: All 11 plans executed without escalation; BeiDou ephemeris type correction and RINEX format bugs caught by TDD before integration

## Cross-Milestone Trends

| Milestone | Phases | Plans | Duration | Notes |
|-----------|--------|-------|----------|-------|
| v1.0 Foundation | 3 | 9 | 2 days | First milestone; patterns established |
| v1.1 GNSS Relay | 3 | 6 | 3 days | Full relay pipeline; hardware-verified throughout |
| v1.3 Reliability Hardening | 7 | 15 | 2 days | Fast execution; post-phase hardware testing caught real bugs |
| v2.0 Field Deployment | 8 | 24 | 4 days | Largest milestone; 3 unplanned field-fix phases; MQTT refactor eliminating Arc<Mutex> |
| v2.1 Server + nostd | 4 | 11 | 1 day | First server + crate milestone; single-day execution; TDD caught 3 plan-level spec errors |
