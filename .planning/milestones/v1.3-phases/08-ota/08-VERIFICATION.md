---
phase: 08-ota
verified: 2026-03-07T00:00:00Z
status: human_needed
score: 10/10 automated must-haves verified
human_verification:
  - test: "OTA trigger delivery and heartbeat continuity (OTA-02, OTA-05)"
    expected: "Publishing {\"url\":\"...\",\"sha256\":\"...\"} to gnss/{device_id}/ota/trigger causes device to begin download; heartbeat continues publishing at ~30s intervals on gnss/{device_id}/heartbeat throughout the download"
    why_human: "Requires live HTTP server hosting a firmware binary and an active MQTT broker with mosquitto_sub observing both topics simultaneously"
  - test: "SHA-256 mismatch rejection (OTA-03 failure path)"
    expected: "Publishing a trigger with an incorrect sha256 value causes the device to publish {\"state\":\"failed\",\"reason\":\"sha256 mismatch: expected X got Y\"} to ota/status; device does NOT reboot"
    why_human: "Requires live MQTT broker and device; cannot verify SHA-256 hash comparison at runtime without actual firmware download"
  - test: "Full OTA success path — download, flash, reboot, mark_valid (OTA-03, OTA-04)"
    expected: "After a trigger with correct URL and sha256: downloading progress messages appear on ota/status, then complete message, then device reboots into new firmware, then Running slot marked valid appears in serial log on the new boot, then no infinite reboot loop (retained trigger was cleared)"
    why_human: "Requires HTTP server serving a known firmware binary, live device with serial monitor, and MQTT subscriber — multiple physical resources required simultaneously"
  - test: "Rollback on missing mark_valid (OTA-04 rollback path)"
    expected: "A firmware deliberately missing the mark_running_slot_valid() call boots once then rolls back to the previous slot on the next reboot"
    why_human: "Requires building a modified firmware, flashing via OTA, and observing bootloader slot selection across two reboots — hardware-only"
---

# Phase 8: OTA Firmware Update Verification Report

**Phase Goal:** An operator can remotely update firmware by publishing a URL to an MQTT topic; the device downloads, flashes, and reboots into new firmware with automatic rollback if the new firmware fails to confirm itself.
**Verified:** 2026-03-07
**Status:** human_needed — all automated checks passed; 4 hardware tests deferred
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | partitions.csv defines otadata + ota_0 + ota_1 with correct offsets for 4MB flash; no factory row | VERIFIED | File contains exact required rows: otadata@0xF000/0x2000, ota_0@0x20000/0x1E0000, ota_1@0x200000/0x1E0000 |
| 2  | sdkconfig.defaults enables CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y and extends watchdog to 30s | VERIFIED | Both keys present at lines 35 and 39 of sdkconfig.defaults |
| 3  | Cargo.toml includes sha2 dependency for streaming SHA-256 verification | VERIFIED | `sha2 = { version = "0.10", default-features = false, features = ["oid"] }` at line 11 |
| 4  | espflash.toml ensures custom OTA partition layout is always flashed | VERIFIED | espflash.toml contains `[idf_format_args]` + `partition_table = "partitions.csv"` |
| 5  | ota.rs spawn_ota() creates 16384-byte thread and returns Ok(()) immediately | VERIFIED | `stack_size(16384)` at line 340; `.map(|_| ())` returns immediately |
| 6  | ota_task() parses url and sha256 from trigger JSON; rejects missing fields with failed status | VERIFIED | extract_json_str() used for both fields; each None path publishes `failed/missing url or sha256` and continues |
| 7  | SHA-256 is verified before update.complete() is called; mismatch aborts without restart | VERIFIED | Lines 263-282: hasher.finalize() compared to sha256 field; mismatch publishes failed and continues; complete() only reached after match |
| 8  | Status messages published to gnss/{device_id}/ota/status for downloading/complete/failed transitions | VERIFIED | publish_status() called for all transitions: downloading@0, downloading@progress, complete, and multiple failed paths |
| 9  | Retained trigger cleared (empty payload, retain=true) before restart() on success | VERIFIED | Lines 301-309: enqueue(&trigger_topic, QoS::AtLeastOnce, true, b"") before sleep + restart() |
| 10 | main.rs calls mark_running_slot_valid() non-fatally after mqtt_connect, before spawning threads; wires ota channel | VERIFIED | Lines 119-127: scoped EspOta block with warn-on-error; lines 136/141/187: channel + pump wiring + spawn_ota |
| 11 | mqtt.rs pump routes /ota/trigger to ota_tx; subscriber_loop subscribes to both /config and /ota/trigger | VERIFIED | Lines 109-113: `t.ends_with("/ota/trigger")` branch in Received handler; lines 147/156: ota_topic subscription in subscriber_loop |
| 12 | OTA thread runs independently of MQTT pump (heartbeat can continue during download) | VERIFIED | spawn_ota() spawns separate thread; pump_mqtt_events is a separate thread; no shared blocking between them |

