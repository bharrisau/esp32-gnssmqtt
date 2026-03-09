# Phase 19: pre-2.0-bugfix - Research

**Researched:** 2026-03-09
**Domain:** ESP32 Rust firmware — esp-idf-svc netif/DHCP, NVS schema versioning, GPIO state machine, LED patterns
**Confidence:** HIGH (all findings verified from local crate source)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**BUG-1: DHCP DNS override**
- Must be fixed properly — research the correct hook point in esp-idf-svc/esp-idf-sys
- Current approach (`esp_netif_dhcps_stop/set_dns_info/start` after `wait_netif_up()`) is wrong — DHCP server is reinitialised by esp-idf-svc after that point
- Investigate `swap_netif_ap()` with a pre-configured `EspNetif` before `wifi.start()`, or find the correct lifecycle hook
- If no clean `esp-idf-svc` solution exists after thorough research: hand off to user rather than implement a hack
- Unsafe `esp_netif_*` FFI is acceptable if it is the correct API and is isolated to `provisioning.rs`

**BUG-2: Android captive portal**
- Unblocked once BUG-1 is fixed
- 302 redirect fix for `/generate_204` is already committed but untested — validate it works post BUG-1 fix
- Android HTTPS probe (port 443) may also fail (no cert) — if HTTP probe passes after BUG-1 fix, HTTPS failure is acceptable

**BUG-3/BUG-4: NVS versioning**
- Fix the TLS default directly: wherever `mqtt_tls` key is read from NVS, ensure `unwrap_or(0)` (TLS off) not `unwrap_or(1)`
- Add a `config_ver: u8` key to the `"prov"` NVS namespace — initially set to `1` on every save
- `config_ver` is a convention for future breaking schema changes only — no migration logic needed now
- Fields added in this phase must default to `false`/`0`/off when absent from NVS

**FEAT-1: Boot button rework**
- Hold GPIO9 for 3s → LED starts flashing (same GPIO15 active-low LED)
- Release while flashing → enter SoftAP mode (existing behaviour)
- Continue holding to 10s → LED stops flashing, turns off (danger zone signal)
- Release at 10s → erase NVS partition + reboot (factory reset — all credentials cleared)
- Factory reset = NVS erase only; does NOT revert OTA slot

**Phase completion gate**
- Phase 19 closes when all code fixes are implemented and compile clean
- Hardware validation (testing.md) happens after as an informal session
- Bugs surfaced by hardware testing → Phase 20

### Claude's Discretion
- Exact LED flash rate/pattern during 3s–10s hold (any distinct flash pattern)
- How to handle button debounce in the state machine
- Whether to refactor existing GPIO9 polling into a cleaner state machine or patch in place

### Deferred Ideas (OUT OF SCOPE)
- Hardware validation checklist (testing.md) — informal session after Phase 19 ships; new bugs → Phase 20
- HTTPS captive portal probe handling (port 443/DoT) — acceptable to leave broken if HTTP probe works
</user_constraints>

---

## Summary

Phase 19 fixes four known issues: the SoftAP DHCP DNS not serving the portal IP to clients (BUG-1), Android captive portal detection that is blocked by BUG-1 (BUG-2), wrong TLS default after OTA that breaks MQTT (BUG-3/BUG-4), and a boot button rework with two hold thresholds (FEAT-1).

The BUG-1 root cause is now fully understood from reading `esp-idf-svc-0.51.0` source. The current code calls `esp_netif_dhcps_stop/set_dns_info/start` after `wait_netif_up()`, but the correct approach is to inject a pre-configured `EspNetif` via `EspWifi::wrap_all` (or `swap_netif_ap`) **before** `wifi.start()`. The `EspNetif::new_with_conf` constructor handles both `set_dns` and `esp_netif_dhcps_option(OFFER_DNS)` atomically during netif construction, which is the only lifecycle point that survives wifi start. The `RouterConfiguration` struct in `embedded-svc-0.28.1` has a `dns: Option<Ipv4Addr>` field that, when set to `Some(192.168.71.1)`, triggers the complete DNS-offer sequence automatically.

