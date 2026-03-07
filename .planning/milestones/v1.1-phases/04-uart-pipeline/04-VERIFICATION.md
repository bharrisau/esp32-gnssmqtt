---
phase: 04-uart-pipeline
verified: 2026-03-07T00:00:00Z
status: human_needed
score: 10/10 must-haves verified
human_verification:
  - test: "NMEA sentences appear in espflash monitor from live UM980"
    expected: "Lines like $GNGGA,123519,... stream continuously in monitor output"
    why_human: "Requires physical UM980 hardware connected to GPIO16/17; cannot verify UART RX data path without device"
  - test: "Typing a UM980 command (e.g. VERSION) via espflash monitor produces UM980 acknowledgment"
    expected: "UM980 responds with a $command,CMD,response: OK line visible in monitor"
    why_human: "Requires physical loopback through TX thread -> UartDriver -> UM980 hardware; cannot verify without device"
  - test: "No regression — WiFi connects, LED turns green, MQTT heartbeat publishes"
    expected: "Boot log shows WiFi connected, GNSS pipeline started, UART bridge started, heartbeat publishes every 30s"
    why_human: "End-to-end boot sequence requires flashing firmware to physical device FFFEB5"
---

# Phase 4: UART Pipeline Verification Report

**Phase Goal:** Read a continuous stream of NMEA bytes from the UM980 over UART0, assemble complete sentences, deliver as (sentence_type, raw_sentence) tuples via mpsc channel, provide TX Sender<String> for command injection.
**Verified:** 2026-03-07
**Status:** human_needed (all automated checks pass; hardware verification already performed per SUMMARY but cannot be re-run programmatically)
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Device reads bytes from UM980 UART0 at 115200 baud 8N1 using a dedicated FreeRTOS thread | VERIFIED | `gnss.rs:48-57` — `UartDriver::new()` with `Config::new().baudrate(Hertz(115_200)).rx_fifo_size(crate::config::UART_RX_BUF_SIZE)`, thread spawned at line 73 |
| 2 | Complete NMEA sentences are assembled from fragmented reads across multiple UART read calls | VERIFIED | `gnss.rs:76-130` — 512-byte `line_buf` accumulator, byte-by-byte loop, `\n`-triggered processing, survives across `read()` calls |
| 3 | Sentence type is extracted from the NMEA prefix (e.g. $GNGGA → GNGGA) and emitted as first tuple element | VERIFIED | `gnss.rs:101-107` — `s[1..].split(',').next().unwrap_or("UNKNOWN")` extracts type; `nmea_tx.send((sentence_type, s.to_string()))` |
| 4 | Raw NMEA lines (including $) are mirrored to stdout for espflash monitor visibility | VERIFIED | `gnss.rs:96-97` — `std::io::stdout().write_all(&line_buf[..end])` + `write_all(b"\n")` before NMEA check |
| 5 | Non-NMEA lines (not starting with $) are logged at WARN and dropped; empty lines are silently skipped | VERIFIED | `gnss.rs:109-115` — `else if end > 0` branch calls `log::warn!`; `end == 0` case has comment "silently skip" with no action |
| 6 | A Sender<String> channel endpoint is returned so callers can write command lines to the UM980 TX | VERIFIED | `gnss.rs:67,157` — `let (cmd_tx, cmd_rx) = mpsc::channel::<String>()`, returned as first element of `Ok((cmd_tx, nmea_rx))` |
| 7 | A Receiver<(String, String)> channel endpoint is returned for Phase 5 NMEA consumer | VERIFIED | `gnss.rs:64,157` — `let (nmea_tx, nmea_rx) = mpsc::channel::<(String, String)>()`, returned as second element |
| 8 | uart_bridge.rs no longer owns or creates a UartDriver | VERIFIED | `uart_bridge.rs:7` — only imports `std::io::{Read, Write}`; no esp_idf_svc imports; no UartDriver |
| 9 | main.rs Step 7 calls gnss::spawn_gnss and receives (gnss_cmd_tx, nmea_rx) | VERIFIED | `main.rs:86-91` — `let (gnss_cmd_tx, nmea_rx) = gnss::spawn_gnss(peripherals.uart0, ...)` |
| 10 | gnss_cmd_tx and nmea_rx are held alive in main.rs idle loop scope for Phase 5/6 handoff | VERIFIED | `main.rs:141-142` — `let _gnss_cmd_tx = gnss_cmd_tx;` and `let _nmea_rx = nmea_rx;` before idle loop |

**Score:** 10/10 truths verified

