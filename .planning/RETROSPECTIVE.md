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

## Cross-Milestone Trends

| Milestone | Phases | Plans | Duration | Notes |
|-----------|--------|-------|----------|-------|
| v1.0 Foundation | 3 | 9 | 2 days | First milestone; patterns established |
| v1.1 GNSS Relay | 3 | 6 | 3 days | Full relay pipeline; hardware-verified throughout |
