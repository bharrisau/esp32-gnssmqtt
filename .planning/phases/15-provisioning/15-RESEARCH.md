# Phase 15: Provisioning - Research

**Researched:** 2026-03-08
**Domain:** ESP-IDF SoftAP, HTTP server, NVS credential storage, GPIO input, WiFi mode switching
**Confidence:** HIGH

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PROV-01 | Device enters SoftAP hotspot mode on first boot when no WiFi credentials exist in NVS | Read NVS "prov" namespace on boot; if no networks stored, call `wifi_start_softap()` instead of `wifi_connect()` |
| PROV-02 | User can open captive-portal web UI to enter WiFi SSID and password | `EspHttpServer` on port 80 serves HTML form; POST handler saves to NVS and triggers reboot |
| PROV-03 | User can configure MQTT broker (host, port, username, password) via provisioning web UI | Same HTML form and POST handler — MQTT fields included alongside WiFi fields |
| PROV-04 | User can store up to 3 WiFi networks via web UI; all persisted to NVS | NVS keys `wifi_ssid_0/1/2` and `wifi_pass_0/1/2`; `wifi_count` u8 stores how many are populated |
| PROV-05 | On all WiFi failures, device retries stored networks indefinitely with backoff; does not auto-enter SoftAP | `wifi_supervisor` loops through all stored networks; `esp_restart()` after configurable timeout — never re-enters SoftAP without user action |
| PROV-06 | GPIO9 held low for 3s enters SoftAP mode; device exits back to WiFi mode after 300s with no client connected | Thread polls `PinDriver::input` with pull-up on GPIO9; measures low duration; `AtomicBool` signals main to re-enter SoftAP |
| PROV-07 | MQTT payload "softap" to `gnss/{device_id}/ota/trigger` enters SoftAP mode; same 300s no-client timeout | Check "softap" in ota_task before "reboot" and OTA JSON parse; signal re-entry via shared channel or flag |
| PROV-08 | LED shows a distinct flash pattern while in SoftAP mode (different from connecting/connected/error) | Add `LedState::SoftAP = 3` variant; 500ms on / 500ms off slow blink; LED task match arm extended |
</phase_requirements>

## Summary

Phase 15 introduces WiFi provisioning via a SoftAP captive portal. The device starts in SoftAP mode when NVS holds no WiFi credentials, serves an HTML form that captures WiFi (up to 3 networks) and MQTT configuration, writes them to NVS, and reboots into station mode. On subsequent boots the stored networks are tried in order with retry backoff. SoftAP re-entry is triggered by GPIO9 held low for 3 seconds, by the MQTT command "softap" to the OTA trigger topic, or automatically on first boot. During SoftAP mode, a 300-second no-client timeout returns the device to WiFi mode (reboot-based transition).

All required APIs exist in the current dependency set (`esp-idf-svc 0.51.0`): `EspWifi` supports `Configuration::AccessPoint`, `EspHttpServer` handles GET/POST, `EspNvs` provides `get_str`/`set_str`/`get_u8`/`set_u8`, and `PinDriver::input` reads GPIO9. No new crate dependencies are required. The main architectural challenge is the boot-path decision (SoftAP vs STA) and the run-time re-entry signal path from the OTA task and GPIO monitor to the main WiFi initialization logic.

The key structural change is splitting `main()` initialization into two paths: a `softap_mode()` function that starts SoftAP + HTTP server and parks until the 300-second timeout or credentials are submitted, and the existing `wifi_connect()` station path. First boot detection reads NVS before any WiFi init. GPIO9 monitoring runs as a dedicated thread after all other threads are spawned, posting a flag that triggers `esp_restart()` back into SoftAP mode on the next boot (via an NVS "force_softap" flag).

