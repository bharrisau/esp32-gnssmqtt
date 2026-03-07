---
phase: 10
slug: memory-diagnostics
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded ESP32 target; no native test runner available |
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
| 10-01-01 | 01 | 1 | HARD-03 | code audit | `grep -n "Vec::from\|Vec::new" src/gnss.rs src/rtcm_relay.rs` returns 0 hits in RTCM path | ✅ | ⬜ pending |
| 10-01-02 | 01 | 1 | HARD-03 | build | `cargo build --release 2>&1 \| grep -E "^error"` | ✅ | ⬜ pending |
| 10-01-03 | 01 | 1 | HARD-03 | code review | Inspect `Err(_)` arm of `free_pool_rx.try_recv()` path exists | ❌ W0 | ⬜ pending |
| 10-02-01 | 02 | 2 | HARD-04 | smoke | `cargo build --release` compiles; log inspection at runtime shows HWM lines | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Pool exhaustion code path — does not exist yet; created in 10-01-PLAN
- [ ] HWM log lines — do not exist yet; created in 10-02-PLAN

*No test framework install needed — project uses build verification only.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Each thread logs HWM line at startup | HARD-04 | Requires flashing and observing serial log output | Flash device, open serial monitor, verify `[HWM] <thread-name>: <N> words remaining` line for each spawned thread |
| Pool exhaustion drops frame + logs warning | HARD-03 | Requires simulating pool exhaustion at runtime | Inject condition or review code path to confirm `Err(_)` arm logs warning and drops frame |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
