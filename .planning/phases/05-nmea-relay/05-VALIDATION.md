---
phase: 5
slug: nmea-relay
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded target; no host test runner |
| **Config file** | none |
| **Quick run command** | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` |
| **Full suite command** | Flash + `espflash monitor` + `mosquitto_sub -h 10.86.32.41 -u user -P C65hSJsm -t 'gnss/FFFEB5/nmea/#' -v` |
| **Estimated runtime** | ~60s (build) + hardware observation |

---

## Sampling Rate

- **After every task commit:** `cargo build --target riscv32imc-esp-espidf` — verify compile success
- **After every plan wave:** `cargo build` + flash + MQTT broker subscription shows NMEA sentences arriving
- **Before `/gsd:verify-work`:** Full hardware verification green
- **Max feedback latency:** ~60 seconds (build) per task

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | NMEA-02 | compile | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` | N/A | ⬜ pending |
| 05-01-02 | 01 | 1 | NMEA-01 | compile | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` | N/A | ⬜ pending |
| 05-01-03 | 01 | 1 | NMEA-01 | compile | `cargo build --target riscv32imc-esp-espidf 2>&1 \| grep -E "^error"` | N/A | ⬜ pending |
| 05-01-04 | 01 | 1 | NMEA-01, NMEA-02 | manual | Flash + `mosquitto_sub gnss/FFFEB5/nmea/#` | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

None — no test files needed. Validation is entirely hardware observation. `cargo build` compile check covers structural correctness.

*Existing infrastructure covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| NMEA sentences published to `gnss/FFFEB5/nmea/{TYPE}` | NMEA-01 | Requires hardware + MQTT broker | `mosquitto_sub -h 10.86.32.41 -u user -P C65hSJsm -t 'gnss/FFFEB5/nmea/#' -v` — verify GNGGA, GNRMC etc arrive with `$`-prefixed payloads |
| Bounded channel (64) — full drops without UART stall | NMEA-02 | Requires hardware saturation test | Monitor espflash output — no "relay channel full" WARN at normal UM980 rate; no UART RX pause |
| UM980 MODE ROVER prerequisite | NMEA-01 | Hardware state | Send `MODE ROVER` via uart_bridge stdin before observing NMEA stream |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
