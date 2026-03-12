# Phase 22: Workspace + Nostd Audit - Research

**Researched:** 2026-03-12
**Domain:** Cargo workspace layout (mixed embedded/host targets) + ESP-IDF dependency audit against embassy/esp-hal ecosystem
**Confidence:** HIGH for workspace patterns and audit categories; MEDIUM for current esp-hal/esp-radio API details (fast-moving ecosystem)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INFRA-01 | Developer can build firmware and server binary from the same Cargo workspace without target conflicts (`resolver = "2"`, firmware/ + server/ + crates/ layout) | Workspace layout patterns, resolver="2" behaviour, .cargo/config.toml scoping rules |
| NOSTD-01 | Complete audit of all `esp-idf-svc`, `esp-idf-hal`, and `esp-idf-sys` usages mapped to embassy/esp-hal equivalents or flagged as gaps | Complete esp-idf usage inventory from source, embassy/esp-hal equivalence table, gap ranking |
</phase_requirements>

---

## Summary

Phase 22 does two independent things: restructure the repo into a Cargo workspace that can build both the ESP32-C6 firmware and a future host server without target conflicts, and produce a written audit that maps every current ESP-IDF dependency call to an embassy/esp-hal equivalent or documents the specific blocker preventing a no_std port today.

The workspace problem has a well-understood solution: the Ferrous Systems nested-workspace pattern places the embedded firmware in a `firmware/` subdirectory with its own `.cargo/config.toml` that specifies the RISC-V target and build-std settings. The server and shared gap crates live in the root workspace, which carries no embedded-target defaults. `resolver = "2"` at the root workspace level prevents std features from being unified into no_std gap crate members when both are built together. Building the server does not trigger the embedded `.cargo/config.toml` because Cargo does not read config files from crates within a workspace when invoked from the workspace root — the firmware config only activates when `cargo build` is run from inside `firmware/`, or when the firmware member is explicitly targeted with `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf`.

The nostd audit must enumerate every API call against six dependency categories (`esp-idf-svc`, `esp-idf-hal`, `esp-idf-sys`, `embedded-svc`, `esp-idf-hal` re-exports, and system calls via `sys::*`). The current firmware uses esp-idf for: WiFi (STA + SoftAP), NVS, OTA, UART, TLS (mbedTLS/EspTls), HTTP server (SoftAP portal), HTTP client (OTA download), MQTT client, SNTP, system calls (`esp_restart`, `esp_timer_get_time`, `esp_get_free_heap_size`, `uxTaskGetStackHighWaterMark`, `nvs_flash_erase`), and a C vprintf hook in `log_shim.c`. The audit must map each of these to the esp-hal 1.x / esp-radio / embassy-net ecosystem or document the specific blocker for each gap.

**Primary recommendation:** Use the nested workspace pattern — firmware in `firmware/` with its own `.cargo/config.toml`, root workspace holds server + gap crates with `resolver = "2"`. Produce the audit document as a committed markdown file enumerating all categories with priority ranking for Phase 23-25 gap crate work.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-hal | 1.0.x | no_std HAL for ESP32 peripherals (GPIO, UART, SPI, I2C) | Official Espressif no_std HAL; 1.0.0-beta.0 released Feb 2025 |
| esp-radio | 0.16.x | no_std WiFi/BLE/ESP-NOW driver (replaced esp-wifi) | Espressif-official; renamed from esp-wifi at esp-hal 1.0-rc.1 |
| embassy-net | 0.7.x | async no_std TCP/UDP network stack (smoltcp) | Standard async embassy networking; UDP sockets sufficient for DNS hijack |
| esp-storage | (merged into esp-hal) | embedded-storage traits for ESP32 NOR flash | Required by sequential-storage for no_std NVS backing |
| sequential-storage | latest | Key-value and queue storage over embedded-storage | no_std NVS replacement candidate; see NOSTD-03 |
| esp-bootloader-esp-idf | 0.4.0 | no_std bootloader support for dual-slot OTA | Oct 2025; supports ESP32-C6 |
| esp-hal-ota | 0.4.6 | no_std OTA write to inactive partition | Companion to esp-bootloader-esp-idf; ESP32-C6 listed but untested per docs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| esp-println | latest | print!/println!/log crate backend for no_std | Replace EspLogger + MqttLogger uart output path |
| embassy-executor | latest | async task executor for embassy | Required with esp-rtos integration |
| embedded-svc | latest | Transport-agnostic traits (WiFi, HTTP, MQTT) | Current firmware already uses these; keep as interface layer |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| esp-radio (esp-hal ecosystem) | smoltcp + custom WiFi driver | esp-radio is the Espressif-official path; custom driver is unsupported |
| esp-bootloader-esp-idf | Custom partition-table OTA | Crate handles dual-slot protocol; DIY is 300+ lines and partition-layout-specific |
| sequential-storage | littlefs2 or ekv | sequential-storage has no file system overhead; best fit for key-value NVS |

