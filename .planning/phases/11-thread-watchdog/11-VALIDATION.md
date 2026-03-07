---
phase: 11
slug: thread-watchdog
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded ESP32-C6 target; no native test runner available |
| **Config file** | none |
| **Quick run command** | `cargo build --release 2>&1 | grep -E "^error"` |
| **Full suite command** | `cargo build --release 2>&1` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release 2>&1 | grep -E "^error"`
- **After every plan wave:** Run `cargo build --release 2>&1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 11-01-01 | 01 | 1 | WDT-01 | build | `cargo build --release 2>&1 \| grep -E "^error"` | ✅ | ⬜ pending |
| 11-01-02 | 01 | 1 | WDT-01 | build + code audit | `cargo build --release 2>&1 \| grep -E "^error"` | ✅ | ⬜ pending |
| 11-02-01 | 02 | 2 | WDT-02 | build | `cargo build --release 2>&1 \| grep -E "^error"` | ✅ | ⬜ pending |
| 11-02-02 | 02 | 2 | WDT-02 | build + config audit | `cargo build --release 2>&1 \| grep -E "^error"` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

None — no test infrastructure to create. Build verification is the automated gate.

*Existing infrastructure covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| GNSS RX and MQTT pump each update heartbeat counter ≤ every 5s | WDT-01 | Requires flashing and observing runtime log; no host-side test framework | Flash device; observe `[WDT] supervisor started` log line; confirm no spurious reboots during 60s of nominal GNSS+MQTT operation |
| Supervisor detects 3 missed beats and calls `esp_restart()` | WDT-02 | Requires physically hanging a thread (e.g., blocking a channel) to trigger detection | Simulate hang; confirm device reboots within ~15s (3 × 5s checks); verify reboot log message |
| Hardware TWDT reboots if supervisor itself stops | WDT-02 | Requires killing the supervisor thread and waiting for HW timeout | Stop supervisor; confirm hardware TWDT triggers reboot within 30s |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
