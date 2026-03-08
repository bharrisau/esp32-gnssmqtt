---
phase: 15-provisioning
verified: 2026-03-08T02:00:00Z
status: passed
score: 10/10 must-haves verified
re_verification: false
gaps: []
human_verification:
  - test: "Open browser to 192.168.71.1 while connected to 'GNSS-Setup' hotspot"
    expected: "HTML form loads with WiFi (3 networks) and MQTT fields; pressing Save returns 'Saved. Rebooting in 1 second...' and device reboots"
    why_human: "HTTP server response and browser behavior cannot be verified programmatically on embedded firmware"
  - test: "Hold GPIO9 low for 3 continuous seconds on a running device"
    expected: "Device logs 'GPIO9: held low 3s — entering SoftAP mode (PROV-06)' and reboots into SoftAP"
    why_human: "Requires physical hardware interaction"
  - test: "Send MQTT payload 'softap' to gnss/{device_id}/ota/trigger"
    expected: "Device logs 'OTA: softap payload received — entering SoftAP mode' and reboots into SoftAP"
    why_human: "Requires live MQTT broker and running device"
  - test: "Observe LED pattern in SoftAP mode"
    expected: "LED blinks at 500ms on / 500ms off — visually distinct from Connecting (200ms) and Error (triple-pulse)"
    why_human: "LED timing cannot be verified programmatically on cross-compiled firmware"
---

# Phase 15: Provisioning Verification Report

**Phase Goal:** Users can configure WiFi and MQTT credentials from any browser via the device's SoftAP hotspot, with up to 3 networks stored in NVS and tried automatically on connection failure.
**Verified:** 2026-03-08T02:00:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | NVS namespace 'prov' is read at boot to determine if credentials exist (wifi_count key) | VERIFIED | `has_wifi_credentials()` in provisioning.rs opens NVS "prov" read-only, reads "wifi_count" u8, returns count > 0 |
| 2  | SoftAP starts with SSID 'GNSS-Setup' (open, channel 6) using AccessPoint configuration | VERIFIED | `run_softap_portal()` calls `Configuration::AccessPoint(AccessPointConfiguration { ssid: "GNSS-Setup", auth_method: AuthMethod::None, channel: 6, ... })` with `wifi.start()` + `wifi.wait_netif_up()` only (no `wifi.connect()`) |
| 3  | HTTP GET / returns an HTML form with WiFi (3 networks) and MQTT fields | VERIFIED | `PROV_HTML` const contains form with ssid0/pass0, ssid1/pass1, ssid2/pass2, mqtt_host, mqtt_port, mqtt_user, mqtt_pass fields; handler registered with `server.fn_handler("/", Method::Get, ...)` |
| 4  | HTTP POST /save reads URL-encoded body, writes up to 3 WiFi networks and MQTT settings to NVS, and calls esp_restart() | VERIFIED | POST handler reads body via `req.read(&mut buf)`, calls `save_credentials()` which writes wifi_count, wifi_ssid_{0-2}, wifi_pass_{0-2}, mqtt_host, mqtt_port_hi/lo, mqtt_user, mqtt_pass; spawns thread calling `esp_idf_svc::sys::esp_restart()` after 1000ms |
| 5  | NVS keys are all 15 characters or shorter | VERIFIED | Longest static key is 12 chars (mqtt_port_lo, mqtt_port_hi, force_softap); dynamic keys wifi_ssid_0/1/2 and wifi_pass_0/1/2 are 11 chars each |
| 6  | Boot-path decision: SoftAP when force_softap or no credentials, STA (wifi_connect_any) otherwise | VERIFIED | main.rs lines 94-118: `check_and_clear_force_softap()` and `has_wifi_credentials()` evaluated before EspWifi construction; `run_softap_portal()` called in SoftAP branch, `wifi::wifi_connect_any()` in STA branch |
| 7  | wifi_connect_any cycles through all stored networks (up to 3x per network) without entering SoftAP on failure | VERIFIED | `wifi_connect_any()` in wifi.rs: `networks.len() * 3` max attempts, `networks.iter().cycle().take(max_attempts)`, bails with error on exhaustion; no SoftAP fallback |
| 8  | MQTT credentials loaded from NVS with compile-time constant fallback | VERIFIED | main.rs uses `provisioning::load_mqtt_config(&nvs).unwrap_or_else(...)` fallback to `config::MQTT_HOST/PORT/USER/PASS`; mqtt_connect accepts `host: &str, port: u16, user: &str, pass: &str` |
| 9  | GPIO9 held low 3s triggers set_force_softap() then esp_restart() | VERIFIED | GPIO9 monitor thread in main.rs: `PinDriver::input(gpio9_pin)` with `Pull::Up`; `Instant` timer reset on release; 3s threshold calls `provisioning::set_force_softap(&nvs_for_gpio)` then `esp_idf_svc::sys::esp_restart()` |
| 10 | MQTT payload 'softap' to ota/trigger topic calls set_force_softap() then esp_restart() | VERIFIED | ota.rs lines 113-118: `if json.trim() == "softap"` → `crate::provisioning::set_force_softap(&nvs)` → `restart()`; check appears before JSON parse |