**Primary recommendation:** Implement in three plans — Plan 15-01: NVS credential layer + first-boot detection + SoftAP mode with HTTP provisioning form; Plan 15-02: multi-network retry in `wifi_supervisor` + PROV-05 compliance; Plan 15-03: GPIO9 monitor + MQTT "softap" trigger + LED SoftAP state + 300-second no-client timeout.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `esp_idf_svc::wifi::{BlockingWifi, EspWifi, Configuration, AccessPointConfiguration}` | 0.51.0 (already in Cargo.toml) | SoftAP mode WiFi driver | Already a dependency; `Configuration::AccessPoint` confirmed in wifi.rs |
| `esp_idf_svc::http::server::{EspHttpServer, Configuration as HttpConfig}` | 0.51.0 (already in Cargo.toml) | HTTP server for captive portal | Confirmed in http/server.rs; start/stop on construction/drop |
| `esp_idf_svc::nvs::{EspNvs, EspDefaultNvsPartition, NvsDefault}` | 0.51.0 (already in Cargo.toml) | Persistent credential storage | `get_str`/`set_str`/`get_u8`/`set_u8` confirmed in nvs.rs |
| `esp_idf_hal::gpio::PinDriver` | 0.45.2 (already in Cargo.toml) | GPIO9 input for button press | `PinDriver::input()` + `set_pull()` + `is_low()` confirmed in gpio.rs |
| `embedded_svc::wifi::AccessPointConfiguration` | 0.28.1 (already in Cargo.toml) | SoftAP SSID/password/channel config | Re-exported from `esp_idf_svc::wifi` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `embedded_svc::io::{Read, Write}` | 0.28.1 (already in Cargo.toml) | HTTP request body reading and response writing | Already imported transitively; POST body read via `connection.read()` |
| `embedded_svc::http::Method` | 0.28.1 (already in Cargo.toml) | HTTP method enum (Get, Post) | Used in `server.fn_handler(path, Method::Post, …)` |
| `unsafe { esp_idf_svc::sys::esp_wifi_ap_get_sta_list(...) }` | sys 0.36.1 (already in Cargo.toml) | Count connected SoftAP clients for 300s timeout | Direct sys call; no high-level wrapper in esp-idf-svc 0.51.0 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| HTML form via `EspHttpServer` | ESP-IDF provisioning component (`esp_prov`) | esp_prov requires separate host-side tool and BLE provisioning; SoftAP + plain HTML works from any browser without app |
| Reboot-to-switch-mode | In-place WiFi mode switching (stop → reconfigure → start) | In-place switching in the same binary run is complex: all threads must stop, channels must be drained, MQTT must shut down cleanly. Reboot is simpler and correct. |
| NVS `set_str` for all fields | `set_raw` with a serialized struct | `set_str` is simpler; each field is a separate key so individual updates don't corrupt the whole config |
| 300s hard timeout | Event-loop based sta-connected tracking | Polling `esp_wifi_ap_get_sta_list` every second avoids need for event callbacks; simpler for a timeout-based exit |

**Installation:**
No new packages needed. All required types are in the existing dependency tree.

## Architecture Patterns

### Recommended Project Structure
```
src/
├── main.rs           # Boot-path decision: SoftAP vs STA; new: read NVS creds before wifi init
├── wifi.rs           # new: wifi_start_softap(), multi-network wifi_connect_any()
├── provisioning.rs   # new: NVS cred load/save, HTTP server, SoftAP portal logic
├── led.rs            # add LedState::SoftAP = 3 variant and blink pattern
├── config.rs         # add SOFTAP_SSID, SOFTAP_TIMEOUT_SECS constants
└── ota.rs            # add "softap" check before "reboot" check
```

### Pattern 1: First-Boot NVS Check
**What:** Before starting WiFi, open NVS namespace "prov" and read `wifi_count`. If zero or absent, enter SoftAP mode instead of calling `wifi_connect`.
**When to use:** At the start of `main()`, before modem is handed to `EspWifi::new()`.

```rust
// Source: esp-idf-svc-0.51.0/examples/nvs_get_set_c_style.rs + src/nvs.rs
// Note: nvs partition must be taken before checking — it's consumed by EspWifi::new later.
// Strategy: take nvs, clone it for NVS check, then pass original to EspWifi::new.
// EspNvsPartition<NvsDefault> is Clone (confirmed in nvs.rs line 257).

fn has_wifi_credentials(nvs_partition: &EspNvsPartition<NvsDefault>) -> bool {
    match EspNvs::new(nvs_partition.clone(), "prov", false) {
        Err(_) => false, // namespace doesn't exist yet
        Ok(nvs) => nvs.get_u8("wifi_count").unwrap_or(None).unwrap_or(0) > 0,
    }
}
```

Key: `EspDefaultNvsPartition` (type alias for `EspNvsPartition<NvsDefault>`) is `Clone` — confirmed in nvs.rs line 257. Clone it before calling `EspWifi::new(modem, sysloop, Some(nvs))`.

### Pattern 2: SoftAP WiFi Mode
**What:** Set `Configuration::AccessPoint` on a stopped `BlockingWifi`, start it (no `connect()` call needed for AP mode), and wait for netif up.
**When to use:** First boot, or when GPIO9 / MQTT trigger fires (via `esp_restart()` with NVS flag set).

```rust
// Source: esp-idf-svc-0.51.0/examples/http_server.rs connect_wifi()
// NOTE: for AP mode, call start() + wait_netif_up() but NOT connect()
use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration};

fn wifi_start_softap(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: "GNSS-Setup".try_into().unwrap(),
        ssid_hidden: false,
        auth_method: AuthMethod::None,  // open network — easiest for first-time users
        channel: 6,
        max_connections: 4,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;
    log::info!("SoftAP started — SSID: GNSS-Setup, IP: 192.168.71.1");
    Ok(())
}
```

The SoftAP default IP is `192.168.71.1` (ESP-IDF lwIP default for AP netif). No DHCP configuration needed — ESP-IDF starts DHCP server automatically on the AP netif.

### Pattern 3: HTTP Provisioning Server
**What:** Start `EspHttpServer`, register GET `/` (HTML form) and POST `/save` (read body, parse fields, write NVS, reboot). Keep `server` handle alive until reboot.
**When to use:** Immediately after `wifi_start_softap()` succeeds.

