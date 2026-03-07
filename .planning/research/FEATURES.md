# Feature Research

**Domain:** Embedded GNSS-to-MQTT bridge firmware — RTCM3 binary relay and OTA firmware update (milestone 2)
**Researched:** 2026-03-07
**Confidence:** HIGH for RTCM3 framing (spec is public and stable); HIGH for OTA partition mechanics (official ESP-IDF docs); MEDIUM for MQTT topic convention for binary RTCM (no single authoritative standard exists — observed practice from open-source projects).

---

## Existing Features (Already Shipped — v1.1)

These are included for completeness. Do not re-implement.

- NMEA sentence relay: `gnss/{device_id}/nmea/{TYPE}` at QoS 0
- Remote config: `gnss/{device_id}/config` → UART TX to UM980 with djb2 dedup
- WiFi + MQTT connectivity with reconnect, LWT, heartbeat
- Status LED, device ID from eFuse MAC

---

## Feature Landscape — New Features for This Milestone

### Table Stakes (Users Expect These)

Features that must exist for the milestone to deliver value. Missing any of these means the stated goal is not achieved.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| RTCM3 frame detection on UART RX (0xD3 preamble) | Without this, UM980 RTCM output is indistinguishable from noise; no relay is possible | MEDIUM | Mixed NMEA+RTCM stream on same UART; must detect preamble byte 0xD3, not consume it as NMEA. Current gnss.rs discards non-NMEA bytes — needs extension. |
| RTCM3 length read (10-bit field in header bytes 1-2) | Frame boundary cannot be known without reading the length field | LOW | Header is 3 bytes total: byte 0 = 0xD3, byte 1 bits[5:0] = length MSBs (6 bits), byte 2 = length LSBs (8 bits). Max payload = 1023 bytes. |
| RTCM3 CRC-24Q verification before relay | Corrupt frames must not be forwarded; NTRIP consumers and RTK rovers will malfunction on bad data | MEDIUM | CRC-24Q (Qualcomm) covers header + payload (6 bytes total overhead). Three CRC bytes follow payload. Algorithm: polynomial 0x1864CFB, widely implemented. |
| RTCM message type extraction | Required for per-type topic routing and for logging/debugging | LOW | Message type is a 12-bit field at bits [0:11] of the payload body (first 2 bytes of payload, big-endian, MSB first). E.g. 1074 decimal. |
| MQTT publish RTCM frames to typed topics | Core value of this milestone: RTCM data reaches the broker | LOW | Once frame is validated, publish raw bytes. Topic: `gnss/{device_id}/rtcm/{MESSAGE_TYPE}` at QoS 0, retain=false. |
| Partition table rework for OTA | Current partitions.csv has only a single `factory` app partition — OTA is structurally impossible without ota_0 + ota_1 slots + otadata | HIGH | This is a precondition for any OTA feature. Requires reflash of the partition table. ESP32-C6 flash is 4MB; each OTA app slot needs ~1.5MB minimum (current factory partition is 3.9MB — must shrink). |
| OTA trigger via MQTT (`gnss/{device_id}/ota`) | Without a trigger mechanism, OTA cannot be initiated remotely | LOW | Subscribe to OTA command topic. Payload contains firmware URL (HTTP/HTTPS). On receipt, spawn OTA task. |
| Firmware download over HTTP | Firmware binary is fetched from a URL, not pushed over MQTT | MEDIUM | Use esp-idf HTTP client to download in chunks; write each chunk to the inactive OTA partition via `esp-ota` crate or `esp_idf_svc::ota::EspOta`. |
| SHA256 verification of downloaded firmware | Without integrity check, a corrupted download bricks the device | MEDIUM | ESP-IDF bootloader verifies SHA256 of the app image stored in partition headers automatically when secure boot is not used. Caller should also verify SHA256 against a known-good hash provided in the MQTT OTA trigger payload. |
| Rollback on first-boot failure | If new firmware cannot reach a "healthy" state, device must revert to the previous version | HIGH | Requires `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` in sdkconfig. App must call `esp_ota::mark_app_valid()` after WiFi+MQTT connect succeeds. If not called before reboot, bootloader rolls back automatically. |

### Differentiators (Competitive Advantage)

