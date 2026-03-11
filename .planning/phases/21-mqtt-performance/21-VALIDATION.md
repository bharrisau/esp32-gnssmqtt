---
phase: 21
slug: mqtt-performance
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 21 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo clippy + cargo build (ESP32-C6 cross-compile) |
| **Config file** | Cargo.toml, sdkconfig.defaults |
| **Quick run command** | `cargo clippy -- -D warnings` |
| **Full suite command** | `cargo build` (cross-compile for riscv32imac-esp-espidf) |
| **Estimated runtime** | ~60-120 seconds (cross-compile) |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy -- -D warnings`
- **After every plan wave:** Run `cargo build` (full cross-compile)
- **Before `/gsd:verify-work`:** Full build must be green + manual bench trigger test on device FFFEB5
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 21-01-01 | 01 | 1 | MqttMessage enum + channel | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |
| 21-01-02 | 01 | 1 | Publish thread owns client | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |
| 21-02-01 | 02 | 1 | NMEA/RTCM topic consolidation | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |
| 21-02-02 | 02 | 1 | bytes crate RTCM buffer | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |
| 21-03-01 | 03 | 2 | bench trigger + observability | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |
| 21-03-02 | 03 | 2 | MQTT_OUTBOX_DROPS counter | compile | `cargo clippy -- -D warnings` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements — this is an ESP32 firmware project with no unit test framework. Validation is via `cargo clippy -- -D warnings` (fast) and `cargo build` (full cross-compile).

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| bench:100 sends 100 msgs to gnss/{id}/bench | Bench trigger | Requires hardware + MQTT broker | Subscribe to gnss/FFFEB5/bench; publish "bench:100" to ota/trigger; count received messages |
| MQTT_ENQUEUE_ERRORS appears in heartbeat | Observability | Requires hardware | Disconnect MQTT mid-flight, reconnect, read heartbeat JSON |
| MQTT_OUTBOX_DROPS increments | Observability | Requires CONFIG_MQTT_REPORT_DELETED_MESSAGES=y | Subscribe gnss/FFFEB5/heartbeat; saturate with bench trigger while MQTT is throttled |
| nmea_drops=0 at 5Hz after refactor | Regression | Requires hardware + GNSS lock | Let device run at 5Hz, monitor heartbeat.nmea_drops over 5 minutes |
| Zero Arc<Mutex<EspMqttClient>> after migration | Architecture | Code review | grep for Arc<Mutex<EspMqttClient>> in src/ — must return no matches |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
