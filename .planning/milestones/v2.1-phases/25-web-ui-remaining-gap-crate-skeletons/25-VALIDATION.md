---
phase: 25
slug: web-ui-remaining-gap-crate-skeletons
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 25 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust unit tests) |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p gnss-server 2>&1 \| tail -20` |
| **Full suite command** | `cargo test --workspace 2>&1 \| tail -40` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p gnss-server 2>&1 | tail -20`
- **After every plan wave:** Run `cargo test --workspace 2>&1 | tail -40`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 25-01-01 | 01 | 1 | UI-01 | unit | `cargo test -p gnss-server test_ws_broadcast` | ❌ W0 | ⬜ pending |
| 25-01-02 | 01 | 1 | UI-02 | unit | `cargo test -p gnss-server test_gsv_accumulation` | ❌ W0 | ⬜ pending |
| 25-01-03 | 01 | 1 | UI-03 | unit | `cargo test -p gnss-server test_snr_bar_chart` | ❌ W0 | ⬜ pending |
| 25-01-04 | 01 | 1 | UI-04 | unit | `cargo test -p gnss-server test_heartbeat_forwarding` | ❌ W0 | ⬜ pending |
| 25-02-01 | 02 | 2 | NOSTD-04b | unit | `cargo test -p gnss-softap test_trait_exists` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/gnss-server/src/nmea_parse.rs` — GSV accumulator + test stubs for UI-01..UI-04
- [ ] `crates/gnss-server/src/web_server.rs` — axum router + AppState test stubs
- [ ] `crates/gnss-softap/src/lib.rs` — trait definition stub for NOSTD-04b
- [ ] `crates/gnss-dns/src/lib.rs` — trait definition stub for NOSTD-04b
- [ ] `crates/gnss-log/src/lib.rs` — trait definition stub for NOSTD-04b

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Browser renders polar SVG skyplot | UI-02 | Requires running ESP32 + browser | Open http://device-ip/, verify satellite dots appear on polar plot |
| SNR bar chart updates live | UI-03 | Requires running ESP32 + browser | Verify bars update when satellites change |
| Health panel within 35s | UI-04 | Requires MQTT heartbeat source | Publish heartbeat MQTT message, verify panel updates within 35s |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