**Installation (firmware crate):**
```bash
# In firmware/Cargo.toml — additions for no_std migration (not Phase 22 itself)
# Phase 22 is audit only; no new dependencies added to firmware
```

---

## Architecture Patterns

### Recommended Project Structure

```
esp32-gnssmqtt/          # root workspace
├── Cargo.toml           # [workspace] resolver="2" members=["firmware","gnss-server","crates/*"]
├── .cargo/
│   └── config.toml      # NO build.target here — server and gap crates use host default
├── firmware/            # ESP32-C6 firmware (inner workspace member)
│   ├── Cargo.toml       # [package] name="esp32-gnssmqtt-firmware"
│   ├── .cargo/
│   │   └── config.toml  # build.target = "riscv32imac-esp-espidf" + ldproxy + build-std
│   ├── src/             # current src/ contents moved here
│   ├── build.rs
│   ├── partitions.csv
│   └── sdkconfig.defaults
├── gnss-server/         # future host server (Phase 23+)
│   └── Cargo.toml       # [package] name="gnss-server"
└── crates/              # shared no_std gap crates
    └── gnss-nvs/        # created Phase 23+
```

**CRITICAL insight — .cargo/config.toml scoping in workspaces:**
Cargo does not read `.cargo/config.toml` files from workspace member subdirectories when invoked from the workspace root. Therefore, `firmware/.cargo/config.toml` containing `build.target = "riscv32imac-esp-espidf"` is ONLY active when you `cd firmware && cargo build` or when you explicitly pass `--target riscv32imac-esp-espidf`. Building the server from the workspace root with `cargo build -p gnss-server` uses the host default target and never reads `firmware/.cargo/config.toml`.

This means the success criterion "firmware builds with `cargo build -p esp32-gnssmqtt-firmware`" REQUIRES passing `--target riscv32imac-esp-espidf` explicitly from the workspace root, or the invocation must be done from inside `firmware/`. The plan must reflect this.

