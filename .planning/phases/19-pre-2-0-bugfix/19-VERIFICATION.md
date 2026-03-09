---
phase: 19-pre-2-0-bugfix
verified: 2026-03-09T15:00:00Z
status: passed
score: 9/9 must-haves verified
re_verification: false
---

# Phase 19: Pre-2.0 Bugfix Verification Report

**Phase Goal:** Fix known bugs (BUG-1 DHCP DNS, BUG-2 Android captive portal, BUG-3/BUG-4 NVS TLS default, FEAT-1 boot button rework) to enable v2.0 milestone close.
**Verified:** 2026-03-09T15:00:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SoftAP DHCP leases serve DNS = 192.168.71.1 (not 8.8.8.8) | VERIFIED | `EspNetif::new_with_conf` with `RouterConfiguration { dns: Some(Ipv4Addr::new(192, 168, 71, 1)) }` at main.rs line 135-146; `WifiDriver::new` + `EspWifi::wrap_all` at lines 147-153 |
| 2 | Android captive portal `/generate_204` handler returns 302 | VERIFIED | `server.fn_handler("/generate_204", ...)` returning `302 Found` with `Location: http://192.168.71.1/` at provisioning.rs line 265-268 |
| 3 | Unsafe post-`wait_netif_up` DHCP block removed from provisioning.rs | VERIFIED | No `esp_netif_dhcps_stop`, `esp_netif_set_dns_info`, or `esp_netif_dhcps_option` anywhere in provisioning.rs; grep confirms zero matches |
| 4 | After OTA from old firmware, MQTT connects (TLS default is false) | VERIFIED | `load_mqtt_config` reads `mqtt_tls` key with `.unwrap_or(None).unwrap_or(0) != 0` — key absence defaults to false; provisioning.rs lines 128-130 |
| 5 | Saving credentials writes `mqtt_tls=0` and `config_ver=1` to NVS | VERIFIED | `save_credentials` at provisioning.rs lines 426-427: `nvs.set_u8("mqtt_tls", 0)` and `nvs.set_u8("config_ver", 1)` |
| 6 | `load_mqtt_config` returns a 5-tuple including tls bool | VERIFIED | Return type `Option<(String, u16, String, String, bool)>` at provisioning.rs line 99; wired to `mqtt_connect` via `mqtt_tls` in main.rs line 252 |
| 7 | Holding GPIO9 for 3s causes LED to flash rapidly (ButtonHold pattern) | VERIFIED | `BtnPhase::Idle` → `BtnPhase::Warning` transition at main.rs line 434-437 stores `LedState::ButtonHold as u8`; `led.rs` line 107-116 drives 100ms/100ms cycle |
| 8 | Releasing GPIO9 between 3s–10s enters SoftAP; holding past 10s causes LED off then factory reset | VERIFIED | `BtnPhase::Warning` release → `set_force_softap` + `esp_restart` at main.rs lines 449-454; `BtnPhase::Danger` transition stores `LedState::Off` at line 440-441; Danger release calls `nvs_flash_erase()` + `esp_restart()` at lines 455-462 |
| 9 | Factory reset does NOT touch OTA partition slots | VERIFIED | Only `nvs_flash_erase()` called — no `esp_ota_set_boot_partition` or OTA slot manipulation anywhere in the Danger release path |