```rust
// Source: esp-idf-svc-0.51.0/src/http/server.rs + examples/http_server.rs
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use embedded_svc::http::Method;
use embedded_svc::io::{Read, Write};

const PROV_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>GNSS Setup</title></head><body>
<h2>GNSS Device Setup</h2>
<form method="POST" action="/save">
  <h3>WiFi Network 1</h3>
  SSID: <input name="ssid0" required><br>
  Password: <input name="pass0" type="password"><br>
  <h3>WiFi Network 2 (optional)</h3>
  SSID: <input name="ssid1"><br>
  Password: <input name="pass1" type="password"><br>
  <h3>WiFi Network 3 (optional)</h3>
  SSID: <input name="ssid2"><br>
  Password: <input name="pass2" type="password"><br>
  <h3>MQTT Broker</h3>
  Host: <input name="mqtt_host" required><br>
  Port: <input name="mqtt_port" value="1883"><br>
  User: <input name="mqtt_user"><br>
  Pass: <input name="mqtt_pass" type="password"><br>
  <input type="submit" value="Save and Reboot">
</form></body></html>"#;

fn start_provisioning_server() -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&HttpConfig {
        stack_size: 10240,  // POST handler parses body; needs more stack than default 6144
        ..Default::default()
    })?;

    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?.write_all(PROV_HTML.as_bytes())
    })?;

    server.fn_handler::<anyhow::Error, _>("/save", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        let max_body = 1024_usize;
        if len > max_body {
            req.into_status_response(413)?.write_all(b"Too large")?;
            return Ok(());
        }
        let mut buf = vec![0u8; len.min(max_body)];
        let (_headers, connection) = req.split();
        connection.read(&mut buf)?;
        // parse url-encoded form data, write to NVS, reboot
        // ... (see NVS write pattern below)
        Ok(())
    })?;

    Ok(server)
}
```

**Important:** `EspHttpServer` stops (and frees resources) when dropped. Keep the handle alive in the calling scope until reboot. The server task runs on its own FreeRTOS thread (default stack 6144 bytes; use 10240 for POST body parsing).

### Pattern 4: NVS Credential Read/Write
**What:** Store WiFi networks as indexed keys in namespace "prov". Store MQTT config in same namespace.
**When to use:** On provisioning form submit (write) and on every boot (read).

```rust
// Source: esp-idf-svc-0.51.0/src/nvs.rs + examples/nvs_get_set_c_style.rs
// NVS key names MUST be <= 15 chars (ESP-IDF NVS limit).

// Write after form submit:
fn save_credentials(nvs_partition: EspNvsPartition<NvsDefault>,
                    networks: &[(&str, &str)],  // (ssid, pass) pairs, up to 3
                    mqtt_host: &str, mqtt_port: u16,
                    mqtt_user: &str, mqtt_pass: &str) -> anyhow::Result<()> {
    let mut nvs = EspNvs::new(nvs_partition, "prov", true)?;
    let count = networks.len().min(3) as u8;
    nvs.set_u8("wifi_count", count)?;
    for (i, (ssid, pass)) in networks.iter().enumerate().take(3) {
        let ssid_key = format!("wifi_ssid_{}", i);  // "wifi_ssid_0" = 11 chars OK
        let pass_key = format!("wifi_pass_{}", i);  // "wifi_pass_0" = 11 chars OK
        nvs.set_str(&ssid_key, ssid)?;
        nvs.set_str(&pass_key, pass)?;
    }
    nvs.set_str("mqtt_host", mqtt_host)?;    // 9 chars OK
    nvs.set_u8("mqtt_port_hi", (mqtt_port >> 8) as u8)?;
    nvs.set_u8("mqtt_port_lo", (mqtt_port & 0xFF) as u8)?;
    nvs.set_str("mqtt_user", mqtt_user)?;    // 9 chars OK
    nvs.set_str("mqtt_pass", mqtt_pass)?;    // 9 chars OK
    Ok(())
}

// Read at boot:
fn load_wifi_networks(nvs: &EspNvs<NvsDefault>) -> Vec<(String, String)> {
    let count = nvs.get_u8("wifi_count").unwrap_or(None).unwrap_or(0) as usize;
    let mut networks = Vec::new();
    let mut ssid_buf = [0u8; 65];
    let mut pass_buf = [0u8; 65];
    for i in 0..count.min(3) {
        let ssid_key = format!("wifi_ssid_{}", i);
        let pass_key = format!("wifi_pass_{}", i);
        if let (Ok(Some(ssid)), Ok(Some(pass))) = (
            nvs.get_str(&ssid_key, &mut ssid_buf),
            nvs.get_str(&pass_key, &mut pass_buf),
        ) {
            networks.push((ssid.to_string(), pass.to_string()));
        }
    }
    networks
}
```

**NVS key length limit:** ESP-IDF NVS keys are limited to 15 characters (null-terminated, so 15 usable chars). `"wifi_ssid_0"` is 11 chars — safe. `"mqtt_port_hi"` is 12 chars — safe. `"mqtt_port_lo"` is 12 chars — safe.

**MQTT port storage:** `set_u8` only stores 0–255. Port is u16, so store as two u8 keys: `mqtt_port_hi` (high byte) and `mqtt_port_lo` (low byte), then reconstruct as `(hi as u16) << 8 | (lo as u16)`.