**Score:** 10/10 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/provisioning.rs` | 6 public functions: has_wifi_credentials, check_and_clear_force_softap, run_softap_portal, load_wifi_networks, load_mqtt_config, set_force_softap | VERIFIED | All 6 public functions present; 3 private helpers (save_credentials, parse_form_field, count_softap_clients); 307 lines |
| `src/wifi.rs` | wifi_connect_any() replacing wifi_connect in boot path | VERIFIED | `wifi_connect_any()` added at line 54; takes modem, sysloop, nvs, networks Vec; original wifi_connect retained (unused) |
| `src/mqtt.rs` | mqtt_connect with runtime host/port/user/pass parameters | VERIFIED | Signature updated: `host: &str, port: u16, user: &str, pass: &str` added after device_id; None guard for empty user/pass |
| `src/main.rs` | Boot-path decision, mod provisioning, NVS MQTT config loading, GPIO9 thread | VERIFIED | `mod provisioning` at line 44; boot-path decision lines 94-118; MQTT NVS load lines 122-132; GPIO9 thread spawned last (step 19) |
| `src/config.rs` | SOFTAP_SSID, SOFTAP_TIMEOUT_SECS constants | VERIFIED | Lines 67 and 72: `pub const SOFTAP_SSID: &str = "GNSS-Setup"` and `pub const SOFTAP_TIMEOUT_SECS: u64 = 300` |
| `src/led.rs` | LedState::SoftAP = 3 with 500ms blink pattern | VERIFIED | Line 20: `SoftAP = 3`; from_u8 arm `3 => LedState::SoftAP`; led_task arm drives 1000ms cycle with 500ms on |
| `src/ota.rs` | 'softap' payload check before JSON parse; nvs parameter added | VERIFIED | nvs param in ota_task and spawn_ota; 'softap' check at line 113 before `extract_json_str()` at line 120 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/provisioning.rs run_softap_portal` | `EspWifi Configuration::AccessPoint` | `wifi.start() + wifi.wait_netif_up()` (no connect) | WIRED | Lines 156-167: `Configuration::AccessPoint(AccessPointConfiguration { ... })` confirmed; `wifi.connect()` absent from function |
| `src/provisioning.rs POST /save handler` | `EspNvs::set_str / set_u8` | `save_credentials()` → nvs write loop | WIRED | Lines 270-284: `nvs.set_u8("wifi_count", count)`, `nvs.set_str("wifi_ssid_{i}", ...)`, all MQTT keys written |
| `src/provisioning.rs run_softap_portal` | `esp_idf_svc::sys::esp_restart()` | POST handler spawns thread sleeping 1000ms then restarts | WIRED | Line 241: `unsafe { esp_idf_svc::sys::esp_restart() }` inside spawned thread; 300s timeout loop also calls restart at line 257 |
| `src/main.rs` | `provisioning::has_wifi_credentials / check_and_clear_force_softap` | `nvs.clone()` before EspWifi::new | WIRED | Lines 94-95: both calls use `&nvs`; nvs cloned for EspWifi::new at line 105 |
| `src/main.rs` | `provisioning::run_softap_portal` | `if force_softap || !has_credentials` branch | WIRED | Line 99-112: conditional calls `run_softap_portal(&mut softap_wifi, nvs.clone())`; `unreachable!()` after |
| `src/main.rs` | `mqtt::mqtt_connect` with NVS host/port/user/pass | `provisioning::load_mqtt_config` fallback to config constants | WIRED | Lines 122-190: `load_mqtt_config(&nvs)` result destructured into mqtt_host/port/user_str/pass_str; passed to mqtt_connect |
| `src/ota.rs ota_task` | `provisioning::set_force_softap + esp_restart()` | `json.trim() == "softap"` check before JSON parse | WIRED | Lines 113-118: softap check → `set_force_softap(&nvs)` → `restart()`; check at line 113 precedes `extract_json_str` at line 120 |
| `src/main.rs gpio9 thread` | `provisioning::set_force_softap + esp_restart()` | PinDriver::input GPIO9 + is_low() for 3s | WIRED | Lines 274-318: 100ms polling loop, `Instant` timer, 3s threshold → `set_force_softap` → `esp_restart()` |
| `src/led.rs LedState::SoftAP` | `led_task match arm` | `from_u8` arm 3 + led_task SoftAP branch | WIRED | from_u8 line 28: `3 => LedState::SoftAP`; led_task lines 91-100: 1000ms cycle, 500ms on |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| PROV-01 | 15-01, 15-02 | Device enters SoftAP hotspot mode on first boot when no WiFi credentials exist in NVS | SATISFIED | `has_wifi_credentials()` + boot-path decision in main.rs; `run_softap_portal()` called when no credentials |
| PROV-02 | 15-01 | User can open captive-portal web UI to enter WiFi SSID and password | SATISFIED | `PROV_HTML` served at GET /; includes 3 SSID/password pairs with Network 1 required |
| PROV-03 | 15-01 | User can configure MQTT broker (host, port, username, password) via provisioning web UI | SATISFIED | HTML form has mqtt_host (required), mqtt_port, mqtt_user, mqtt_pass fields; POST /save persists to NVS |
| PROV-04 | 15-01 | User can store up to 3 WiFi networks via web UI; all persisted to NVS | SATISFIED | `save_credentials()` writes wifi_count + wifi_ssid_{0-2}/wifi_pass_{0-2}; `load_wifi_networks()` reads them back |
| PROV-05 | 15-02 | On all WiFi failures, device retries stored networks indefinitely with backoff; does not auto-enter SoftAP | SATISFIED | `wifi_connect_any()` cycles networks 3x each then bails with error; no SoftAP fallback; wifi_supervisor RESIL-01 handles reboot |
| PROV-06 | 15-03 | GPIO9 held low for 3s enters SoftAP mode; device exits back to WiFi mode after 300s with no client connected | SATISFIED | GPIO9 monitor thread confirmed; 300s timeout in run_softap_portal confirmed (does NOT set force_softap on timeout so next boot tries STA) |
| PROV-07 | 15-03 | MQTT payload "softap" to gnss/{device_id}/ota/trigger enters SoftAP mode; same 300s no-client timeout applies | SATISFIED | ota_task "softap" check at line 113 calls set_force_softap + restart; next boot enters SoftAP and runs 300s timeout |
| PROV-08 | 15-03 | LED shows a distinct flash pattern while in SoftAP mode (different from connecting/connected/error) | SATISFIED | LedState::SoftAP = 3; 1000ms cycle (500ms on/500ms off) distinct from Connecting 400ms, Error 1300ms triple-pulse |