Features that go beyond baseline and add real operational value for RTK use cases.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| MSM4 message set selection (1074/1084/1094/1124) | Sufficient for most RTK rovers; lower bandwidth than MSM7; universally supported | LOW | Configure via existing MQTT config relay: `rtcm1074 1`, `rtcm1084 1`, `rtcm1094 1`, `rtcm1124 1`. No firmware change needed — just UM980 config. |
| MSM7 message set option (1077/1087/1097/1127) | Higher resolution pseudorange + carrier phase + Doppler; better ambiguity resolution for precision applications; required by some calibration workflows | LOW | Same config relay path: `rtcm1077 1`, etc. UM980 supports both MSM4 and MSM7 simultaneously — operator chooses per use case. |
| 1005/1006 base position at slow rate (30s) | NTRIP clients require 1005 or 1006 at the start of the stream to locate the base; slow rate (30s) conserves bandwidth | LOW | UM980 config: `rtcm1005 30`. This is data the firmware relays, not generates — no firmware logic change. |
| 1230 GLONASS code-phase biases | Required by some RTK solvers (RTKLIB, u-blox) to resolve GLONASS phase ambiguity; without it GLONASS MSM data is harder to use | LOW | UM980 config: not always supported; verify with UM980 command reference. Include if available. |
| OTA status reporting to MQTT | Allows remote monitoring of OTA progress and failure reasons | LOW | Publish to `gnss/{device_id}/ota/status` with payload `{"state": "downloading", "progress_pct": 42}`. Use existing MQTT publish path. |
| Anti-rollback version tracking | Prevents downgrade attacks (flashing older vulnerable firmware) | LOW | ESP-IDF supports anti-rollback via eFuse security counter. Requires bumping `CONFIG_BOOTLOADER_APP_ANTI_ROLLBACK=y` and setting app security version in Cargo.toml/sdkconfig. Defer to security milestone unless needed. |
| Dual-rate RTCM: position messages at 1s, 1005 at 30s | Position data at 1Hz keeps rovers updated; base position at 30s avoids redundant data | LOW | Pure UM980 configuration, not a firmware feature. Document as recommended operator config. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| RTCM payload as base64 in MQTT | JSON/text tool compatibility; some brokers handle text better | Adds 33% size overhead to already-binary data; introduces encode/decode step; MQTT supports binary payloads natively; all RTK software (RTKLIB, SNIP, rtkbase) expects raw bytes; base64 requires consumer-side decode before use | Publish raw bytes directly. MQTT is a binary protocol. Use raw payload. Any consumer that cannot handle binary payloads is the wrong consumer. |
| RTCM reassembly / NTRIP server in firmware | "Close the loop" — device acts as its own NTRIP caster | ESP32-C6 has 512KB RAM; an NTRIP server requires TCP connection management, HTTP state machine, and concurrent client handling — disproportionate complexity for a relay device; NTRIP is HTTP-based and needs TLS for external access | Relay raw RTCM to MQTT broker. A separate NTRIP-from-MQTT bridge (e.g. rtkbase, SNIP, or a small Python script) handles NTRIP serving. Separation of concerns. |
| OTA firmware push over MQTT | MQTT is already connected; push firmware binary as MQTT payload | MQTT is not designed for large binary transfers; 1MB firmware cannot fit in a single MQTT message; fragmentation logic, reassembly, and partial-transfer recovery add high complexity; HTTP pull is standard and well-supported by ESP-IDF | HTTP pull: device fetches firmware from URL provided in MQTT trigger message. Reliable, chunked, standard. |
| Firmware signature verification in firmware | "Extra security" | Secure boot + code signing requires eFuse key programming during manufacture — cannot be added after deployment without re-provisioning the device; adds irrecoverable brick risk if key management fails | SHA256 hash verification against a publisher-provided hash is sufficient for this use case. Reserve secure boot for a security-dedicated milestone with proper key management design. |
| Parsing RTCM messages in firmware (decode fields) | "Know what's inside" for logging | RTCM field decoding requires implementing the full RTCM3 spec (complex bit-packed structures, scale factors, constellation-specific encodings); relay firmware must not be an RTCM decoder; adds hundreds of lines of code | Relay opaque frames. RTKLIB or pyrtcm on the consumer side decodes fields. Firmware validates CRC and publishes. |
| Storing RTCM frames in NVS | "Don't lose corrections during WiFi drop" | RTCM corrections are time-stamped and ephemeral; a correction more than a few seconds old is useless or harmful for RTK; NVS write endurance (10K-100K cycles) is incompatible with 1Hz frame writes | Accept loss during WiFi drop. RTK rovers handle correction gaps gracefully (hold last fix, degrade to float). |

---

## Feature Dependencies

