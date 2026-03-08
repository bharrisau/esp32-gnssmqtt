---
phase: 17-ntrip-client
plan: 04
subsystem: provisioning
tags: [captive-portal, dns, udp, softap, android, ios, esp32]

# Dependency graph
requires:
  - phase: 17-02
    provides: run_softap_portal HTTP server with GET / and POST /save handlers

provides:
  - DNS hijack UDP server on port 53 responding with 192.168.71.1 to all A queries
  - Captive portal probe URL handlers for Android, iOS, Windows in EspHttpServer
  - Automatic OS-level "Sign in to network" prompt on GNSS-Setup SoftAP connection

affects:
  - provisioning
  - softap

# Tech tracking
tech-stack:
  added: [std::net::UdpSocket (ESP-IDF lwIP)]
  patterns:
    - DNS hijack via raw UDP packet construction (RFC 1035 minimal A-record response)
    - Captive portal probe URL detection via HTTP meta-refresh redirect handlers
    - DNS server thread with 2s read timeout for idle-safe blocking loop

key-files:
  created: []
  modified: [src/provisioning.rs]

key-decisions:
  - "DNS thread intentionally not stopped before 300s timeout: esp_restart() terminates all threads; 1s reboot delay is sufficient for browser to receive HTTP 200"
  - "Meta-refresh HTML used for probe URL redirect (not HTTP 302 into_response) — consistent with existing into_ok_response() handler style in EspHttpServer"
  - "/library/test/success.html registered with warn fallback (path may exceed EspHttpServer limit) — core probes /generate_204 and /hotspot-detect.html are non-fallback"
  - "DNS thread stack_size 4096: minimal viable for UDP recv loop with Vec allocation on stack"
  - "QR bit and QDCOUNT checks added: ignore DNS responses and empty queries to avoid malformed reply loops"

patterns-established:
  - "Captive portal DNS hijack: UdpSocket::bind(0.0.0.0:53) in spawned thread, 2s read_timeout, RFC 1035 response built from query bytes with pointer 0xC00C"

requirements-completed: []

# Metrics
duration: 1min
completed: 2026-03-08
---

# Phase 17 Plan 04: Captive Portal DNS Hijack and Probe URL Handlers Summary

**DNS hijack UDP server (port 53, all queries -> 192.168.71.1) and OS captive portal probe URL handlers added to run_softap_portal() — firmware compiles; hardware verification pending**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-03-08T14:21:52Z
- **Completed:** 2026-03-08T14:23:31Z (automated tasks)
- **Tasks:** 2 of 3 automated tasks complete; Task 3 is hardware checkpoint
- **Files modified:** 1

## Accomplishments

- Added 7 OS captive portal probe URL handlers to EspHttpServer (Android /generate_204 and /connectivitycheck, iOS /hotspot-detect.html and /success.html and /library/test/success.html, Windows /ncsi.txt, generic /redirect)
- Added DNS hijack UDP server thread: binds 0.0.0.0:53, answers all A queries with 192.168.71.1, uses RFC 1035 minimal response with NAME pointer, 30s TTL
- cargo build --release and cargo clippy both exit 0 with no errors or warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Add probe URL redirect handlers to EspHttpServer** - `3e3d30f` (feat)
2. **Task 2: DNS hijack UDP server thread inside run_softap_portal** - `8b1775e` (feat)
3. **Task 3: Hardware verification** - DEFERRED to end of milestone (not failed)

## Files Created/Modified

- `src/provisioning.rs` - Added UdpSocket import, 7 probe URL handlers, DNS hijack thread with RFC 1035 response builder

## Decisions Made

- Used meta-refresh HTML redirect (200 OK with `<meta http-equiv='refresh'>`) for probe URL handlers rather than HTTP 302, to match the existing `into_ok_response()` handler style already in the file
- DNS thread stack_size 4096 — sufficient for the UDP recv loop and Vec-based response building; consistent with other minimal utility threads in this project
- `/library/test/success.html` registered with `if let Err` fallback warning rather than `?` propagation, in case the path length exceeds EspHttpServer's limit
- QR bit check (`buf[2] & 0x80 != 0`) added to ignore DNS response packets; QDCOUNT check added to skip empty queries — both prevent malformed reply loops

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None — both tasks compiled cleanly on first attempt.

## User Setup Required

None - no external service configuration required.

## Deferred Verification

**Task 3: Hardware verification — captive portal detection on mobile device**

- **Status:** DEFERRED (not failed) — hardware SoftAP test deferred to end of milestone
- **What to verify:** Flash firmware, connect Android or iOS device to "GNSS-Setup" AP, confirm OS shows automatic "Sign in to network" prompt without manual navigation
- **Verification steps:**
  1. `cargo espflash flash --release --monitor`
  2. Connect mobile device to "GNSS-Setup" SSID
  3. Expected (Android): "Sign in to GNSS-Setup" notification appears automatically
  4. Expected (iOS): captive portal sheet displays automatically
  5. Check serial logs for: "DNS hijack: listening on UDP port 53" and "DNS hijack started"
  6. Optional: `nslookup example.com 192.168.71.1` should return 192.168.71.1

## Next Phase Readiness

- Captive portal DNS hijack and probe URL handler code is implemented and compiles clean
- Hardware verification (Task 3) deferred to end of milestone — will be validated alongside Phase 18 hardware sign-off
- provisioning.rs is complete for Phase 17 scope; no further changes needed before Phase 18

## Self-Check: PASSED

- FOUND: src/provisioning.rs
- FOUND: commit 3e3d30f (Task 1)
- FOUND: commit 8b1775e (Task 2)

---
*Phase: 17-ntrip-client*
*Completed: 2026-03-08*
