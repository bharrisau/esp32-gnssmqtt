# ESP-IDF Nostd Audit

**Date:** 2026-03-12
**Scope:** All `esp-idf-svc`, `esp-idf-hal`, `esp-idf-sys`, and `embedded-svc` usages in the v2.0 firmware
**Purpose:** Map each usage to an embassy/esp-hal equivalent or document the specific blocker preventing a no_std port

## Status Legend

| Status | Meaning |
|--------|---------|
| SOLVABLE | No_std equivalent exists; implementation straightforward |
| RESOLVED | Was previously a blocker; now solved in current ecosystem |
| GAP | No complete no_std equivalent; specific blocker documented |
| REPLACED | ESP-IDF concept does not map to embassy; firmware approach changes |
| UNKNOWN | Needs verification before Phase 23-25 implementation |

## Categories

### 1. WiFi — STA Mode

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| wifi.rs | `BlockingWifi<EspWifi>`, `ClientConfiguration`, `AuthMethod` | esp-radio `WifiController` + embassy-net | SOLVABLE |
| main.rs | `EspSystemEventLoop::take()` | embassy event system | SOLVABLE |
| wifi.rs | `wifi.wait_netif_up()` | embassy-net DHCP ready signal | SOLVABLE |

### 2. WiFi — SoftAP Mode

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs, provisioning.rs | `EspNetif`, `NetifConfiguration`, `RouterConfiguration` (DHCP + DNS) | esp-radio `AccessPointConfig` + embassy-net `StaticConfigV4` | SOLVABLE |
| main.rs | `EspWifi::wrap_all` + `BlockingWifi::wrap` (AP mode) | esp-radio AP mode | SOLVABLE |
| provisioning.rs | `WifiDriver`, `EspWifi` AP configuration | esp-radio with `AccessPointConfig::with_password(WPA2Personal)` | **RESOLVED** — SoftAP password supported in esp-radio |

### 3. NVS (Non-Volatile Storage)

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs, config_relay.rs, ntrip_client.rs, ota.rs | `EspNvs`, `EspNvsPartition<NvsDefault>`, `get_str/get_u8/get_blob/set_*` | Log-based KV store over esp-storage (sequential-storage likely implements this via append-only records) | GAP — sequential-storage on ESP32-C6 unverified; Phase 23 validates and wraps with ecosystem-reusable crate |
| main.rs | `EspDefaultNvsPartition::take()` | esp-storage partition init | GAP — see above |

**NVS approach:** Use a log-based KV store strategy — records are appended sequentially, with compaction when the flash region fills. `sequential-storage` likely already implements this pattern. Two implementation paths:
1. Use `sequential-storage` directly over `esp-hal` flash driver
2. Create a thin, ecosystem-reusable crate wrapping `sequential-storage` with a typed key-value API

**Crate naming:** New crates must be generic and ecosystem-reusable (not firmware-specific). The goal is to fill gaps in the esp-hal/rust-embedded ecosystem, not to create project-specific glue.

### 4. OTA

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ota.rs | `EspOta`, `EspOta::initiate_update()`, `update.write()`, `update.complete()` | esp-hal-ota (dual-slot OTA for esp-hal targets) | GAP — esp-hal-ota 0.4.6 ESP32-C6 support unconfirmed; willing to contribute if C6 is untested |
| ota.rs | `EspOta::mark_running_slot_valid()` | esp-hal-ota rollback API | GAP — same blocker |
| ota.rs | `EspHttpConnection` (HTTP client for firmware download) | embassy-net TCP + manual HTTP/1.1 or available esp-hal ecosystem HTTP client | GAP — evaluate available HTTP clients in the esp-hal ecosystem (target is esp-hal, not pure no_std) |

**OTA target clarification:** The migration target is **esp-hal**, not necessarily fully no_std/bare-metal. There may be intermediate points in the esp-hal ecosystem (e.g., using esp-hal with some ROM/IDF library calls). Evaluate what HTTP clients are available specifically for esp-hal targets. We are willing to help mature `esp-hal-ota` if ESP32-C6 support is incomplete — contribution is viable.

### 5. UART

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| gnss.rs | `UartDriver`, `Config`, `Uart`, `Hertz`, `NON_BLOCK` | esp-hal `Uart` driver | SOLVABLE — esp-hal UART is stable in 1.0 |
| gnss.rs | `AnyIOPin`, `OutputPin`, `InputPin` traits | esp-hal GPIO traits | SOLVABLE |

### 6. TLS

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ntrip_client.rs | `EspTls`, `TlsConfig`, `use_crt_bundle_attach`, `InternalSocket` | rustls (unbuffered API) with pinned cert hash | GAP — two viable approaches; see below |
| ota.rs | `EspHttpConnection` (may use TLS) | see above | GAP — same |