```
[Partition table rework (ota_0, ota_1, otadata)]
    └──required by──> [OTA trigger via MQTT]
                          └──required by──> [Firmware download over HTTP]
                                                └──required by──> [SHA256 verification]
                                                └──required by──> [Rollback on first-boot failure]

[RTCM3 frame detection (0xD3 preamble)]
    └──required by──> [RTCM3 length read]
                          └──required by──> [RTCM3 CRC-24Q verification]
                                                └──required by──> [RTCM message type extraction]
                                                                      └──required by──> [MQTT publish RTCM to typed topics]

[UM980 config (via existing config relay)]
    └──configures──> [RTCM output messages on UM980 UART]
                          └──feeds──> [RTCM3 frame detection]

[CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y]
    └──required by──> [Rollback on first-boot failure]
    └──requires──> [Partition table rework] (rollback needs two OTA slots to roll back to)

[WiFi + MQTT connected state]
    └──required by──> [OTA trigger via MQTT] (must be subscribed to receive trigger)
    └──required by──> [mark_app_valid() call] (must confirm healthy state before calling)
```

### Dependency Notes

- **Partition rework is a hard prerequisite for OTA:** The current `partitions.csv` allocates the entire 3.9MB flash to a single `factory` partition. OTA requires two `ota_N` app partitions plus an `otadata` partition. This requires a partition table change and a full reflash — it cannot be done over the air on the first OTA-enabled build.
- **RTCM framing is independent of NMEA relay:** RTCM detection in the UART stream is additive — existing NMEA detection logic does not need to change. The two streams are byte-level multiplexed and distinguished by the 0xD3 preamble vs. the `$` preamble.
- **OTA mark-valid timing:** `mark_app_valid()` must be called only after the firmware confirms healthy operation (WiFi connected + MQTT connected). Calling it too early (e.g. on first line of main) defeats rollback protection entirely.
- **RTCM message selection is a UM980 config concern, not a firmware concern:** The firmware relays whatever RTCM frames the UM980 emits. The operator controls message selection via the existing MQTT config relay. Document recommended UM980 RTCM configs separately from firmware requirements.

---

## RTCM3 Framing Specification (Reference)

Source: RTCM 10403 standard (behind paywall) — framing independently confirmed by RTKLIB source (`rtcm.c`, `#define RTCM3PREAMB 0xD3`) and multiple open implementations. Confidence: HIGH.

### Frame Structure

```
Byte 0:     0xD3  (preamble, always)
Byte 1:     [7:6] = reserved, always 0b00
            [5:0] = length bits [9:8] (MSBs of 10-bit length)
Byte 2:     length bits [7:0] (LSBs of 10-bit length)
Bytes 3..(3+length-1): payload
Bytes (3+length)..(3+length+2): CRC-24Q (3 bytes, big-endian)

Total frame size = 3 (header) + length + 3 (CRC) = length + 6 bytes
Maximum payload = 1023 bytes → max frame = 1029 bytes
```

### CRC-24Q Algorithm

- Polynomial: 0x1864CFB
- Initial value: 0x000000
- Covers: all bytes from byte 0 (preamble) through last payload byte
- Does NOT cover the CRC bytes themselves
- Reference implementation in RTKLIB `src/rtcm.c` function `crc24q()`

### Message Type Extraction

- Payload byte 0 bits [7:4] = message type bits [11:8]
- Payload byte 0 bits [3:0] = message type bits [7:4]  (first byte = upper 8 bits of 12-bit type)
- Payload byte 1 bits [7:4] = message type bits [3:0]
- Simplified: `msg_type = (payload[0] << 4) | (payload[1] >> 4)` — 12-bit value, range 1-4095

### Parsing State Machine (Implementation Pattern)

```
State::Idle          → on byte 0xD3 → State::Header1
State::Header1       → read byte, extract length MSBs → State::Header2
State::Header2       → read byte, extract length LSBs, compute total length → State::Payload
State::Payload       → accumulate (length + 3 CRC) bytes → State::Validate
State::Validate      → compute CRC-24Q, compare, publish or discard → State::Idle
```

Important: a 0xD3 byte inside a payload is not a preamble. Only re-enter Idle state after CRC validation (success or fail) — do not scan for 0xD3 inside payloads.

---

## RTCM Message Selection for RTK Base Station + Calibration

Source: UM980 reference commands manual (SparkFun mirror), SNIP RTCM cheat sheet, Tersus GNSS MSM description, onocoy/rtkbase community documentation. Confidence: MEDIUM-HIGH (UM980 message list confirmed from manufacturer docs; MSM semantics confirmed from multiple sources).

