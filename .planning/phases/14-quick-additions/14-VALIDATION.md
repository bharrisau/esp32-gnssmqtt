---
phase: 14
slug: quick-additions
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 14 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded Rust firmware, no host test runner |
| **Config file** | none |
| **Quick run command** | `cargo build --release` |
| **Full suite command** | `cargo build --release` + flash + `espflash monitor` observation |
| **Estimated runtime** | ~120 seconds (build) + manual flash/observe |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release`
- **After every plan wave:** Run `cargo build --release` + flash + manual observation
- **Before `/gsd:verify-work`:** All four manual criteria observed on device FFFEB5

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 14-01-01 | 01 | 1 | MAINT-02 | manual+build | `cargo build --release` | N/A | ⬜ pending |
| 14-01-02 | 01 | 1 | MAINT-02 | manual+build | `cargo build --release` | N/A | ⬜ pending |
| 14-02-01 | 02 | 1 | MAINT-01 | manual+build | `cargo build --release` | N/A | ⬜ pending |
| 14-02-02 | 02 | 1 | CMD-01, CMD-02 | manual+build | `cargo build --release` | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No new test files needed.

*Build check (`cargo build --release`) serves as the automated verification gate for all tasks.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| ISO timestamps in log output after WiFi connects | MAINT-02 | Requires flashed hardware + serial monitor observation | Flash device; connect serial monitor; verify log lines show `HH:MM:SS.mmm` format within ~5s of WiFi connect |
| `"reboot"` payload triggers restart within 5s | MAINT-01 | Requires MQTT publish to live device | Publish `reboot` to `gnss/FFFEB5/ota/trigger`; observe device restart in serial monitor |
| `/command` payload forwarded to UM980 once | CMD-01 | Requires UART observation or UM980 response | Publish a known UM980 query command; verify response appears on `gnss/FFFEB5/nmea/response` |
| No retained replay of commands on reconnect | CMD-02 | Requires MQTT session inspection | Disconnect device; re-connect; verify old commands are not re-executed |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s (build check)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