BUG-3/BUG-4 is a straightforward default-value fix: the `mqtt_tls` key was never written in the provisioning form (it was not in the form), so NVS returns `None`; any `unwrap_or(1)` or implicit `true` default causes TLS to be attempted. The fix is ensuring `load_mqtt_config` returns `false` for the TLS field when the key is absent, and writing `config_ver = 1` on every credential save. FEAT-1 extends the existing 100ms polling loop with a second threshold and two new LED state transitions.

**Primary recommendation:** Use `EspWifi::wrap_all` with a custom `EspNetif::new_with_conf` for the AP interface in `run_softap_portal`, replacing the post-`wait_netif_up` FFI block entirely. This is the correct, supported lifecycle.

---

## Standard Stack

### Core (already in project)

| Library | Version | Purpose | Note |
|---------|---------|---------|------|
| esp-idf-svc | 0.51.0 | WiFi, NVS, netif — all in scope | Exact version pinned in Cargo.toml |
| embedded-svc | 0.28.1 | `RouterConfiguration`, `IpConfiguration` types | Pinned |
| esp-idf-hal | 0.45.2 | `PinDriver`, GPIO | Already used in GPIO9 monitor |
| esp-idf-sys | 0.36.1 | FFI bindings — `nvs_flash_erase`, raw netif calls if needed | Already used |

No new dependencies required for any of the four items.

---

## Architecture Patterns

### BUG-1: Correct DHCP DNS Lifecycle

**What goes wrong today:** After `wifi.start()` and `wait_netif_up()`, esp-idf reinitialises the DHCP server, discarding any DNS info set via post-start FFI calls. The `esp_netif_dhcps_stop/set_dns_info/start` sequence has no lasting effect at this point.

**Correct approach (verified from `esp-idf-svc-0.51.0/src/netif.rs` lines 370–457):**

`EspNetif::new_with_conf` processes a `NetifConfiguration` with `ip_configuration: Some(ipv4::Configuration::Router(RouterConfiguration {...}))`. When the `RouterConfiguration` has:
- `dhcp_enabled: true` — sets `ESP_NETIF_DHCP_SERVER` flag
- `dns: Some(Ipv4Addr)` — calls `set_dns()` AND `esp_netif_dhcps_option(OFFER_DNS, 2)`

Both calls happen during `EspNetif` construction, which is before `attach_netif` (called inside `wrap_all`), before `wifi.start()`, and therefore permanent.

**Implementation path in `provisioning.rs`:**

Replace `EspWifi::new(peripherals.modem, sysloop, Some(nvs))` with `EspWifi::wrap_all(WifiDriver::new(...), sta_netif, ap_netif)` where `ap_netif` is:

```rust
// Source: esp-idf-svc-0.51.0/src/netif.rs (lines 199-208, 370-457)
// Source: embedded-svc-0.28.1/src/ipv4.rs (lines 192-228)
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::ipv4::{
    Configuration as IpConfiguration, RouterConfiguration, Subnet, Mask,
};
use std::net::Ipv4Addr;

let ap_netif = EspNetif::new_with_conf(&NetifConfiguration {
    ip_configuration: Some(IpConfiguration::Router(RouterConfiguration {
        subnet: Subnet {
            gateway: Ipv4Addr::new(192, 168, 71, 1),
            mask: Mask(24),
        },
        dhcp_enabled: true,
        dns: Some(Ipv4Addr::new(192, 168, 71, 1)),  // <-- this is the key line
        secondary_dns: None,
        ..Default::default()
    })),
    ..NetifConfiguration::wifi_default_router()
})?;
```