**Score:** 12/12 automated truths verified

---

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `partitions.csv` | OTA dual-slot partition layout | VERIFIED | 4 rows: nvs, otadata, ota_0, ota_1; correct offsets and sizes; no factory row |
| `sdkconfig.defaults` | Rollback enable + watchdog extension | VERIFIED | CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y and CONFIG_ESP_TASK_WDT_TIMEOUT_S=30 present |
| `Cargo.toml` | sha2 dependency | VERIFIED | sha2 = "0.10" with default-features = false |
| `espflash.toml` | Partition table reference for espflash | VERIFIED | [idf_format_args] section with partition_table = "partitions.csv" |
| `src/ota.rs` | Complete OTA task: HTTP download, SHA-256 verify, EspOta flash write, status publish, restart | VERIFIED | 344 lines; exports spawn_ota() and ota_task(); all 13 steps implemented |
| `src/main.rs` | OTA channel creation, mark_valid call, spawn_ota call | VERIFIED | mod ota declared; scoped mark_valid block; ota_tx/ota_rx channel; ota_tx passed to pump; spawn_ota() called |
| `src/mqtt.rs` | ota_tx routing in pump; /ota/trigger subscription in subscriber_loop | VERIFIED | pump_mqtt_events(ota_tx param) with /ota/trigger routing; subscriber_loop subscribes to both topics |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| src/ota.rs (ota_task) | esp_idf_svc::ota::EspOta | EspOta::new() + initiate_update() + write() + complete() | WIRED | Lines 176-295: full EspOta lifecycle present; abort-on-drop used for error paths |
| src/ota.rs (ota_task) | esp_idf_svc::http::client::EspHttpConnection | HttpClient::wrap(EspHttpConnection::new(&http_conf)).get(&url).submit() | WIRED | Lines 119-162: full HTTP request/response chain with status check |
| src/ota.rs (publish_status) | mqtt_client Arc<Mutex<EspMqttClient>> | client.lock().enqueue(status_topic, AtMostOnce, false, json) | WIRED | Lines 26-34: publish_status() mirrors heartbeat_loop pattern; called at all state transitions |
| src/main.rs (after mqtt_connect) | esp_idf_svc::ota::EspOta::mark_running_slot_valid | scoped EspOta block dropped before thread spawning | WIRED | Lines 119-127: match EspOta::new() { Ok + mark_running_slot_valid }; Err paths log::warn and continue |
| src/mqtt.rs pump_mqtt_events | ota_tx.send(data.to_vec()) | topic.ends_with("/ota/trigger") branch in Received handler | WIRED | Lines 109-113: confirmed routing branch present |
| src/main.rs | ota::spawn_ota | spawn_ota(mqtt_client.clone(), device_id.clone(), ota_rx) | WIRED | Line 187: exact call with correct arguments; ota_tx passed to pump at line 141 |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| OTA-01 | 08-01 | Partition table redesigned to otadata + ota_0 + ota_1 (~1.875MB each) for 4MB flash | SATISFIED | partitions.csv contains correct layout; espflash.toml ensures it is always flashed; hardware boot from ota_0 confirmed by user checkpoint |
| OTA-02 | 08-02, 08-03 | Device subscribes to gnss/{device_id}/ota/trigger (QoS 1); payload triggers update | SATISFIED (code) / DEFERRED (runtime) | subscriber_loop subscribes to ota_topic at QoS::AtLeastOnce; pump routes /ota/trigger to ota_tx; runtime delivery requires hardware test |
| OTA-03 | 08-02 | Device HTTP-pulls firmware binary, verifies SHA256 during streaming download, writes to inactive OTA partition | SATISFIED (code) / DEFERRED (runtime) | EspHttpConnection streaming loop + sha2 hasher + update.write() all present; SHA-256 compared before complete(); runtime requires hardware test |
| OTA-04 | 08-03 | Device reboots into new partition; calls mark_running_slot_valid() after WiFi+MQTT confirmed; rolls back if not called | SATISFIED (code) / PARTIALLY VERIFIED (hardware Test 1 passed) | Non-fatal mark_valid in scoped block after mqtt_connect confirmed working on hardware; rollback path not hardware-tested |
| OTA-05 | 08-02, 08-03 | OTA download runs in dedicated task; MQTT pump and keep-alive remain active during download | SATISFIED (code) | spawn_ota() is a separate thread from pump; heartbeat_loop is a third separate thread; channel decoupling verified in code |
| OTA-06 | 08-02 | Device reports status to gnss/{device_id}/ota/status — downloading/complete/failed | SATISFIED (code) / DEFERRED (runtime) | publish_status() called at all state transitions; runtime delivery requires hardware test |

