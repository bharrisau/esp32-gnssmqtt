---
phase: 15
slug: provisioning
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo build --release (compile-time verification) |
| **Config file** | none — embedded firmware, no test runner |
| **Quick run command** | `cargo build --release 2>&1 | tail -5` |
| **Full suite command** | `cargo build --release` |
| **Estimated runtime** | ~60-120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 15-01-01 | 01 | 1 | PROV-01 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-01-02 | 01 | 1 | PROV-02 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-02-01 | 02 | 2 | PROV-03 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-02-02 | 02 | 2 | PROV-04 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-03-01 | 03 | 3 | PROV-05 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-03-02 | 03 | 3 | PROV-06 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-03-03 | 03 | 3 | PROV-07 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 15-03-04 | 03 | 3 | PROV-08 | manual | n/a | n/a | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — cargo build is the automated gate.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| SoftAP hotspot visible and web UI accessible | PROV-03 | Requires live device + phone/laptop | Flash device with no NVS; scan WiFi; connect to ESP32-GNSS-XXXX; navigate to 192.168.71.1 |
| Form submission saves credentials and reboots | PROV-04 | Requires live device | Fill form with test WiFi + MQTT; submit; confirm device reboots and connects |
| 3-network tryout on connection failure | PROV-02 | Requires network manipulation | Store 3 SSIDs; make first two unavailable; confirm device tries all three in order |
| GPIO9 held 3s triggers SoftAP | PROV-05 | Requires hardware button | Hold GPIO9 low for 3s from running STA mode; confirm SoftAP hotspot appears |
| 300s no-client SoftAP timeout | PROV-06 | Requires timed test | Enter SoftAP; wait 300s with no client; confirm device reboots to STA mode |
| MQTT softap trigger | PROV-07 | Requires live MQTT broker | Publish "softap" to OTA trigger topic; confirm device enters SoftAP |
| LED pattern distinct in SoftAP | PROV-08 | Visual inspection | Observe LED in SoftAP mode vs connecting/connected/error states |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