### Pattern 5: URL-Encoded Form Parsing
**What:** Browser form POST sends `application/x-www-form-urlencoded` body like `ssid0=MyNet&pass0=secret&...`. Parse without a crate dependency.
**When to use:** In the POST `/save` handler.

```rust
// No crate needed — form fields are simple ASCII with percent-encoding for special chars.
// For field values (SSID, passwords), percent-decoding is needed for non-ASCII and symbols.
// Simple approach: require ASCII-printable only in the form; reject non-ASCII SSIDs.
// This is acceptable for v2.0 (most home SSIDs are ASCII).

fn parse_form_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("{}=", key);
    let start = body.find(&search)? + search.len();
    let end = body[start..].find('&').map(|i| start + i).unwrap_or(body.len());
    Some(&body[start..end])
}

// Usage:
let body_str = std::str::from_utf8(&buf).unwrap_or("");
let ssid0 = parse_form_field(body_str, "ssid0").unwrap_or("");
// NOTE: does not handle percent-encoding. Acceptable for v2.0; special chars in SSID
// or password require a proper URL decoder. Flag as a known limitation.
```

**Limitation (LOW confidence):** URL-encoded bodies percent-encode characters like `+`, `%`, `=`, `&`, and non-ASCII. A password of `my+pass` will arrive as `my%2Bpass`. For v2.0, document that WiFi passwords with `&`, `%`, `+`, `=` characters are unsupported in the web UI. Post-v2.0 improvement: add `urlencoding` crate (pure Rust, no-std compatible).

### Pattern 6: GPIO9 Hold Detection
**What:** Dedicate a thread to polling GPIO9 (as input with pull-up). Measure continuous low duration. At 3 seconds, set a shared flag and call `esp_restart()`.
**When to use:** Thread spawned after all other subsystems are running.

```rust
// Source: esp-idf-hal-0.45.2/src/gpio.rs — PinDriver::input(), set_pull(), is_low()
use esp_idf_hal::gpio::{PinDriver, Pull};

fn gpio9_monitor_task(gpio9_pin: impl esp_idf_hal::gpio::InputPin + 'static) -> ! {
    let mut pin = PinDriver::input(gpio9_pin).expect("GPIO9 PinDriver failed");
    pin.set_pull(Pull::Up).expect("GPIO9 pull-up failed");

    let mut low_since: Option<std::time::Instant> = None;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if pin.is_low() {
            let since = low_since.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() >= std::time::Duration::from_secs(3) {
                log::info!("GPIO9 held low 3s — entering SoftAP mode");
                // Set NVS force_softap flag then restart
                // (write to NVS here; main reads it on next boot)
                unsafe { esp_idf_svc::sys::esp_restart(); }
            }
        } else {
            low_since = None; // reset on release
        }
    }
}
```

GPIO9 on ESP32-C6: This is the BOOT button on the XIAO ESP32-C6. It is connected to GPIO9 with an external 10k pull-up. Pressing it pulls GPIO9 to GND. The pin is safe to use as a GPIO input in normal operation (the bootloader only samples it during reset).

### Pattern 7: SoftAP Re-Entry via NVS Flag
**What:** When GPIO9 or "softap" MQTT trigger fires, write a `force_softap` flag to NVS, then call `esp_restart()`. On the next boot, `main()` reads the flag before checking credentials — if set, clear the flag and enter SoftAP mode regardless of stored networks.
**When to use:** For both GPIO9 and MQTT "softap" trigger paths.

```rust
// Write flag before restart:
fn set_force_softap(nvs_partition: EspNvsPartition<NvsDefault>) {
    if let Ok(mut nvs) = EspNvs::new(nvs_partition, "prov", true) {
        let _ = nvs.set_u8("force_softap", 1);
    }
}

// Read and clear flag at boot:
fn check_and_clear_force_softap(nvs: &mut EspNvs<NvsDefault>) -> bool {
    match nvs.get_u8("force_softap") {
        Ok(Some(1)) => {
            let _ = nvs.set_u8("force_softap", 0);
            true
        }
        _ => false,
    }
}
```

This is simpler than in-place mode switching (which would require tearing down all threads, channels, and the MQTT connection cleanly). Reboot is clean and deterministic.

### Pattern 8: SoftAP 300-Second No-Client Timeout
**What:** After starting SoftAP, poll `esp_wifi_ap_get_sta_list()` every second. If no client is associated for 300 continuous seconds, call `esp_restart()` (without setting `force_softap`, so next boot goes to STA mode with stored credentials).
**When to use:** In the SoftAP mode loop, after HTTP server is started.

```rust
// No high-level wrapper in esp-idf-svc 0.51.0 for sta list count.
// Use sys crate directly.
fn count_softap_clients() -> u8 {
    let mut sta_list = esp_idf_svc::sys::wifi_sta_list_t::default();
    let ret = unsafe { esp_idf_svc::sys::esp_wifi_ap_get_sta_list(&mut sta_list) };
    if ret == 0 { sta_list.num as u8 } else { 0 }
}

fn softap_timeout_loop() -> ! {
    let mut no_client_since = std::time::Instant::now();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let clients = count_softap_clients();
        if clients > 0 {
            no_client_since = std::time::Instant::now(); // reset while client connected
        } else if no_client_since.elapsed().as_secs() >= 300 {
            log::info!("SoftAP: no client for 300s — returning to STA mode");
            unsafe { esp_idf_svc::sys::esp_restart(); }
        }
    }
}
```

