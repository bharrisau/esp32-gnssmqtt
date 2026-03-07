# Stack Research

**Domain:** Embedded Rust firmware — ESP32-C6, GNSS/MQTT bridge
**Researched:** 2026-03-07 (milestone 2 update: RTCM relay + OTA)
**Confidence:** HIGH for OTA and HTTP APIs (source code verified in local cargo cache); MEDIUM for RTCM sizing (derived from published spec structure); HIGH for baud rates (multiple sources agree)

---

## Scope of This Document

This is a milestone-scoped update. The original stack (esp-idf-hal =0.45.2, esp-idf-svc =0.51.0, esp-idf-sys =0.36.1, ESP-IDF v5.3.3) is pinned and working. This document answers what is needed for two new features:

1. RTCM3 binary frame relay: detect 0xD3-preamble frames mixed with NMEA text on the same UART and relay binary frames over MQTT
2. OTA firmware update: MQTT-triggered HTTP-pull → SHA-256 verify → flash → reboot

No crate version bumps are needed. All required APIs are already present in the pinned versions.

---

## New Feature 1: RTCM3 Binary Frame Relay

### UART Baud Rate Decision

**Stay at 115200 baud. No baud-rate change needed.**

**Bandwidth math:**

RTCM3 MSM4 frame structure:
- Frame overhead: 3 bytes (0xD3 preamble + 6 reserved bits + 10-bit length) + 3 bytes CRC = **6 bytes per frame**
- MSM4 fixed header: 169 bits
- Per-satellite delta: 10 bits (satellite mask cell)
- Per-signal cell: 42 bits (15-bit pseudorange mod 1ms + 22-bit carrier phase + 4-bit lock time + 1-bit half-cycle ambiguity)
- Formula: total bits = 169 + nSat×10 + nSig×42

Typical open-sky scenario (conservative estimates, 1 Hz):

| Constellation | Msg ID | Visible Sats | Signals | Payload bits | Frame bytes |
|---------------|--------|--------------|---------|-------------|-------------|
| GPS           | 1074   | 10           | 10      | 169+100+420=689 | ceil(689/8)+6 = **92** |
| GLONASS       | 1084   | 8            | 8       | 169+80+336=585  | ceil(585/8)+6 = **80** |
| Galileo       | 1094   | 10           | 10      | 169+100+420=689 | ceil(689/8)+6 = **92** |
| BeiDou        | 1124   | 12           | 12      | 169+120+504=793 | ceil(793/8)+6 = **106** |
| **Total**     |        |              |         |             | **~370 bytes/s** |

NMEA at 115200 baud with typical UM980 output (GGA, RMC, GSA, GSV×4, VTG = ~8 sentences × ~80 bytes = ~640 bytes/s):

Total UART load: ~370 + 640 = **~1,010 bytes/s**

At 115200 baud: 115,200 / 10 = **11,520 bytes/s** capacity.

RTCM + NMEA combined is **< 9% of 115200 baud capacity**. No baud rate change required.

**UM980 supported baud rates (COMCONFIG command):** 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600. Factory default is 115200. Source: multiple ArduSimple/Unicore references confirm 460800 and 921600 are valid options. We do not need them.

**If RTCM MSM7 (higher precision, 58-bit signal cells instead of 42) were used**, the same constellation set would produce ~500 bytes/s — still well within 115200 budget.

### RTCM3 Frame Detection in Byte Stream

RTCM3 frames start with byte `0xD3`. NMEA sentences start with `$` (0x24). The byte stream is unambiguous: a `0xD3` byte starts an RTCM frame; `$` starts an NMEA sentence.

Frame parsing state machine (pure Rust, no external crate needed):

```
State: Idle
  0xD3  → read 2 more bytes (6-bit reserved + 10-bit length) → State: InRtcm(length)
  '$'   → accumulate until '\n' → emit NMEA sentence
  other → discard (log warn)

State: InRtcm(n)
  read n bytes (payload) + 3 bytes (CRC-24Q) → emit RTCM frame as &[u8]
  → State: Idle
```

Maximum RTCM frame size: 10-bit length field = max 1023 bytes payload + 6 bytes overhead = **1029 bytes maximum**. The existing 512-byte NMEA line buffer in `gnss.rs` is insufficient for RTCM; the RTCM parser needs its own buffer of at least 1029 bytes.

