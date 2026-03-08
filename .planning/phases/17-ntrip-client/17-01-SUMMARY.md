---
phase: 17-ntrip-client
plan: 01
subsystem: gnss
tags: [ntrip, uart, tcp, nvs, rtcm, arc, esp32]

# Dependency graph
requires:
  - phase: 15-provisioning
    provides: NVS partition pattern (EspNvs::new, two-u8 port storage, namespace pattern)
  - phase: 16-remote-logging
    provides: existing gnss.rs with Arc<UartDriver> internal Arc pattern
provides:
  - spawn_gnss now returns Arc<UartDriver<'static>> as 5th tuple element
  - src/ntrip_client.rs with NTRIP_STATE, NtripConfig, spawn_ntrip_client, NVS load/save
affects: [17-02-wire-up, 17-03-log-quality, 17-04-um980-reboot]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Arc<UartDriver> shared across threads without Mutex (read/write take &self)"
    - "NVS two-u8 port storage pattern (ntrip_port_hi / ntrip_port_lo)"
    - "NTRIP v1 TCP session: GET request, ICY 200 OK byte-scan, raw RTCM byte forwarding"
    - "Custom inline base64 encoder (RFC 4648 §4) to avoid dependency"
    - "Backoff via recv_timeout on config channel during sleep window"

key-files:
  created:
    - src/ntrip_client.rs
  modified:
    - src/gnss.rs

key-decisions:
  - "spawn_gnss returns Arc<UartDriver<'static>> as 5th tuple element; main.rs update deferred to Plan 02"
  - "RTCM correction bytes written directly to Arc<UartDriver> (not through gnss_cmd_tx String channel)"
  - "Custom base64 encoder inline (~30 lines) avoids adding base64 crate dependency"
  - "NTRIP config deduplication intentionally omitted (unlike config_relay.rs) — same payload should force reconnect"
  - "Known concurrent write race (GNSS TX + NTRIP thread) documented and accepted for Phase 17; low practical risk"
  - "NVS namespace 'ntrip' with six keys all within 15-char NVS limit"

patterns-established:
  - "NTRIP v1 session: byte-scan for CRLF-CRLF header terminator, check first line for ICY 200 OK"
  - "Config update during session detected via try_recv in streaming loop, returns new config to trigger reconnect"

requirements-completed: [NTRIP-01, NTRIP-03]

# Metrics
duration: 4min
completed: 2026-03-08
---

# Phase 17 Plan 01: NTRIP Client Foundation Summary

**NTRIP v1 TCP client module with ICY 200 OK session handling, exponential backoff reconnect, NVS config persistence, and Arc<UartDriver> exposure from spawn_gnss for direct RTCM byte injection**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-03-08T14:06:15Z
- **Completed:** 2026-03-08T14:09:55Z
- **Tasks:** 2
- **Files modified:** 2 (1 modified, 1 created)

## Accomplishments

- Modified `spawn_gnss` to return `Arc<UartDriver<'static>>` as its 5th tuple element, enabling `ntrip_client` to write RTCM bytes directly to the UM980 UART without the String-typed `gnss_cmd_tx` channel
- Created `src/ntrip_client.rs` with the complete NTRIP v1 client: TCP connect, `ICY 200 OK` header validation, 60s read timeout, RTCM byte streaming to UART, config updates mid-session via `try_recv`, and `NTRIP_STATE` AtomicU8 for heartbeat telemetry
- Implemented exponential backoff (5/10/20/40s cap) with config channel poll during sleep window, NVS persistence using "ntrip" namespace, and inline base64 encoder per RFC 4648 to avoid adding a crate dependency

## Task Commits

Each task was committed atomically:

1. **Task 1: Expose Arc<UartDriver> from spawn_gnss** - `c327e45` (feat)
2. **Task 2: Create ntrip_client.rs module** - `3b15946` (feat)

## Files Created/Modified

- `src/gnss.rs` — Added `uart_for_ntrip = Arc::clone(&uart)` before TX thread move; updated return type signature and doc comment to reflect 5-element tuple
- `src/ntrip_client.rs` — New module: `NTRIP_STATE` AtomicU8, `NtripConfig` struct, `load_ntrip_config` / `save_ntrip_config` (NVS), `parse_ntrip_config_payload` (manual JSON), `base64_encode`, `build_ntrip_request`, `read_ntrip_headers`, `run_ntrip_session`, `spawn_ntrip_client`

## Decisions Made

- `spawn_gnss` returns the 5th Arc before the TX thread moves the original; clone order is `uart_rx` (RX thread) → `uart_tx` (TX thread via move) → `uart_for_ntrip` (returned). main.rs destructure update intentionally deferred to Plan 02.
- RTCM bytes go directly to `uart.write()` — the `gnss_cmd_tx: SyncSender<String>` channel would corrupt binary data as the TX thread appends `\r\n`.
- Inline base64 encoder (~30 lines) chosen over `base64` crate — project avoids external dependencies (no serde, no json crate), and credentials are always short ASCII.
- No djb2 deduplication on NTRIP config (unlike `config_relay.rs`) — repeated identical payload should force reconnect (operator may be re-triggering after a bad session).
- Concurrent UART write race documented (`KNOWN-RACE` comment + reference to RESEARCH.md Pitfall 3) and accepted for Phase 17; UM980 commands are rare and operator-triggered.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None. Build verification confirms exactly one compile error in `src/main.rs` (tuple pattern mismatch on 4-element vs 5-element), which is the expected state before Plan 02 updates the destructure.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Plan 02 (wire-up): update `main.rs` to destructure the 5-element tuple, create the `ntrip_config_rx` channel, pass it to `spawn_ntrip_client`, subscribe the MQTT subscriber to `/ntrip/config`, and dispatch NTRIP payloads from the MQTT callback.
- The `NTRIP_STATE` AtomicU8 is ready for Plan 02's heartbeat extension (NTRIP-04).
- No blockers.

---
*Phase: 17-ntrip-client*
*Completed: 2026-03-08*
