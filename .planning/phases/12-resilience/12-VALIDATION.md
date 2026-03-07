---
phase: 12
slug: resilience
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded target (ESP32-C6); no host-side test runner |
| **Config file** | none |
| **Quick run command** | `cargo build --release 2>&1` |
| **Full suite command** | `cargo build --release 2>&1` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full build must pass + flash + observe reboot behavior

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 12-01-01 | 01 | 1 | RESIL-01 | build | `cargo build --release` | ✅ | ⬜ pending |
| 12-01-02 | 01 | 1 | RESIL-01 | manual | flash + disconnect WiFi 10 min (or 30s dev constant) | N/A | ⬜ pending |
| 12-02-01 | 02 | 1 | RESIL-02 | build | `cargo build --release` | ✅ | ⬜ pending |
| 12-02-02 | 02 | 1 | RESIL-02 | manual | flash + block MQTT port 5 min (or 30s dev constant) | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.*

No test infrastructure to create. `cargo build --release` is the automated gate.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `wifi_supervisor` calls `esp_restart()` after 10-min WiFi outage; log precedes restart | RESIL-01 | Embedded target — requires hardware WiFi disruption | Set `WIFI_DISCONNECT_REBOOT_TIMEOUT` to 30s for dev. Flash, disconnect AP, observe `[RESIL-01]` log + reboot + reconnect |
| MQTT disconnect timer triggers `esp_restart()` after 5 min with WiFi up; log precedes restart | RESIL-02 | Embedded target — requires broker disruption while WiFi stays connected | Set `MQTT_DISCONNECT_REBOOT_SECS` to 30s for dev. Flash, block MQTT port (not WiFi), observe `[RESIL-02]` log + reboot + reconnect |
| Device reconnects normally after reboot | RESIL-01, RESIL-02 | Embedded target | Observe `espflash flash --monitor` — GNSS data flowing to MQTT after restart |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s (build gate)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
