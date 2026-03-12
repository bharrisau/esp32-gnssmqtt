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
| provisioning.rs, config_relay.rs, ntrip_client.rs, ota.rs | `EspNvs`, `EspNvsPartition<NvsDefault>`, `get_str/get_u8/get_blob/set_*` | sequential-storage over esp-storage | GAP — sequential-storage on ESP32-C6 unverified; Phase 23 adds gnss-nvs crate + validation |
| main.rs | `EspDefaultNvsPartition::take()` | esp-storage partition init | GAP — see above |

### 4. OTA

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ota.rs | `EspOta`, `EspOta::initiate_update()`, `update.write()`, `update.complete()` | esp-bootloader-esp-idf + esp-hal-ota | GAP — esp-hal-ota 0.4.6 ESP32-C6 unconfirmed |
| ota.rs | `EspOta::mark_running_slot_valid()` | esp-bootloader-esp-idf rollback API | GAP — same blocker |
| ota.rs | `EspHttpConnection` (HTTP client for firmware download) | embassy-net TCP + manual HTTP/1.1 client or picohttp | GAP — no turnkey no_std HTTP client for OTA streaming |

### 5. UART

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| gnss.rs | `UartDriver`, `Config`, `Uart`, `Hertz`, `NON_BLOCK` | esp-hal `Uart` driver | SOLVABLE — esp-hal UART is stable in 1.0 |
| gnss.rs | `AnyIOPin`, `OutputPin`, `InputPin` traits | esp-hal GPIO traits | SOLVABLE |

### 6. TLS

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ntrip_client.rs | `EspTls`, `TlsConfig`, `use_crt_bundle_attach`, `InternalSocket` | embedded-tls (TLS 1.3 client only) or rustls-embedded | GAP — embedded-tls supports TLS 1.3 client but AUSCORS may require TLS 1.2; no CA bundle |
| ota.rs | `EspHttpConnection` (may use TLS) | see above | GAP — same |

**TLS Blocker detail:** `embedded-tls` is no_std but TLS 1.2 support is limited. The ESP-IDF approach uses mbedTLS with the bundled Espressif CA certificate store. In embassy, there is no equivalent CA bundle; CAs must be embedded at compile time. This is a significant gap for AUSCORS (port 443).

### 7. HTTP Server (SoftAP Portal)

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs | `EspHttpServer`, `EspHttpConnection`, `EspHttpRequest` | picoserve (no_std HTTP server) or hand-rolled TCP handler | GAP — no production-ready no_std HTTP server with form parsing |
| provisioning.rs | `embedded_svc::http::{Headers, Method}` | picoserve traits / manual | GAP |
| provisioning.rs | `embedded_svc::io::Write` | heapless / embedded-io | SOLVABLE |

### 8. MQTT Client

| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| mqtt.rs, mqtt_publish.rs | `EspMqttClient`, MQTT 3.1.1 over TCP | rumqttc (requires std) / minimq (no_std) / rust-mqtt (no_std) | GAP — minimq and rust-mqtt are no_std but lack the reliability of ESP-IDF MQTT; Phase 22 audit only |
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
| 1 | **NVS** (sequential-storage backing) | Phase 23 | Credential persistence is required for any autonomous device; blocks all provisioning |
| 2 | **OTA** (esp-hal-ota dual-slot) | Phase 24 | Remote firmware update critical for field device; ESP32-C6 validation needed |
| 3 | **SoftAP + DNS hijack** (gnss-softap + gnss-dns) | Phase 25 | Provisioning path; SoftAP password RESOLVED in esp-radio; DNS hijack SOLVABLE with UDP |
| 4 | **Log hook** (vprintf) | Phase 25 | Existing approach (C FFI vprintf hook) works unchanged in no_std esp-hal |
| 5 | **TLS** (embedded-tls) | Future | AUSCORS/port-443 NTRIP requires TLS 1.2; embedded-tls is TLS 1.3-only; hard blocker |
| 6 | **HTTP server** (SoftAP portal) | Future | No production-ready no_std HTTP server with form parsing; need picoserve or DIY |
| 7 | **MQTT client** | Future | minimq/rust-mqtt exist but maturity gap vs ESP-IDF MQTT |
| 8 | **SNTP** | Future | Low priority; uptime from SystemTimer is acceptable |

## Key Findings

**RESOLVED items (previously thought to be gaps):**
- SoftAP WPA2 password: supported in esp-radio 0.16.x via `AccessPointConfig::with_password().with_auth_method(AuthMethod::Wpa2Personal)` — not a gap
- DNS hijack: implementable with embassy-net UDP sockets (~50 lines); not an ESP-IDF-specific feature
- Log hook (vprintf): `esp_log_set_vprintf()` is a C ROM function available in any esp-hal build; the C shim works unchanged

**Hard GAPs for full embassy port:**
- TLS: embedded-tls supports TLS 1.3 client only; AUSCORS NTRIP (port 443) requires TLS 1.2; no CA bundle in no_std
- HTTP server: no production-ready no_std HTTP server with multi-field form POST parsing
- MQTT client: minimq/rust-mqtt exist but maturity gap vs ESP-IDF MQTT client
- OTA: esp-hal-ota 0.4.6 ESP32-C6 support unconfirmed ("cannot test if it works properly on esp32c6")
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