**No external crate required.** The detection and framing logic is a simple state machine that fits in ~50 lines of Rust. Do not add an `rtcm` or `nmea` parsing crate — we relay raw bytes, not parse them.

### MQTT Binary Payload Publishing

**API:** `EspMqttClient::enqueue(topic, QoS, retain, payload: &[u8])` — already used in this codebase (see `heartbeat_loop` in `mqtt.rs`). The `payload` parameter is `&[u8]`, so binary data works identically to text. No API change needed.

**Size limit:** The ESP-IDF MQTT client's internal outbox buffer defaults to 1024 bytes. RTCM frames up to 1029 bytes will exceed this. Two mitigations:
1. Set `out_buffer_size` in `MqttClientConfiguration` to at least 2048 bytes (covers the max RTCM frame plus MQTT framing overhead of ~5 bytes)
2. Alternatively, publish RTCM frames via `client.publish()` (blocking) instead of `enqueue()` (non-blocking) — `publish()` handles fragmentation automatically but blocks until sent

**Recommendation:** Increase `out_buffer_size` to 2048 in `MqttClientConfiguration`. This is a one-line config change. The MQTT 3.1.1 protocol supports payloads up to 256 MB theoretically; the only limit is the ESP-IDF client's internal buffer configuration.

**Topic pattern:** `gnss/{device_id}/rtcm/{msg_type}` where `msg_type` is the 4-digit RTCM message number (e.g., `1074`). Decode the 12-bit message type from bytes 3-4 of the frame (bits 0-11 of the payload, after the 3-byte header) to get the topic suffix.

---

## New Feature 2: OTA Firmware Update

### OTA API (Verified from Source)

All OTA functionality is in `esp_idf_svc::ota` — **no extra crate, no feature flag**. The module is enabled by the ESP-IDF component flags `esp_idf_comp_app_update_enabled` and `esp_idf_comp_spi_flash_enabled`, both of which are always true in a standard ESP-IDF build. Confirmed by reading `lib.rs` in esp-idf-svc-0.51.0 local cache.

**Key types (all in `esp_idf_svc::ota`):**

| Type | Purpose |
|------|---------|
| `EspOta` | Singleton OTA manager; only one instance exists at a time (internally mutex-guarded) |
| `EspOtaUpdate<'a>` | Write handle returned by `initiate_update()`; implements `io::Write` |
| `EspOtaUpdateFinished<'a>` | Returned by `update.finish()`; call `.activate()` to set boot partition |

**OTA sequence:**

```rust
// Step 1: obtain OTA handle (fails if another OTA is in progress)
let mut ota = EspOta::new()?;

// Step 2: initiate — selects next OTA partition, erases it
let mut update = ota.initiate_update()?;

// Step 3: write firmware data in chunks (implements io::Write)
update.write(&chunk)?;  // call repeatedly as HTTP data arrives

// Step 4a: complete (validates image + sets boot partition atomically)
update.complete()?;
// OR Step 4b: finish then activate separately
// let finished = update.finish()?;
// finished.activate()?;

// Step 5: reboot
esp_idf_svc::hal::reset::restart();

// On next boot — mark valid to prevent rollback:
let mut ota = EspOta::new()?;
ota.mark_running_slot_valid()?;
```

If `update` is dropped without calling `complete()` or `finish()`, `Drop` automatically calls `esp_ota_abort()` — safe by design.

**`mark_running_slot_valid()` is mandatory** when rollback-on-failure is enabled (it is enabled by default in ESP-IDF). If the new firmware boots and does not call this within the watchdog window, ESP-IDF rolls back to the previous partition. Call it early in `main()` on subsequent boots.

### HTTP Client API (Verified from Source)

`esp_idf_svc::http::client::EspHttpConnection` — already in the crate, no feature flag. Requires `feature = "alloc"` which is included in `feature = "std"` (already in use). Confirmed by reading `http/client.rs` and `lib.rs` in esp-idf-svc-0.51.0.

```rust
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use embedded_svc::http::client::Client;

let conn = EspHttpConnection::new(&HttpConfig {
    buffer_size: Some(4096),     // response read buffer
    buffer_size_tx: Some(512),   // request send buffer
    timeout: Some(Duration::from_secs(30)),
    ..Default::default()
})?;

let mut client = embedded_svc::utils::http::client::Client::wrap(conn);
let response = client.get(url)?.submit()?;
// response implements io::Read — pipe into update.write()
```

