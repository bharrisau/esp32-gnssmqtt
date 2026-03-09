---
phase: 18-telemetry-and-ota-validation
verified: 2026-03-09T00:00:00Z
status: human_needed
score: 9/10 must-haves verified
human_verification:
  - test: "OTA firmware update end-to-end on device FFFEB5"
    expected: "Device downloads canary binary, verifies SHA-256, reboots into new slot, marks slot valid (mark_running_slot_valid appears in /log), publishes at least one heartbeat post-OTA"
    why_human: "Requires physical hardware (ESP32 device FFFEB5), running MQTT broker, and HTTP server on same network. Cannot be verified programmatically."
  - test: "SoftAP captive portal detection on mobile device"
    expected: "iOS or Android device connects to GNSS-Setup WiFi and automatically shows captive portal notification or opens browser to provisioning UI; form renders correctly"
    why_human: "Requires physical mobile device and running firmware on hardware. DNS hijack behavior and browser captive portal detection are OS-specific and cannot be tested programmatically."
  - test: "Heartbeat JSON observed with fix_type/satellites/hdop fields post-OTA"
    expected: "After OTA reboot with active GNSS, heartbeat includes numeric fix_type, satellites, hdop. Before GNSS lock, all three fields are null."
    why_human: "Requires live device with GNSS receiver outputting GGA sentences. Sentinel-to-null JSON path is code-verified but live field population needs hardware."
---

# Phase 18: Telemetry and OTA Validation Verification Report

**Phase Goal:** The health heartbeat reports live GNSS fix quality so operators can assess RTK performance remotely; the OTA update pipeline is validated end-to-end on hardware before v2.0 is marked complete
**Verified:** 2026-03-09T00:00:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Heartbeat JSON includes fix_type, satellites, and hdop fields | VERIFIED | `src/mqtt.rs` lines 397-411: three gnss_state loads + format! string with `fix_type`, `satellites`, `hdop` interpolated |
| 2 | Fields are populated from the most recent GGA sentence received | VERIFIED | `src/nmea_relay.rs` lines 47-49: `ends_with("GGA")` branch calls `parse_gga_into_atomics`; function writes GGA_FIX_TYPE, GGA_SATELLITES, GGA_HDOP_X10 atomics on each matching sentence |
| 3 | When no GGA has been received, all three fields are JSON null | VERIFIED | `src/mqtt.rs` lines 400-403: sentinel checks `== 0xFF` and `== 0xFFFF` emit `"null".to_string()` for each field; `src/gnss_state.rs` initialises atomics at 0xFF/0xFFFF |
| 4 | Clippy -D warnings is clean after all changes | VERIFIED | Three task commits (5d4a04f, 9e74dff, cb02b95) each confirmed clippy clean in SUMMARY; no dead_code allows remain in gnss_state.rs (all atomics written by nmea_relay and read by mqtt) |
| 5 | OTA code pipeline is fully implemented and wired | VERIFIED | `src/ota.rs` contains `spawn_ota`; `src/mqtt.rs` passes `ota_tx` channel (line 33); `src/main.rs` line 252 calls `mark_running_slot_valid` post-boot — all wiring confirmed from Phase 8 |
| 6 | Canary firmware image is built and ready for hardware validation | VERIFIED | Commit dbbf794 adds canary log line to `src/main.rs` (line 82); `testing.md` records SHA-256 `a395675b...`; `testing.md` at project root provides complete hardware checklist |
| 7 | README.md covers all v2.0 features | VERIFIED | README.md (244 lines) confirmed present with all required sections: Overview, Hardware, Features, MQTT Topic Reference (11 topics), First-Time Setup, Building and Flashing, NTRIP Configuration, OTA Firmware Update, LED States, Health Heartbeat, Troubleshooting |
| 8 | OTA end-to-end validated on hardware (device FFFEB5) | HUMAN NEEDED | MAINT-03 hardware sign-off deferred by user approval; testing.md checklist ready; code pipeline verified above |
| 9 | SoftAP captive portal confirmed on mobile device | HUMAN NEEDED | Deferred from Phase 17 Plan 04; consolidated into testing.md Part C; cannot verify without physical mobile device |
| 10 | Heartbeat GNSS fields observed live on hardware | HUMAN NEEDED | testing.md Part B covers this; live GNSS field population requires physical device |

