---
phase: 8
slug: ota
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | None — embedded firmware; compile-check + manual hardware validation |
| **Config file** | none — Wave 0 creates partitions.csv + sdkconfig.defaults additions |
| **Quick run command** | `cargo build --release 2>&1 \| grep -E "^error"` |
| **Full suite command** | `cargo build --release` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --release 2>&1 | grep -E "^error"`
- **After every plan wave:** Run `cargo build --release`
- **Before `/gsd:verify-work`:** Full suite must be green + on-device smoke test
- **Max feedback latency:** 30 seconds (compile); hardware smoke test required before phase close

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 8-01-01 | 01 | 0 | OTA-01 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-01-02 | 01 | 1 | OTA-02 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-01-03 | 01 | 1 | OTA-03 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-01-04 | 01 | 1 | OTA-04 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-01-05 | 01 | 1 | OTA-05 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-01-06 | 01 | 1 | OTA-06 | compile | `cargo build --release` | ❌ W0 | ⬜ pending |
| 8-02-01 | 02 | 2 | OTA-01,OTA-04 | manual | Flash + boot verification on hardware | N/A | ⬜ pending |
| 8-02-02 | 02 | 2 | OTA-02,OTA-03,OTA-05,OTA-06 | manual | `mosquitto_sub -t 'gnss/+/ota/status'` during active OTA | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `partitions.csv` — updated for OTA layout (nvs + otadata + ota_0 + ota_1) — OTA-01
- [ ] `src/ota.rs` — new module stub (OTA-02 through OTA-06)
- [ ] `sdkconfig.defaults` additions: `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`, `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Partition table accepted; device boots from ota_0 | OTA-01 | Requires USB reflash with new partition layout | `espflash erase-flash && espflash flash --monitor`; confirm boot from ota_0 in serial log |
| OTA trigger delivers URL to ota task | OTA-02 | Requires live MQTT broker + device | Publish `{"url":"...","sha256":"..."}` to `gnss/{id}/ota/trigger`; observe serial log |
| Download + SHA256 + flash completes without error | OTA-03 | Requires test HTTP server hosting known firmware | Serve known .bin file; trigger OTA; confirm `{"state":"complete"}` on status topic |
| New firmware boots and stays after mark_valid called | OTA-04 | FreeRTOS rollback requires hardware observation | Boot new firmware; observe `mark_running_slot_valid()` in logs; reboot; confirm stays on new slot |
| Rollback: firmware without valid call reverts | OTA-04 | Requires deliberate omission of valid call | Build test firmware without valid call; flash via OTA; reboot; confirm rollback to previous slot |
| Heartbeat continues during OTA download | OTA-05 | Requires live observation during active download | Subscribe to `gnss/{id}/status`; trigger OTA; confirm heartbeat at 30s intervals |
| Status messages published (downloading/complete/failed) | OTA-06 | Requires live MQTT broker | `mosquitto_sub -t 'gnss/+/ota/status'` during OTA; verify all state transitions |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s (compile); hardware smoke test documented
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