### UM980-Supported RTCM Output Messages

The UM980 supports: 1005, 1006, 1033, 1074, 1077, 1084, 1087, 1094, 1097, 1117, 1124, 1127. Note: 1114 (QZSS MSM4) may not be listed in all UM980 firmware versions; verify against installed firmware.

### Message Selection Matrix

| Message | Constellation | MSM Level | Content | RTK Use | Calibration Use | Rate |
|---------|--------------|-----------|---------|---------|-----------------|------|
| 1005 | — | — | Base ARP position (no antenna height) | Required by all NTRIP clients | Required | 30s |
| 1006 | — | — | Base ARP + antenna height | Better than 1005 if antenna height known | Recommended | 30s |
| 1074 | GPS | MSM4 | Pseudorange, carrier phase, CNR | Good; widely supported | Good | 1s |
| 1077 | GPS | MSM7 | Pseudorange + phase (high-res) + Doppler + CNR | Better ambiguity resolution | Best | 1s |
| 1084 | GLONASS | MSM4 | Pseudorange, carrier phase, CNR | Good | Good | 1s |
| 1087 | GLONASS | MSM7 | High-res + Doppler | Better | Best | 1s |
| 1094 | Galileo | MSM4 | Pseudorange, carrier phase, CNR | Good | Good | 1s |
| 1097 | Galileo | MSM7 | High-res + Doppler | Better | Best | 1s |
| 1124 | BeiDou | MSM4 | Pseudorange, carrier phase, CNR | Good | Good | 1s |
| 1127 | BeiDou | MSM7 | High-res + Doppler | Better | Best | 1s |
| 1117 | QZSS | MSM7 | High-res (Asia-Pacific only) | Regional only | Regional only | 1s |
| 1230 | GLONASS | — | Code-phase biases | Needed by RTKLIB for GLONASS fix | Needed | 10s |
| 1033 | — | — | Receiver + antenna descriptor | Optional; useful for CORS networks | Optional | 10s |

### Recommended Minimum Set (RTK Base + Calibration)

This is the minimum that a rover or calibration tool (RTKLIB, rtkbase, u-blox u-center, Emlid Flow) can use for an RTK fix:

```
rtcm1005 30    (or rtcm1006 30 if antenna height measured)
rtcm1074 1     (GPS MSM4 — or 1077 1 for MSM7)
rtcm1084 1     (GLONASS MSM4 — or 1087 1 for MSM7)
rtcm1094 1     (Galileo MSM4 — or 1097 1 for MSM7)
rtcm1124 1     (BeiDou MSM4 — or 1127 1 for MSM7)
rtcm1230 10    (GLONASS biases — required for GLONASS carrier-phase use)
```

### MSM4 vs MSM7 Decision

Use MSM4 when bandwidth is constrained or when connecting legacy rovers. Use MSM7 when operating a precision reference station for post-processing or when connecting modern rovers (ZED-F9P, UM980 in rover mode, Septentrio). MSM7 messages are ~30-50% larger than MSM4 for the same constellation. UM980 outputs both simultaneously if both are configured — this is valid and some setups use MSM4 for low-latency and MSM7 for archival.

---

## MQTT Topic Convention for RTCM Binary Relay

Source: RVMT/RVMP open-source RTCM-via-MQTT project (GitHub), SNIP documentation, observed practice in open-source RTK communities. No single authoritative standard exists. Confidence: MEDIUM.

### Recommendation: Raw Bytes, Per-Type Topics

**Topic pattern:** `gnss/{device_id}/rtcm/{message_type}`

Examples:
- `gnss/fffeb5/rtcm/1005` — base position
- `gnss/fffeb5/rtcm/1074` — GPS MSM4
- `gnss/fffeb5/rtcm/1084` — GLONASS MSM4

**Payload encoding:** Raw bytes (binary MQTT payload). Do NOT base64-encode.

**QoS:** 0 (fire-and-forget). RTCM corrections are time-sensitive; retransmission of stale corrections is harmful. Same rationale as NMEA relay.

**Retain:** false. Stale corrections are useless (position fixes advance every epoch).

**Exception for 1005/1006:** Some implementations publish base position with retain=true so that a consumer connecting mid-stream immediately gets the base location. This is a reasonable differentiator but not table stakes.

### Rationale for Raw Bytes