**TLS Blocker detail:** `embedded-tls` is no_std but TLS 1.2 support is limited. The ESP-IDF approach uses mbedTLS with the bundled Espressif CA certificate store. In embassy/esp-hal, there is no equivalent CA bundle; CAs must be embedded at compile time or an alternative trust approach used.

**NTRIP TLS options (pick the better approach as primary recommendation):**

Option 1: Drop NTRIP input stream entirely. Receive RTCM corrections via MQTT from an external server. Avoids TLS client complexity entirely; RTCM over MQTT is already part of the architecture.

Option 2 (preferred): Keep NTRIP client. Send a trusted cert hash with the NTRIP settings in the provisioning config payload. Use **rustls** with its unbuffered API — this is likely fine for streaming NTRIP data where the TLS session is long-lived. The provisioning form gains a `cert_hash` field; the firmware pins to this hash instead of using a CA bundle.

**Evaluate:** rustls is the primary TLS library to evaluate for this path. The unbuffered API reduces stack/heap pressure suitable for embedded contexts.

### 7. HTTP Server (SoftAP Portal)

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs | `EspHttpServer`, `EspHttpConnection`, `EspHttpRequest` | picoserve (no_std HTTP server) — see also nanofish | GAP — no production-ready no_std HTTP server with form parsing |
| provisioning.rs | `embedded_svc::http::{Headers, Method}` | picoserve traits / manual | GAP |
| provisioning.rs | `embedded_svc::io::Write` | heapless / embedded-io | SOLVABLE |

**HTTP server candidates:**
- **picoserve**: looks suitable; no_std async HTTP server
- **nanofish**: does both HTTP client and server; may be smaller than picoserve — worth evaluating if binary size matters

### 8. MQTT Client

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| mqtt.rs, mqtt_publish.rs | `EspMqttClient`, MQTT 3.1.1 over TCP | rumqttc (requires std) / minimq (no_std) / rust-mqtt (no_std) | GAP — BENCHMARK PHASE 23 / IMPL PHASE 24 |
| mqtt_publish.rs | `embedded_svc::mqtt::client::{QoS, EventPayload}` | minimq / rust-mqtt enums | SOLVABLE (trait-level) |

### 9. DNS Hijack (Captive Portal)

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs | `std::net::UdpSocket` (port 53) | embassy-net `UdpSocket` | **SOLVABLE** — embassy-net UDP socket supports bind + recv + send |

**Note:** The DNS hijack is not an ESP-IDF-specific feature. It uses a standard UDP socket. embassy-net provides async UDP sockets. Implementation needs custom DNS query parser (simple: extract QNAME, reply with A record for portal IP). This is ~50 lines, not a gap.

### 10. System Calls (esp-idf-sys direct)

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs, wifi.rs, watchdog.rs, provisioning.rs | `esp_restart()` | `esp_hal::reset::software_reset()` | SOLVABLE |
| main.rs | `nvs_flash_erase()` | esp-storage partition erase | SOLVABLE |
| main.rs, many | `uxTaskGetStackHighWaterMark()` | N/A in embassy (no FreeRTOS tasks) | REPLACED — embassy tasks have no stack HWM API; remove diagnostics |
| mqtt.rs, main.rs | `esp_timer_get_time()` (uptime) | esp-hal `SystemTimer::now()` | SOLVABLE |
| mqtt.rs | `esp_get_free_heap_size()` (heap diagnostic) | embassy no_std has no general heap API | GAP (minor) — heartbeat field becomes None |
| main.rs | `esp_wifi_ap_get_sta_list()` | esp-radio AP station list API | UNKNOWN — needs verification |
| device_id.rs | `esp_efuse_mac_get_default` | esp-hal eFuse API | SOLVABLE — esp-hal provides eFuse access |

### 11. Log Hook

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| log_relay.rs | `EspLogger::new()` (UART log output) | esp-println + log crate | SOLVABLE |
| log_relay.rs | `esp_log_system_timestamp()` / `esp_log_timestamp()` | esp-hal SystemTimer | SOLVABLE |
| log_shim.c | `esp_log_set_vprintf()` (C vprintf hook) | **Still available in no_std esp-hal** — ESP-IDF C runtime is present | **SOLVABLE** — vprintf hook works in any esp-hal context; C ROM function always present |

### 12. SNTP

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs | `EspSntp::new_default()` | embassy-net DNS + NTP client (sntp crate / manual NTP over UDP) | GAP (low priority) — NTP useful but not critical; heartbeat uptime works without wall-clock |

## Gap Priority Ranking

Per Phase 22 success criteria, NVS/OTA/SoftAP/DNS/log hook are explicitly ranked for Phase 23-25 implementation order.