### Pattern 9: Multi-Network WiFi Retry (PROV-05)
**What:** Load all stored networks from NVS. Try each in order with exponential backoff. Never enter SoftAP automatically — rely on RESIL-01 (10-minute WiFi-down reboot) to recover.
**When to use:** Replace the existing single-credential `wifi_connect()` with a multi-network variant.

```rust
// Replace wifi_connect() with wifi_connect_any():
fn wifi_connect_any(wifi: &mut BlockingWifi<EspWifi<'static>>,
                    networks: &[(String, String)]) -> anyhow::Result<()> {
    for (ssid, pass) in networks.iter().cycle().take(networks.len() * 3) {
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid.as_str().try_into().unwrap_or_default(),
            password: pass.as_str().try_into().unwrap_or_default(),
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        }))?;
        wifi.start()?;
        match wifi.connect() {
            Ok(_) => {
                wifi.wait_netif_up()?;
                return Ok(());
            }
            Err(e) => {
                log::warn!("WiFi connect to {} failed: {:?}", ssid, e);
                wifi.stop()?;
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
    anyhow::bail!("All WiFi networks failed")
}
```

Note: `wifi.start()` must be called for each new configuration attempt if `wifi.stop()` was called. The existing `wifi_supervisor` only calls `wifi.connect()` (not `start()`), which is correct for single-network reconnect. Multi-network switching requires `stop()` + reconfigure + `start()` + `connect()`.

### Pattern 10: LedState::SoftAP
**What:** Add variant 3 to `LedState`, update `from_u8` match, and add a blink pattern arm.
**When to use:** Set atomically when entering SoftAP mode; clear when rebooting back to STA.

```rust
// In led.rs — add variant:
pub enum LedState {
    Connecting = 0,
    Connected  = 1,
    Error      = 2,
    SoftAP     = 3,  // NEW: slow blink 500ms on / 500ms off — visually distinct from 200ms Connecting
}

// In from_u8:
3 => LedState::SoftAP,

// In led_task match:
LedState::SoftAP => {
    // 500ms on / 500ms off — 1000ms cycle
    let pos = elapsed_ms % 1000;
    if pos < 500 { pin.set_low().ok(); } else { pin.set_high().ok(); }
}
```

### Anti-Patterns to Avoid
- **In-place WiFi mode switch without reboot:** Switching from STA to AP on a live device requires tearing down all threads, channels, MQTT client, and the WiFi netif in the correct order. This is complex and error-prone. Use `esp_restart()` with an NVS flag instead.
- **Keeping EspHttpServer across reboot:** The server is dropped on reboot. No explicit server teardown is needed — just restart.
- **Using `wifi.connect()` in AP mode:** AP mode only needs `start()` + `wait_netif_up()`. Calling `connect()` in AP mode returns an error.
- **Forgetting `nvs_commit()`:** `EspNvs::set_str()` calls `nvs_commit()` internally (confirmed in nvs.rs line 580). No manual commit call needed.
- **NVS key names longer than 15 characters:** ESP-IDF NVS keys have a 15-character limit. Keys like `"wifi_password_0"` (15 chars — exactly on the limit) are fine. `"wifi_password_00"` (16 chars) would fail silently or error.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WiFi SoftAP mode | Manual `esp_wifi_set_mode()` calls | `EspWifi::set_configuration(&Configuration::AccessPoint(...))` | Handles netif attachment, DHCP server start, mode transitions |
| HTTP server for form | Raw TCP socket server | `EspHttpServer` from `esp_idf_svc::http::server` | Handles HTTP parsing, chunked responses, concurrent connections, session management |
| NVS persistence | Custom flash page management | `EspNvs::get_str` / `set_str` | Handles wear leveling, power-loss safety, namespacing |
| GPIO debounce | Custom interrupt + counter | Polling with `std::time::Instant` elapsed check | Simpler, no interrupt registration complexity; 100ms poll is fine for 3s hold |
| DHCP server for clients | Custom DHCP server | Automatic (ESP-IDF starts it on AP netif) | No configuration needed; clients get IPs in `192.168.71.0/24` range |

**Key insight:** All mechanisms exist in the current dependency set. The engineering work is in wiring them together correctly (boot-path decision, NVS key layout, timeout loop, re-entry flag).

## Common Pitfalls

### Pitfall 1: Calling `wifi.connect()` in AP Mode
**What goes wrong:** `wifi.connect()` returns `EspError` when the WiFi is in AP mode.
**Why it happens:** `connect()` initiates a STA association, which has no meaning in AP mode.
**How to avoid:** In AP mode, call only `start()` and `wait_netif_up()`. Skip `connect()` entirely.
**Warning signs:** `wifi_connect` returns error `ESP_ERR_WIFI_MODE` immediately after `set_configuration(AccessPoint(...))`.

