# Project Research Summary

**Project:** esp32-gnssmqtt — v1.2 milestone (RTCM binary relay + OTA firmware update)
**Domain:** Embedded Rust firmware — ESP32-C6 GNSS-to-MQTT bridge
**Researched:** 2026-03-07
**Confidence:** HIGH

## Executive Summary

This milestone adds two new capabilities to the existing v1.1 firmware: relaying binary RTCM3 correction frames from the UM980 GNSS receiver over MQTT, and enabling remote OTA firmware updates triggered via MQTT. Both capabilities are additive to the working v1.1 codebase and require no crate version bumps — all necessary APIs (`esp_idf_svc::ota`, `esp_idf_svc::http::client`) are already present in the pinned stack (esp-idf-svc =0.51.0, esp-idf-hal =0.45.2, esp-idf-sys =0.36.1, ESP-IDF v5.3.3), verified by direct source inspection of the local cargo cache. The RTCM relay is purely a UART parsing and MQTT publishing concern. OTA requires a structural change to the flash partition table that must be performed via USB reflash and cannot be bootstrapped over the air on the first deployment.

The recommended implementation order is RTCM relay first, OTA second. RTCM relay requires no partition table changes and its central change — refactoring the `gnss.rs` RX thread from a line-based parser to a dual-mode state machine — is also the prerequisite for the MQTT topic discrimination fix that OTA depends on. Completing RTCM relay first validates the state machine, the binary MQTT publish path, and the pump routing before the higher-risk OTA partition work begins. The two phases have no circular dependencies and can be shipped independently.

The primary risks are structural and well-understood. On the OTA side: the existing partition table allocates the entire 4MB flash to a single factory partition, leaving zero room for OTA slots; rollback requires an explicit `mark_running_slot_valid()` call that must not be omitted; and the OTA download task must run independently of the MQTT pump thread. On the RTCM side: the existing 512-byte NMEA line buffer is insufficient for MSM7 frames (up to 1029 bytes) and the current line-based parser will corrupt RTCM binary data and lose NMEA sync when RTCM output is enabled on the UM980. Both sets of risks have concrete mitigations documented in research and are straightforward to address with the correct implementation sequence.

## Key Findings

### Recommended Stack

The existing pinned stack requires no version changes for this milestone. Two configuration changes are required: `MqttClientConfiguration::out_buffer_size` raised to 2048 bytes (RTCM frames up to 1029 bytes exceed the default 1024-byte MQTT outbox buffer), and `partitions.csv` redesigned to replace the single `factory` partition with `otadata + ota_0 + ota_1`. One Cargo.toml change is likely needed: add `features = ["ota"]` to the `esp-idf-svc` entry (the feature gating is referenced in documentation; STACK.md found evidence it may be unconditional from lib.rs — verify by reading the local Cargo.toml before implementing). Two sdkconfig.defaults additions are required: `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` and `CONFIG_ESP_HTTPS_OTA_ALLOW_HTTP=y`.

**Core technologies (existing — unchanged):**
- `esp-idf-svc =0.51.0` — OTA, HTTP client, MQTT; all needed APIs verified in local source
- `esp-idf-hal =0.45.2` — UART driver (unchanged); no new HAL features needed
- `EspMqttClient::enqueue(topic, QoS, retain, &[u8])` — already used; binary payloads identical to text

**New additions (configuration, not crates):**
- Hand-written CRC-24Q state machine (~50 lines, polynomial 0x864CFB) — relays raw RTCM frames; no parsing crate
- `partitions.csv` redesign — otadata (0x2000) + ota_0 (0x1E0000) + ota_1 (0x1E0000); verified to fit 4MB flash
- `MqttClientConfiguration::out_buffer_size = 2048` — one-line change in MQTT init

**What not to add:**
- No `rtcm` or `rtcm3` parsing crate — relay is opaque frame forwarding, not RTCM field decoding
- No SHA-256 crate for pre-write download verification — full binary (~1MB) cannot fit in ESP32-C6 heap (~320KB available); rely on ESP-IDF built-in image validation (`esp_ota_end()` calls `esp_image_verify()`) and rollback as safety net
- No baud rate change — RTCM MSM4 + NMEA combined is under 9% of 115200 baud capacity (~1,010 bytes/s of 11,520 byte/s capacity); no headroom problem exists

### Expected Features