| Rank | Capability | Phase | Rationale |
|------|-----------|-------|-----------|
| 1 | **NVS** (log-based KV store; sequential-storage backing) | Phase 23 | Credential persistence is required for any autonomous device; blocks all provisioning |
| 2 | **OTA** (esp-hal-ota dual-slot) | Phase 24 | Remote firmware update critical for field device; ESP32-C6 validation needed; contribute if C6 untested |
| 3 | **SoftAP + DNS hijack** (ecosystem-reusable crates) | Phase 25 | Provisioning path; SoftAP password RESOLVED in esp-radio; DNS hijack SOLVABLE with UDP |
| 4 | **Log hook** (vprintf) | Phase 25 | Existing approach (C FFI vprintf hook) works unchanged in no_std esp-hal |
| 5 | **TLS** (rustls unbuffered, cert-hash pinning) | Future | NTRIP TLS — preferred approach is cert-hash pinning via rustls; alternative is RTCM-over-MQTT |
| 6 | **HTTP server** (SoftAP portal) | Future | No production-ready no_std HTTP server with form parsing; evaluate picoserve vs nanofish |
| 7 | **MQTT client** | BENCHMARK PHASE 23 / IMPL PHASE 24 | minimq/rust-mqtt exist but maturity gap vs ESP-IDF MQTT; benchmark before committing |
| 8 | **SNTP** | Future | Low priority; uptime from SystemTimer is acceptable |

## Key Findings

**RESOLVED items (previously thought to be gaps):**
- SoftAP WPA2 password: supported in esp-radio 0.16.x via `AccessPointConfig::with_password().with_auth_method(AuthMethod::Wpa2Personal)` — not a gap
- DNS hijack: implementable with embassy-net UDP sockets (~50 lines); not an ESP-IDF-specific feature
- Log hook (vprintf): `esp_log_set_vprintf()` is a C ROM function available in any esp-hal build; the C shim works unchanged

**Hard GAPs for full embassy/esp-hal port:**
- TLS: no CA bundle in esp-hal; preferred path is cert-hash pinning via rustls unbuffered API; alternative is dropping NTRIP and routing corrections via MQTT
- HTTP server: no production-ready no_std HTTP server with multi-field form POST parsing; picoserve and nanofish are candidates
- MQTT client: minimq/rust-mqtt exist but maturity gap vs ESP-IDF MQTT client; benchmark required
- OTA: esp-hal-ota 0.4.6 ESP32-C6 support unconfirmed; willing to contribute if C6 path is untested
- NVS: sequential-storage on ESP32-C6 flash unverified; build/hardware test needed in Phase 23

## State of the Art Notes

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| esp-wifi crate | esp-radio crate (replaces esp-wifi) | esp-hal 1.0-rc.1 (Oct 2024) | All esp-wifi docs/examples now show esp-radio API |
| esp-hal-embassy | esp-rtos (scheduler extracted) | esp-hal 1.0 | Embassy integration now through esp-rtos |
| No SoftAP password in esp-wifi | SoftAP password + WPA2Personal in esp-radio | esp-radio 0.16.x | **Removes previously noted SoftAP blocker** |
| embedded-storage in esp-storage repo | esp-storage merged into esp-hal | May 2024 | esp-storage repo archived; use esp-hal |

**Deprecated/outdated:**
- `esp-wifi` crate: replaced by `esp-radio`; crates.io page still exists but development moved
- `esp-hal-embassy` crate: functionality merged into `esp-rtos`
- esp-storage standalone repository: archived, moved to esp-hal monorepo

## Implementation Notes

Additional requirements and design decisions to carry forward into Phase 23-25 implementation.

### WiFi Scan in SoftAP Portal

When the SoftAP provisioning portal is running, perform a WiFi station scan (scan for nearby APs while in AP+STA mode) and display the discovered SSIDs on the portal web page. This lets the user select their home/office network from a dropdown rather than typing the SSID manually — improves UX for WiFi credential entry significantly.

### UM980 Reset on Config Apply

Add a `reset: true` boolean field to the `/config` MQTT payload spec. Behaviour:
- When `reset: true` is present and set, emit a RESET command to the UM980 and wait for it to respond with the device name before applying the new configuration.
- Also trigger this reset+reapply sequence on the **first config apply after a device reboot** (device may have lost volatile UM980 state during power cycle).

This ensures the UM980 is always in a known clean state when configuration is applied.

### SoftAP SSID

Change the SoftAP SSID from the current value to `GNSS-[ID]` where `[ID]` is the device ID (same value used elsewhere in the firmware, e.g., the MQTT client ID). Use this same `GNSS-[ID]` string as the WPA2 PSK password for the SoftAP network. Both SSID and password derive from the device ID, making pairing straightforward without per-device configuration.