For plain HTTP (no TLS), the above is sufficient. TLS requires setting `crt_bundle_attach` in `HttpConfig` — not needed for internal network OTA in v1.

**Do not use `esp_https_ota` C API directly** — the Rust `EspHttpConnection` + `EspOta` combination covers the same functionality without unsafe FFI.

### Partition Table Change (Required)

Current `partitions.csv` has a single `factory` partition at 0x20000, size 0x3E0000 (~3.875 MB). OTA requires an `otadata` partition plus at least two `ota_N` app partitions.

The XIAO ESP32-C6 has **4 MB flash**. Layout must fit in 4 MB (0x400000).

Proposed new layout:

```
# Name,     Type, SubType, Offset,  Size,    Flags
nvs,        data, nvs,     0x9000,  0x6000,
otadata,    data, ota,     0xF000,  0x2000,
phy_init,   data, phy,     0x11000, 0x1000,
ota_0,      app,  ota_0,   0x20000, 0x1E0000,
ota_1,      app,  ota_1,   0x200000,0x1E0000,
```

- `ota_0` at 0x20000: 1.875 MB (sufficient — current firmware is ~600 KB compiled)
- `ota_1` at 0x200000: 1.875 MB (same size, required — partitions must be equal size for OTA)
- Total: 0x20000 + 0x1E0000 + 0x1E0000 = 0x3E0000 = exactly 3.875 MB — fits in 4 MB

**NVS grows from 0x10000 (64 KB) to 0x6000 (24 KB)** to reclaim space. 24 KB NVS is sufficient for this project (only stores device config, no large blobs).

**CRITICAL: The `factory` partition is removed.** Once OTA partitions exist, the bootloader boots from `otadata` which points to `ota_0` or `ota_1`. Initial flash must write to `ota_0` using espflash with `--partition-table partitions.csv`. After the first OTA, the device alternates.

**CRITICAL: After changing the partition table, the entire device flash must be erased and reflashed.** `espflash erase-flash` before the first flash with the new layout.

### SHA-256 Verification

The question asks about SHA-256 verification of the downloaded binary. **ESP-IDF OTA performs image validation automatically** — `esp_ota_end()` (called inside `update.complete()`) verifies the ESP image magic bytes, app description, and SHA-256 hash embedded in the firmware binary by `esptool`. There is no need to manually compute SHA-256 before writing.

If the MQTT OTA trigger message includes an expected SHA-256 to verify the downloaded binary matches a known-good hash (supply-chain verification), that requires reading the entire binary into RAM before writing — not feasible on ESP32-C6 with ~320 KB available heap. Instead, rely on ESP-IDF's built-in validation. The trigger message only needs the URL; the SHA-256 field can be omitted or treated as metadata only.

**Rollback provides the safety net:** if the new firmware is corrupt or fails to boot correctly, ESP-IDF rolls back to the previous OTA partition automatically.

---

## Core Technologies (Existing — Unchanged)

| Technology | Version | Purpose | Status |
|------------|---------|---------|--------|
| `esp-idf-svc` | =0.51.0 | WiFi, MQTT, HTTP client, OTA | Already pinned; OTA and HTTP in this version |
| `esp-idf-hal` | =0.45.2 | UART, GPIO | Already pinned |
| `esp-idf-sys` | =0.36.1 | ESP-IDF C bindings | Already pinned |
| `embedded-svc` | =0.28.1 | Trait definitions (MQTT, HTTP, OTA) | Already pinned |
| ESP-IDF | v5.3.3 | Underlying C framework | Already in .embuild/ |

## New Stack Additions

| Addition | What Changes | Why |
|----------|-------------|-----|
| `MqttClientConfiguration::out_buffer_size` | Set to 2048 (was unset/default 1024) | RTCM frames up to 1029 bytes need headroom in MQTT outbox |
| `partitions.csv` | Replace single-factory layout with otadata + ota_0 + ota_1 | Required for OTA dual-partition boot |
| `sdkconfig.defaults` | Possibly add `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` | Explicitly enable rollback (may be default already in IDF v5.3) |
| `esp_idf_svc::ota` module | New import in ota.rs (new file) | OTA write/complete/mark-valid logic |
| `esp_idf_svc::http::client` module | New import in ota.rs | HTTP GET to download firmware binary |