**Must have (table stakes):**
- RTCM3 frame detection (0xD3 preamble), 10-bit length read, CRC-24Q verification — without CRC, relay forwards corrupt data to RTK engines
- MQTT publish raw RTCM bytes to `gnss/{device_id}/rtcm/{message_type}` at QoS 0, retain=false
- Partition table rework (otadata + ota_0 + ota_1) — hard prerequisite for OTA; requires USB reflash before first OTA-enabled build
- OTA trigger via MQTT (`gnss/{device_id}/ota/trigger`) — payload: JSON with firmware URL
- HTTP firmware download in chunks, streamed to `EspOta::initiate_update()` → `update.write()`
- `mark_running_slot_valid()` called after UART init succeeds — mandatory for rollback to engage correctly
- `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` in sdkconfig.defaults

**Should have (v1.2.x, add after core validation):**
- OTA progress reporting to `gnss/{device_id}/ota/status`
- Clear retained OTA trigger after successful update (publish empty retained message) to prevent re-trigger on reconnect
- 1005/1006 base position published with retain=true so new subscribers get base position immediately

**Defer (v2+):**
- TLS for OTA HTTP download (HTTPS) — requires certificate bundle setup; HTTP over internal network is acceptable for v1
- Anti-rollback via eFuse security counter — requires security milestone with key management design
- OTA firmware signature verification (secure boot) — requires eFuse provisioning at manufacture time; irrecoverable brick risk if misimplemented

**Anti-features (explicitly excluded):**
- RTCM base64 encoding — 33% overhead, mandatory decode step on every consumer, no benefit; MQTT is a binary protocol
- RTCM reassembly or NTRIP server in firmware — ESP32-C6 RAM (512KB) is insufficient for TCP connection management; relay to MQTT and use rtkbase/SNIP as NTRIP caster
- OTA firmware push over MQTT — MQTT cannot reliably transport 1MB binary; HTTP pull is the standard ESP-IDF pattern

### Architecture Approach

The architecture extends the existing multi-thread, typed-channel pattern. The `gnss.rs` RX thread becomes a dual-mode `RxState` enum state machine that dispatches NMEA frames to the existing `SyncSender<(String,String)>` (bound 64, unchanged) and RTCM frames to a new `SyncSender<(u16,Vec<u8>)>` (bound 32; carries message type + raw frame bytes). A new `rtcm_relay.rs` module mirrors `nmea_relay.rs` exactly in structure. A new `ota.rs` module runs an independent listener thread, blocking on `Receiver<String>` for URL triggers from the MQTT pump and performing chunked HTTP download + EspOta write without touching the MQTT event loop. The MQTT pump receives a fix to discriminate topics before routing to `config_tx` or `ota_tx`.

**Major components:**
1. `gnss.rs` (MODIFIED) — `RxState` machine replaces flat line assembler; `spawn_gnss` returns triple `(cmd_tx, nmea_rx, rtcm_rx)`; RTCM buffer sized at 1100 bytes (covers max 1029-byte frame)
2. `rtcm_relay.rs` (NEW) — consumes `Receiver<(u16,Vec<u8>)>`; publishes binary MQTT payload to `gnss/{id}/rtcm/{msg_type}`; stack 8KB
3. `ota.rs` (NEW) — independent listener thread; `EspHttpConnection` GET → chunk write to `EspOta` → `complete()` → `esp_restart()`; stack 16KB (HTTP client requires more stack than relay threads)
4. `mqtt.rs` (MODIFIED) — pump adds topic discrimination in `Received` arm; `subscriber_loop` adds OTA trigger subscription; current bug (all Received events routed to config_tx regardless of topic) is fixed here
5. `main.rs` (MODIFIED) — destructures triple from `spawn_gnss`; wires `rtcm_rx` and `ota_tx/rx` channels; spawns `rtcm_relay` and `ota_listener`
6. `partitions.csv` (MODIFIED) — factory replaced with otadata (0x2000) + ota_0 (0x1E0000) + ota_1 (0x1E0000)
7. `sdkconfig.defaults` (MODIFIED) — adds `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`, `CONFIG_ESP_HTTPS_OTA_ALLOW_HTTP=y`

**Key architectural decisions:**
- Two separate typed channels from `gnss.rs` (not a single mixed-type channel) — `nmea_relay.rs` and `rtcm_relay.rs` are independent consumers; a single channel cannot have two consumers without a dispatcher thread
- CRC-24Q verification happens in `gnss.rs` before `try_send` — gnss.rs is the UART owner and framing authority; only verified frames enter the channel
- OTA task is fully independent of MQTT pump — pump sends URL string to `ota_tx`; OTA thread handles download; MQTT event loop remains unblocked during multi-second HTTP download
- `Vec<u8>` for RTCM frame accumulation in the state machine is acceptable (heap-allocated per frame, up to 1029 bytes); fixed `[u8; 1100]` stack buffer is the preferred alternative to avoid heap fragmentation during concurrent OTA