MQTT is a binary protocol — there is no transport-layer reason to base64. NTRIP clients, RTKLIB, and rtkbase all expect raw RTCM byte streams. A Mosquitto-to-NTRIP bridge or Python subscriber receives the binary payload and can pipe it directly to an NTRIP caster or RTKLIB instance without transformation. Base64 adds 33% overhead, a mandatory decode step on every consumer, and complexity for no gain. The only argument for base64 is JSON envelope compatibility — but wrapping binary corrections in JSON is wrong for this domain.

### "Done" Criteria for RTCM Relay

RTCM relay is operationally complete when:

1. A Mosquitto subscriber receives binary frames on `gnss/{device_id}/rtcm/1074` (and other configured types)
2. Frame size matches `length + 6` bytes from the UM980 output
3. CRC-24Q computed by a consumer tool (e.g. `pyrtcm`, `rtklib`, or `node-NTRIP/rtcm`) passes on every received frame
4. An NTRIP caster (rtkbase, SNIP free tier, or a minimal Python NTRIP server reading from MQTT) accepts the stream and a rover receiver achieves RTK float or fix
5. RTKLIB `str2str` can receive frames from MQTT (via a pipe or MQTT-to-TCP bridge) and output a position solution

Alternatively (simpler validation): pipe raw bytes to `rtk2rtklib` or `pyrtcm` decode and verify message type parsing is correct.

---

## OTA Update Flow (ESP32-C6, Rust, esp-idf-svc)

Source: ESP-IDF OTA official docs (v5.x), esp-ota crate (crates.io, GitHub: faern/esp-ota), esp-idf-svc ota module docs (docs.esp-rs.org). Confidence: HIGH.

### Partition Table Change (Hard Prerequisite)

Current `partitions.csv` (single factory partition) must become:

```
# Name,    Type,  SubType,   Offset,   Size
nvs,       data,  nvs,       0x9000,   0x10000
phy_init,  data,  phy,       0x19000,  0x1000
otadata,   data,  ota,       0x1A000,  0x2000
ota_0,     app,   ota_0,     0x20000,  0x1C0000
ota_1,     app,   ota_1,     0x1E0000, 0x1C0000
```

Note: Each OTA slot is 1.75MB. Current factory build is ~500KB — well within this size. The 4MB flash on ESP32-C6 supports two 1.75MB slots comfortably. This partition table change requires a full USB reflash; it cannot be applied via OTA.

### OTA Update Flow

```
1. MQTT message arrives on gnss/{device_id}/ota
   Payload: {"url": "http://192.168.1.10:8080/firmware.bin", "sha256": "abc123..."}

2. Parse URL and SHA256 from payload

3. Spawn OTA task (do not block MQTT pump thread):
   a. Begin OTA: esp_ota::OtaUpdate::begin()
      → ESP-IDF finds inactive OTA slot (ota_0 or ota_1)
      → Erases target partition

   b. HTTP GET firmware binary in chunks (e.g. 4KB chunks)
      → Write each chunk: ota_update.write(chunk)
      → Track bytes written for progress reporting

   c. Finalize: completed = ota_update.finalize()
      → ESP-IDF writes app descriptor + SHA256 to partition header
      → Validates image integrity (built-in SHA256 check)

   d. Verify SHA256 of written image against provided hash
      → esp_ota_get_app_elf_sha256() or compute over flash directly

   e. Set boot partition: completed.set_as_boot_partition()
      → Updates otadata partition to boot from new slot

   f. Restart: completed.restart()
      → ESP-IDF reboots into new firmware (marked ESP_OTA_IMG_NEW)

4. On first boot of new firmware:
   → Bootloader sees ESP_OTA_IMG_NEW state
   → If CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y: starts watchdog
   → Firmware runs its normal startup (WiFi connect, MQTT connect)
   → After WiFi + MQTT confirmed connected: call esp_ota::mark_app_valid()
   → Image state transitions to ESP_OTA_IMG_VALID → rollback window closed

5. If firmware panics or never calls mark_app_valid():
   → On next reboot, bootloader detects image in ESP_OTA_IMG_NEW state
   → Bootloader rolls back to previous slot
   → Device recovers to last known-good firmware
```

### Key sdkconfig Changes for OTA

```
CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
CONFIG_APP_PROJECT_VER="1.2.0"  (increment per release)
```

### OTA Crate Options

| Option | Crate | Notes |
|--------|-------|-------|
| esp-ota | `esp-ota` (faern/esp-ota) | Transport-agnostic, safe Rust; recommended; well-maintained as of 2024 |
| esp-idf-svc::ota | built into esp-idf-svc | Available but documented as experimental; fewer examples in the wild |

