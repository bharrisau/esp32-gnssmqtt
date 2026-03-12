---
phase: 22-workspace-nostd-audit
plan: "02"
subsystem: infra
tags: [nostd, esp-hal, embassy, audit, nvs, ota, tls, mqtt, ntrip]

requires:
  - phase: 22-workspace-nostd-audit
    provides: "Workspace restructure establishing Cargo workspace with firmware/ and server members"

provides:
  - "docs/nostd-audit.md — complete ESP-IDF dependency audit covering all 12 categories with gap priority ranking and implementation notes"

affects:
  - 23-mqtt-rtcm3-nvs
  - 24-rinex-ota
  - 25-webui-gap-skeletons

tech-stack:
  added: []
  patterns:
    - "Audit document captures ecosystem decisions to avoid relitigating in later phases"
    - "New crates must be ecosystem-reusable, not project-specific"
    - "Log-based KV store (sequential-storage) as NVS replacement pattern"
    - "rustls unbuffered API with cert-hash pinning as TLS approach for embedded"

key-files:
  created:
    - docs/nostd-audit.md
  modified: []

key-decisions:
  - "NVS: log-based KV store approach; sequential-storage likely implements this; crates must be ecosystem-reusable (no gnss-specific naming)"
  - "OTA target is esp-hal (not pure no_std); willing to contribute to esp-hal-ota if ESP32-C6 is untested"
  - "NTRIP TLS preferred path: rustls unbuffered API with cert-hash pinning sent in config payload; alternative is RTCM-over-MQTT"
  - "HTTP server candidates: picoserve (primary) and nanofish (smaller, client+server); evaluate for size tradeoff"
  - "MQTT client: benchmark in Phase 23, implement in Phase 24"
  - "SoftAP SSID: GNSS-[ID] with same value as WPA2 PSK password"
  - "UM980 reset: reset:true field in /config payload; also triggered on first config apply after device reboot"
  - "SoftAP portal: perform WiFi station scan to display nearby SSIDs as dropdown"

patterns-established:
  - "Audit document is living reference; corrections applied via human review then committed"

requirements-completed:
  - NOSTD-01

duration: 2min
completed: 2026-03-12
---

# Phase 22 Plan 02: Nostd Audit Summary

**Complete ESP-IDF dependency audit across 12 categories with human-reviewed corrections for NVS (log-based KV), OTA (esp-hal target), TLS (rustls+cert-hash), and three new implementation notes for SoftAP portal**

## Performance

- **Duration:** ~2 min (continuation task — human review already done)
- **Started:** 2026-03-12T03:12:57Z
- **Completed:** 2026-03-12T03:15:00Z
- **Tasks:** 2 (Task 1 done in prior session; Task 2 = human review + corrections applied here)
- **Files modified:** 1

## Accomplishments

- Human review corrections applied to docs/nostd-audit.md covering NVS strategy, OTA target clarification, NTRIP TLS options, HTTP server candidates, and MQTT phase assignment
- Three new implementation notes added: WiFi scan for SoftAP portal, UM980 reset on config apply, SoftAP SSID format
- Gap priority table updated with MQTT row marked "BENCHMARK PHASE 23 / IMPL PHASE 24"
- All changes committed as single atomic revision of the audit document

## Task Commits

Each task was committed atomically:

1. **Task 1: Write docs/nostd-audit.md from research audit tables** — `114f4dc` (docs)
2. **Task 2: Apply human review corrections** — `ec954cf` (docs)

## Files Created/Modified

- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/docs/nostd-audit.md` — Complete ESP-IDF dependency audit with all human review corrections applied; 12 categories, gap priority table, implementation notes

## Decisions Made

- **NVS**: Log-based KV store approach preferred; `sequential-storage` likely already implements this via append-only records. Implementation can use sequential-storage directly or wrap it in an ecosystem-reusable crate. Crate names must be generic (not firmware-project-specific).
- **OTA**: Target is esp-hal (not pure no_std). There may be intermediate approaches using ROM/IDF calls. HTTP client evaluation must consider the esp-hal ecosystem specifically. Willing to contribute to esp-hal-ota if ESP32-C6 is untested.
- **NTRIP TLS**: Two options. Preferred: send trusted cert hash in NTRIP config payload; use rustls unbuffered API with cert pinning. Alternative: drop NTRIP and receive RTCM corrections via MQTT. Rustls is the primary library to evaluate.
- **HTTP server**: picoserve looks suitable. Also note nanofish (both client and server, potentially smaller). Evaluate size tradeoff when implementing.
- **MQTT**: Move benchmarks to Phase 23, implementation to Phase 24. Gap priority table updated accordingly.

## Deviations from Plan

### Human Review Corrections Applied

The human review (Task 2) identified corrections and additions not present in the initial document. All were applied:

1. NVS section updated with log-based KV store strategy and ecosystem-reusable crate naming guidance
2. OTA section updated with esp-hal target clarification and contribution willingness
3. TLS section restructured with two options and rustls as primary library; cert-hash pinning as preferred approach
4. HTTP server section updated with nanofish as second candidate
5. MQTT row in gap priority table updated to "BENCHMARK PHASE 23 / IMPL PHASE 24"
6. New "Implementation Notes" section added with three items: WiFi scan UX, UM980 reset semantics, SoftAP SSID format

---

**Total deviations:** 0 auto-fix rule deviations; all changes were the intended work of Task 2 (human review application)
**Impact on plan:** All corrections aligned with plan intent; document now more accurate and actionable

## Issues Encountered

None — human review corrections were clear and unambiguous.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- docs/nostd-audit.md is committed and authoritative
- Phase 23 can begin: MQTT benchmarking, RTCM3 server, gnss-nvs crate with sequential-storage validation on ESP32-C6
- Key Phase 23 inputs from this audit: NVS = sequential-storage; evaluate rustls for NTRIP TLS; MQTT client benchmark required before committing

## Gap Count by Status

From the audit document:

| Status | Count (approx) |
|--------|---------------|
| SOLVABLE | 12 |
| RESOLVED | 3 |
| GAP | 10 |
| REPLACED | 1 |
| UNKNOWN | 1 |

Total usage entries: ~27 across 12 categories.

---
*Phase: 22-workspace-nostd-audit*
*Completed: 2026-03-12*