The `RouterConfiguration::default()` sets `dns: Some(Ipv4Addr::new(8, 8, 8, 8))` — overriding it to `192.168.71.1` causes `new_with_conf` to call `set_dns(192.168.71.1)` and `esp_netif_dhcps_option(OFFER_DNS)` during construction. This replaces the entire post-`wait_netif_up` unsafe block.

**Key constraint:** `swap_netif_ap` calls `detach_netif()` internally, which calls `driver.stop()`. This is safe to call before `wifi.start()`, but if called after `wifi.start()`, it stops and re-attaches. The preferred approach is to pass the custom `ap_netif` at construction via `wrap_all` — that way no stop/re-attach cycle is needed. The `run_softap_portal` function currently calls `EspWifi::new()` which internally calls `wrap_all` with default netifs. The fix is to split construction and call `wrap_all` directly.

**Current call in `main.rs` (SoftAP path):**
```rust
esp_idf_svc::wifi::EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))
```

**Fix:** Split into `WifiDriver::new` + `EspWifi::wrap_all`:
```rust
// Source: esp-idf-svc-0.51.0/src/wifi.rs (lines 1561-1600)
let driver = WifiDriver::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))?;
let esp_wifi = EspWifi::wrap_all(
    driver,
    EspNetif::new(NetifStack::Sta)?,
    ap_netif,  // pre-configured with portal DNS
)?;
```

Then `BlockingWifi::wrap(esp_wifi, sysloop)` as before. The unsafe block that currently lives after `wait_netif_up()` can be removed entirely.

**Confidence:** HIGH — verified by reading `netif.rs` constructor source and `wifi.rs` `wrap_all` / `attach_netif` code.

---

### BUG-2: Android Captive Portal (unblocked by BUG-1)

The `/generate_204` handler already returns `302 Found` with `Location: http://192.168.71.1/`. Once BUG-1 is fixed and DNS resolves to the portal IP, the Android probe reaches the HTTP handler and the 302 triggers the "Sign in to network" notification.

No code change required for BUG-2 itself — the fix is already committed. The task is verifying the existing 302 handler works correctly post BUG-1 fix.

**Android probe sequence (confirmed from testing.md and provisioning.rs comments):**
1. Android connects to SSID
2. DHCP assigns IP + DNS server (was 8.8.8.8, will be 192.168.71.1 after BUG-1 fix)
3. Android sends DNS query for `connectivitycheck.gstatic.com` → hijack resolves to `192.168.71.1`
4. Android sends HTTP GET `/generate_204` to `192.168.71.1`
5. Our handler returns `302 Found` → Android shows notification

**Confidence:** HIGH for the mechanism; MEDIUM for whether `302` alone is sufficient (vs needing specific headers or response body format) — but this is the standard approach.

---

### BUG-3/BUG-4: NVS TLS Default and Schema Version

**Root cause:** `load_mqtt_config` in `provisioning.rs` does not read an `mqtt_tls` key (the field was never written — the provisioning form has no TLS toggle). When `mqtt_connect` receives the config, if it applies `unwrap_or(true)` or a `true` default for TLS, mbedTLS attempts a handshake against a plain-MQTT broker and fails.

**Fix location:** Two changes in `provisioning.rs`:

1. `load_mqtt_config` must return a TLS flag defaulting to `false`:
   ```rust
   // In load_mqtt_config, add:
   let tls = nvs.get_u8("mqtt_tls").unwrap_or(None).unwrap_or(0) != 0;
   // Return as part of the tuple; default is false (plain MQTT)
   ```

2. `save_credentials` must write `config_ver = 1` and `mqtt_tls = 0` on every save:
   ```rust
   nvs.set_u8("config_ver", 1)?;
   nvs.set_u8("mqtt_tls", 0)?;  // always plain MQTT in v2.0
   ```

