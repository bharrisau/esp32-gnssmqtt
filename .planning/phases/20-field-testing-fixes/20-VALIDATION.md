---
phase: 20
slug: field-testing-fixes
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 20 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — hardware-only validation (embedded firmware) |
| **Config file** | none |
| **Quick run command** | `cargo clippy -- -D warnings` |
| **Full suite command** | `cargo build --release` |
| **Estimated runtime** | ~60 seconds (build) |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy -- -D warnings`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full build must be clean + hardware validation on FFFEB5
- **Max feedback latency:** ~60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------|-------------------|--------|
| 20-01-xx | 01 | 1 | BUG-5 Windows | build | `cargo clippy -- -D warnings` | ⬜ pending |
| 20-01-xx | 01 | 1 | BUG-5 iOS | build | `cargo clippy -- -D warnings` | ⬜ pending |
| 20-02-xx | 02 | 2 | PERF-1 | build | `cargo clippy -- -D warnings` | ⬜ pending |
| 20-03-xx | 03 | 2 | FEAT-2 | build | `cargo clippy -- -D warnings` | ⬜ pending |
| 20-04-xx | 04 | 3 | FEAT-3 | build | `cargo clippy -- -D warnings` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No test stubs needed — all verification is manual hardware testing on device FFFEB5.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Windows shows "Internet access" on GNSS-Setup SSID | BUG-5 | Requires physical Windows device | Connect Windows 10/11 PC to `GNSS-Setup` AP; verify network icon shows "Internet access" or "No Internet access" changes to connected |
| iOS shows captive portal notification | BUG-5 | Requires physical iOS device | Connect iPhone to `GNSS-Setup`; verify "Sign in to network" notification appears |
| NMEA relay sustains 5 Hz without drops | PERF-1 | Requires UM980 at 5 Hz rate | Set UM980 `GPGGA 0.2`; monitor `nmea_drops` in heartbeat over 60s |
| UM980 config re-applies after power cycle | FEAT-2 | Requires hardware power cycle | Send GNSS config via MQTT; power-cycle UM980 UART power; verify config re-sent in `/log` within 10s |
| NTRIP connects to AUSCORS port 443 | FEAT-3 | Requires AUSCORS account + internet | Configure host=`ntrip.data.gnss.ga.gov.au` port=443 tls=true; verify `ntrip: "connected"` in heartbeat |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
