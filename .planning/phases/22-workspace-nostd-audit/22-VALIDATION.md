---
phase: 22
slug: workspace-nostd-audit
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 22 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo build (compiler as validator) + shell scripts |
| **Config file** | Cargo.toml (workspace root) |
| **Quick run command** | `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf` |
| **Full suite command** | `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf && cargo build -p gnss-server` |
| **Estimated runtime** | ~60 seconds (incremental) |

---

## Sampling Rate

- **After every task commit:** Run `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf`
- **After every plan wave:** Run full suite command above
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 22-01-01 | 01 | 1 | INFRA-01 | build | `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf` | ✅ | ⬜ pending |
| 22-01-02 | 01 | 1 | INFRA-01 | build | `cargo build -p gnss-server` | ✅ | ⬜ pending |
| 22-01-03 | 01 | 1 | INFRA-01 | inspect | `cargo metadata --format-version 1 | jq '.resolve'` | ✅ | ⬜ pending |
| 22-02-01 | 02 | 2 | NOSTD-01 | manual | Review audit document coverage | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. This phase is primarily workspace restructuring and documentation — the compiler itself serves as the test harness.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Audit document completeness | NOSTD-01 | Requires human judgment to verify all esp-idf-svc/hal/sys usages are enumerated | grep all esp-idf imports; verify each appears in audit with mapping or gap note |
| Gap priority ranking | NOSTD-01 | Subjective ordering requires domain knowledge review | Review NOSDT-AUDIT.md gap table ordering against Phase 23-25 plan |
| .cargo/config.toml scoping | INFRA-01 | Must verify server build is unaffected by firmware target config | Build server from workspace root; confirm no riscv target artifacts generated |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