### Pattern 1: Nested .cargo/config.toml Scope Isolation
**What:** Firmware inner member has its own `.cargo/config.toml` with embedded target settings. Root workspace `.cargo/config.toml` has no `build.target`.
**When to use:** Any mixed-target workspace with embedded + host members.
**Example:**
```toml
# firmware/.cargo/config.toml  (active only when building from firmware/ directly)
[build]
target = "riscv32imac-esp-espidf"

[target.riscv32imac-esp-espidf]
linker = "ldproxy"
runner = "espflash flash --monitor"
rustflags = ["--cfg", "espidf_time64"]

[unstable]
build-std = ["std", "panic_abort"]
build-std-features = []

[env]
MCU = "esp32c6"
ESP_IDF_VERSION = "v5.3.3"
```

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "firmware",
    "gnss-server",
    "crates/*",
]
```

### Pattern 2: resolver="2" for std/no_std Feature Isolation
**What:** `resolver = "2"` prevents features from being unified across workspace members for target-specific dependencies.
**When to use:** Any workspace with both std and no_std members sharing dependencies.
**Example:**
```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [...]
```
With resolver="2", a dependency that has a `std` feature being requested by `gnss-server` does NOT propagate that feature into no_std gap crates that share the dependency.

### Pattern 3: Gap Crate with BLOCKER.md
**What:** A crate skeleton that defines traits but has no implementation. A `BLOCKER.md` documents the specific missing capability.
**When to use:** Any ESP-IDF subsystem with no complete no_std/embassy equivalent today.
**Example structure:**
```
crates/gnss-ota/
├── Cargo.toml            # [package] name="gnss-ota"; #![no_std]
├── src/
│   └── lib.rs            # pub trait OtaStore { ... }
└── BLOCKER.md            # "Blocker: esp-hal-ota v0.4.6 cannot confirm ESP32-C6 support..."
```

### Anti-Patterns to Avoid
- **Root .cargo/config.toml with build.target:** Setting an embedded target at the workspace root breaks all host-target member builds. Never put `build.target = "riscv32imac-esp-espidf"` at the root.
- **Building firmware from workspace root without --target:** `cargo build -p esp32-gnssmqtt-firmware` at workspace root uses the host target and fails on esp-idf-sys. Always specify `--target riscv32imac-esp-espidf`.
- **Resolving to resolver="1" by omission:** Pre-edition-2021 workspaces default to resolver="1" which unifies features. Always set `resolver = "2"` explicitly.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| no_std NOR flash key-value store | Custom k/v store over raw flash | sequential-storage | Handles wear levelling, corruption recovery, power-loss safety |
| Dual-slot OTA partition writing | Custom partition table parser + write logic | esp-bootloader-esp-idf + esp-hal-ota | Partition table format, slot selection, and rollback logic are tricky and hardware-specific |
| no_std WiFi + TCP/UDP | Custom WiFi driver | esp-radio + embassy-net | ~300k+ lines of Espressif WiFi firmware; entirely infeasible |
| Workspace resolver configuration | Per-crate feature tricks | `resolver = "2"` in workspace Cargo.toml | One line; handles all target-specific feature unification |

**Key insight:** The gap crate work in Phase 22 is documentation only — the hard problems (SoftAP password, DNS hijack, log hook, TLS) are partially or fully solvable in the embassy ecosystem but require careful blocker documentation to guide later implementation phases.

---

## Common Pitfalls

### Pitfall 1: .cargo/config.toml Not Read From Workspace Root
**What goes wrong:** Developer puts `build.target = "riscv32imac-esp-espidf"` in `firmware/.cargo/config.toml` and expects `cargo build -p esp32-gnssmqtt-firmware` from workspace root to pick it up. It doesn't. Build uses host target and fails.
**Why it happens:** Cargo only reads config files in the current directory and its ancestors, not in workspace member subdirectories.
**How to avoid:** Always explicitly pass `--target riscv32imac-esp-espidf` when building firmware from workspace root. Document this in a `just` recipe or Makefile.
**Warning signs:** `error: package 'esp-idf-sys' failed to build` or `link error: cannot find -lldproxy` when building from workspace root.

### Pitfall 2: Feature Unification Breaking no_std Gap Crates
**What goes wrong:** `gnss-server` depends on a crate with `std` feature. A gap crate depends on the same crate defaulting to no_std. With resolver="1", the `std` feature gets unified into the gap crate and it fails to build as no_std.
**Why it happens:** Cargo resolver="1" unifies all features across all workspace members regardless of target.
**How to avoid:** Set `resolver = "2"` in workspace root Cargo.toml. Verify with `cargo tree -e features -p gnss-nvs` that no std features appear.
**Warning signs:** Gap crate compiles fine standalone but fails when built as part of the workspace.

### Pitfall 3: OTA Gap — ESP32-C6 Unconfirmed in esp-hal-ota
**What goes wrong:** Phase 24 implements gnss-ota gap crate assuming esp-hal-ota works on ESP32-C6. The crate docs say "I cannot test if it works properly on esp32c6."
**Why it happens:** esp-hal-ota 0.4.6 only confirmed working on ESP32, ESP32-S3, ESP32-C3.
**How to avoid:** The BLOCKER.md for gnss-ota must explicitly call out "ESP32-C6 support unconfirmed in esp-hal-ota 0.4.6" as the primary blocker.
**Warning signs:** OTA partition write succeeds but device boots factory image; or esp_ota_begin fails at runtime.

### Pitfall 4: SoftAP Password Was Previously a Blocker — Now Resolved
**What goes wrong:** Audit incorrectly lists SoftAP password as a hard blocker. This was true for older esp-wifi versions but is resolved in esp-radio.
**Why it happens:** STATE.md references "SoftAP password-protection" as a blocker from earlier planning. The STATE.md note was correct at the time but esp-radio now supports `AccessPointConfig::with_password().with_auth_method(AuthMethod::Wpa2Personal)`.
**How to avoid:** Verify during audit — esp-radio 0.16.x provides WPA2Personal SoftAP with password. The audit should mark this as RESOLVED.
**Warning signs:** Blocker document says SoftAP password is impossible with esp-radio — this is wrong.

### Pitfall 5: DNS Hijack Requires UDP Raw Socket or Custom Responder
**What goes wrong:** Assuming embassy-net DNS resolver is what's needed for captive portal. The current firmware implements a DNS SERVER (responding to client queries), not a DNS client.
**Why it happens:** "DNS" is ambiguous. The captive portal hijack requires binding UDP port 53, receiving queries, and replying with a fixed IP — all resolvable with embassy-net UDP sockets but no crate provides this as a turnkey solution.
**How to avoid:** The audit should note that the DNS hijack is implementable with embassy-net's UDP socket API — it is NOT a hard gap. Mark as SOLVABLE with implementation notes.

### Pitfall 6: Log Hook (vprintf) Requires C FFI in Any Framework
**What goes wrong:** Assuming the C vprintf hook approach in `log_shim.c` is ESP-IDF-specific. Any framework running on top of esp-idf (including nostd) can still install a vprintf hook because the C runtime is always present.
**Why it happens:** log_shim.c uses `esp_log_set_vprintf()` which is an ESP-IDF C API, making it seem like an ESP-IDF dependency.
**How to avoid:** The audit should mark the log hook as SOLVABLE via FFI in any esp-hal context. The Rust side (MqttLogger wrapping `log::Log`) is framework-agnostic. The C shim just needs to call `esp_log_set_vprintf()` which is in ROM and available in no_std esp-hal builds.

---

## Code Examples

Verified patterns from official sources and codebase analysis:

### Workspace Root Cargo.toml
```toml
# Source: Official Cargo docs, resolver="2" pattern
[workspace]
resolver = "2"
members = [
    "firmware",
    "gnss-server",
    "crates/*",
]

