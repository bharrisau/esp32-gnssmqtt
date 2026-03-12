---
phase: 23
slug: mqtt-rtcm3-gnss-nvs-crate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 23 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test (`cargo test`) |
| **Config file** | none — `#[cfg(test)]` modules inline |
| **Quick run command** | `cargo test --workspace --exclude esp32-gnssmqtt-firmware` |
| **Full suite command** | `cargo clippy --workspace --exclude esp32-gnssmqtt-firmware -- -D warnings && cargo test --workspace --exclude esp32-gnssmqtt-firmware` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **After every plan wave:** Run `cargo clippy --workspace --exclude esp32-gnssmqtt-firmware -- -D warnings && cargo test --workspace --exclude esp32-gnssmqtt-firmware`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 23-??-01 | gnss-nvs crate | 1 | NOSTD-02 | build | `cargo check -p gnss-nvs --features esp-idf` | ❌ W0 | ⬜ pending |
| 23-??-02 | gnss-nvs crate | 1 | NOSTD-03 | build | `cargo check -p gnss-nvs --features sequential` | ❌ W0 | ⬜ pending |
| 23-??-03 | RTCM decode | 2 | RTCM-01 | unit | `cargo test -p gnss-server -- rtcm_decode::tests` | ❌ W0 | ⬜ pending |
| 23-??-04 | RTCM decode | 2 | RTCM-02 | unit | `cargo test -p gnss-server -- rtcm_decode::tests::gal_bds` | ❌ W0 | ⬜ pending |
| 23-??-05 | RTCM decode | 2 | RTCM-03 | unit | `cargo test -p gnss-server -- rtcm_decode::tests::eph` | ❌ W0 | ⬜ pending |
| 23-??-06 | epoch grouping | 2 | RTCM-04 | unit | `cargo test -p gnss-server -- epoch::tests` | ❌ W0 | ⬜ pending |
| 23-??-07 | MQTT supervisor | 2 | SRVR-02 | unit | `cargo test -p gnss-server -- mqtt::tests` | ❌ W0 | ⬜ pending |
| 23-??-08 | MQTT subscribe | 3 | SRVR-01 | integration | Manual — requires running broker | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/gnss-nvs/Cargo.toml` and `crates/gnss-nvs/src/` — crate scaffold for NOSTD-02, NOSTD-03
- [ ] `gnss-server/src/rtcm_decode.rs` — `#[cfg(test)]` module stubs for RTCM-01, RTCM-02, RTCM-03
- [ ] `gnss-server/src/epoch.rs` — `#[cfg(test)]` module stubs for RTCM-04
- [ ] `gnss-server/src/mqtt.rs` — `#[cfg(test)]` module stubs for SRVR-02
- [ ] `gnss-server/tests/fixtures/rtcm_sample.bin` — RTCM3 frames extracted from `gnss.log`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| MQTT subscription to correct topics | SRVR-01 | Requires running MQTT broker | Connect broker, send messages to `gnss/{id}/rtcm`, verify server logs receipt; verify reconnect with exponential backoff by killing broker and restarting |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
