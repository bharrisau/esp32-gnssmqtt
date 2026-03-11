---
phase: 20-field-testing-fixes
verified: 2026-03-11T14:00:00Z
status: human_needed
score: 9/9 must-haves verified
human_verification:
  - test: "Connect Windows 10/11 device to GNSS-Setup AP"
    expected: "Captive portal notification or 'Internet access' network status appears within 10 seconds"
    why_human: "Cannot verify OS-level captive portal detection logic without live hardware and a Windows machine"
  - test: "Connect iPhone to GNSS-Setup AP"
    expected: "'Sign in to network' captive portal notification appears"
    why_human: "iOS captive portal detection requires live hardware; cannot simulate iOS probe responses in test"
  - test: "Configure UM980 at 5 Hz, monitor MQTT /log topic for 60+ seconds"
    expected: "Throughput log shows ~40 msg/s every 2.5s; heartbeat nmea_drops stays 0"
    why_human: "GNSS hardware required; 5 Hz field test cannot be simulated statically"
  - test: "Send GNSS config via MQTT /config topic; power-cycle UM980 UART; watch /log"
    expected: "Log shows 'saved N bytes to NVS (gnss/gnss_config)' on config receive; 're-applying N bytes' within 2s of UM980 reboot banner"
    why_human: "NVS writes and UM980 reboot detection require live hardware"
  - test: "Factory-reset device (erase NVS), trigger UM980 reboot"
    expected: "Log shows 'no saved config in NVS — skipping re-apply'; no crash"
    why_human: "NVS state after factory reset requires live hardware to verify"
  - test: "Enter AUSCORS credentials (host=ntrip.data.gnss.ga.gov.au, port=443, tls=checked) via portal or MQTT; verify NTRIP TLS connection attempt in /log"
    expected: "Log shows 'NTRIP: TLS connecting to ntrip.data.gnss.ga.gov.au:443'; either connects or logs 'EspTls::new() failed (heap?)' with backoff"
    why_human: "TLS NTRIP requires live hardware and network access to AUSCORS"
---

# Phase 20: Field Testing Fixes Verification Report

**Phase Goal:** Fix bugs found during first field deployment of device FFFEB5 — Windows/iOS captive portal detection (BUG-5), MQTT throughput for 5 Hz GNSS output (PERF-1), UM980 config persistence and auto-reapply on reset (FEAT-2), and TLS NTRIP client for AUSCORS port 443 (FEAT-3)
**Verified:** 2026-03-11T14:00:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Windows 10/11 `GET /connecttest.txt` returns `Microsoft Connect Test` (200 OK, exact body) | VERIFIED | `provisioning.rs` line 318-320: `server.fn_handler("/connecttest.txt", ..., \|req\| { req.into_ok_response()?.write_all(b"Microsoft Connect Test") })` |
| 2 | Windows older `GET /ncsi.txt` returns `Microsoft NCSI` (200 OK, exact body) | VERIFIED | `provisioning.rs` line 321-323: `server.fn_handler("/ncsi.txt", ..., \|req\| { req.into_ok_response()?.write_all(b"Microsoft NCSI") })` |
| 3 | iOS `GET /hotspot-detect.html` returns exact Apple success HTML (200 OK, not redirect) | VERIFIED | `provisioning.rs` line 307-309: `const IOS_SUCCESS_HTML: &[u8] = b"<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>"` used in handler |
| 4 | Android probes still return 302 redirect | VERIFIED | `provisioning.rs` lines 299-306: `/generate_204` and `/connectivitycheck` return `302 Found` with `Location: http://192.168.71.1/` |
| 5 | NMEA channel capacity is 128 | VERIFIED | `gnss.rs` line 151: `mpsc::sync_channel::<(String, String)>(128)` with comment "at 5 Hz x 8 sentence types = 40 sentences/sec" |
| 6 | Throughput diagnostic log appears every 100 sentences | VERIFIED | `nmea_relay.rs` lines 43-44, 64-71: `sentence_count: u64`, `throughput_tick: Instant`, `if sentence_count % 100 == 0` logs `msg/s` |
| 7 | MQTT outbox expiry set to 5000ms | VERIFIED | `sdkconfig.defaults` line 58: `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000` |
| 8 | After receiving GNSS config via MQTT, payload saved to NVS `gnss/gnss_config` blob | VERIFIED | `config_relay.rs` lines 69-70: `save_gnss_config(&payload, &nvs_for_relay)` called after `apply_config`; `save_gnss_config()` calls `nvs.set_blob("gnss_config", payload)` |
| 9 | UM980 reboot detection reads NVS blob and calls `apply_config()`; skips silently when no config saved | VERIFIED | `main.rs` lines 344-365: `EspNvs::new(..., "gnss", false)` + `nvs.get_blob("gnss_config", &mut buf)` + `config_relay::apply_config(data, &gnss_cmd_for_reboot)`; `Ok(None)` branch logs "no saved config" |
| 10 | `NtripConfig.tls` field; EspTls session path dispatched when `tls=true` | VERIFIED | `ntrip_client.rs` lines 58, 353-357: `tls: bool` field; `run_ntrip_session()` dispatches to `run_ntrip_session_tls()` when `config.tls` |
| 11 | `load_ntrip_config` reads `ntrip_tls` NVS key; `save_ntrip_config` writes it | VERIFIED | `ntrip_client.rs` line 122: `config.tls = nvs.get_u8("ntrip_tls")...`; lines 160-162: `nvs.set_u8("ntrip_tls", ...)` |
| 12 | Portal form includes NTRIP section with TLS checkbox; form POST saves `ntrip_tls` to NVS | VERIFIED | `provisioning.rs` lines 34-40: NTRIP HTML section with `<input name="ntrip_tls" type="checkbox" value="1">`; lines 222-227, 258-271: parse and call `save_ntrip_credentials()` which writes `ntrip_tls` NVS key |
| 13 | `read_ntrip_headers` accepts `HTTP/1.1 200` and `HTTP/1.0 200` in addition to `ICY 200 OK` | VERIFIED | `ntrip_client.rs` lines 329-331: `ok` computed with all three prefix checks |
| 14 | `EspTls::new()` failure caught and returned as `Err` (not panic); triggers backoff | VERIFIED | `ntrip_client.rs` lines 462-466: `EspTls::new().map_err(...)` wraps error and `?` propagates to caller; caller at line 667-670 applies backoff |

