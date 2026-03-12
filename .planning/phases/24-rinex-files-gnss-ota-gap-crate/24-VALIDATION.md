---
phase: 24
slug: rinex-files-gnss-ota-gap-crate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 24 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo test -p gnss-rtcm3 -p gnss-ota 2>&1 \| tail -20` |
| **Full suite command** | `cargo test --workspace 2>&1 \| tail -40` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p gnss-rtcm3 -p gnss-ota 2>&1 | tail -20`
- **After every plan wave:** Run `cargo test --workspace 2>&1 | tail -40`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 24-01-01 | 01 | 0 | RINEX-01 | unit | `cargo test -p gnss-rtcm3 test_rinex_obs` | ❌ W0 | ⬜ pending |
| 24-01-02 | 01 | 1 | RINEX-01 | unit | `cargo test -p gnss-rtcm3 test_rinex_obs_header` | ❌ W0 | ⬜ pending |
| 24-01-03 | 01 | 1 | RINEX-02 | unit | `cargo test -p gnss-rtcm3 test_rinex_column_format` | ❌ W0 | ⬜ pending |
| 24-02-01 | 02 | 1 | RINEX-03 | unit | `cargo test -p gnss-rtcm3 test_rinex_nav` | ❌ W0 | ⬜ pending |
| 24-02-02 | 02 | 1 | RINEX-03 | unit | `cargo test -p gnss-rtcm3 test_rinex_nav_header` | ❌ W0 | ⬜ pending |
| 24-03-01 | 03 | 2 | RINEX-04 | manual | — | — | ⬜ pending |
| 24-04-01 | 04 | 1 | NOSTD-04a | unit | `cargo test -p gnss-ota` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/gnss-rtcm3/src/rinex.rs` — RINEX writer module stubs
- [ ] `crates/gnss-rtcm3/tests/rinex_tests.rs` — unit test stubs for obs/nav format
- [ ] `crates/gnss-ota/src/lib.rs` — OTA trait stub
- [ ] `crates/gnss-ota/tests/` — placeholder test file

*Wave 0 creates the file structure so subsequent tasks have targets to test against.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| RTKLIB processes output files without parse errors | RINEX-04 | Requires running rnx2rtkp/rtkplot against real RTCM3 stream data | Run `rnx2rtkp -x 5 obs.26O nav.26P` and verify no parse errors in output |
| Hourly file rotation at UTC boundary | RINEX-01, RINEX-03 | Requires waiting for clock rollover or mocking time | Set system time near hour boundary and verify new file opened |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