---

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/gnss.rs` | spawn_gnss public function + RX thread + TX thread | VERIFIED | 158 lines, full implementation; no stubs; exports correct signature |
| `src/uart_bridge.rs` | TX-only stdin bridge, spawn_bridge(cmd_tx: Sender<String>) | VERIFIED | 89 lines; Thread B only; no UART peripherals; `cmd_tx.send()` on Enter |
| `src/main.rs` | Wired Step 7, mod gnss declared, channel endpoints held | VERIFIED | `mod gnss;` at line 31; Step 7 at lines 85-97; idle loop bindings at lines 141-142 |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| gnss.rs RX thread | nmea_tx (mpsc::Sender<(String, String)>) | nmea_tx.send() | WIRED | `gnss.rs:107` — `let _ = nmea_tx.send((sentence_type, s.to_string()))` |
| gnss.rs TX thread | UartDriver::write() | for line in &cmd_rx | WIRED | `gnss.rs:148-150` — `for line in &cmd_rx { let _ = uart_tx.write(line.as_bytes()); let _ = uart_tx.write(b"\r\n"); }` |
| gnss.rs RX thread | stdout | write_all | WIRED | `gnss.rs:96-97` — `std::io::stdout().write_all(&line_buf[..end])` |
| main.rs Step 7 | gnss::spawn_gnss | let (gnss_cmd_tx, nmea_rx) = gnss::spawn_gnss(...) | WIRED | `main.rs:86` — call with uart0, gpio16, gpio17 |
| main.rs | uart_bridge::spawn_bridge | spawn_bridge(gnss_cmd_tx.clone()) | WIRED | `main.rs:95` — `uart_bridge::spawn_bridge(gnss_cmd_tx.clone())` |
| uart_bridge Thread B | cmd_tx mpsc channel | cmd_tx.send(String) | WIRED | `uart_bridge.rs:52-53` — `cmd_tx.send(s).unwrap_or_else(...)` on Enter key |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| UART-01 | 04-01-PLAN, 04-02-PLAN | Device reads raw bytes from UM980 UART at 115200 baud 8N1 using a dedicated thread | SATISFIED | `gnss.rs:48-57` — UartDriver at 115200 baud; RX thread at lines 73-137 |
| UART-02 | 04-01-PLAN, 04-02-PLAN | Device accumulates bytes into complete NMEA sentences, handling fragmented reads | SATISFIED | `gnss.rs:76-130` — 512-byte accumulator with byte-by-byte loop |
| UART-03 | 04-01-PLAN, 04-02-PLAN | Device extracts sentence type from NMEA prefix for MQTT topic use | SATISFIED | `gnss.rs:102-106` — split at first comma, strip leading $ |
| UART-04 | (none — explicitly deferred) | Checksum validation | OUT OF SCOPE | Explicitly deferred in 04-RESEARCH.md and 04-CONTEXT.md; both plans document as deferred to a later phase |

**UART-04 Note:** UART-04 (checksum validation) is cited in the ROADMAP as "UART-01 through UART-04" but was explicitly deferred out of scope for Phase 4 per 04-CONTEXT.md and 04-RESEARCH.md. Neither plan claims it in their `requirements:` frontmatter. This is documented deferred scope, not an orphaned gap.

---

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | — | — | — | — |

No TODO/FIXME/placeholder comments, no empty implementations, no println! violations, no stub return values found in any of the three modified files.

---

## Human Verification Required

### 1. NMEA Stream Visibility

**Test:** Flash firmware to device FFFEB5 (`cargo build && espflash flash --monitor`) and observe monitor output after boot.
**Expected:** NMEA sentences stream continuously — lines like `$GNGGA,123519,...` and `$GNRMC,...` appear in the monitor without structured log prefixes (raw stdout mirror from gnss.rs RX thread).
**Why human:** Requires physical UM980 connected to GPIO16/17; the RX data path cannot be exercised without hardware.

Note: 04-02-SUMMARY.md reports this was already verified on device FFFEB5 during Task 3. The SUMMARY documents one deviation: no `log::info!` line is emitted on successful NMEA parse (only raw stdout mirror). This is accepted behavior — sentences appear in monitor, sentence type is NOT visible as a structured log line on the happy path.

### 2. TX Command Injection

**Test:** In espflash monitor, type `VERSION` and press Enter.
**Expected:** UM980 responds with a `$command,CMD,response: OK` acknowledgment line visible in monitor.
**Why human:** Requires the full TX path: stdin -> uart_bridge Thread B -> cmd_tx.send() -> gnss TX thread -> UartDriver::write() -> UM980 hardware response.

### 3. No Regression

**Test:** Observe full boot sequence and wait ~30 seconds.
**Expected:** Boot log shows LED task started, WiFi connected, GNSS pipeline started, UART bridge started, MQTT client created, all subsystems operational. Heartbeat publishes every 30 seconds. LED transitions from blue (connecting) to green (operational).
**Why human:** Requires physical device; WiFi credentials and MQTT broker must be reachable.

Note: 04-02-SUMMARY.md reports all three human verification steps passed on device FFFEB5 on 2026-03-07.

---

## Verification of Commit Artifacts

All commits claimed in SUMMARYs confirmed to exist in git history:

- `48d5ac2` — feat(04-01): add src/gnss.rs (VERIFIED in git log)
- `c55083d` — refactor(04-02): refactor uart_bridge to TX-only stdin bridge (VERIFIED)
- `8f31bd2` — feat(04-02): wire main.rs Step 7 with gnss::spawn_gnss pipeline (VERIFIED)

---

## Summary

Phase 4 goal achievement is verified at the code level. All three artifacts exist with full, non-stub implementations. All six key links are wired. The NMEA pipeline is structurally complete:

- **gnss.rs** owns the UartDriver exclusively, runs two threads (RX sentence assembly, TX command write), returns `(Sender<String>, Receiver<(String, String)>)` to callers.
- **uart_bridge.rs** is reduced to a TX-only stdin bridge with no UART hardware ownership; it sends via `cmd_tx.send()` on Enter.
- **main.rs** declares `mod gnss`, calls `gnss::spawn_gnss` at Step 7, passes a clone to uart_bridge, and holds both channel endpoints alive in the idle loop for Phase 5/6 handoff.

UART-04 (checksum validation) was explicitly deferred out of scope for this phase and is not a gap.

The three human verification items require flashing to device FFFEB5. The 04-02-SUMMARY.md reports these were already performed and passed. No automated substitute exists for hardware verification of UART data flow.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
