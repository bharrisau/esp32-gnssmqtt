---
phase: 17-ntrip-client
verified: 2026-03-09T00:00:00Z
status: human_needed
score: 14/15 must-haves verified
re_verification: false
human_verification:
  - test: "Connect Android or iOS device to GNSS-Setup SoftAP and confirm automatic captive portal prompt"
    expected: "Android shows 'Sign in to GNSS-Setup' notification; iOS shows captive portal sheet — without any manual browser navigation"
    why_human: "Requires physical hardware flash and a mobile device; OS-level captive portal detection behavior cannot be verified programmatically"
---

# Phase 17: NTRIP Client Verification Report

**Phase Goal:** Enable RTK-grade positioning by implementing an NTRIP v1 client that streams RTCM3 corrections from a configurable caster to the UM980 GNSS receiver over UART, with runtime configuration via MQTT retained topics.

**Verified:** 2026-03-09

**Status:** HUMAN_NEEDED — all automated checks pass; one hardware checkpoint deferred per plan (Plan 17-04 Task 3).

**Re-verification:** No — initial verification.

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | NTRIP client thread starts, loads config from NVS if present, and connects to a caster | VERIFIED | `spawn_ntrip_client` in `ntrip_client.rs` calls `load_ntrip_config` at thread entry; `TcpStream::connect` called in `run_ntrip_session` |
| 2 | RTCM3 correction bytes received from caster are written directly to UART (not through gnss_cmd_tx String channel) | VERIFIED | `uart.write(&buf[..n])` on line 382 of `ntrip_client.rs`; `Arc<UartDriver>` passed directly; `gnss_cmd_tx` not used |
| 3 | On TCP drop or read timeout, NTRIP_STATE resets to 0 and the thread reconnects with exponential backoff | VERIFIED | `NTRIP_STATE.store(0, Ordering::Relaxed)` on error/disconnect; backoff via `NTRIP_BACKOFF_STEPS = &[5, 10, 20, 40]`; `recv_timeout(Duration::from_secs(delay))` in error arm |
| 4 | spawn_gnss returns Arc<UartDriver> so ntrip_client can share UART write access | VERIFIED | `spawn_gnss` returns 5-element tuple; 5th element is `Arc<UartDriver<'static>>`; `uart_for_ntrip = Arc::clone(&uart)` at line 438 of `gnss.rs` |
| 5 | Publishing JSON to gnss/{device_id}/ntrip/config causes the NTRIP client to connect or reconnect | VERIFIED | `mqtt.rs` line 111 dispatches `/ntrip/config` topic to `ntrip_config_tx` before the `/config` branch; `ntrip_config_rx` wired to `spawn_ntrip_client` in `main.rs` |
| 6 | The retained config is re-applied after reboot (NVS persistence) without waiting for MQTT | VERIFIED | `load_ntrip_config` reads "ntrip" NVS namespace at thread entry; `save_ntrip_config` called on every new payload; 6 NVS keys all within 15-char limit |
| 7 | The heartbeat JSON includes a 'ntrip' field with value 'connected' or 'disconnected' | VERIFIED | `mqtt.rs` lines 393-399: reads `crate::ntrip_client::NTRIP_STATE`, formats `"ntrip\":\"connected"` or `"ntrip\":\"disconnected"` |
| 8 | The device subscribes to /ntrip/config at QoS AtLeastOnce on every MQTT connection | VERIFIED | `subscriber_loop` in `mqtt.rs` lines 216-219: `c.subscribe(&ntrip_config_topic, QoS::AtLeastOnce)` on every `Connected` event |
| 9 | Boot log messages are not silently dropped during the initial 30ms burst (channel 32 -> 128) | VERIFIED | `log_relay.rs` line 179: `sync_channel::<String>(128)` |
| 10 | MQTT Subscribed and Published ACK events do not appear as warnings | VERIFIED | `mqtt.rs` lines 149-153: `EventPayload::Subscribed(_) \| EventPayload::Published(_)` handled explicitly before catch-all `warn!` |
| 11 | Log messages from C components via vprintf hook contain no ANSI color escapes on MQTT log topic | VERIFIED | `strip_ansi(s: String) -> String` function in `log_relay.rs` (byte scan, no regex); called in `rust_log_try_send` line 128 before `tx.try_send` |
| 12 | When UM980 reboots and sends $devicename banner, GNSS RX thread detects it and signals re-apply | VERIFIED | `gnss.rs` lines 252-263: `if sentence_type == "devicename"` triggers `reboot_tx.try_send(())`; reboot monitor thread in `main.rs` receives signal and logs warning directing operator to re-send config via MQTT |
| 13 | DNS queries for any hostname resolve to 192.168.71.1 while SoftAP is active | VERIFIED | `provisioning.rs`: `UdpSocket::bind("0.0.0.0:53")` in spawned thread; RFC 1035 response with RDATA `[192, 168, 71, 1]`; QR bit and QDCOUNT checks present |
| 14 | Probe URLs used by Android and iOS return portal HTML, causing the OS to show captive portal UI | VERIFIED | `provisioning.rs`: handlers for `/generate_204`, `/connectivitycheck`, `/hotspot-detect.html`, `/success.html`, `/ncsi.txt`, `/redirect` (7 total); meta-refresh HTML redirect to `http://192.168.71.1/` |
| 15 | Android or iOS device connecting to GNSS-Setup SoftAP receives automatic OS-level 'Sign in to network' notification | HUMAN_NEEDED | Requires physical hardware flash and mobile device; cannot verify OS captive portal detection behavior programmatically |

