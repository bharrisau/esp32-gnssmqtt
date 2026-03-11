---
phase: 20-field-testing-fixes
plan: 01
subsystem: provisioning
tags: [captive-portal, softap, windows, ios, android, http]

# Dependency graph
requires:
  - phase: 19-pre-2-0-bugfix
    provides: SoftAP DNS hijack (all hostnames resolve to 192.168.71.1); HTTP server on port 80
provides:
  - Exact-body probe handlers for Windows msftconnecttest (/connecttest.txt) and Windows ncsi (/ncsi.txt)
  - Exact-body probe handler for iOS captive.apple.com (/hotspot-detect.html)
  - Android /generate_204 and /connectivitycheck 302 redirects unchanged
affects: [20-02, 20-03, 20-04, testing]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "OS captive portal probes require exact response bodies — redirects cause silent failure on Windows and iOS"
    - "req.into_ok_response()?.write_all(b\"exact body\") pattern for static probe responses"

key-files:
  created: []
  modified:
    - src/provisioning.rs

key-decisions:
  - "Windows 10/11 /connecttest.txt must return exactly 'Microsoft Connect Test' (no trailing newline, 200 OK) — redirect_html caused OS to skip captive notification"
  - "Windows older /ncsi.txt must return exactly 'Microsoft NCSI' (200 OK) — same issue"
  - "iOS /hotspot-detect.html must return exact '<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>' (200 OK) — any redirect causes iOS to miss captive detection"
  - "IOS_SUCCESS_HTML defined as module-local const in run_softap_portal(); non-move closure used since &[u8] is 'static"

patterns-established:
  - "Captive portal probe handlers: OS-specific exact bodies for Windows/iOS; 302 redirect preserved for Android"

requirements-completed:
  - BUG-5

# Metrics
duration: 15min
completed: 2026-03-11
---

# Phase 20 Plan 01: Windows and iOS Captive Portal Probe Fix Summary

**Replaced incorrect redirect responses with OS-exact probe bodies so Windows 10/11 and iOS trigger captive portal notifications on SoftAP connect**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-11T13:20:42Z
- **Completed:** 2026-03-11T13:35:00Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- Added `/connecttest.txt` handler returning `Microsoft Connect Test` (200 OK) — Windows 10/11 captive detection
- Fixed `/ncsi.txt` handler to return `Microsoft NCSI` (200 OK) instead of redirect_html — older Windows captive detection
- Fixed `/hotspot-detect.html` handler to return exact Apple success HTML (200 OK) instead of redirect_html — iOS captive detection
- Android probes (`/generate_204`, `/connectivitycheck`) preserved as 302 redirects — unchanged behavior
- `cargo clippy -- -D warnings` passes clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix iOS and Windows captive portal probe handlers** - `b586b49` (fix)

**Plan metadata:** (docs commit follows)

## Files Created/Modified
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/provisioning.rs` - Added /connecttest.txt handler; fixed /ncsi.txt and /hotspot-detect.html to return OS-exact bodies; updated comment block

## Decisions Made
- Windows 10/11 /connecttest.txt returns `b"Microsoft Connect Test"` with non-move closure (static byte literal, no capture needed)
- Windows /ncsi.txt returns `b"Microsoft NCSI"` with non-move closure
- iOS /hotspot-detect.html uses local `const IOS_SUCCESS_HTML: &[u8]` for clarity
- `/success.html` and `/library/test/success.html` left unchanged with meta-refresh redirect (not OS-critical probes)

## Deviations from Plan

None - plan executed exactly as written.

Note: During clippy verification, stale incremental build cache showed false positives from unrelated pre-existing uncommitted code (partial 20-03 work in working tree). A `cargo clean` + fresh clippy confirmed the build and all checks pass clean.

## Issues Encountered
- Stale incremental cargo cache showed false clippy errors after our changes. `cargo clean` resolved this — the working tree was correct. The pre-existing uncommitted 20-03 partial work (config_relay.rs + main.rs) was already consistent and compiling.

## Next Phase Readiness
- 20-01 complete; Windows and iOS captive portal detection fixed
- 20-02 through 20-04 plans ready to execute in sequence
- Hardware verification (flash to FFFEB5, test Windows 10/11 and iPhone connection to GNSS-Setup) deferred to end-of-milestone sign-off

---
*Phase: 20-field-testing-fixes*
*Completed: 2026-03-11*
