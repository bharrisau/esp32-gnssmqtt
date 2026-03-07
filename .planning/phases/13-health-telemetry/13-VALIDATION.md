---
phase: 13
slug: health-telemetry
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 13 — Validation Strategy

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
- **Before `/gsd:verify-work`:** Full build must pass + flash + observe MQTT broker

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 13-01-01 | 01 | 1 | METR-02 | build | `cargo build --release` | ✅ | ⬜ pending |
| 13-01-02 | 01 | 1 | METR-01 | build | `cargo build --release` | ✅ | ⬜ pending |
| 13-01-03 | 01 | 1 | METR-01 | manual | flash + `mosquitto_sub -t 'gnss/+/heartbeat'` | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.*

No test infrastructure to create. `cargo build --release` is the automated gate.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| JSON published to `gnss/{device_id}/heartbeat` with 5 fields at configured cadence | METR-01 | Embedded target — requires live device + MQTT broker | Flash, run `mosquitto_sub -t 'gnss/+/heartbeat'`, verify JSON with all 5 fields every ~30s |
| `NMEA_DROPS` / `RTCM_DROPS` increment at `TrySendError::Full` | METR-02 | Requires observable channel saturation on hardware | Saturate channel (very small channel size test or high GNSS output rate), observe counter increase in heartbeat payload |
| Retained `"online"` published to `gnss/{device_id}/status` on reconnect | METR-01 | Requires live broker | Disconnect + reconnect device, `mosquitto_sub -r -t 'gnss/+/status'` shows "online" |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s (build gate)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