# Shared workspace dependencies (optional — avoids version skew)
[workspace.dependencies]
log = "0.4"
bytes = "1"
```

### Firmware Member Cargo.toml
```toml
# firmware/Cargo.toml
[package]
name = "esp32-gnssmqtt-firmware"
version = "0.1.0"
edition = "2021"
rust-version = "1.77"

# All current [dependencies] remain unchanged
[dependencies]
esp-idf-svc = { version = "=0.51.0", features = [] }
esp-idf-hal = "=0.45.2"
esp-idf-sys = "=0.36.1"
# ... etc (no changes for Phase 22)
```

### Building From Workspace Root
```bash
# Firmware: must pass --target explicitly (firmware/.cargo/config.toml not read from root)
cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf

# Server: uses host default target, no --target needed
cargo build -p gnss-server

# Verify resolver="2" prevents std feature leakage into gap crates:
cargo tree -e features -p gnss-nvs 2>/dev/null | grep std || echo "no std in gap crates"
```

### esp-radio SoftAP with Password (RESOLVED — not a gap)
```rust
// Source: esp32.implrust.com/wifi/access-point/index.html (MEDIUM confidence)
// This shows SoftAP password IS supported in current esp-radio
let ap_config = AccessPointConfig::default()
    .with_ssid(SSID.into())
    .with_password(PASSWORD.into())
    .with_auth_method(esp_radio::wifi::AuthMethod::Wpa2Personal);
