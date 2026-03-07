---
phase: 7
slug: rtcm-relay
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded firmware; compile-check + manual hardware validation |
| **Config file** | none — Wave 0 installs no test framework |
| **Quick run command** | `cargo build --release 2>&1 \| grep -E "error\|warning"` |
| **Full suite command** | `cargo build --release` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release 2>&1 | grep -E "error|warning"`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 7-01-01 | 01 | 1 | RTCM-05 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 7-01-02 | 01 | 1 | RTCM-01 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 7-01-03 | 01 | 1 | RTCM-02 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 7-01-04 | 01 | 1 | RTCM-03 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 7-02-01 | 02 | 2 | RTCM-04 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 7-02-02 | 02 | 2 | RTCM-04 | manual | Flash + `mosquitto_sub -t 'gnss/+/rtcm/+'` | N/A | ⬜ pending |
| 7-02-03 | 02 | 2 | RTCM-02 | manual | Inject corrupt 0xD3 byte; verify NMEA continues | N/A | ⬜ pending |
| 7-02-04 | 02 | 2 | RTCM-05 | manual | Send OTA trigger; verify UM980 UART TX silent | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- No new test files required — embedded firmware; all automated checks are compile-only
- Existing `cargo build --release` in CI covers compile-time validation

*Existing infrastructure covers all automated phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| RTCM3 frames published to `gnss/{id}/rtcm/{type}` | RTCM-03, RTCM-04 | Requires hardware + UM980 RTCM output enabled | Flash firmware; enable RTCM on UM980 via `/config`; run `mosquitto_sub -t 'gnss/+/rtcm/+' -v`; verify frames arrive |
| MSM7 frames (up to 1029 bytes) not truncated | RTCM-04 | Requires live UM980 MSM7 output | Subscribe as above; pipe to hexdump; verify full frame length matches RTCM header length field |
| CRC failure causes resync without NMEA interruption | RTCM-02 | Requires hardware byte injection | Corrupt one byte in UART stream; verify NMEA sentences continue appearing on `gnss/{id}/nmea/+` |
| 1029-byte buffer fits in thread stack | RTCM-01 | FreeRTOS canary detection only | Enable stack HWM logging; flash; monitor for stack overflow panics |
| OTA trigger not forwarded to UM980 | RTCM-05 | Requires hardware UART TX monitoring | Publish to `/ota/trigger`; verify UM980 UART TX is silent (no command bytes sent) |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
