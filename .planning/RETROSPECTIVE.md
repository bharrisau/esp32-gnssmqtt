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

## Cross-Milestone Trends

| Milestone | Phases | Plans | Duration | Notes |
|-----------|--------|-------|----------|-------|
| v1.0 Foundation | 3 | 9 | 2 days | First milestone; patterns established |