```

---

## ESP-IDF Dependency Audit

This is the core deliverable for NOSTD-01. Every usage in the firmware mapped by category.

### Category: WiFi (STA mode)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| wifi.rs | `BlockingWifi<EspWifi>`, `ClientConfiguration`, `AuthMethod` | esp-radio `WifiController` + embassy-net | SOLVABLE |
| main.rs | `EspSystemEventLoop::take()` | embassy event system | SOLVABLE |
| wifi.rs | `wifi.wait_netif_up()` | embassy-net DHCP ready signal | SOLVABLE |

### Category: WiFi (SoftAP mode)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs, provisioning.rs | `EspNetif`, `NetifConfiguration`, `RouterConfiguration` (DHCP + DNS) | esp-radio `AccessPointConfig` + embassy-net `StaticConfigV4` | SOLVABLE |
| main.rs | `EspWifi::wrap_all` + `BlockingWifi::wrap` (AP mode) | esp-radio AP mode | SOLVABLE |
| provisioning.rs | `WifiDriver`, `EspWifi` AP configuration | esp-radio with `AccessPointConfig::with_password(WPA2Personal)` | **RESOLVED** — SoftAP password supported in esp-radio |

### Category: NVS (Non-Volatile Storage)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs, config_relay.rs, ntrip_client.rs, ota.rs | `EspNvs`, `EspNvsPartition<NvsDefault>`, `get_str/get_u8/get_blob/set_*` | sequential-storage over esp-storage | GAP — sequential-storage on ESP32-C6 unverified; Phase 23 adds gnss-nvs crate + validation |
| main.rs | `EspDefaultNvsPartition::take()` | esp-storage partition init | GAP — see above |

### Category: OTA
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ota.rs | `EspOta`, `EspOta::initiate_update()`, `update.write()`, `update.complete()` | esp-bootloader-esp-idf + esp-hal-ota | GAP — esp-hal-ota 0.4.6 ESP32-C6 unconfirmed |
| ota.rs | `EspOta::mark_running_slot_valid()` | esp-bootloader-esp-idf rollback API | GAP — same blocker |
| ota.rs | `EspHttpConnection` (HTTP client for firmware download) | embassy-net TCP + manual HTTP/1.1 client or picohttp | GAP — no turnkey no_std HTTP client for OTA streaming |

### Category: UART
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| gnss.rs | `UartDriver`, `Config`, `Uart`, `Hertz`, `NON_BLOCK` | esp-hal `Uart` driver | SOLVABLE — esp-hal UART is stable in 1.0 |
| gnss.rs | `AnyIOPin`, `OutputPin`, `InputPin` traits | esp-hal GPIO traits | SOLVABLE |

### Category: TLS
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| ntrip_client.rs | `EspTls`, `TlsConfig`, `use_crt_bundle_attach`, `InternalSocket` | embedded-tls (TLS 1.3 client only) or rustls-embedded | GAP — embedded-tls supports TLS 1.3 client but AUSCORS may require TLS 1.2; no CA bundle |
| ota.rs | `EspHttpConnection` (may use TLS) | see above | GAP — same |

**TLS Blocker detail:** `embedded-tls` is no_std but TLS 1.2 support is limited. The ESP-IDF approach uses mbedTLS with the bundled Espressif CA certificate store. In embassy, there is no equivalent CA bundle; CAs must be embedded at compile time. This is a significant gap for AUSCORS (port 443).

### Category: HTTP Server (SoftAP portal)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs | `EspHttpServer`, `EspHttpConnection`, `EspHttpRequest` | picoserve (no_std HTTP server) or hand-rolled TCP handler | GAP — no production-ready no_std HTTP server with form parsing |
| provisioning.rs | `embedded_svc::http::{Headers, Method}` | picoserve traits / manual | GAP |
| provisioning.rs | `embedded_svc::io::Write` | heapless / embedded-io | SOLVABLE |

### Category: MQTT Client
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| mqtt.rs, mqtt_publish.rs | `EspMqttClient`, MQTT 3.1.1 over TCP | rumqttc (requires std) / minimq (no_std) / rust-mqtt (no_std) | GAP — minimq and rust-mqtt are no_std but lack the reliability of ESP-IDF MQTT; Phase 22 audit only |
| mqtt_publish.rs | `embedded_svc::mqtt::client::{QoS, EventPayload}` | minimq / rust-mqtt enums | SOLVABLE (trait-level) |

### Category: DNS Hijack (Captive Portal)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| provisioning.rs | `std::net::UdpSocket` (port 53) | embassy-net `UdpSocket` | **SOLVABLE** — embassy-net UDP socket supports bind + recv + send |

**Note:** The DNS hijack is not an ESP-IDF-specific feature. It uses a standard UDP socket. embassy-net provides async UDP sockets. Implementation needs custom DNS query parser (simple: extract QNAME, reply with A record for portal IP). This is ~50 lines, not a gap.

### Category: System Calls (esp-idf-sys)
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs, wifi.rs, watchdog.rs, provisioning.rs | `esp_restart()` | `esp_hal::reset::software_reset()` | SOLVABLE |
| main.rs | `nvs_flash_erase()` | esp-storage partition erase | SOLVABLE |
| main.rs, many | `uxTaskGetStackHighWaterMark()` | N/A in embassy (no FreeRTOS tasks) | REPLACED — embassy tasks have no stack HWM API; remove diagnostics |
| mqtt.rs, main.rs | `esp_timer_get_time()` (uptime) | esp-hal `SystemTimer::now()` | SOLVABLE |
| mqtt.rs | `esp_get_free_heap_size()` (heap diagnostic) | embassy no_std has no general heap API | GAP (minor) — heartbeat field becomes None |
| main.rs | `esp_wifi_ap_get_sta_list()` | esp-radio AP station list API | UNKNOWN — needs verification |
| device_id.rs | `esp_efuse_mac_get_default` | esp-hal eFuse API | SOLVABLE — esp-hal provides eFuse access |

### Category: Log Hook
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| log_relay.rs | `EspLogger::new()` (UART log output) | esp-println + log crate | SOLVABLE |
| log_relay.rs | `esp_log_system_timestamp()` / `esp_log_timestamp()` | esp-hal SystemTimer | SOLVABLE |
| log_shim.c | `esp_log_set_vprintf()` (C vprintf hook) | **Still available in no_std esp-hal** — ESP-IDF C runtime is present | **SOLVABLE** — vprintf hook works in any esp-hal context; C ROM function always present |

### Category: SNTP
| Module | ESP-IDF API | no_std Equivalent | Status |
|--------|------------|-------------------|--------|
| main.rs | `EspSntp::new_default()` | embassy-net DNS + NTP client (sntp crate / manual NTP over UDP) | GAP (low priority) — NTP useful but not critical; heartbeat uptime works without wall-clock |

---

## Gap Priority Ranking

Per the success criterion, NVS/OTA/SoftAP/DNS/log hook must be explicitly ranked:

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

---

## State of the Art

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

---

## Open Questions

1. **esp-hal-ota v0.4.6 on ESP32-C6**
   - What we know: Crate docs say "I cannot test if it works properly on esp32c6" — ESP32, ESP32-S3, ESP32-C3 confirmed
   - What's unclear: Whether the C6 RISC-V OTA partition format differs from confirmed chips
   - Recommendation: Phase 24 BLOCKER.md must document this as primary OTA gap; plan to hardware-validate in Phase 24 execution

2. **sequential-storage on ESP32-C6 flash**
   - What we know: esp-storage (now in esp-hal) supports ESP32-C6; sequential-storage uses embedded-storage traits
   - What's unclear: Minimum sector alignment and page-erase size compatibility; Phase 23 includes a minimal build test
   - Recommendation: Phase 23 plan includes a `cargo build` smoke test of the gnss-nvs crate targeting riscv32imac-esp-espidf

3. **`esp_wifi_ap_get_sta_list` equivalent in esp-radio**
   - What we know: provisioning.rs uses this to count connected STA clients during SoftAP portal
   - What's unclear: Whether esp-radio exposes a connected station count API
   - Recommendation: Mark as LOW priority in audit; portal works without it (count just won't be logged)

4. **picoserve maturity for SoftAP portal**
   - What we know: picoserve exists as a no_std HTTP server
   - What's unclear: Whether it handles multi-part form POST parsing needed for the provisioning form
   - Recommendation: Phase 25 blocker doc should evaluate picoserve; fallback is hand-rolled TCP HTTP handler (~300 lines)

---

## Validation Architecture

`workflow.nyquist_validation` key is absent from config.json — treat as enabled.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Cargo built-in (`cargo build` + `cargo check`) |
| Config file | workspace Cargo.toml (to be created) |
| Quick run command | `cargo check -p gnss-server && cargo check -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf` |
| Full suite command | `cargo build -p gnss-server && cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf` |

Note: Phase 22 is a structural/documentation phase. Tests are compile checks and file existence checks, not unit tests.

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INFRA-01 | Firmware builds from workspace root for riscv target | Build smoke | `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf` | ❌ Wave 0 |
| INFRA-01 | Server builds from workspace root for host target | Build smoke | `cargo build -p gnss-server` | ❌ Wave 0 |
| INFRA-01 | resolver="2" in workspace Cargo.toml | File check | `grep 'resolver = "2"' Cargo.toml` | ❌ Wave 0 |
| NOSTD-01 | Audit document exists and covers all 6 categories | File check | `ls docs/nostd-audit.md` or similar | ❌ Wave 0 |
| NOSTD-01 | Gap list includes ranked priorities for NVS/OTA/SoftAP/DNS/log | Manual review | — | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo check -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf && cargo check -p gnss-server`
- **Per wave merge:** `cargo build -p esp32-gnssmqtt-firmware --target riscv32imac-esp-espidf && cargo build -p gnss-server`
- **Phase gate:** Both builds green + audit document committed before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `Cargo.toml` (workspace root) — covers INFRA-01
- [ ] `firmware/Cargo.toml` — move current package definition
- [ ] `firmware/.cargo/config.toml` — move current `.cargo/config.toml`
- [ ] `gnss-server/Cargo.toml` — minimal stub (empty binary, no dependencies yet)
- [ ] `docs/nostd-audit.md` (or committed markdown in repo) — covers NOSTD-01