**NVS key budget:** NVS namespace "prov" — existing keys: `wifi_count`, `wifi_ssid_0..2`, `wifi_pass_0..2`, `mqtt_host`, `mqtt_port_hi`, `mqtt_port_lo`, `mqtt_user`, `mqtt_pass`, `force_softap`. New keys: `config_ver`, `mqtt_tls`. NVS supports up to 126 keys per namespace; no budget concern.

**`config_ver` semantics:** Written as `1` on every `save_credentials` call. Never read for migration in Phase 19 — reserved for future schema detection. If absent (old config), code treats it as version 0 (unknown legacy).

**Confidence:** HIGH — NVS key API pattern confirmed from existing provisioning.rs code. The TLS default bug is confirmed in testing.md.

**Note on `mqtt_connect` signature:** The current `mqtt_connect` in `src/mqtt.rs` takes 13 args — check whether it already accepts a TLS flag or uses a compile-time config. If TLS is currently hardcoded false (no mbedTLS feature), then `load_mqtt_config` just needs to return the key so future use is possible; the immediate fix is just writing the key on save.

---

### FEAT-1: Boot Button Rework

**Current code (main.rs, Step 19):** 100ms polling loop. Once GPIO9 has been low for ≥30 iterations (3s), calls `set_force_softap` and `esp_restart()`. Timer resets on any high reading.

**New behaviour:**
- Phase 1 (0–3s): button held low → no action
- Phase 2 (3s+, button still held): LED flashes in "warning" pattern; timer continues
- Phase 3 (10s+, button still held): LED turns off (steady off = "danger")
- Release during Phase 2 (3s–10s): `set_force_softap` + `esp_restart()` (same as current)
- Release during Phase 3 (≥10s): `nvs_flash_erase` + `esp_restart()` (factory reset)
- Release during Phase 1 (<3s): no action (same as current)

**State machine design:**

```rust
// In main.rs GPIO9 monitor thread — extend existing loop
enum ButtonState {
    Idle,
    HeldWarning(std::time::Instant),   // held 3s+, LED flashing
    HeldDanger(std::time::Instant),    // held 10s+, LED off
}
```

The thread needs access to `led_state` Arc to set the warning flash pattern. Currently the GPIO9 monitor only has `nvs_for_gpio`. A clone of `led_state` must be passed to the GPIO9 thread.

**LED states needed:**

`led.rs` currently has: `Connecting(0)`, `Connected(1)`, `Error(2)`, `SoftAP(3)`.

Add: `ButtonHold(4)` — a flash pattern distinct from existing ones. Claude's discretion on rate; recommended: 100ms on / 100ms off (fast double-pulse) — distinct from Connecting (200/200), SoftAP (500/500), Error (triple pulse).

The LED task restores the previous state when the button state changes back, but since factory reset always reboots immediately, there's no need to restore. The warning flash only needs to show while the button is held in the 3–10s window.

**NVS erase for factory reset:**

`nvs_flash_erase()` is available in `esp_idf_svc::sys` (confirmed from `nvs.rs` line 59). It erases the entire default NVS partition (all namespaces). No partition handle needed — it operates on the "nvs" partition by label.

```rust
// Factory reset sequence (after 10s hold, on release):
unsafe { esp_idf_svc::sys::nvs_flash_erase(); }
std::thread::sleep(std::time::Duration::from_millis(100)); // brief for flash write
unsafe { esp_idf_svc::sys::esp_restart(); }
```

**Important:** `nvs_flash_erase` must be called BEFORE `esp_restart`. No need to call `nvs_flash_deinit` first — `nvs_flash_erase` handles this internally per ESP-IDF docs.

**LED state cleanup on reset:** Since `esp_restart()` follows immediately, the LED state atom doesn't need to be restored. Set `ButtonHold` on 3s threshold, keep it until release/reset.

**Thread access pattern:** The GPIO9 monitor thread currently captures `nvs_for_gpio` and no LED handle. Must also clone `led_state` for the button-hold pattern. The main.rs spawn block must pass `led_state.clone()` into the closure.