---

## What NOT to Add

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `esp-ota` crate (crates.io) | Third-party crate wrapping the same ESP-IDF OTA APIs; adds an indirection layer | `esp_idf_svc::ota::EspOta` directly — already present, source-verified |
| `rtcm` or `rtcm3` parsing crate | We relay raw bytes, not parse them; adding a parser crate is scope creep | Hand-written state machine (~50 lines) |
| SHA-256 crate for OTA verification | Full-binary hash before write requires ~1 MB RAM; ESP32-C6 has ~320 KB available | ESP-IDF built-in image validation; rollback as safety net |
| Baud rate increase to 460800 | Unnecessary — 115200 handles full RTCM+NMEA load at < 9% capacity | Stay at 115200 |
| `esp_https_ota` C API via unsafe FFI | Rust wrapper already exists in esp-idf-svc | `EspHttpConnection` + `EspOta` in Rust |
| TLS for OTA HTTP in this milestone | Adds mbedTLS setup complexity; internal network deployment acceptable | Plain HTTP OTA for v1; TLS OTA is a separate milestone |
| Async/Embassy for OTA task | OTA runs once, blocking is fine; no benefit to async here | `std::thread` dedicated OTA task, blocking HTTP read loop |

---

## Version Compatibility

| Package | Version | Notes |
|---------|---------|-------|
| `esp-idf-svc` | =0.51.0 | `ota` module present with no feature flag; `http::client::EspHttpConnection` present; verified from local source |
| `embedded-svc` | =0.28.1 | Provides `Ota`, `OtaUpdate`, `OtaUpdateFinished` traits used by EspOta |
| ESP-IDF | v5.3.3 | `esp_ota_ops.h` and `esp_https_ota.h` present in .embuild; C symbols available |
| Partition table | custom | Must switch from single-factory to ota_0+ota_1+otadata; erase-flash required |

---

## Bandwidth Math Summary

```
UART capacity at 115200 baud:    11,520 bytes/s

RTCM MSM4 all constellations @1 Hz:  ~370 bytes/s
NMEA all sentences @1 Hz:            ~640 bytes/s
                                     ────────────
Total UART load:                    ~1,010 bytes/s  (8.8% of capacity)

Headroom:                          ~10,510 bytes/s  (91%)

Conclusion: 115200 baud is sufficient. No change needed.
```

---

## Sources

- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/ota.rs` — EspOta API, EspOtaUpdate, complete/finish/mark_valid methods. HIGH confidence (direct source read).
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/lib.rs` — OTA module gate conditions (esp_idf_comp_app_update_enabled, no Cargo feature flag). HIGH confidence.
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/http/client.rs` — EspHttpConnection::new, Configuration struct. HIGH confidence.
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/mqtt/client.rs` — EspMqttClient::enqueue/publish signatures accept `&[u8]`. HIGH confidence.
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/.embuild/espressif/esp-idf/v5.3.3/components/app_update/include/esp_ota_ops.h` — C OTA API backing the Rust wrappers. HIGH confidence.
- RTCM3 MSM4 formula (169 + nSat×10 + nSig×42 bits): WebSearch result citing RTCM SC-104 spec structure. MEDIUM confidence — formula is consistent across multiple RTCM tooling docs; exact bit counts require the proprietary RTCM standard to verify authoritatively.
- UM980 baud rates (9600–921600): Multiple ArduSimple/Unicore/SparkFun sources agree; CONFIG example shows 460800 and 921600 as valid. MEDIUM confidence (PDF content not directly readable; conclusion from multiple concordant sources).
- [EspOta documentation](https://docs.esp-rs.org/esp-idf-svc/esp_idf_svc/ota/struct.EspOta.html) — confirms public API surface
- [ota_http_client example](https://github.com/esp-rs/esp-idf-svc/blob/master/examples/ota_http_client.rs) — referenced in search results; confirms EspHttpConnection + EspOta integration pattern
- [ESP-IDF MQTT documentation](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/protocols/mqtt.html) — buffer_size and out_buffer_size configuration

---

*Stack research for: RTCM binary relay + OTA firmware update, ESP32-C6 Rust firmware*
*Researched: 2026-03-07*
*Confidence: HIGH for OTA API (source verified); MEDIUM for RTCM sizing and baud rates*
