---
phase: 17
slug: ntrip-client
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 17 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded firmware (no test runner) |
| **Config file** | none |
| **Quick run command** | `cargo build --release` |
| **Full suite command** | `cargo build --release && cargo clippy -- -D warnings` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release`
- **After every plan wave:** Run `cargo build --release && cargo clippy -- -D warnings`
- **Before `/gsd:verify-work`:** Full suite must be green + hardware test
- **Max feedback latency:** 60 seconds (compile-time gate only)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 17-01-01 | 01 | 1 | NTRIP-01 | compile | `cargo build --release` | ✅ | ⬜ pending |
| 17-01-02 | 01 | 1 | NTRIP-01 | compile | `cargo build --release` | ✅ | ⬜ pending |
| 17-01-03 | 01 | 1 | NTRIP-01 | compile | `cargo build --release` | ✅ | ⬜ pending |
| 17-02-01 | 02 | 2 | NTRIP-02 | compile | `cargo build --release` | ✅ | ⬜ pending |
| 17-02-02 | 02 | 2 | NTRIP-03 | compile | `cargo build --release` | ✅ | ⬜ pending |
| 17-02-03 | 02 | 2 | NTRIP-04 | compile | `cargo build --release` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements (no test framework — compile-time only).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| NTRIP TCP connect + RTCM stream to UART | NTRIP-01 | Requires NTRIP caster + UM980 hardware | Flash device; publish config to retained topic; observe RTK status change |
| UM980 achieves RTK Float/Fix | NTRIP-01 | Hardware-only observable | Monitor UM980 NMEA output for GNGGA quality 4 or 5 |
| Config persists after reboot | NTRIP-02 | Requires device reboot cycle | Publish config; reboot; verify reconnection with no re-publish |
| Auto-reconnect on TCP drop | NTRIP-03 | Requires external caster control | Kill caster session; observe reconnect within backoff window (~30s) |
| Heartbeat includes ntrip field | NTRIP-04 | Live MQTT subscription | Subscribe to heartbeat topic; verify `"ntrip":"connected"` or `"disconnected"` field |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
