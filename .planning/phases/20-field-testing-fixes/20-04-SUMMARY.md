---
phase: 20-field-testing-fixes
plan: "04"
subsystem: ntrip
tags: [esp32, ntrip, tls, esptls, mbedtls, softap, provisioning, nvs]

# Dependency graph
requires:
  - phase: 20-01
    provides: provisioning.rs post BUG-1 DNS fix (file ownership pre-condition)
provides:
  - NtripConfig.tls field with EspTls session path in ntrip_client.rs
  - SoftAP portal NTRIP section with TLS checkbox in provisioning.rs
  - ntrip_tls NVS key (u8 0/1) readable by load_ntrip_config
affects:
  - hardware-testing (AUSCORS TLS connection verification)

# Tech tracking
tech-stack:
  added: [esp_idf_svc::tls::EspTls, esp_idf_svc::tls::Config, esp_idf_svc::tls::InternalSocket]
  patterns:
    - TCP/TLS session dispatch via config.tls bool in run_ntrip_session
    - EspTls::new() heap failure caught and propagated as std::io::Error for backoff compatibility
    - extract_json_bool helper for JSON true/1 and false/0 parsing (no serde)
    - HTTP/1.1 with Host header for TLS connections to port 443 virtual hosts

key-files:
  created: []
  modified:
    - src/ntrip_client.rs
    - src/provisioning.rs

key-decisions:
  - "run_ntrip_session dispatches to run_ntrip_session_tcp or run_ntrip_session_tls based on config.tls — no trait objects, no generics; clean split avoids read_timeout unavailability on EspTls"
  - "EspTls read returns Result<usize, EspError> not std::io::Error — wrapped with map_err in session loop and header reader"
  - "read_ntrip_headers updated to accept ICY 200 OK, HTTP/1.1 200, HTTP/1.0 200 — AUSCORS uses standard HTTP response not NTRIP v1 ICY"
  - "build_ntrip_request_v11 sends HTTP/1.1 with Host header — required for virtual hosting on port 443"
  - "Portal NTRIP section save is non-fatal — WiFi/MQTT creds already committed before NTRIP save attempt"
  - "ntrip_tls NVS key uses same u8 0/1 convention as mqtt_tls — consistent schema across provisioning namespaces"

patterns-established:
  - "TCP/TLS session dispatch: single dispatcher fn, two concrete session fns (no generics)"
  - "EspTls integration: new() → connect() → write_all() → read() loop, all returning EspError, wrapped to std::io::Error at boundary"

requirements-completed: [FEAT-3]

# Metrics
duration: 25min
completed: 2026-03-11
---

# Phase 20 Plan 04: NTRIP TLS Support Summary

**Optional TLS session path for NTRIP client using EspTls (mbedTLS), with AUSCORS-compatible HTTP/1.1 headers and SoftAP portal NTRIP config section including TLS checkbox**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-03-11T13:44:00Z
- **Completed:** 2026-03-11T13:54:16Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- NtripConfig gains `tls: bool` field (default false); NVS, JSON parse, and session dispatch all updated
- EspTls session path with HTTP/1.1 request, CA bundle validation (use_crt_bundle_attach=true), and graceful heap-fail logging
- SoftAP portal form now includes NTRIP host/port/mountpoint/user/pass/TLS fields for field provisioning without MQTT
- `ntrip_tls` NVS key (u8 0/1) consistent with `mqtt_tls` convention, loadable by `load_ntrip_config`
- `read_ntrip_headers` now accepts ICY 200 OK, HTTP/1.1 200, and HTTP/1.0 200 — NTRIP v2 over HTTPS compatibility

## Task Commits

1. **Task 1: Add TLS support to ntrip_client.rs** - `9334c15` (feat)
2. **Task 2: Add NTRIP config section to SoftAP portal form** - `db2226d` (feat)

## Files Created/Modified

- `src/ntrip_client.rs` - NtripConfig.tls field; EspTls session path; extract_json_bool; HTTP/1.1 request builder; TLS header reader; updated ICY/HTTP response check
- `src/provisioning.rs` - NTRIP HTML form section; parse/save ntrip_* fields in POST handler; save_ntrip_credentials() function

## Decisions Made

- `run_ntrip_session` dispatches to `run_ntrip_session_tcp` or `run_ntrip_session_tls` based on `config.tls` — no trait objects or generics; clean split also avoids the issue that `set_read_timeout()` is unavailable on EspTls
- EspTls `read()` returns `Result<usize, EspError>` not `std::io::Error` — wrapped at the session loop boundary with `map_err` to preserve the existing error-handling contract
- `read_ntrip_headers` updated to accept `ICY 200 OK`, `HTTP/1.1 200`, and `HTTP/1.0 200` — AUSCORS uses standard HTTP (not NTRIP v1 ICY format)
- `build_ntrip_request_v11` sends HTTP/1.1 with a Host header — required for virtual hosting on port 443 (AUSCORS)
- Portal NTRIP credential save is non-fatal: WiFi/MQTT credentials are committed first; NTRIP save failure only logs a warning
- `ntrip_tls` NVS key uses u8 0/1 consistent with `mqtt_tls` — uniform schema across prov and ntrip namespaces

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. EspTls API matched the plan interfaces exactly. `use_crt_bundle_attach` is gated behind `#[cfg(esp_idf_mbedtls_certificate_bundle)]` in the Config struct but since mbedTLS is confirmed active (MQTT TLS works), this compiles as expected.

**Runtime heap note (to be verified in hardware testing):** EspTls::new() may return ESP_ERR_NO_MEM on heap-constrained builds. If this occurs on FFFEB5 hardware when connecting to AUSCORS (port 443), the log will show "EspTls::new() failed (heap?)". In that case, the recommended approach is a server-side MQTT relay that subscribes to AUSCORS over TLS and publishes RTCM frames to the device via MQTT — the device already receives RTCM from the NTRIP stream without firmware changes for the relay path.

## User Setup Required

None - no external service configuration required for the firmware changes. Hardware validation of AUSCORS TLS connection is tracked in `testing.md`.

## Next Phase Readiness

- TLS NTRIP path ready for hardware testing (AUSCORS ntrip.data.gnss.ga.gov.au:443)
- Portal NTRIP form visible after next flash to FFFEB5
- If heap is insufficient for EspTls: log guidance directs to MQTT relay path (post-v2.0 server-side work)

---
*Phase: 20-field-testing-fixes*
*Completed: 2026-03-11*