Recommendation: Use `esp-ota` crate. It wraps the ESP-IDF OTA partition API in safe Rust, is transport-agnostic (caller handles HTTP), and the rollback API (`mark_app_valid()` / `rollback_and_reboot()`) is explicit and well-documented.

---

## MVP Definition for This Milestone

### Launch With (v1.2)

Minimum viable product for RTCM relay + OTA milestone.

- [ ] **Partition table rework** — hard prerequisite; ship in first phase of milestone
- [ ] **RTCM3 frame detection + length + CRC-24Q verification** — without CRC, relay is unsafe
- [ ] **MQTT publish raw RTCM bytes to `gnss/{device_id}/rtcm/{type}`** — core relay function
- [ ] **OTA trigger via MQTT** — control plane for updates
- [ ] **HTTP firmware download in chunks** — fetch mechanism
- [ ] **Rollback on first-boot failure** — safety net; do not ship OTA without this
- [ ] **mark_app_valid() after WiFi+MQTT connect** — required for rollback to work correctly

### Add After Validation (v1.2.x)

- [ ] **OTA progress reporting to MQTT** — useful for monitoring; trivial to add
- [ ] **SHA256 hash verification in firmware** — adds second integrity layer; low complexity once OTA flow is working
- [ ] **1005/1006 retain=true** — makes base position immediately available to new subscribers

### Future Consideration (v2+)

- [ ] **Anti-rollback (eFuse security counter)** — requires security milestone with key management design
- [ ] **TLS for OTA download (HTTPS)** — add when HTTP pull is validated; requires certificate handling
- [ ] **OTA signature verification (secure boot)** — requires eFuse provisioning at manufacture time

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| RTCM3 frame detection + CRC | HIGH | MEDIUM | P1 |
| RTCM MQTT publish (raw bytes) | HIGH | LOW | P1 |
| Partition table rework | HIGH (enables OTA) | MEDIUM | P1 |
| OTA MQTT trigger + HTTP pull | HIGH | MEDIUM | P1 |
| Rollback on first-boot failure | HIGH | MEDIUM | P1 |
| mark_app_valid() after connect | HIGH | LOW | P1 |
| OTA progress reporting | MEDIUM | LOW | P2 |
| SHA256 hash cross-check | MEDIUM | LOW | P2 |
| 1005 retain=true | LOW | LOW | P2 |
| Anti-rollback (eFuse) | MEDIUM | HIGH | P3 |
| HTTPS for OTA download | MEDIUM | HIGH | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

---

## Sources

- RTCM3 framing: RTKLIB source `src/rtcm.c` (tomojitakasu/RTKLIB on GitHub) — preamble 0xD3, CRC-24Q polynomial — HIGH confidence
- RTCM message types: [SNIP RTCM cheat sheet](https://www.use-snip.com/kb/knowledge-base/an-rtcm-message-cheat-sheet/), [kernelsat.com](https://kernelsat.com/kb/kb_rtcm3.php), [Tersus GNSS MSM explainer](https://www.tersus-gnss.com/tech_blog/new-additions-in-rtcm3-and-What-is-msm) — MEDIUM-HIGH
- UM980 RTCM message list: [SparkFun UM980 hookup guide](https://docs.sparkfun.com/SparkFun_UM980_Triband_GNSS_RTK_Breakout/single_page/) and [Unicore commands manual](https://globalgpssystems.com/wp-content/uploads/2021/08/Unicore-Reference-Commands-Manual-For-High-Precision-Products_V2_EN_R3.2.pdf) — HIGH
- MQTT binary RTCM relay: [RVMT/RVMP project](https://github.com/hagre/RVMT_RTCM-VIA-MQTT-TRANSMITTER), [RVMP protocol spec](https://github.com/hagre/RVMP_RTCM-VIA-MQTT-PROTOCOL) — MEDIUM (no single authoritative standard)
- ESP-IDF OTA: [Official docs v5.x](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/ota.html) — HIGH
- esp-ota crate: [GitHub faern/esp-ota](https://github.com/faern/esp-ota), [crates.io](https://crates.io/crates/esp-ota) — HIGH
- Rust ESP32 OTA example: [quan.hoabinh.vn post (2024)](https://quan.hoabinh.vn/post/2024/3/programming-esp32-with-rust-ota-firmware-update), [esp-idf-ota-http-template](https://github.com/rust-esp32/esp-idf-ota-http-template) — MEDIUM

---

*Feature research for: esp32-gnssmqtt — RTCM3 relay and OTA firmware update milestone*
*Researched: 2026-03-07*