**Score:** 9/9 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/main.rs` | SoftAP construction uses `WifiDriver::new` + `EspWifi::wrap_all` with pre-configured `ap_netif` | VERIFIED | Lines 135-155; `dns: Some(Ipv4Addr::new(192, 168, 71, 1))` in `RouterConfiguration` at line 142 |
| `src/main.rs` | `load_mqtt_config` 5-tuple destructured; `mqtt_tls` passed to `mqtt_connect` | VERIFIED | Lines 170-180 destructure; line 252 passes `mqtt_tls` as 6th argument |
| `src/main.rs` | `led_state_btn` clone + `BtnPhase` state machine in GPIO9 thread | VERIFIED | `led_state_btn` cloned at line 103 (before `led_state` moves into `led_task`); `BtnPhase` enum defined inside thread closure at line 423 |
| `src/provisioning.rs` | `run_softap_portal` has no unsafe DHCP block; uses pre-configured netif comment | VERIFIED | Lines 173-175 document the new approach; all four dangerous FFI symbols absent from file |
| `src/provisioning.rs` | `load_mqtt_config` returns 5-tuple with tls defaulting false | VERIFIED | Line 130: `.unwrap_or(None).unwrap_or(0) != 0`; line 132 returns `(host, port, user, pass, tls)` |
| `src/provisioning.rs` | `save_credentials` writes `mqtt_tls=0` and `config_ver=1` | VERIFIED | Lines 426-427 |
| `src/mqtt.rs` | `mqtt_connect` accepts `tls: bool`; `broker_url` switches scheme | VERIFIED | Parameter at line 30; `broker_url` if/else at lines 40-44 |
| `src/led.rs` | `LedState::ButtonHold = 4` with 100ms/100ms pattern; `LedState::Off = 5` with steady-off | VERIFIED | Enum variants at lines 23-24; `from_u8` arms at lines 33-34; `led_task` match arms at lines 107-121 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `main.rs` SoftAP path | `provisioning.rs run_softap_portal` | `BlockingWifi<EspWifi<'static>>` built with `wrap_all` + `ap_netif` | WIRED | `run_softap_portal(&mut softap_wifi, nvs.clone())` at main.rs line 157; `ap_netif` carries pre-configured DNS |
| `EspNetif::new_with_conf RouterConfiguration` | DHCP DNS offer = 192.168.71.1 | `dns: Some(Ipv4Addr::new(192, 168, 71, 1))` field | WIRED | Field present at main.rs line 142; research confirmed this triggers `set_dns()` + `esp_netif_dhcps_option(OFFER_DNS)` during construction |
| `provisioning.rs save_credentials` | NVS namespace "prov" | `nvs.set_u8("mqtt_tls", 0)` and `nvs.set_u8("config_ver", 1)` | WIRED | Lines 426-427; namespace "prov" opened at line 414 |
| `provisioning.rs load_mqtt_config` | `main.rs mqtt_connect` call | tls bool returned in 5-tuple, passed as `mqtt_tls` argument | WIRED | main.rs lines 170-252: destructure → `mqtt_tls` → `mqtt_connect(..., mqtt_tls, ...)` |
| `main.rs GPIO9 thread` | `led.rs LedState::ButtonHold` | `led_state_btn.store(LedState::ButtonHold as u8, Relaxed)` | WIRED | main.rs line 436; `led_state_btn` is `Arc<AtomicU8>` clone of the same atomic the LED task polls |
| `main.rs BtnPhase::Danger release` | `esp_idf_svc::sys::nvs_flash_erase()` | `unsafe` block on button release after 10s | WIRED | main.rs line 458; no OTA calls follow |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| BUG-1 | 19-01 | DHCP DNS override not surviving `wifi.start()` in SoftAP mode | SATISFIED | `EspNetif::new_with_conf` with `RouterConfiguration.dns` pre-configures DNS before start; post-start unsafe block removed |
| BUG-2 | 19-01 | Android captive portal 302 redirect | SATISFIED | `/generate_204` returns `302 Found` with `Location: http://192.168.71.1/` at provisioning.rs line 265-268; unblocked by BUG-1 fix |
| BUG-3 | 19-02 | NVS TLS absent key defaults to `true` causing MQTT failure post-OTA | SATISFIED | `unwrap_or(0)` ensures absence = false; confirmed in `load_mqtt_config` line 130 |
| BUG-4 | 19-02 | MQTT fails post-OTA due to wrong TLS default | SATISFIED | Caused by BUG-3; resolved by same fix; `save_credentials` writes `mqtt_tls=0` for forward compatibility |
| FEAT-1 | 19-03 | Boot button rework: 3s LED flash warning, 10s LED off + factory reset | SATISFIED | Full 3-phase state machine implemented; `ButtonHold` and `Off` LED states present; `nvs_flash_erase()` called on 10s+ release |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | — | — | None found |

No `TODO/FIXME/PLACEHOLDER` comments in modified files. No empty implementations. No stub return values in any of the four modified source files (`src/main.rs`, `src/provisioning.rs`, `src/mqtt.rs`, `src/led.rs`). The only `unsafe` blocks in `provisioning.rs` are for `esp_restart()` and `esp_wifi_ap_get_sta_list()` — both correct uses of ESP-IDF FFI.

### Human Verification Required

#### 1. SoftAP DHCP DNS on device

**Test:** Connect a phone to the "GNSS-Setup" SoftAP. Check the DNS server address assigned in the DHCP lease (network details on Android/iOS or `ipconfig /all` on Windows).
**Expected:** DNS server = 192.168.71.1 (not 8.8.8.8).
**Why human:** Cannot observe DHCP lease contents from firmware source analysis alone; requires on-device behaviour.

#### 2. Android captive portal sign-in prompt

**Test:** With BUG-1 fixed (DNS resolving to 192.168.71.1), connect an Android device to "GNSS-Setup". Observe whether the OS automatically shows the captive portal sign-in notification.
**Expected:** Android captive portal notification appears; tapping opens the provisioning form.
**Why human:** Requires live Android device; depends on OS behaviour (DNS interception + HTTP probe).

#### 3. Post-OTA MQTT connection with old NVS

**Test:** Flash old firmware (pre-Phase-19) to a device, provision with credentials (so NVS contains no `mqtt_tls` key), then OTA-update to Phase-19 firmware. Observe MQTT connection in logs.
**Expected:** MQTT connects successfully; log shows `tls=false`.
**Why human:** Requires two firmware versions and hardware; cannot simulate NVS key-absence in static analysis.

#### 4. Boot button 3-phase behaviour on hardware

**Test:** Hold GPIO9 for 3s, observe LED pattern; release; verify SoftAP boot. Separately: hold GPIO9 past 10s, observe LED off, then release and verify NVS erased + reboot.
**Expected:** 3s → fast flash (100ms); 3–10s release → SoftAP; 10s → steady off; 10s+ release → all credentials erased.
**Why human:** Requires physical button and hardware; LED timing cannot be verified from source alone; factory reset outcome (NVS erased) requires post-reboot provisioning attempt.

### Gaps Summary

No gaps found. All five requirements (BUG-1, BUG-2, BUG-3, BUG-4, FEAT-1) are fully implemented and wired. All committed changes are substantive (not stubs). All key links between modules are connected and confirmed.

The four human verification items are runtime/hardware behaviours that cannot be confirmed statically. They align with the existing `testing.md` hardware sign-off checklist. No new gaps are introduced by Phase 19.

---

_Verified: 2026-03-09T15:00:00Z_
_Verifier: Claude (gsd-verifier)_