### Critical Pitfalls

1. **Existing partition table has zero room for OTA** — the `factory` partition at 0x20000 occupies 0x3E0000 bytes (~3.9MB), leaving only 24KB free. OTA requires `otadata` (exactly 0x2000 bytes — mandatory for bootloader OTA state tracking) plus two equal-sized `ota_N` partitions. The table must be redesigned and the device must be erased and reflashed (`espflash erase-flash`) before any OTA code is testable. Warning sign: `espflash` reports partition overlap; `EspOta::initiate_update()` returns error.

2. **RTCM binary data corrupts the existing NMEA parser** — the current `gnss.rs` RX loop treats `\n` (0x0A) as a frame delimiter. RTCM3 binary payloads can contain 0x0A bytes anywhere in the payload, causing the parser to split frames mid-payload, log spurious "non-NMEA line dropped" warnings, and permanently lose byte-stream sync until the next valid NMEA sentence. Warning sign: "non-NMEA line dropped" warnings flood the log after RTCM output is enabled on the UM980; NMEA topics go silent.

3. **`mark_running_slot_valid()` omission causes every reboot to trigger rollback** — if `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` is set and the new firmware never calls `EspOta::mark_running_slot_valid()`, the bootloader sees `ESP_OTA_IMG_PENDING_VERIFY` on every boot and rolls back to the previous partition. Call it after UART init succeeds — before network operations, but not on the first line of `main()`. Warning sign: device boots into new firmware once, then reverts; log shows "Falling back to previous version".

4. **Task watchdog fires during OTA partition erase** — erasing a 1.875MB OTA partition at `esp_ota_begin()` with `OTA_SIZE_UNKNOWN` takes 4-8 seconds (SPI flash erase ~50ms/sector × ~200 sectors). The default watchdog timeout is 5 seconds. Use sequential erase mode (pass `OTA_WITH_SEQUENTIAL_WRITES` equivalent via `EspOta`) to spread erase time across the download. Warning sign: device reboots during OTA at a consistent byte offset; "Task watchdog got triggered" in log.

5. **OTA download inside the MQTT pump blocks the event loop** — the pump must call `connection.next()` continuously. A 20-second HTTP download inside the pump causes MQTT keep-alive timeout and broker disconnect, aborting the OTA attempt. The pump must send the URL to `ota_tx` and return immediately; the `ota.rs` thread handles the download independently. Warning sign: MQTT heartbeat stops publishing during OTA; broker logs a client disconnect.

## Implications for Roadmap

The milestone decomposes into two phases with a hard dependency ordering. Both phases are self-contained and can be shipped independently — Phase A (RTCM relay) delivers immediate operational value for RTK base station use cases without waiting for OTA.

### Phase A: RTCM Relay

**Rationale:** Zero partition risk; validates the `gnss.rs` state machine change that is also a prerequisite for OTA routing; fixes the MQTT pump topic-discrimination bug (which OTA also requires) in a lower-stakes context. RTCM relay is the lower-risk half of the milestone and should reach hardware-verified status before the partition table is touched.

**Delivers:** UM980 RTCM3 frames appear on `gnss/{device_id}/rtcm/NNNN` MQTT topics as raw binary at QoS 0; downstream RTK clients (rtkbase, RTKLIB, SNIP) can consume corrections directly; NMEA relay continues to function unchanged alongside RTCM relay.

**Addresses:** RTCM frame detection + CRC-24Q verification, MQTT binary publish, MQTT topic discrimination fix, `out_buffer_size=2048` config change.

**Avoids:**
- Pitfall 2 (RTCM binary corrupts NMEA parser) — resolved by `RxState` machine; 0xD3 and `$` as unambiguous first-byte discriminators
- Pitfall 6 (buffer too small for MSM7) — RTCM frame buffer sized at 1100 bytes from the start; not tunable after implementation

**Sub-tasks:**
- A1: `RxState` enum in `gnss.rs`; `spawn_gnss` returns triple `(cmd_tx, nmea_rx, rtcm_rx)`; 1100-byte RTCM accumulation buffer
- A2: `rtcm_relay.rs` (mirrors `nmea_relay.rs`); wire into `main.rs`; set `out_buffer_size=2048` in MQTT config
- A3: MQTT pump topic discrimination fix — route `/config` to `config_tx`, `/ota/trigger` to `ota_tx`, unrouted topics to warn+drop
- Hardware verify: RTCM frames visible on broker under `gnss/{id}/rtcm/NNNN`; CRC pass rate >99% over 100 frames; NMEA topics continue uninterrupted

