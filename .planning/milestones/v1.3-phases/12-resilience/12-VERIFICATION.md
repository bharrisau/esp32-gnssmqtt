---
phase: 12-resilience
verified: 2026-03-07T00:00:00Z
status: human_needed
score: 7/8 must-haves verified (1 requires hardware)
re_verification: false
human_verification:
  - test: "Flash firmware, disconnect WiFi AP, wait 35s (with WIFI_DISCONNECT_REBOOT_TIMEOUT=30s), observe serial monitor"
    expected: "'[RESIL-01] WiFi disconnected for Xs — rebooting' appears, device reboots, then reconnects to WiFi and MQTT normally"
    why_human: "Cannot verify that reboot resolves the stuck state without real hardware; Success Criterion 3 requires round-trip validation"
  - test: "Flash firmware, keep WiFi up, stop MQTT broker, wait 35s (with MQTT_DISCONNECT_REBOOT_SECS=30), observe serial monitor"
    expected: "'[RESIL-02] MQTT disconnected for Xs (WiFi up) — rebooting' appears, device reboots, then reconnects to MQTT normally"
    why_human: "Cannot verify that reboot resolves the stuck state without real hardware; Success Criterion 3 requires round-trip validation"
---

# Phase 12: Resilience Verification Report

**Phase Goal:** The device recovers from extended connectivity loss without manual intervention by rebooting itself after configurable disconnection timeouts
**Verified:** 2026-03-07
**Status:** human_needed (all automated checks passed; hardware test required for Success Criterion 3)
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | After WiFi disconnection exceeds WIFI_DISCONNECT_REBOOT_TIMEOUT, wifi_supervisor calls esp_restart() with [RESIL-01] log line before the call | VERIFIED | `wifi.rs:80-82`: `log::error!("[RESIL-01] WiFi disconnected for {}s — rebooting"...)` immediately before `esp_restart()` in the `!connected` branch |
| 2 | After MQTT disconnection exceeds MQTT_DISCONNECT_REBOOT_SECS while WiFi is up, wifi_supervisor calls esp_restart() with [RESIL-02] log line before the call | VERIFIED | `wifi.rs:139-141`: `log::error!("[RESIL-02] MQTT disconnected for {}s (WiFi up) — rebooting"...)` immediately before `esp_restart()` in the connected branch |
| 3 | MQTT disconnect timer resets when WiFi drops, preventing combined-outage false triggers | VERIFIED | `wifi.rs:87`: `crate::resil::MQTT_DISCONNECTED_AT.store(0, Relaxed)` in `!connected` branch, before reconnect backoff — clears RESIL-02 timer on every WiFi-down poll cycle |
| 4 | Timeout constants are configurable via named constants in config.example.rs | VERIFIED | `config.example.rs:51-59`: `WIFI_DISCONNECT_REBOOT_TIMEOUT = Duration::from_secs(10 * 60)` and `MQTT_DISCONNECT_REBOOT_SECS: u32 = 5 * 60` |
| 5 | MQTT callback stamps MQTT_DISCONNECTED_AT on Disconnected (compare_exchange from 0) | VERIFIED | `mqtt.rs:91-95`: `MQTT_DISCONNECTED_AT.compare_exchange(0, crate::resil::now_secs(), Relaxed, Relaxed).ok()` in `EventPayload::Disconnected` arm |
| 6 | MQTT callback clears MQTT_DISCONNECTED_AT on Connected (store 0) | VERIFIED | `mqtt.rs:72`: `crate::resil::MQTT_DISCONNECTED_AT.store(0, Relaxed)` in `EventPayload::Connected` arm |
| 7 | MQTT callback never calls EspMqttClient methods — only atomic stores | VERIFIED | No `.subscribe`, `.publish`, `.enqueue`, or other EspMqttClient method calls inside the `new_cb` closure (lines 66-123); re-entrancy constraint preserved per module doc comment |
| 8 | After a triggered reboot, the device reconnects normally (Success Criterion 3) | NEEDS HUMAN | Cannot verify round-trip reboot-then-reconnect without hardware |

**Score:** 7/8 truths verified (1 requires human)

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/resil.rs` | MQTT_DISCONNECTED_AT AtomicU32 static and now_secs() helper | VERIFIED | File exists, 38 lines; exports `pub static MQTT_DISCONNECTED_AT: AtomicU32` and `pub fn now_secs() -> u32`; no other module imports (self-contained) |
| `src/config.example.rs` | WIFI_DISCONNECT_REBOOT_TIMEOUT and MQTT_DISCONNECT_REBOOT_SECS constants | VERIFIED | Lines 51-59: both constants present with correct durations (600s and 300s) and doc comments referencing RESIL-01/RESIL-02 |
| `src/wifi.rs` | wifi_supervisor extended with RESIL-01 and RESIL-02 timeout checks | VERIFIED | Lines 68-145: `disconnected_since: Option<Instant>` declared, RESIL-01 block at lines 76-83, RESIL-02 clear at line 87, RESIL-02 read at lines 133-143 |
| `src/mqtt.rs` | MQTT callback writes MQTT_DISCONNECTED_AT on Disconnected/Connected events | VERIFIED | Lines 70-95: both write sites present and correct |
| `src/main.rs` | `mod resil;` declaration | VERIFIED | Line 42: `mod resil;` declared adjacent to `mod watchdog;` as specified |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/wifi.rs` (wifi_supervisor, !connected branch) | `src/resil.rs::MQTT_DISCONNECTED_AT` | `crate::resil::MQTT_DISCONNECTED_AT.store(0, Relaxed)` | WIRED | `wifi.rs:87`: store(0) clears MQTT timer on WiFi drop |
| `src/wifi.rs` (wifi_supervisor, connected branch) | `src/resil.rs::MQTT_DISCONNECTED_AT` | `crate::resil::MQTT_DISCONNECTED_AT.load(Relaxed)` | WIRED | `wifi.rs:135`: load into `mqtt_disc_at`; used in elapsed comparison at line 138 |
| `src/wifi.rs` (wifi_supervisor, connected branch) | `esp_idf_svc::sys::esp_restart` | `unsafe { esp_idf_svc::sys::esp_restart(); }` after log::error! | WIRED | `wifi.rs:82` (RESIL-01) and `wifi.rs:141` (RESIL-02): both reboot calls preceded by log line |
| `src/mqtt.rs` (EventPayload::Disconnected arm) | `src/resil.rs::MQTT_DISCONNECTED_AT` | `compare_exchange(0, now_secs(), Relaxed, Relaxed).ok()` | WIRED | `mqtt.rs:91-95`: compare_exchange stamps disconnect time; only first disconnect sets timer |
| `src/mqtt.rs` (EventPayload::Connected arm) | `src/resil.rs::MQTT_DISCONNECTED_AT` | `store(0, Relaxed)` | WIRED | `mqtt.rs:72`: unconditional clear when MQTT reconnects |
| `src/wifi.rs` (wifi_supervisor) | `src/resil.rs::now_secs()` | `crate::resil::now_secs().saturating_sub(mqtt_disc_at)` | WIRED | `wifi.rs:137`: now_secs() used for elapsed calculation; type is u32 matching MQTT_DISCONNECTED_AT |