**Score:** 7/7 automated truths verified; 3 items human-only

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/gnss_state.rs` | Shared GGA atomics: GGA_FIX_TYPE, GGA_SATELLITES, GGA_HDOP_X10 | VERIFIED | File exists (22 lines); all three pub statics declared with correct sentinel values (0xFF, 0xFF, 0xFFFF); no dead_code allows present |
| `src/nmea_relay.rs` | GGA parser that updates atomics on each GNGGA/GPGGA sentence | VERIFIED | `parse_gga_into_atomics` function present (lines 87-111); called in Ok arm for all `sentence_type.ends_with("GGA")` sentences (lines 46-49) |
| `src/mqtt.rs` | Heartbeat JSON extended with fix_type, satellites, hdop | VERIFIED | Lines 397-411 load all three atomics, apply sentinel-to-null logic, and interpolate into format! string at line 409 |
| `src/main.rs` | mod gnss_state declaration | VERIFIED | Line 44: `mod gnss_state;` present |
| `src/ota.rs` | OTA pipeline — fully implemented; mark_running_slot_valid present | VERIFIED | `spawn_ota` at line 372; `mark_running_slot_valid` referenced in `main.rs` line 252 (called via EspOtaUpdate handle) |
| `README.md` | Open-source project documentation covering all v2.0 features | VERIFIED | 244 lines; MQTT Topic Reference table (11 topics); all required sections present |
| `testing.md` | Hardware validation checklist for deferred sign-off session | VERIFIED | 108 lines; covers Part A (OTA), Part B (heartbeat fields), Part C (SoftAP); SHA-256 recorded |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/nmea_relay.rs` | `src/gnss_state.rs` | `crate::gnss_state::GGA_FIX_TYPE.store()` | WIRED | Lines 95, 101, 108 of nmea_relay.rs call `.store()` on all three atomics within `parse_gga_into_atomics` |
| `src/mqtt.rs` | `src/gnss_state.rs` | `crate::gnss_state::GGA_FIX_TYPE.load()` | WIRED | Lines 397-399 of mqtt.rs load all three atomics inside `heartbeat_loop` |
| MQTT `gnss/FFFEB5/ota/trigger` | `src/ota.rs spawn_ota` | `ota_tx` channel in mqtt.rs subscriber | WIRED | `mqtt.rs` line 33 accepts `ota_tx: SyncSender<Vec<u8>>`; line 126 calls `ota_tx.try_send()`; `spawn_ota` consumes receiver end |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| TELEM-01 | 18-01-PLAN, 18-03-PLAN | Health heartbeat includes GNSS fix type, satellite count, and HDOP parsed from the most recent GGA sentence | SATISFIED | `src/gnss_state.rs` (atomics) + `src/nmea_relay.rs` (parser) + `src/mqtt.rs` (heartbeat output) — full pipeline implemented and wired; all three code artifacts verified substantive and connected |
| MAINT-03 | 18-02-PLAN, 18-03-PLAN | OTA firmware update validated on hardware (device FFFEB5) as explicit sign-off gate before v2.0 milestone is marked complete | NEEDS HUMAN | OTA code pipeline fully implemented (Phase 8); canary build ready (commit dbbf794); `testing.md` provides step-by-step checklist; hardware sign-off session not yet completed — approved deferral to end of milestone |