---

## Sources

### Primary (HIGH confidence)
- Cargo Reference — Configuration: https://doc.rust-lang.org/cargo/reference/config.html — workspace .cargo/config.toml scoping rules (confirmed: member configs NOT read from workspace root)
- Cargo Reference — Resolver: https://doc.rust-lang.org/cargo/reference/resolver.html — resolver="2" behaviour
- Current codebase (`src/*.rs`) — complete grep inventory of all `use esp_idf*` and `use embedded_svc*` imports

### Secondary (MEDIUM confidence)
- Ferrous Systems: https://ferrous-systems.com/blog/test-embedded-app/ — nested workspace pattern with separate .cargo/config.toml
- impl Rust for ESP32 (esp32.implrust.com/wifi/access-point/) — confirmed SoftAP password support in esp-radio
- esp-radio docs: https://docs.espressif.com/projects/rust/esp-radio/0.16.0/esp32c3/esp_radio/wifi/index.html — AccessPointConfig, AuthMethod::Wpa2Personal
- esp-bootloader-esp-idf lib.rs: https://lib.rs/crates/esp-bootloader-esp-idf — v0.4.0, ESP32-C6 listed as optional dependency
- esp-hal-ota docs.rs: https://docs.rs/esp-hal-ota — v0.4.6; ESP32-C6 "cannot confirm"
- Espressif blog (2025-02): https://developer.espressif.com/blog/2025/02/rust-esp-hal-beta/ — esp-hal 1.0-beta.0, esp-radio as next stabilization target
- nickb.dev feature unification: https://nickb.dev/blog/cargo-workspace-and-the-feature-unification-pitfall/ — resolver="2" prevents std feature leakage

### Tertiary (LOW confidence)
- Rust Users Forum thread: https://users.rust-lang.org/t/cargo-workspace-members-with-different-target-architectures/122464 — per-package-target (unstable), workarounds
- GitHub issue: https://github.com/rust-lang/cargo/issues/9956 — resolver="2" not default in workspaces

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — workspace patterns verified from official Cargo docs; esp-hal versions from official Espressif sources
- Architecture: HIGH — nested workspace pattern verified from Ferrous Systems blog and Cargo docs
- Pitfalls: HIGH — .cargo/config.toml scoping confirmed from official Cargo reference; other pitfalls confirmed from source code analysis
- Audit categories: MEDIUM — coverage complete from grep of source; embassy/esp-hal equivalents based on docs and community sources; some API details may have changed in latest esp-hal versions

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 for workspace patterns (very stable); 2026-03-26 for esp-hal API details (fast-moving, re-verify before Phase 23)
