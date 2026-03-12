# gnss-ota: nostd Implementation Blockers

**Crate:** gnss-ota
**Date assessed:** 2026-03-12
**Target:** ESP32-C6 (riscv32imac-esp-espidf or bare-metal riscv32imac-unknown-none-elf)

## Background

The `OtaSlot` and `OtaManager` traits define the interface for dual-slot OTA firmware updates.
A complete nostd implementation for ESP32-C6 is blocked by the issues described below.

## Blocker 1: esp-storage lacks partition table API

**Issue:** Locating the `ota_0` and `ota_1` application partitions requires parsing the ESP32
partition table (stored in flash at offset 0x8000). The `esp-storage` crate (the primary
nostd flash abstraction for esp-hal) does not provide an API to read or parse the partition
table.

**Tracking:** https://github.com/esp-rs/esp-hal/issues/3259
**Status as of 2026-03-12:** Open. No scheduled milestone.

**Impact:** Without partition table parsing, the OTA partition start address and size cannot
be discovered at runtime. A workaround is to hardcode flash offsets (e.g., `ota_0` at
`0x10000`, `ota_1` at `0x1B0000`), but this is fragile and breaks if the partition layout
changes.

## Blocker 2: esp-hal-ota is untested on ESP32-C6 and uses ESP-IDF internal structures

**Crate:** https://github.com/filipton/esp-hal-ota
**Version assessed:** latest as of 2026-03-12

**Issue 1 — C6 untested:** The crate's README explicitly lists ESP32, ESP32-C3, and ESP32-S3
as tested targets. ESP32-C6 is not listed. The crate uses pointer arithmetic to locate
`EspOtaSelectEntry` structs in the OTA data partition — these structs are part of the ESP-IDF
bootloader binary format and their offsets may differ between chip variants.

**Issue 2 — "Pointer magic" from ESP-IDF internals:** `esp-hal-ota` reads bootloader-internal
structures (`EspOtaSelectEntry`) that are not part of any stable public API. The structure
layout is derived from ESP-IDF source (`esp_ota_ops.h`) and could change across IDF versions.
This creates a maintenance burden and a potential for silent data corruption on layout mismatch.

**Issue 3 — No embedded-storage abstraction:** `esp-hal-ota` takes direct ownership of the
flash peripheral rather than accepting an `embedded-storage::Storage` impl. This prevents
composing OTA with other flash users (e.g., sequential-storage NVS) without unsafe sharing.

**Impact:** Cannot reliably use `esp-hal-ota` as a backing implementation for `OtaSlot` on
ESP32-C6 today.

## Recommended Path Forward

1. Monitor `esp-rs/esp-hal#3259` for partition table API addition to `esp-storage`.
2. Once partition table API is available, implement `OtaManager` using `embedded-storage`
   and the partition table to locate `ota_0`/`ota_1` addresses.
3. Validate `esp-hal-ota` on ESP32-C6 hardware, or contribute an `embedded-storage`-based
   alternative that does not rely on ESP-IDF internal struct layouts.
4. Until then, the ESP-IDF implementation (via `esp-idf-svc::ota::EspOta`) remains the only
   working OTA path for the firmware — see `firmware/src/main.rs` for current usage.
