---
phase: 6
slug: remote-config
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded target; no unit test runner |
| **Config file** | none |
| **Quick run command** | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` |
| **Full suite command** | Flash + `espflash monitor` + `mosquitto_pub` config payload test |
| **Estimated runtime** | ~60s build; hardware test ~5min |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --target riscv32imc-esp-espidf` — verify compile success
- **After every plan wave:** Flash + `mosquitto_pub` config test shows commands reaching UM980 (espflash monitor log)
- **Before `/gsd:verify-work`:** Full hardware cycle: publish config → observe relay → reconnect → observe dedup skip
- **Max feedback latency:** ~60 seconds (compile check)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 6-01-01 | 01 | 1 | CONF-01 | compile | `cargo build --target riscv32imc-esp-espidf` | N/A | ⬜ pending |
| 6-01-02 | 01 | 1 | CONF-02 | compile | `cargo build --target riscv32imc-esp-espidf` | N/A | ⬜ pending |
| 6-01-03 | 01 | 1 | CONF-03 | compile | `cargo build --target riscv32imc-esp-espidf` | N/A | ⬜ pending |
| 6-02-01 | 02 | 2 | CONF-01 | manual | Flash + mosquitto_pub + espflash monitor | N/A | ⬜ pending |
| 6-02-02 | 02 | 2 | CONF-02 | manual | Reconnect cycle + observe "payload unchanged" log | N/A | ⬜ pending |
| 6-02-03 | 02 | 2 | CONF-03 | manual | Observe ~100ms/200ms log timing in espflash monitor | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

No test stubs or framework installation needed — this is an embedded target with no unit test runner. Validation is via `cargo build` (compile check) and hardware observation.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Subscribe to config topic QoS 1; forward payload line-by-line to UM980 UART TX | CONF-01 | Requires hardware + MQTT broker + UM980 device | Publish `{"delay_ms": 200, "commands": ["MODE ROVER", "CONFIGSAVE"]}` to `gnss/FFFEB5/config`; observe log lines `"Config relay: sending command: MODE ROVER"` and `"sending command: CONFIGSAVE"` in espflash monitor |
| Hash dedup prevents re-applying unchanged config on reconnect | CONF-02 | Requires MQTT broker retained message + device reconnect cycle | Power cycle or force MQTT reconnect; observe `"Config relay: payload unchanged"` — config must NOT be re-applied |
| 100ms default delay between commands; override via `delay_ms` JSON field | CONF-03 | Observable only in espflash monitor log timing | Send payload with `"delay_ms": 200`; verify ~200ms between command log lines. Send plain text payload; verify ~100ms default delay. |
| Plain text fallback (no leading `{`) | CONF-01 | Hardware only | Publish `"MODE ROVER\nCONFIGSAVE\n"` (no `{` prefix); verify both commands forwarded with 100ms delay |
| Empty payload guard | CONF-02 | Hardware only | Publish empty retained payload to clear; verify `"retained message cleared"` log, no commands forwarded |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s (compile check)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