**Score:** 14/15 truths verified (1 deferred to human verification per Plan 17-04 Task 3 design)

---

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/ntrip_client.rs` | NTRIP client module with spawn_ntrip_client, NTRIP_STATE, NVS load/save, base64 encoder, session loop | VERIFIED | File exists, 521 lines; all exported symbols present; substantive implementation throughout |
| `src/gnss.rs` | spawn_gnss returns Arc<UartDriver<'static>> as 5th return value | VERIFIED | Signature at line 130; `uart_for_ntrip` cloned at line 438; returned at line 482 |
| `src/main.rs` | ntrip_config channel, spawn_gnss destructure updated, spawn_ntrip_client called, mod ntrip_client declared | VERIFIED | `mod ntrip_client` at line 47; `uart_arc` destructured; channel at line 212; spawn at line 350 |
| `src/mqtt.rs` | ntrip_config_tx dispatch, /ntrip/config subscription, ntrip field in heartbeat JSON | VERIFIED | All three present: dispatch line 111, subscription line 217, heartbeat lines 393-399 |
| `src/log_relay.rs` | sync_channel capacity 128; ANSI strip on C-path strings | VERIFIED | Capacity 128 at line 179; `strip_ansi` function and call site confirmed |
| `src/provisioning.rs` | DNS hijack UDP server thread; probe URL handlers in EspHttpServer | VERIFIED | `UdpSocket` import at line 12; DNS thread binding port 53; 7 probe URL handlers registered |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `ntrip_client.rs` | `gnss.rs UartDriver` | `Arc<UartDriver>` clone passed into `spawn_ntrip_client` | WIRED | `Arc::clone(&uart_arc)` in `main.rs` line 350; `uart.write(&buf[..n])` in session loop |
| `ntrip_client.rs` | NTRIP caster TCP | `TcpStream::connect` | WIRED | Line 334: `TcpStream::connect(&addr)?` |
| `ntrip_client.rs` | UM980 UART | `uart.write(&buf[..n])` | WIRED | Line 382: `uart.write(&buf[..n])` inside `Ok(n)` arm of streaming loop |
| `mqtt.rs` mqtt_connect callback | ntrip_config_tx SyncSender | `t.ends_with("/ntrip/config") -> try_send` | WIRED | Lines 111-118; dispatched before `/config` branch to prevent routing collision |
| `mqtt.rs` subscriber_loop | `gnss/{device_id}/ntrip/config` | `client.subscribe(ntrip_config_topic, QoS::AtLeastOnce)` | WIRED | Lines 216-219; fires on every `Connected` event |
| `mqtt.rs` heartbeat_loop | `crate::ntrip_client::NTRIP_STATE` | `NTRIP_STATE.load(Ordering::Relaxed)` | WIRED | Lines 393-399 |
| `log_relay.rs` rust_log_try_send | ANSI strip before tx.try_send | `strip_ansi(s)` applied before channel send | WIRED | Line 128; function defined at line 139 |
| `gnss.rs` NmeaLine completion | config re-apply trigger | `sentence_type == "devicename"` detection | WIRED | Lines 252-263; `reboot_tx.try_send(())` fires on detection |
| `provisioning.rs` DNS thread | 0.0.0.0:53 UDP socket | `UdpSocket::bind` | WIRED | Line 294: `UdpSocket::bind("0.0.0.0:53")` |
| `provisioning.rs` EspHttpServer | probe URL handlers | `server.fn_handler("/generate_204", ...)` | WIRED | Line 256; `/hotspot-detect.html` line 262; 7 handlers total |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| NTRIP-01 | Plan 17-01 | Device connects to configured NTRIP caster and streams RTCM3 corrections to UM980 UART | SATISFIED | `run_ntrip_session`: TCP connect, ICY 200 OK validation, `uart.write(&buf[..n])` in streaming loop |
| NTRIP-02 | Plan 17-02 | NTRIP settings configurable via retained MQTT topic `gnss/{device_id}/ntrip/config` | SATISFIED | `mqtt.rs` dispatches `/ntrip/config` payloads; subscriber subscribes at QoS::AtLeastOnce; NVS saves config for reboot persistence |
| NTRIP-03 | Plan 17-01 | NTRIP client reconnects automatically on connection loss | SATISFIED | Exponential backoff loop (5/10/20/40s) in `spawn_ntrip_client`; `NTRIP_STATE` reset to 0 on every error path; retry is unconditional |
| NTRIP-04 | Plan 17-02 | NTRIP connection state included in health heartbeat | SATISFIED | `heartbeat_loop` reads `NTRIP_STATE` atomic and includes `"ntrip":"connected"/"disconnected"` in every heartbeat JSON |

All 4 NTRIP requirements are satisfied. No orphaned requirements detected.

---

## Additional Improvements (Plans 17-03 and 17-04)

These are not in the NTRIP-xx requirement IDs but are in scope for Phase 17:

| Improvement | Plan | Status |
|-------------|------|--------|
| Log channel capacity 32 -> 128 to prevent boot burst drops | 17-03 | VERIFIED |
| ANSI strip on C vprintf-path log strings before MQTT publish | 17-03 | VERIFIED |
| MQTT Subscribed/Published ACK events handled without warn! noise | 17-03 | VERIFIED |
| UM980 reboot detection via $devicename banner in GNSS RX thread | 17-03 | VERIFIED |
| Captive portal DNS hijack UDP server on port 53 | 17-04 | VERIFIED (code); HUMAN_NEEDED (hardware) |
| OS captive portal probe URL handlers (Android/iOS/Windows) | 17-04 | VERIFIED (code); HUMAN_NEEDED (hardware) |

**Note on UM980 reboot re-apply:** The reboot monitor uses a warning fallback rather than automatic NVS-backed config re-apply. This is a documented deviation in Plan 17-03: `config_relay` receives commands from the MQTT channel at runtime and does not persist them to NVS, making a direct re-apply impossible without significant refactoring. The detection path (the primary value) is fully implemented. The operator is directed via a prominent log warning to re-send the UM980 config via MQTT after detection.

---

## Anti-Patterns Found

No blockers or warnings found. Scanned files: `src/ntrip_client.rs`, `src/gnss.rs`, `src/mqtt.rs`, `src/main.rs`, `src/log_relay.rs`, `src/provisioning.rs`.

- No TODO/FIXME/PLACEHOLDER comments in delivered code
- No empty implementations (`return null`, `return {}`, no-op handlers)
- The `KNOWN-RACE` comment in `ntrip_client.rs` is a documented and accepted design decision (concurrent UART writes between GNSS TX thread and NTRIP thread), not an incomplete implementation
- `cargo build --release` exits 0 with no errors

---

## Human Verification Required

### 1. Captive Portal Detection on Mobile Device

**Test:** Flash firmware with `cargo espflash flash --release --monitor`. On an Android or iOS device, scan for WiFi networks and connect to "GNSS-Setup".

**Expected:**
- Android: A notification appears automatically saying "Sign in to GNSS-Setup" (or similar); tapping it opens the provisioning form
- iOS: A captive portal sheet appears automatically showing the provisioning web page
- Serial monitor shows: "DNS hijack: listening on UDP port 53" and "DNS hijack started"
- Optional laptop check: `nslookup example.com 192.168.71.1` should return 192.168.71.1

**Why human:** OS-level captive portal detection depends on the mobile OS making HTTP probe requests, receiving DNS responses, and deciding to show the captive portal UI. This behavior varies by OS version, device, and network state. It cannot be verified by code inspection or build checks alone. This checkpoint was explicitly marked as a hardware gate in Plan 17-04 Task 3 and deferred to end-of-milestone per the plan's design.

---

## Gaps Summary

No gaps blocking goal achievement. All NTRIP-01 through NTRIP-04 requirements are fully implemented in the compiled firmware. The single human verification item (captive portal prompt on mobile device) is a hardware acceptance test deferred by design, not a code gap.

The firmware binary was produced by `cargo build --release` with no errors. All key integration points have been verified against the actual codebase, not the SUMMARY claims.

---

_Verified: 2026-03-09_
_Verifier: Claude (gsd-verifier)_