### Pitfall 2: NVS Key Too Long
**What goes wrong:** `EspNvs::set_str("wifi_password_01", ...)` silently truncates or returns error.
**Why it happens:** ESP-IDF NVS key names are limited to 15 characters (including null terminator in C; 15 usable chars in Rust).
**How to avoid:** Count every key name character. Use short keys: `"wifi_ssid_0"` (11), `"wifi_pass_0"` (11), `"mqtt_host"` (9), `"mqtt_user"` (9), `"mqtt_pass"` (9), `"wifi_count"` (10), `"mqtt_port_hi"` (12), `"mqtt_port_lo"` (12), `"force_softap"` (12). All are within the limit.
**Warning signs:** `set_str` returns `EspError` with `ESP_ERR_NVS_KEY_TOO_LONG`; or key silently not found on read.

### Pitfall 3: NVS Partition Consumed Before Check
**What goes wrong:** `EspDefaultNvsPartition::take()` is called once; the returned value is consumed by `EspWifi::new(modem, sysloop, Some(nvs))`. If the NVS check is done after `EspWifi::new`, the partition is gone.
**Why it happens:** `EspWifi::new` takes ownership of the NVS partition.
**How to avoid:** `EspNvsPartition<NvsDefault>` is `Clone` (confirmed in nvs.rs). Clone it: `let nvs_check = nvs.clone(); let creds = has_wifi_credentials(&nvs_check);` before passing `nvs` to `EspWifi::new`.
**Warning signs:** Compile error "use of moved value `nvs`" when trying to open NVS after WiFi init.

### Pitfall 4: HTTP Server Handler Stack Size
**What goes wrong:** POST handler crashes with stack overflow during body parsing.
**Why it happens:** Default HTTP server stack is 6144 bytes. A POST handler that reads a body, creates `String`s, and writes NVS uses significantly more stack.
**How to avoid:** Set `stack_size: 10240` in `HttpConfig` (same as the official example uses for JSON parsing).
**Warning signs:** Device reboots during POST submission; log shows "Guru Meditation Error" or stack canary failure from the httpd task.

### Pitfall 5: Form Body Percent-Encoding
**What goes wrong:** WiFi password `my&pass` arrives in POST body as `my%26pass`; stored literally in NVS; WiFi connect fails.
**Why it happens:** Browsers URL-encode form field values. `&`, `%`, `+`, `=` and non-ASCII are encoded.
**How to avoid:** For v2.0, document the limitation: passwords with `&`, `%`, `+`, `=`, or non-ASCII characters are not supported in the provisioning web UI. For v2.1, add `urlencoding` crate for proper decoding.
**Warning signs:** WiFi connect fails with SSID/password that should work; stored NVS value contains `%XX` sequences.

### Pitfall 6: Multi-Network `wifi.stop()` / `wifi.start()` Required
**What goes wrong:** Calling `wifi.set_configuration()` on an already-started WiFi driver then `wifi.connect()` connects to a new network, but the mode change may fail.
**Why it happens:** In BlockingWifi, changing `ClientConfiguration` after start does not require stop/start for the same auth mode, but switching networks is more reliable with stop → reconfigure → start.
**How to avoid:** In `wifi_connect_any()`, call `wifi.stop()` before reconfiguring for the next network on connection failure. Confirmed safe: `start()` after `stop()` is the normal idiom.
**Warning signs:** Second network attempt never connects despite correct credentials.

### Pitfall 7: GPIO9 is the BOOT Button
**What goes wrong:** Holding GPIO9 during reset enters bootloader download mode instead of normal boot.
**Why it happens:** The ESP32-C6 bootloader samples GPIO9 at reset; if low, it enters serial download mode.
**How to avoid:** This is expected hardware behavior and not a firmware bug. The 3-second hold detection only runs after the bootloader has released control (i.e., after firmware starts). A brief press during normal operation does not trigger the bootloader. Document this in the UI: "Do not hold the BOOT button while pressing RESET."
**Warning signs:** Device enters download mode instead of SoftAP when BOOT is held during a hardware reset.

### Pitfall 8: SoftAP HTTP Server IP
**What goes wrong:** User types `192.168.4.1` (common ESP32 default) but the ESP32-C6 AP netif uses `192.168.71.1`.
**Why it happens:** The default AP netif IP depends on the ESP-IDF lwIP configuration. ESP32-C6 with ESP-IDF v5 defaults to `192.168.71.1` (not `192.168.4.1` which is older/different chips).
**How to avoid:** Log the AP IP address after `wait_netif_up()` using `wifi.wifi().ap_netif().get_ip_info()`. Display the IP in the log and optionally in the SSID (e.g., `GNSS-Setup` with instructions to visit `192.168.71.1`). Alternatively, use mDNS to make `gnss.local` resolve — but mDNS is an additional complexity not needed for v2.0.
**Warning signs:** User cannot reach the web UI despite being connected to the SSID.

## Code Examples

Verified patterns from official sources (local registry):

