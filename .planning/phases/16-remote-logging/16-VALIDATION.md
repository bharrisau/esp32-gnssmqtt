---
phase: 16
slug: remote-logging
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 16 — Validation Strategy

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
| 16-01-01 | 01 | 1 | LOG-01 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 16-01-02 | 01 | 1 | LOG-01 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 16-02-01 | 02 | 2 | LOG-02 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 16-02-02 | 02 | 2 | LOG-03 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — cargo build is the automated gate.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Log messages appear on MQTT topic within 1s | LOG-01 | Requires live device + MQTT broker | Flash device; subscribe to gnss/{id}/log; observe log output appears in real time |
| No feedback loop from MQTT publish path | LOG-01 | Requires live device observation | Confirm MQTT publish calls do not generate additional log topic messages |
| Runtime level change takes effect immediately | LOG-02 | Requires live device + MQTT | Publish "warn" to log/level; confirm only WARN+ messages appear on log topic |
| Drop on full channel (no stall) | LOG-03 | Requires stress test on device | Flood logs while disconnected; confirm firmware does not hang |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
