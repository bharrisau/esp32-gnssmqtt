---
phase: 18
slug: telemetry-and-ota-validation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-09
---

# Phase 18 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — ESP32 bare-metal firmware; no automated test runner |
| **Config file** | none |
| **Quick run command** | `cargo clippy -- -D warnings` |
| **Full suite command** | `cargo build --release` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy -- -D warnings`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full suite must be green + hardware observations complete
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 18-01-01 | 01 | 1 | TELEM-01 | build | `cargo clippy -- -D warnings` | ✅ | ⬜ pending |
| 18-01-02 | 01 | 1 | TELEM-01 | build | `cargo clippy -- -D warnings` | ✅ | ⬜ pending |
| 18-01-03 | 01 | 1 | TELEM-01 | manual | MQTT subscribe observation | n/a | ⬜ pending |
| 18-02-01 | 02 | 2 | MAINT-03 | hardware | n/a — hardware procedure | n/a | ⬜ pending |
| 18-02-02 | 02 | 2 | MAINT-03 | hardware | n/a — hardware procedure | n/a | ⬜ pending |
| 18-03-01 | 03 | 3 | — | manual | n/a — documentation | n/a | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.*

No test files required — ESP32 firmware validated via hardware observation and `cargo clippy -- -D warnings` per project convention.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Heartbeat JSON includes fix_type, satellites, hdop | TELEM-01 | ESP32 firmware — no unit test runner | Subscribe to heartbeat MQTT topic; verify JSON fields present and populated with GGA data |
| Heartbeat shows null/sentinel when no GGA received | TELEM-01 | Hardware state dependency | Power up before GNSS lock; observe heartbeat for null/sentinel values |
| OTA update completes end-to-end on device FFFEB5 | MAINT-03 | Hardware validation required | Build canary image, serve via HTTP, publish MQTT trigger, verify reboot and mark-valid log |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