### SoftAP Configuration
```rust
// Source: esp-idf-svc-0.51.0/examples/http_server.rs + src/wifi.rs:801-807
use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration};

wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
    ssid: "GNSS-Setup".try_into().unwrap(),
    ssid_hidden: false,
    auth_method: AuthMethod::None,  // open — no password required from user
    channel: 6,
    max_connections: 4,
    ..Default::default()
}))?;
wifi.start()?;
// NO wifi.connect() for AP mode
wifi.wait_netif_up()?;
```

### HTTP Server with POST Handler
```rust
// Source: esp-idf-svc-0.51.0/src/http/server.rs lines 1051-1076 (read pattern)
// Source: esp-idf-svc-0.51.0/examples/http_server.rs lines 70-93
server.fn_handler::<anyhow::Error, _>("/save", Method::Post, move |mut req| {
    let len = req.content_len().unwrap_or(0) as usize;
    if len > 1024 {
        req.into_status_response(413)?.write_all(b"Too large")?;
        return Ok(());
    }
    let mut buf = vec![0u8; len];
    let (_headers, connection) = req.split();
    connection.read(&mut buf)?;
    // process buf...
    req.into_ok_response()?.write_all(b"Saved. Rebooting...")?;
    // trigger reboot after response is sent
    Ok(())
})?;
```

### NVS String Write (with commit)
```rust
// Source: esp-idf-svc-0.51.0/src/nvs.rs lines 571-583
// set_str() calls nvs_commit() internally — no separate commit needed.
let mut nvs = EspNvs::new(nvs_partition, "prov", true)?;
nvs.set_str("mqtt_host", "10.86.32.41")?;  // key <= 15 chars, auto-committed
nvs.set_u8("wifi_count", 1)?;
```

### NVS String Read
```rust
// Source: esp-idf-svc-0.51.0/src/nvs.rs lines 547-569
// get_str returns None if key absent, Some(str) if found.
// buf must be large enough to hold the stored value + null terminator.
let mut buf = [0u8; 65];
match nvs.get_str("mqtt_host", &mut buf) {
    Ok(Some(host)) => { /* use host */ }
    Ok(None) => { /* not stored yet */ }
    Err(e) => log::warn!("NVS read error: {:?}", e),
}
```

### GPIO Input with Pull-Up
```rust
// Source: esp-idf-hal-0.45.2/src/gpio.rs lines 465-478, 976-1007, 835-847
use esp_idf_hal::gpio::{PinDriver, Pull};

let mut gpio9 = PinDriver::input(peripherals.pins.gpio9)?;
gpio9.set_pull(Pull::Up)?;
// poll:
if gpio9.is_low() { /* button pressed */ }
```

### SoftAP Client Count (sys call)
```rust
// Source: ESP-IDF docs — esp_wifi_ap_get_sta_list; no high-level wrapper in esp-idf-svc 0.51.0
let mut sta_list = esp_idf_svc::sys::wifi_sta_list_t::default();
let ret = unsafe { esp_idf_svc::sys::esp_wifi_ap_get_sta_list(&mut sta_list as *mut _) };
let client_count = if ret == 0 { sta_list.num as u8 } else { 0 };
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hardcoded compile-time credentials (current) | NVS-stored credentials read at runtime | Phase 15 | `config.rs` WIFI_SSID/PASS/MQTT_* constants become fallbacks only, or are removed |
| Single WiFi network | Up to 3 stored networks with ordered retry | Phase 15 | PROV-05: supervisor loops all networks before giving up |
| STA-only mode | SoftAP on first boot / on demand | Phase 15 | Requires boot-path decision before WiFi init |

**Deprecated/outdated (for this project after Phase 15):**
- `config.rs` compile-time `WIFI_SSID` / `WIFI_PASS` constants: These are currently used by `wifi_connect()`. After Phase 15, `wifi_connect()` reads from NVS instead. The compile-time constants should remain as documentation/emergency fallback but `wifi_connect()` will not use them directly.

## Open Questions

1. **Captive portal redirect**
   - What we know: When a phone connects to an open WiFi SSID, some OS captive portal detectors expect a specific URL or response (Apple: `captive.apple.com`, Android: `connectivitycheck.gstatic.com`). If not handled, iOS may show "No Internet Connection" even though the portal is reachable at `192.168.71.1`.
   - What's unclear: Whether the ESP32-C6's AP + DHCP configuration triggers captive portal detection and whether ESP-IDF's httpd can redirect these probes.
   - Recommendation: For v2.0, do not implement captive portal redirect. Document that users must manually navigate to `192.168.71.1` in their browser. Post-v2.0: add DNS redirect to catch all requests, or add `Location: http://192.168.71.1/` redirect handler for Apple/Android probe URLs.