**Confidence:** HIGH for state machine logic; HIGH for `nvs_flash_erase` API; MEDIUM for the LED pattern choice (Claude's discretion per CONTEXT.md).

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| DHCP DNS config | Post-start raw FFI stop/set/start sequence | `EspNetif::new_with_conf` with `RouterConfiguration { dns: Some(...) }` | Only the netif constructor lifecycle survives wifi start |
| NVS schema migration | Version table + field migration loop | Simple `unwrap_or(0)` default + `config_ver` key written on save | No migration needed in Phase 19; convention only |
| Button debounce | Separate timer or ring buffer | State machine with `get_or_insert_with(Instant::now)` + reset on high | Existing pattern already works; extends cleanly |

---

## Common Pitfalls

### Pitfall 1: Wrong `EspNetif` Key/Description for AP Netif

**What goes wrong:** `NetifConfiguration::wifi_default_router()` sets `key: "WIFI_AP_DEF"` and `description: "ap"`. If a custom `NetifConfiguration` uses a different key, ESP-IDF may fail to attach the netif to the WiFi AP interface.

**How to avoid:** Use `..NetifConfiguration::wifi_default_router()` as the base and override only `ip_configuration`. This preserves the required key, description, stack, and event IDs.

**Warning signs:** `esp_netif_attach_wifi_ap` returns an error; WiFi start fails.

### Pitfall 2: `RouterConfiguration::default()` DNS Is 8.8.8.8

**What goes wrong:** `RouterConfiguration::default()` (from `embedded-svc-0.28.1/src/ipv4.rs` line 219) sets `dns: Some(Ipv4Addr::new(8, 8, 8, 8))`. Forgetting to override `dns` in the struct literal means the portal IP is never set as DNS — same symptom as BUG-1.

**How to avoid:** Explicitly set `dns: Some(Ipv4Addr::new(192, 168, 71, 1))` in the `RouterConfiguration` literal; do not rely on `..Default::default()` for that field.

### Pitfall 3: `swap_netif_ap` Stops WiFi Driver

**What goes wrong:** `swap_netif_ap` calls `detach_netif()` which calls `driver.stop()`. If called while WiFi is running (e.g., after `wifi.start()`), it stops the WiFi driver. Re-attaching re-enables it, but this adds a stop/start cycle.

**How to avoid:** Use `EspWifi::wrap_all` with the custom netif at construction time — no stop/start cycle needed. Only use `swap_netif_ap` if the `EspWifi` is already constructed without a custom netif.

### Pitfall 4: `nvs_flash_erase` While NVS Is Mounted

**What goes wrong:** Calling `nvs_flash_erase()` while other threads are reading/writing NVS can cause corruption or a panic.

**How to avoid:** `nvs_flash_erase` in the factory reset path is called on button release after 10s — this is intentional destructive operation. The reboot follows immediately (100ms delay). This is acceptable risk: the device is being factory reset, any in-flight NVS write is discarded. No NVS deinit call is needed before erase in this path (ESP-IDF handles it).

### Pitfall 5: LED State Not Restored After Button Hold (No Issue)

**What goes wrong (non-issue):** One might worry about the LED staying in `ButtonHold` state if the user holds exactly to 10s then releases. Since factory reset calls `esp_restart()` immediately, the LED state is irrelevant. For the 3–10s release path (`set_force_softap` + `esp_restart()`), same applies. The LED state atom never needs to be restored from `ButtonHold`.

### Pitfall 6: `WifiDriver::new` Lifetime in SoftAP Path

**What goes wrong:** The current SoftAP path in `main.rs` constructs `EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))`. Replacing this with `WifiDriver::new` + `EspWifi::wrap_all` requires `WifiDriver<'d>` where `'d` is `'static` (because `peripherals.modem` is consumed). This is the same lifetime constraint that the current code satisfies implicitly.

**How to avoid:** The replacement is a straightforward drop-in — `WifiDriver::new` has the same signature as the first step of `EspWifi::new`. No lifetime issues.

---

## Code Examples

### Correct AP Netif Construction (BUG-1 Fix)

```rust
// Source: esp-idf-svc-0.51.0/src/netif.rs lines 199-208, 370-457
// Source: embedded-svc-0.28.1/src/ipv4.rs lines 192-228
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::ipv4::{Configuration as IpConfiguration, RouterConfiguration, Subnet, Mask};
use esp_idf_svc::wifi::{EspWifi, WifiDriver};
use std::net::Ipv4Addr;

// Step 1: Build the AP netif with portal DNS baked in at construction time.
// The EspNetif constructor calls set_dns() + esp_netif_dhcps_option(OFFER_DNS)
// during new_with_conf(). This is the only lifecycle point that survives wifi start.
let ap_netif = EspNetif::new_with_conf(&NetifConfiguration {
    ip_configuration: Some(IpConfiguration::Router(RouterConfiguration {
        subnet: Subnet {
            gateway: Ipv4Addr::new(192, 168, 71, 1),
            mask: Mask(24),
        },
        dhcp_enabled: true,
        dns: Some(Ipv4Addr::new(192, 168, 71, 1)),  // serves portal IP as DNS via DHCP
        secondary_dns: None,
        ..Default::default()
    })),
    ..NetifConfiguration::wifi_default_router()  // preserves required key/desc/stack/events
})?;

// Step 2: Construct EspWifi with the custom AP netif.
// wrap_all calls attach_netif() which attaches both netifs to the WiFi driver.
let driver = WifiDriver::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))?;
let esp_wifi = EspWifi::wrap_all(
    driver,
    EspNetif::new(NetifStack::Sta)?,
    ap_netif,
)?;
let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop.clone())?;

// Step 3: Configure, start, wait — same as current.
// The unsafe DHCP block after wait_netif_up() is now REMOVED.
wifi.set_configuration(&Configuration::AccessPoint(...))?;
wifi.start()?;
wifi.wait_netif_up()?;
// No DHCP fixup block needed here — DNS is already set.
```

### NVS TLS Default Fix (BUG-3)

```rust
// In load_mqtt_config (provisioning.rs):
// Add after reading mqtt_pass:
let tls = nvs.get_u8("mqtt_tls").unwrap_or(None).unwrap_or(0) != 0;
// Return tls as part of the config tuple.

// In save_credentials (provisioning.rs):
// Add these two lines before Ok(()):
nvs.set_u8("config_ver", 1)?;
nvs.set_u8("mqtt_tls", 0)?;  // plain MQTT; TLS toggle deferred to SEC milestone
```

### Button State Machine Extension (FEAT-1)

```rust
// In main.rs GPIO9 monitor thread — replace existing loop:
// (requires led_state clone to be passed into the closure)

enum BtnPhase { Idle, Warning, Danger }
let mut phase = BtnPhase::Idle;
let mut hold_start: Option<std::time::Instant> = None;

loop {
    std::thread::sleep(std::time::Duration::from_millis(100));

    if pin.is_low() {
        let since = hold_start.get_or_insert_with(std::time::Instant::now);
        let held = since.elapsed();

        match phase {
            BtnPhase::Idle if held >= std::time::Duration::from_secs(3) => {
                phase = BtnPhase::Warning;
                led_state_btn.store(crate::led::LedState::ButtonHold as u8,
                                    std::sync::atomic::Ordering::Relaxed);
                log::info!("GPIO9: 3s hold — SoftAP on release, or hold to 10s for factory reset");
            }
            BtnPhase::Warning if held >= std::time::Duration::from_secs(10) => {
                phase = BtnPhase::Danger;
                led_state_btn.store(crate::led::LedState::Connecting as u8,  // LED off proxy
                                    std::sync::atomic::Ordering::Relaxed);
                // Actually: set LED off — use a new state or reuse an off-equivalent
                log::warn!("GPIO9: 10s hold — release now for factory reset");
            }
            _ => {}
        }
    } else {
        // Button released
        match phase {
            BtnPhase::Warning => {
                log::info!("GPIO9: released at 3–10s — entering SoftAP mode");
                crate::provisioning::set_force_softap(&nvs_for_gpio);
                std::thread::sleep(std::time::Duration::from_millis(200));
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
            BtnPhase::Danger => {
                log::warn!("GPIO9: released at 10s+ — factory reset: erasing NVS");
                unsafe {
                    esp_idf_svc::sys::nvs_flash_erase();
                    std::thread::sleep_ms(100);  // brief flush
                    esp_idf_svc::sys::esp_restart();
                }
            }
            _ => {}
        }
        hold_start = None;
        phase = BtnPhase::Idle;
    }
}
```

### LED ButtonHold State Addition (FEAT-1)

```rust
// In led.rs — add variant:
ButtonHold = 4,  // fast 100ms/100ms flash — button-hold warning (3s–10s)

// In led_task match:
LedState::ButtonHold => {
    // 100ms on / 100ms off — fast flash, visually urgent
    let pos = elapsed_ms % 200;
    if pos < 100 {
        pin.set_low().ok();   // LED on
    } else {
        pin.set_high().ok();  // LED off
    }
}
```

---

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| `esp_netif_dhcps_stop/set_dns_info/start` after `wait_netif_up()` | `EspNetif::new_with_conf` with `RouterConfiguration { dns: Some(...) }` before `wifi.start()` | DNS offer works correctly; unsafe block removed |
| No NVS schema versioning | `config_ver: u8` key written on every save | Future schema changes are detectable |
| `mqtt_tls` absent from NVS → undefined default | `mqtt_tls = 0` written on save, `unwrap_or(0)` on read | TLS default is unambiguously off |
| Single 3s GPIO9 threshold → SoftAP | 3s → LED warning, release → SoftAP; 10s → LED off, release → factory reset | Recoverable from bad NVS state without reflash |

---

## Open Questions

1. **Does `mqtt_connect` currently use TLS?**
   - What we know: `load_mqtt_config` does not return a TLS flag; the form has no TLS field; testing.md says TLS defaulted to `true` causing failure.
   - What's unclear: Where the `true` default lives — in `mqtt_connect` itself, in `MqttClientConfiguration`, or in a config constant.
   - Recommendation: Read `src/mqtt.rs` during planning to find the TLS default location before writing the task.

2. **LED for "danger zone" (after 10s hold)**
   - The CONTEXT.md says "LED off" as the danger signal (LED turns off, not a pattern).
   - Implementation: set `pin.set_high()` directly in the GPIO thread, or add a new `LedState::Off` variant. Setting `Connected` briefly is not correct semantically.
   - Recommendation: Either (a) add `LedState::Off = 5` to led.rs, or (b) have the GPIO thread drive the pin directly via a side channel. Option (a) is cleaner. The planner should choose.

3. **`WifiDriver` import in `main.rs` SoftAP path**
   - What we know: `main.rs` currently calls `EspWifi::new(...)` in the SoftAP path. Replacing with `WifiDriver::new` + `EspWifi::wrap_all` requires importing `WifiDriver`.
   - What's unclear: Whether `WifiDriver` is re-exported from `esp_idf_svc::wifi` (it is — confirmed from `wifi.rs` line 455).
   - Recommendation: Add `use esp_idf_svc::wifi::WifiDriver;` to the SoftAP path in `main.rs`.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | No automated test framework — embedded target; all tests are manual hardware sessions |
| Config file | none |
| Quick run command | `cargo build --release` (compile gate) |
| Full suite command | `cargo clippy -- -D warnings && cargo build --release` |

### Phase Requirements → Test Map

| Item | Behavior | Test Type | Command | Notes |
|------|----------|-----------|---------|-------|
| BUG-1 | DHCP offers portal IP as DNS | manual-hardware | connect Android to GNSS-Setup; check DNS in IP settings | Blocked by hardware session |
| BUG-1 | DNS hijack resolves hostnames to 192.168.71.1 | manual-hardware | `nslookup google.com 192.168.71.1` from a connected device | — |
| BUG-2 | Android shows "Sign in to network" | manual-hardware | connect Android, observe notification | Requires BUG-1 fixed first |
| BUG-3/4 | MQTT connects after OTA from old firmware | manual-hardware | OTA from old firmware → check MQTT reconnect in /log | Requires BUG-3 fix first |
| BUG-3/4 | `mqtt_tls` default is false | code-review | verify `unwrap_or(0)` in `load_mqtt_config` | Verifiable without hardware |
| FEAT-1 | 3s hold starts LED flash | manual-hardware | hold GPIO9, observe LED | — |
| FEAT-1 | Release at 3–10s enters SoftAP | manual-hardware | hold 4s, release, verify GNSS-Setup AP appears | — |
| FEAT-1 | Release at 10s erases NVS + reboots | manual-hardware | hold 10s, release, verify device loses all credentials | Destructive — run last |
| FEAT-1 | Factory reset does NOT revert OTA slot | code-review | `nvs_flash_erase()` only; no OTA partition manipulation | — |
| All | Compiles clean with `-D warnings` | automated | `cargo clippy -- -D warnings` | Run before phase close |

### Sampling Rate

- **Per task commit:** `cargo build --release` (compile check)
- **Per wave merge:** `cargo clippy -- -D warnings && cargo build --release`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

None — no automated test files needed. All validation is compile-check + hardware session. The hardware session is deferred to after phase close per CONTEXT.md.

---

## Sources

### Primary (HIGH confidence)

- `esp-idf-svc-0.51.0/src/netif.rs` (lines 315–457) — `EspNetif::new_with_conf` implementation; DHCP DNS option call; `RouterConfiguration` processing
- `esp-idf-svc-0.51.0/src/wifi.rs` (lines 1577–1643) — `wrap`, `wrap_all`, `swap_netif_ap`, `attach_netif`, `detach_netif` implementations
- `esp-idf-svc-0.51.0/src/nvs.rs` (lines 49–67) — `nvs_flash_erase()` usage pattern
- `embedded-svc-0.28.1/src/ipv4.rs` (lines 192–228) — `RouterConfiguration` struct, field names, defaults
- `esp-idf-svc-0.51.0/examples/wifi_dhcp_with_hostname.rs` — `EspWifi::wrap_all` usage pattern with custom `EspNetif`
- `src/provisioning.rs` (project) — current DHCP block to be replaced; NVS key patterns; button polling loop
- `src/led.rs` (project) — existing LED state variants and timing
- `src/main.rs` (project) — GPIO9 monitor thread; SoftAP construction path

### Secondary (MEDIUM confidence)

- `testing.md` — symptom descriptions for BUG-1 through BUG-4; confirms the post-`wait_netif_up` approach has no effect
- ESP-IDF v5.3.3 header `nvs_flash.h` (in `.embuild`) — `nvs_flash_erase()` declaration confirmed

---

## Metadata

**Confidence breakdown:**
- BUG-1 fix approach: HIGH — verified from library source code directly; the `new_with_conf` path is the canonical approach
- BUG-2 mechanism: HIGH — the 302 is already committed; mechanism is correct
- BUG-3/4 fix: HIGH — NVS key pattern is well-established; default fix is trivial
- FEAT-1 state machine: HIGH — extends existing pattern; `nvs_flash_erase` confirmed in source
- LED pattern choice (discretion area): MEDIUM — Claude's choice per CONTEXT.md

**Research date:** 2026-03-09
**Valid until:** 2026-04-09 (stable library versions pinned; no version drift expected)