**Score:** 14/14 truths verified (all automated checks pass; 6 items require hardware for full confidence)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/provisioning.rs` | Exact-body probe handlers for Windows msftconnecttest, Windows ncsi, iOS hotspot-detect; NTRIP form section with ntrip_tls | VERIFIED | `/connecttest.txt` returns `b"Microsoft Connect Test"`; `/ncsi.txt` returns `b"Microsoft NCSI"`; `/hotspot-detect.html` returns `IOS_SUCCESS_HTML`; NTRIP form and `save_ntrip_credentials()` present |
| `src/gnss.rs` | NMEA channel capacity 128 | VERIFIED | `sync_channel::<(String, String)>(128)` at line 151 |
| `src/nmea_relay.rs` | Per-100-sentences throughput log | VERIFIED | `sentence_count` and `throughput_tick` initialized before loop; `% 100 == 0` log at lines 65-71 |
| `sdkconfig.defaults` | `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000` | VERIFIED | Present at line 58 |
| `src/config_relay.rs` | `nvs_partition` param; `set_blob("gnss_config")`; `apply_config()` pub | VERIFIED | Signature at line 32 includes `nvs_partition: EspNvsPartition<NvsDefault>`; `save_gnss_config()` calls `set_blob`; `apply_config` declared `pub` at line 129 |
| `src/main.rs` | UM980 reboot monitor reads NVS `gnss_config` blob and calls `config_relay::apply_config()` | VERIFIED | Lines 344-365: NVS read + apply_config call; silent skip on `Ok(None)`; stack 8192 |
| `src/ntrip_client.rs` | `NtripConfig.tls` field; `EspTls` session path; `read_ntrip_headers` extended; `extract_json_bool` | VERIFIED | All present at lines 58, 353-525, 329-331, 233-246 respectively |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| DNS hijack (all hostnames → 192.168.71.1) | HTTP server probe handlers | All A-query DNS responses return 192.168.71.1; HTTP handlers on port 80 | VERIFIED | DNS hijack thread present at `provisioning.rs` line 333; `/connecttest.txt`, `/ncsi.txt`, `/hotspot-detect.html` handlers registered |
| `config_relay::spawn_config_relay` | NVS namespace "gnss" key "gnss_config" | `save_gnss_config()` called after `apply_config` | VERIFIED | `config_relay.rs` line 70: `save_gnss_config(&payload, &nvs_for_relay)` after `apply_config` |
| main.rs UM980 reboot monitor | `config_relay::apply_config()` | `get_blob("gnss_config")` then call pub fn | VERIFIED | `main.rs` lines 348-351: `nvs.get_blob("gnss_config", &mut buf)` → `config_relay::apply_config(data, ...)` |
| `NtripConfig.tls` | `run_ntrip_session` TLS branch | `if config.tls { run_ntrip_session_tls } else { run_ntrip_session_tcp }` | VERIFIED | `ntrip_client.rs` lines 353-357 |
| provisioning.rs NTRIP form | NVS `ntrip_tls` key | `parse_form_field("ntrip_tls")` → `save_ntrip_credentials()` → `nvs.set_u8("ntrip_tls", ...)` | VERIFIED | `provisioning.rs` line 227: `ntrip_tls` parsed; line 506: `nvs.set_u8("ntrip_tls", ...)` |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| BUG-5 | 20-01 | Windows/iOS captive portal detection fails — OS probes return wrong response body | SATISFIED | `/connecttest.txt`, `/ncsi.txt`, `/hotspot-detect.html` all return OS-exact bodies; hardware verification deferred |
| PERF-1 | 20-02 | MQTT throughput insufficient for 5 Hz GNSS output — drops at high sentence rates | SATISFIED | Channel 64→128; throughput log added; MQTT outbox 5s timeout configured; field test at 5 Hz deferred |
| FEAT-2 | 20-03 | UM980 GNSS config lost on power cycle — no auto-reapply after reset | SATISFIED | NVS blob save in `config_relay.rs`; reboot monitor with NVS read + `apply_config()` in `main.rs`; hardware verification deferred |
| FEAT-3 | 20-04 | NTRIP client cannot connect to AUSCORS port 443 (requires TLS) | SATISFIED (code complete) | `NtripConfig.tls`; `EspTls` session path; portal NTRIP form with TLS checkbox; heap feasibility requires hardware validation |

Note: BUG-5, PERF-1, FEAT-2, FEAT-3 are not present in `.planning/REQUIREMENTS.md` — they are phase-level bug/feature IDs tracked in PLAN frontmatter, separate from the milestone requirement IDs (HARD-xx, PROV-xx, etc.). The REQUIREMENTS.md traceability table covers only v1.3/v2.0 milestone requirements; phase 20 items are field-deployment fixes added post-milestone. No orphaned requirement IDs were found.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | — | — | No anti-patterns found in modified files |

All modified files (`provisioning.rs`, `gnss.rs`, `nmea_relay.rs`, `sdkconfig.defaults`, `config_relay.rs`, `main.rs`, `ntrip_client.rs`) were scanned for TODO/FIXME/placeholder comments, empty return statements, and stub implementations. The old stub `"automatic config re-apply not yet implemented"` in main.rs has been replaced with the full NVS read + re-apply logic. No stub patterns remain.

### Human Verification Required

### 1. Windows Captive Portal Detection

**Test:** Connect a Windows 10 or Windows 11 device to the GNSS-Setup SoftAP
**Expected:** Captive portal notification appears in the system tray, or the network icon shows "Internet access" status, or a browser window opens automatically
**Why human:** OS-level captive detection requires the live Windows network stack responding to firmware probes; cannot be verified statically

### 2. iOS Captive Portal Detection

**Test:** Connect an iPhone to the GNSS-Setup SoftAP
**Expected:** "Sign in to network" notification appears; tapping opens the captive portal page at 192.168.71.1
**Why human:** iOS captive portal detection requires the live iOS network stack; cannot be simulated

### 3. 5 Hz NMEA Throughput Under Load

**Test:** Configure UM980 for 5 Hz output (e.g. `GPGGA 0.2`), flash firmware to FFFEB5, monitor the `/log` MQTT topic for 60+ seconds
**Expected:** Throughput log messages appear approximately every 2.5 seconds showing ~40 msg/s; the heartbeat `nmea_drops` field remains 0 after 60s of continuous output
**Why human:** GNSS hardware required; rate-dependent behavior cannot be verified from source code alone

### 4. UM980 Config NVS Persistence and Re-apply

**Test:** Send a GNSS config payload via MQTT `/config` topic; power-cycle the UM980 UART; watch the `/log` topic
**Expected:** Log shows "Config relay: saved N bytes to NVS (gnss/gnss_config)" after config receive; within 2 seconds of UM980 `$devicename` banner, log shows "UM980 reboot monitor: re-applying N bytes of saved config" followed by "config re-apply complete"
**Why human:** NVS writes and UM980 reboot detection require live hardware with a configurable UM980

### 5. First Boot / No NVS Config (Factory Reset Edge Case)

**Test:** Erase flash NVS partition; trigger UM980 reboot (UART power-cycle or hard reset)
**Expected:** Log shows "UM980 reboot monitor: 'gnss' NVS namespace not found — no config to re-apply" or "no saved config in NVS — skipping re-apply"; no panic, no crash
**Why human:** NVS absence state requires hardware with erased NVS

### 6. AUSCORS TLS NTRIP Connection

**Test:** Enter AUSCORS credentials (host=ntrip.data.gnss.ga.gov.au, port=443, tls=checked) via the SoftAP portal or MQTT `/ntrip/config` topic; observe /log
**Expected:** Log shows "NTRIP: TLS connecting to ntrip.data.gnss.ga.gov.au:443 mount=..."; either TLS handshake succeeds and RTCM streaming begins, or heap failure is logged with "EspTls::new() failed (heap?)" and exponential backoff proceeds (no panic)
**Why human:** TLS connection requires live hardware with network access; heap feasibility at runtime is device-specific

## Overall Assessment

All 14 observable truths are verified at the code level. Every artifact specified in the PLAN frontmatter `must_haves` exists, is substantive (not a stub), and is correctly wired. The previous "automatic config re-apply not yet implemented" stub in `main.rs` has been fully replaced. All four requirement IDs (BUG-5, PERF-1, FEAT-2, FEAT-3) have implementation evidence in the codebase.

The `human_needed` status reflects that all six hardware tests remain pending — these were explicitly deferred to end-of-milestone sign-off (see 20-01-SUMMARY.md and 20-04-SUMMARY.md). The firmware changes are code-complete and build-clean.

---

_Verified: 2026-03-11T14:00:00Z_
_Verifier: Claude (gsd-verifier)_