2. **Concurrent HTTP requests during provisioning**
   - What we know: `EspHttpServer` default `max_open_sockets` is 4, `max_sessions` is 16. Only one client (the provisioner's browser) is expected.
   - What's unclear: Whether browser retry behavior during the "Saved. Rebooting..." response can cause issues if reboot is triggered in the same handler that sends the response.
   - Recommendation: Send the response first (`into_ok_response()?.write_all(...)?`), then sleep 200ms to allow the response to be flushed, then call `esp_restart()`. This mirrors the OTA "reboot" path pattern.

3. **`wifi_sta_list_t` field availability on ESP32-C6**
   - What we know: `esp_wifi_ap_get_sta_list` is a standard ESP-IDF WiFi AP function; `wifi_sta_list_t.num` gives connected station count.
   - What's unclear: Whether this type is correctly bound in esp-idf-sys 0.36.1 for the RISC-V C6 target. The sys crate generates bindings from ESP-IDF headers; these are chip-specific.
   - Recommendation: Verify at compile time — if `wifi_sta_list_t` is not available, use a counter incremented on `ApStaConnected` event and decremented on `ApStaDisconnected` via the system event loop subscription. This requires subscribing to AP events, which adds complexity.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — embedded Rust firmware, no host test runner |
| Config file | N/A |
| Quick run command | `cargo build --release` (compilation check) |
| Full suite command | `cargo build --release` + flash + manual verification |

This is embedded firmware running on ESP32-C6 hardware. No host-side test harness exists. Validation is compilation success + hardware observation on device FFFEB5.

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PROV-01 | Fresh-flashed device (NVS erased) broadcasts SoftAP hotspot | manual | `cargo build --release` | N/A |
| PROV-02 | Web UI loads at device IP; WiFi SSID/password fields submit correctly | manual | `cargo build --release` | N/A |
| PROV-03 | MQTT host/port/user/pass fields appear in web UI and persist to NVS | manual | `cargo build --release` | N/A |
| PROV-04 | Three WiFi network entries stored; all readable after reboot | manual | `cargo build --release` | N/A |
| PROV-05 | Device retries all stored networks on failure; does not enter SoftAP automatically | manual | `cargo build --release` | N/A |
| PROV-06 | GPIO9 held 3s enters SoftAP; releases at 300s with no client | manual | `cargo build --release` | N/A |
| PROV-07 | MQTT "softap" payload enters SoftAP; same 300s timeout applies | manual | `cargo build --release` | N/A |
| PROV-08 | LED blinks at 500ms rate (visually distinct) during SoftAP mode | manual | `cargo build --release` | N/A |

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release` + flash + `espflash monitor` observation
- **Phase gate:** All 8 manual criteria observed on device FFFEB5 before `/gsd:verify-work`

### Wave 0 Gaps
None — existing build infrastructure covers all phase requirements. No new test files needed.

**Hardware test procedure notes:**
- To test PROV-01: run `espflash erase-flash` before flashing, then flash — device must broadcast SSID on boot
- To test PROV-05: store networks, then power device with no AP in range; verify it retries indefinitely without entering SoftAP
- To test PROV-06: use a jumper wire to hold GPIO9 low for 3 seconds during normal operation

## Sources

### Primary (HIGH confidence)
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/examples/http_server.rs` — canonical SoftAP + EspHttpServer usage (GET/POST handlers, AccessPointConfiguration)
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/http/server.rs` — EspHttpServer::fn_handler API, Configuration struct, POST body read pattern
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/wifi.rs` — AccessPointConfiguration, Configuration::AccessPoint, is_ap_started, set_configuration, BlockingWifi AP mode sequence
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/nvs.rs` — EspNvs::get_str, set_str, get_u8, set_u8, Clone impl for EspNvsPartition
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/examples/nvs_get_set_c_style.rs` — NVS namespace open + read/write pattern
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-hal-0.45.2/src/gpio.rs` — PinDriver::input(), set_pull(Pull::Up), is_low(), is_high()
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/wifi.rs` — existing wifi_connect() and wifi_supervisor() to understand integration point
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/main.rs` — initialization order, NVS partition ownership flow
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/led.rs` — LedState enum (to add SoftAP variant)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/ota.rs` — "reboot" check pattern (PROV-07 "softap" follows same structure)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/sdkconfig.defaults` — existing Kconfig; no new options needed for Phase 15

### Secondary (MEDIUM confidence)
- ESP32-C6 default AP netif IP `192.168.71.1`: Based on ESP-IDF lwIP defaults for ESP32-C6; verified consistent with the http_server.rs example comment "Go to 192.168.71.1 to test"
- NVS key 15-character limit: Documented in ESP-IDF NVS API documentation; consistent with to_cstring_arg usage in nvs.rs

### Tertiary (LOW confidence)
- `esp_wifi_ap_get_sta_list` binding availability in esp-idf-sys 0.36.1 for ESP32-C6: Not directly verified; standard ESP-IDF API but binding generation is target-specific. Needs compile-time verification.
- URL-encoding behavior of browser form submissions: Standard HTML spec behavior; not verified against specific mobile browsers.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all APIs verified directly in local registry source files
- Architecture: HIGH — boot-path NVS decision, SoftAP init sequence, and NVS read/write patterns all verified in source
- NVS key layout: HIGH — key name lengths counted manually; nvs.rs confirms set_str/get_str API
- Pitfalls: HIGH for code pitfalls (verified in source); MEDIUM for form encoding limitation (standard HTML behavior)
- Open questions: wifi_sta_list_t binding availability is LOW — needs compile verification

**Research date:** 2026-03-08
**Valid until:** 2026-06-08 (stable; ESP-IDF 5.3.3 and esp-idf-svc 0.51.0 are pinned versions)