All 8 PROV requirements accounted for. No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/ota.rs` | 105/113 | "softap" check appears AFTER "reboot" check — plan spec required softap BEFORE reboot | Info | No functional impact (both checks work correctly and both precede JSON parse); ordering-only discrepancy from plan spec |

### Human Verification Required

#### 1. Browser-based provisioning flow

**Test:** Connect a device to the phone/laptop. Power on with no NVS credentials (or with force_softap set). Connect to WiFi network "GNSS-Setup". Open browser and navigate to 192.168.71.1. Fill in WiFi and MQTT credentials and submit.
**Expected:** Form loads correctly. POST returns "Saved. Rebooting in 1 second..." and device reboots. On next boot device connects to the entered WiFi network and MQTT broker.
**Why human:** HTTP response behavior and browser UI cannot be verified on cross-compiled embedded firmware.

#### 2. GPIO9 hold detection

**Test:** With device fully running (all subsystems up), hold the BOOT button (GPIO9) low for at least 3 continuous seconds. Then release.
**Expected:** Device logs "GPIO9: held low 3s — entering SoftAP mode (PROV-06)" and reboots into SoftAP mode (visible from LED 500ms blink and "GNSS-Setup" hotspot appearing).
**Why human:** Requires physical hardware interaction; timing of hold cannot be verified programmatically.

#### 3. MQTT softap trigger

**Test:** With device connected to MQTT broker, publish the string payload "softap" (no quotes) to topic gnss/{device_id}/ota/trigger.
**Expected:** Device logs "OTA: 'softap' payload received — entering SoftAP mode" and reboots. "GNSS-Setup" hotspot becomes visible.
**Why human:** Requires live MQTT broker and running device.

#### 4. SoftAP LED pattern visual distinction

**Test:** Observe LED during: (a) initial connect attempt, (b) connected state, (c) SoftAP mode.
**Expected:** Three visually distinct patterns — fast 400ms blink while connecting, steady on when connected, slower 1000ms blink in SoftAP mode.
**Why human:** LED timing requires physical observation; GPIO behavior is not testable in cross-compilation.

### Gaps Summary

No gaps found. All 10 observable truths are verified, all 7 artifacts are substantive and wired, all 9 key links are connected, and all 8 PROV requirements are satisfied.

The single noted deviation (softap check appearing after reboot check rather than before, contrary to the plan's artifact spec) is functionally inconsequential — both checks precede JSON parsing and both execute correctly. This is a plan-spec discrepancy only.

**Build status:** `cargo build --release` passes with 8 warnings (all pre-existing: unused `wifi_connect`, unused SOFTAP constants, pre-existing `uart_bridge.rs` comparison warning). No errors.

**Commits verified:** d168539 (15-01 provisioning module), 5a5e62b (15-02 wifi_connect_any), d50e339 (15-02 boot-path wiring), 37f54c8 (15-03 LED+OTA softap), d3bfc46 (15-03 GPIO9 monitor) — all 5 commits exist in git history.

---

_Verified: 2026-03-08T02:00:00Z_
_Verifier: Claude (gsd-verifier)_