All 6 OTA requirements are accounted for. No orphaned requirements.

---

## Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| None | — | — | No TODOs, FIXMEs, placeholders, or stub implementations found in any Phase 8 file |

No anti-patterns detected. All error paths publish failed status and continue rather than panic or silently drop.

---

## Notable Implementation Details

**mark_running_slot_valid() made non-fatal (per user note):** The original plan used `.expect()`. Hardware testing revealed that factory boots have no otadata state, causing EspOta::new() or mark_running_slot_valid() to return Err. The implementation correctly uses match arms that log::warn and continue rather than panic. This is correct behavior — the device remains operational on factory partition builds and during development.

**espflash.toml added (deviation from plan, correct fix):** The PLAN.md for 08-01 did not include espflash.toml, but hardware testing revealed that without it, cargo espflash flash uses the default partition layout (no OTA slots). espflash.toml with [idf_format_args] section is the correct fix. Cargo.toml also has [package.metadata.espflash] as a belt-and-suspenders reference.

**Cargo.toml [package.metadata.espflash]:** Present at line 21-23, pointing to partitions.csv. This is additive and does not conflict with espflash.toml.

---

## Human Verification Required

### 1. OTA Trigger Delivery and Heartbeat Continuity (OTA-02, OTA-05)

**Test:** Flash current firmware. In a second terminal, subscribe: `mosquitto_sub -v -t 'gnss/+/ota/status' -t 'gnss/+/heartbeat'`. Publish a valid trigger: `mosquitto_pub -t 'gnss/{device_id}/ota/trigger' -r -m '{"url":"http://{server}/firmware.bin","sha256":"{correct_hex}"}'`
**Expected:** Heartbeat messages continue at ~30s intervals while `{"state":"downloading","progress":N}` messages appear on ota/status. After completion, `{"state":"complete"}` appears and device reboots.
**Why human:** Requires live HTTP server hosting firmware binary, live MQTT broker, and physical device — cannot automate.

### 2. SHA-256 Mismatch Rejection (OTA-03 failure path)

**Test:** Publish trigger with wrong sha256 (e.g., change last character): `mosquitto_pub -t 'gnss/{device_id}/ota/trigger' -m '{"url":"http://{server}/firmware.bin","sha256":"wrong"}'`
**Expected:** `{"state":"failed","reason":"sha256 mismatch: expected wrong got {actual}"}` published to ota/status. Device does NOT reboot. OTA thread accepts next trigger normally.
**Why human:** Runtime SHA-256 comparison requires actual download completion — cannot verify without hardware + server.

### 3. Full OTA Success Path (OTA-03, OTA-04 confirm path)

**Test:** Publish trigger with correct URL and correct sha256. Monitor serial log.
**Expected:** downloading progress messages, then complete message, then reboot. On new boot: "Running slot marked valid" appears in serial log after MQTT connects. No reboot loop (retained trigger cleared).
**Why human:** Full end-to-end requires HTTP server + live device + serial monitor simultaneously.

### 4. Rollback Path (OTA-04 rollback)

**Test (optional):** Build firmware with mark_running_slot_valid() call removed. Flash via OTA. Observe across two boots.
**Expected:** New firmware boots once. On next reboot, bootloader detects slot not confirmed and returns to previous slot.
**Why human:** Requires modified firmware build and two-reboot observation — hardware-only, no automated path.

---

## Gaps Summary

No automated gaps. All code artifacts are present, substantive, and fully wired. The four items above require physical hardware, a running HTTP server, and live MQTT broker observation — they cannot be verified programmatically.

Hardware Test 1 (OTA-04 mark_valid on normal boot) was confirmed by the operator during the Plan 03 checkpoint. Tests 2-4 were deferred as manual-only per the SUMMARY.

---

_Verified: 2026-03-07_
_Verifier: Claude (gsd-verifier)_