---

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| RESIL-01 | 12-01 | `wifi_supervisor` triggers `esp_restart()` if WiFi has not been connected for a configurable duration (default 10 minutes) | SATISFIED | `wifi.rs:76-83`: Option<Instant> timer, WIFI_DISCONNECT_REBOOT_TIMEOUT check, log + restart. `config.example.rs:51-52`: 600s constant. REQUIREMENTS.md marked complete. |
| RESIL-02 | 12-01, 12-02 | MQTT pump signals a reboot timer; if MQTT stays disconnected for a configurable duration after WiFi is up (default 5 minutes), device restarts | SATISFIED | Full loop: `mqtt.rs:91-95` stamps disconnect; `mqtt.rs:72` clears on connect; `wifi.rs:133-143` checks elapsed and calls restart after MQTT_DISCONNECT_REBOOT_SECS (300s). REQUIREMENTS.md marked complete. |

No orphaned requirements — REQUIREMENTS.md table maps both RESIL-01 and RESIL-02 to Phase 12, and both plans claim them.

---

### Notable Deviation: AtomicU64 vs AtomicU32

The PLAN frontmatter (12-01-PLAN.md `must_haves`) specified `AtomicU64` and `u64`, but the actual implementation uses `AtomicU32` throughout (`resil.rs`, `config.example.rs`, `wifi.rs`). This was a documented auto-fix — the ESP32 Xtensa LX6/LX7 target does not support `AtomicU64`. The u32 epoch-second value is sufficient for the 5-minute and 10-minute comparison windows (wraps only after ~136 years). Functional semantics are identical; the PLAN `must_haves` text is superseded by the implementation as documented in 12-01-SUMMARY.md.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/config.example.rs` | 33 | Stale comment: "Phase 12 (RESIL-01) will add esp_restart() at this threshold." | Info | Comment references a planned future change that has now been implemented — the comment is outdated but does not affect behavior. The RESIL-01 timer in `MAX_WIFI_RECONNECT_ATTEMPTS` context is separate from the `WIFI_DISCONNECT_REBOOT_TIMEOUT` path; no functional issue. |

No blocker or warning anti-patterns found.

---

### Human Verification Required

#### 1. RESIL-01 Round-Trip (Success Criterion 3a)

**Test:** Edit `config.rs`: set `WIFI_DISCONNECT_REBOOT_TIMEOUT = Duration::from_secs(30)`. Flash with `cargo espflash flash --release --monitor`. Allow device to boot and connect. Disconnect the WiFi access point. Wait 35+ seconds.
**Expected:** Serial monitor shows `[RESIL-01] WiFi disconnected for Xs — rebooting`, device reboots, then reconnects to WiFi and MQTT and resumes normal heartbeat publishing. Restore timeout to 600s after testing.
**Why human:** Code paths are fully wired but correctness of the Instant-based elapsed measurement and actual esp_restart() behavior on ESP32 hardware cannot be confirmed by static analysis alone. Success Criterion 3 explicitly requires the reconnect-after-reboot to be demonstrated.

#### 2. RESIL-02 Round-Trip (Success Criterion 3b)

**Test:** Edit `config.rs`: set `MQTT_DISCONNECT_REBOOT_SECS = 30`. Flash with `cargo espflash flash --release --monitor`. Allow device to boot and connect. Stop the MQTT broker (or block TCP port 1883) while keeping WiFi active. Wait 35+ seconds.
**Expected:** Serial monitor shows `[RESIL-02] MQTT disconnected for Xs (WiFi up) — rebooting`, device reboots, then MQTT reconnects and resumes normal operation. Restore constant to 300 after testing.
**Why human:** The RESIL-02 path crosses two threads (MQTT callback write, wifi_supervisor read) — correct behavior depends on ESP-IDF MQTT callback firing the Disconnected event reliably when the broker disappears, which requires a live broker and device.

---

### Gaps Summary

No functional gaps. All code paths are fully implemented and wired. The only outstanding item is hardware confirmation of Success Criterion 3 (reboot resolves the stuck state and device reconnects normally), which is inherently a human/hardware concern.

The stale comment at `config.example.rs:33` ("Phase 12 will add esp_restart()") is cosmetic — the feature it anticipated is now implemented.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