**Research flag:** No additional research needed. State machine design is fully specified in ARCHITECTURE.md. CRC-24Q algorithm and polynomial are confirmed from RTKLIB source. All decisions are made.

### Phase B: OTA Firmware Update

**Rationale:** Depends on Phase A completing the pump topic discrimination fix. Requires a USB reflash to change the partition table — this is the harder reset point of the milestone and should come after RTCM relay is validated. OTA's rollback safety net depends on the partition table being correct from the very first OTA-enabled build.

**Delivers:** Operator publishes a firmware URL to `gnss/{device_id}/ota/trigger`; device downloads, validates, flashes to the inactive OTA slot, and reboots into new firmware; failed boots (firmware does not mark itself valid) auto-rollback to the previous slot; OTA progress and completion status published to `gnss/{device_id}/ota/status`.

**Addresses:** Partition table rework, OTA MQTT trigger, HTTP firmware download in chunks, rollback on first-boot failure, `mark_running_slot_valid()`, watchdog-safe sequential erase.

**Avoids:**
- Pitfall 1 (no OTA partition space) — redesign `partitions.csv` as first act of Phase B; `espflash erase-flash` before building OTA code
- Pitfall 2 (interrupted download leaves corrupt partition) — `EspOta`'s `Drop` calls `esp_ota_abort()` automatically; never call `complete()` before the full download succeeds
- Pitfall 3 (boot loop without mark-valid) — `mark_running_slot_valid()` called after UART init, before network operations; added in initial implementation, never as a follow-up
- Pitfall 4 (watchdog during erase) — sequential erase mode from the start
- Pitfall 5 (OTA inside MQTT pump) — independent `ota.rs` thread; pump sends URL string via channel

**Sub-tasks:**
- B1: `partitions.csv` redesign; `sdkconfig.defaults` additions; `espflash erase-flash` + reflash; verify layout with `espflash partition-table`
- B2: `ota.rs` module; Cargo.toml `ota` feature (verify feature name from local `esp-idf-svc-0.51.0/Cargo.toml`); `EspHttpConnection` + `EspOta` imports; 16KB thread stack
- B3: Wire `ota_listener` into `main.rs`; add `ota_tx/rx` channel; add `mark_running_slot_valid()` call in startup sequence
- B4: Post-OTA: publish empty retained message to `/ota/trigger` to clear trigger; publish status to `/ota/status`
- Hardware verify: trigger OTA from MQTT; new firmware boots and marks valid; force reboot before mark-valid and confirm rollback to previous slot

**Research flag:** Verify exact Cargo feature name for OTA in `esp-idf-svc-0.51.0/Cargo.toml` before B2. STACK.md (from lib.rs inspection) found no feature gate; ARCHITECTURE.md (from docs) suggests `features = ["ota"]`. This takes 2 minutes to check and resolves a build-time ambiguity.

### Phase Ordering Rationale

- **RTCM before OTA:** RTCM requires no partition changes; OTA requires a full device erase. Validating RTCM while the current partition table is intact eliminates one variable from debugging and delivers value sooner.
- **State machine (A1) before relay (A2):** `gnss.rs` change is a shared dependency; the RTCM channel must exist before `rtcm_relay.rs` can consume it.
- **Topic discrimination (A3) in Phase A:** The existing pump bug (all `Received` events routed to `config_tx` regardless of topic) must be fixed before adding OTA subscription — otherwise OTA trigger payloads would be forwarded to the UM980 as configuration commands. Fixing it in Phase A makes Phase B simpler and removes a source of hard-to-diagnose bugs.
- **Partition table (B1) before OTA code (B2-B4):** The new firmware must be built and flashed against the OTA-compatible layout before OTA logic is exercised. Testing `EspOta` against a factory-only partition produces misleading failures that do not reflect the final deployment.

### Research Flags

Phases needing deeper research during planning:
- **Phase B, task B2:** Verify `esp-idf-svc` OTA feature gating by reading `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/Cargo.toml`. Takes 2 minutes; resolves whether `features = ["ota"]` is needed or the module is unconditionally compiled. Do this before writing the Cargo.toml change.