Note: REQUIREMENTS.md traceability table marks both TELEM-01 and MAINT-03 as "Complete" for Phase 18. TELEM-01 is code-complete and verifiable. MAINT-03 requires hardware sign-off per its own definition ("explicit sign-off gate") — the code prerequisite is met; the gate itself is the pending human step.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/main.rs` | 82 | `log::info!("esp32-gnssmqtt v2.0-ota-canary — OTA validation build")` — canary log line intentionally left in | Info | Documented in testing.md and 18-02-SUMMARY: decision on keep/revert deferred to sign-off session; not a code quality issue |

No blockers or warnings found. The canary line is intentional and documented.

### Human Verification Required

#### 1. OTA Firmware Update End-to-End (MAINT-03)

**Test:** Follow `testing.md` Part A on device FFFEB5. Serve canary binary via `python3 -m http.server 8080`. Publish OTA trigger with SHA-256 `a395675b9d8fc951070100dfedacedc27881eb0585be11a6d52543aeac611dda`. Monitor `gnss/FFFEB5/ota/status`, `gnss/FFFEB5/log`, `gnss/FFFEB5/heartbeat`.

**Expected:**
- `/ota/status` transitions to `downloading`
- Device reboots
- `/log` shows `esp32-gnssmqtt v2.0-ota-canary` after reboot
- `/log` shows `mark_running_slot_valid` (confirms new slot marked valid, no rollback)
- Device reconnects and publishes at least one heartbeat

**Why human:** Requires physical ESP32 device, running MQTT broker, and HTTP file server on the same network.

#### 2. Heartbeat GNSS Fields Live Verification (TELEM-01 live)

**Test:** Follow `testing.md` Part B. After OTA reboot with GNSS receiver active, subscribe to `gnss/FFFEB5/heartbeat` and examine the JSON payload.

**Expected:**
- Heartbeat JSON contains `fix_type`, `satellites`, `hdop` fields
- Before GNSS lock or before first GGA: all three fields are `null`
- With active GNSS fix: `fix_type` shows a numeric value (4 = RTK Fixed, 5 = RTK Float, 1 = SPS); `satellites` shows integer count; `hdop` shows float like `"0.9"`

**Why human:** Live GNSS receiver required to generate GGA sentences; field population depends on receiver outputting GNGGA/GPGGA sentences.

#### 3. SoftAP Captive Portal Detection on Mobile Device (Phase 17 deferred)

**Test:** Follow `testing.md` Part C. Trigger SoftAP via GPIO9 or MQTT `"softap"` payload. Connect iOS or Android device to `GNSS-Setup` open WiFi network. Verify captive portal behaviour.

**Expected:**
- Mobile OS shows captive portal notification or automatically opens browser
- Provisioning web form (WiFi SSIDs, MQTT broker fields) renders correctly on mobile browser
- Device returns to normal station-mode operation after 300-second no-client timeout or reboot

**Why human:** Requires physical mobile device; captive portal detection behaviour is OS-specific (differs between iOS and Android) and cannot be tested programmatically.

### Summary

Phase 18 automated implementation is complete and fully verified:

**TELEM-01 (code complete):** `src/gnss_state.rs` provides three module-level atomics with correct sentinel values. `src/nmea_relay.rs` parses all GGA sentence variants via `ends_with("GGA")` and updates all three atomics with proper empty-field guards. `src/mqtt.rs` heartbeat loop reads the atomics, applies sentinel-to-null conversion, and includes `fix_type`, `satellites`, `hdop` in the published JSON. The full pipeline is wired end-to-end in the codebase.

**MAINT-03 (hardware pending):** The OTA code pipeline has been implemented since Phase 8 and is fully wired. A canary firmware build is available (SHA-256 recorded in `testing.md`). The `testing.md` checklist at project root consolidates all three deferred hardware validations (OTA update, TELEM-01 live observation, SoftAP captive portal) into a single sign-off session. Deferral was explicitly approved by the user.

**README.md:** Complete open-source project documentation covering all v2.0 subsystems. MQTT topic reference table documents all 11 topics.

The phase goal is achieved at the code level. Hardware sign-off is the remaining gate before v2.0 milestone tag.

---

_Verified: 2026-03-09T00:00:00Z_
_Verifier: Claude (gsd-verifier)_
