---
phase: 19
slug: pre-2-0-bugfix
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-09
---

# Phase 19 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo clippy + cargo build (no unit test suite for embedded firmware) |
| **Config file** | Cargo.toml |
| **Quick run command** | `cargo clippy -- -D warnings` |
| **Full suite command** | `cargo build` |
| **Estimated runtime** | ~60–120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy -- -D warnings`
- **After every plan wave:** Run `cargo build`
- **Before `/gsd:verify-work`:** Full build must be green; manual hardware tests per testing.md
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------|-------------------|--------|
| BUG-1 DHCP fix | 01 | 1 | PROV-02 | build + manual | `cargo clippy -- -D warnings` | ⬜ pending |
| BUG-2 302 validation | 01 | 1 | PROV-02 | build + manual | `cargo clippy -- -D warnings` | ⬜ pending |
| BUG-3/4 NVS TLS default | 02 | 1 | MAINT-03 | build | `cargo clippy -- -D warnings` | ⬜ pending |
| config_ver key | 02 | 1 | BUG-3 | build | `cargo clippy -- -D warnings` | ⬜ pending |
| FEAT-1 button rework | 03 | 2 | PROV-06 | build + manual | `cargo clippy -- -D warnings` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.* No test framework to install — embedded firmware verified via clippy + build + hardware flash.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| DHCP DNS override | PROV-02/BUG-1 | Requires hardware — connect device to GNSS-Setup SoftAP, inspect DHCP lease for DNS server | Flash firmware; connect phone/laptop; check DNS via `ipconfig`/`ip route`; confirm 192.168.71.1 served |
| Android captive portal | PROV-02/BUG-2 | Requires Android device — captive portal notification only appears on Android | Connect Android to GNSS-Setup; verify "Sign in to network" notification appears |
| Post-OTA MQTT connect | MAINT-03/BUG-3 | Requires OTA flash of old firmware then upgrade | Flash old firmware; trigger OTA; confirm MQTT connects on new firmware |
| Boot button 3s SoftAP | PROV-06/FEAT-1 | Requires hardware GPIO hold | Hold GPIO9 3s; release; verify SoftAP mode starts |
| Boot button 10s factory reset | FEAT-1 | Requires hardware GPIO hold + NVS verification | Hold GPIO9 10s; release; verify NVS erased + SoftAP on reboot |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