Phases with standard patterns (skip research-phase):
- **Phase A (RTCM relay):** State machine design, CRC-24Q algorithm, channel types, MQTT binary publish — all fully specified in research files with no ambiguity.
- **Phase B OTA sequence:** `EspOta` call sequence (initiate → write → complete → restart → mark_valid), partition layout math, and sequential erase rationale — all verified from local ESP-IDF v5.3.3 source and confirmed by multiple independent sources.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | OTA and HTTP APIs verified by direct source read of `esp-idf-svc-0.51.0` local files; MQTT binary payload API confirmed from existing codebase usage; one minor gap (OTA feature name in Cargo.toml, easily resolved) |
| Features | HIGH | RTCM3 framing confirmed from RTKLIB source (canonical reference implementation); OTA flow verified against ESP-IDF v5.3.3 C source; MQTT topic convention is MEDIUM (no single authoritative standard, but observed practice from multiple open-source projects is consistent) |
| Architecture | HIGH | All component boundaries and data flows are concrete; partition math verified; state machine design confirmed against RTCM spec; `RxState` enum structure is explicitly specified including byte-level transitions |
| Pitfalls | HIGH | OTA/partition pitfalls verified against ESP-IDF v5.3.3 `esp_ota_ops.c` and `esp_ota_ops.h` in `.embuild/`; RTCM pitfalls confirmed from RTKLIB and RTCM spec; UM980 baud rate sequencing is MEDIUM (command syntax confirmed from multiple manufacturer sources; PDF not machine-readable) |

**Overall confidence:** HIGH

### Gaps to Address

- **OTA Cargo feature name:** STACK.md (from lib.rs inspection) found OTA is gated only by ESP-IDF component flags, not a Cargo feature flag. ARCHITECTURE.md (from docs) references `features = ["ota"]`. Read `esp-idf-svc-0.51.0/Cargo.toml` from local cargo cache before Phase B implementation to confirm. If OTA is unconditional, no Cargo.toml change is needed for the module itself.

- **OTA trigger QoS:** FEATURES.md recommends QoS 1 for the OTA trigger subscription (OTA is a one-shot action where dropped messages matter). The existing codebase uses QoS 0 for NMEA and heartbeat. Confirm that `EspMqttClient` subscription supports QoS 1 and that the `subscriber_loop` can specify per-topic QoS during Phase B planning.

- **RTCM MSM7 bandwidth in practice:** Bandwidth analysis used MSM4 estimates (~370 bytes/s). MSM7 is ~30-50% larger per frame, yielding ~500-550 bytes/s total with NMEA — still under 6% of 115200 baud capacity. Buffer sizing (1100 bytes) and MQTT `out_buffer_size` (2048) are sized for the maximum 1029-byte frame regardless of MSM level. No action needed, but verify that the UM980 RTCM configuration enabled on device FFFEB5 matches the MSM level assumed in testing.

## Sources

### Primary (HIGH confidence)

- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/ota.rs` — `EspOta`, `EspOtaUpdate`, `complete()`, `finish()`, `mark_running_slot_valid()` methods
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/lib.rs` — OTA module gate conditions (`esp_idf_comp_app_update_enabled`)
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/http/client.rs` — `EspHttpConnection::new`, `Configuration` struct
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/mqtt/client.rs` — `enqueue`/`publish` accept `&[u8]`
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/.embuild/espressif/esp-idf/v5.3.3/components/app_update/esp_ota_ops.c` — OTA write atomicity, sequential erase behavior, abort handle lifecycle
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/.embuild/espressif/esp-idf/v5.3.3/components/app_update/include/esp_ota_ops.h` — `esp_ota_mark_app_valid_cancel_rollback()`, `esp_ota_abort()`, rollback state machine
- RTKLIB `src/rtcm.c` — RTCM3 preamble 0xD3, CRC-24Q polynomial 0x864CFB, resync strategy after CRC failure
- Espressif partition table documentation — `otadata` 0x2000 requirement, alignment constraints, dual-slot OTA mechanics

### Secondary (MEDIUM confidence)

- SNIP RTCM cheat sheet — message type matrix (1005, 1074-1127 MSM4/MSM7, 1230)
- ArduSimple/Unicore UM980 hookup guide and commands manual — RTCM output message list, baud rate options
- RVMT/RVMP project (hagre/GitHub) — binary RTCM over MQTT topic convention (per-type topics, raw bytes)
- pyrtcm library — CRC failure resync strategy (scan byte-by-byte for next 0xD3 or `$`)
- ESP-IDF OTA practical walkthrough (quan.hoabinh.vn, 2024) — `EspHttpConnection` + `EspOta` Rust integration pattern
- esp-ota crate (faern/esp-ota) — transport-agnostic OTA Rust crate; confirms binary must be ESP app image format (first byte 0xE9)

### Tertiary (LOW confidence / needs validation before use)

- UM980 baud rate command syntax (`COM COM1 <rate>`) — confirmed from multiple web sources; PDF manual not machine-readable in this session; verify against Unicore Reference Commands Manual before implementing baud rate change (deferred to future milestone, not part of v1.2)

---
*Research completed: 2026-03-07*
*Ready for roadmap: yes*
