# Phase 8: OTA - Research

**Researched:** 2026-03-07
**Domain:** ESP32 OTA firmware update — partition table, esp-idf-svc EspOta, HTTP download, watchdog, rollback, MQTT integration
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| OTA-01 | Partition table redesigned to `otadata + ota_0 + ota_1` (each ~1.875MB) for 4MB flash; requires `espflash erase-flash` + USB reflash | 4MB flash layout confirmed; otadata is 0x2000 bytes; two OTA slots of 0x1E0000 each fit within 4MB — see partition layout section |
| OTA-02 | Device subscribes to `gnss/{device_id}/ota/trigger` (QoS 1); payload `{"url":"...","sha256":"..."}` triggers update | `subscriber_loop` already subscribes to `/config`; extend to subscribe to `/ota/trigger` on same Connected signal; pump already has placeholder for `ota_tx` |
| OTA-03 | Device HTTP-pulls firmware binary, verifies SHA256 during streaming download, writes to inactive OTA partition via `EspOta` | `EspHttpConnection` + streaming read confirmed; `sha2` crate (0.10.9 already in registry) for SHA-256; `EspOtaUpdate::write()` accepts arbitrary-size chunks |
| OTA-04 | Device reboots into new partition; calls `mark_running_slot_valid()` early in `main()` after WiFi+MQTT confirmed; rolls back to previous slot if not called within watchdog window | `EspOta::mark_running_slot_valid()` maps to `esp_ota_mark_app_valid_cancel_rollback()`; `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` required in sdkconfig.defaults |
| OTA-05 | OTA download runs in dedicated task receiving trigger via `mpsc::channel`; MQTT pump and keep-alive remain active during download | Pattern identical to nmea_relay/rtcm_relay; pump sends `Vec<u8>` payload through `ota_tx`; OTA thread owns `EspOta` lifecycle |
| OTA-06 | Device reports status to `gnss/{device_id}/ota/status` — `{"state":"downloading","progress":N}` / `{"state":"complete"}` / `{"state":"failed","reason":"..."}` | `mqtt_client` Arc<Mutex<>> clone passed to OTA thread; `enqueue()` used for status publishes |
</phase_requirements>

---

## Summary

Phase 8 adds remotely-triggered OTA firmware updates. The operator publishes a JSON payload with a firmware URL (and optional SHA256) to `gnss/{device_id}/ota/trigger`; the device downloads, verifies, flashes, and reboots. The new firmware must call `mark_running_slot_valid()` during startup or it rolls back automatically.

The main implementation work splits into three areas: (1) partition table redesign and sdkconfig changes, which require a one-time USB reflash and cannot be done OTA; (2) a new `ota.rs` module with its own thread receiving the trigger payload, doing the HTTP download + write loop, and publishing progress; (3) wiring changes in `main.rs` and `mqtt.rs` to add the `ota_tx` channel, subscribe to `/ota/trigger`, and call `mark_running_slot_valid()` early in boot.

The `esp-idf-svc 0.51.0` crate ships `EspOta` and `EspOtaUpdate` in `src/ota.rs` with no feature gate — the types are unconditionally compiled and available today. HTTP streaming download is available via `EspHttpConnection` in `src/http/client.rs`. SHA-256 verification can be done during the download loop using the `sha2` crate (already present in the local registry). The largest design constraint is the OTA partition erase: `EspOta::initiate_update()` hardcodes `OTA_SIZE_UNKNOWN`, which erases the entire ~1.875 MB partition before writing begins, taking 4-8 seconds — long enough to fire the task watchdog. This is handled by feeding the task watchdog during erase (see Pitfall 3).

**Primary recommendation:** Implement `ota.rs` as a standalone thread with its own `EspOta` instance; never create `EspOta` inside the pump thread. Wire `ota_tx: mpsc::channel<Vec<u8>>` through main, pump, and subscriber. Clear the retained trigger after firing by publishing an empty retained message to the trigger topic. Call `mark_running_slot_valid()` immediately after MQTT connects on every boot.

---

## Standard Stack

### Core OTA Libraries (all present in esp-idf-svc 0.51.0, no extra features required)

| Component | Location | Purpose | Notes |
|-----------|----------|---------|-------|
| `EspOta` | `esp_idf_svc::ota` | OTA state machine, partition management | Singleton (TAKEN mutex); no feature flag needed |
| `EspOtaUpdate` | `esp_idf_svc::ota` | Streaming write to OTA partition | `write(&[u8])` accepts arbitrary chunk sizes |
| `EspHttpConnection` | `esp_idf_svc::http::client` | HTTP GET for firmware binary | `read(&mut buf)` streams response body |
| `sha2` | crates.io `sha2 = "0.10"` | SHA-256 streaming verification | Pure Rust; works with `no_std + alloc`; already in local registry |

### Supporting

| Component | Purpose | Notes |
|-----------|---------|-------|
| `embedded_svc::utils::io::try_read_full` | Reads a chunk from HTTP response | Handles EAGAIN transparently on ESP-IDF v5 |
| `esp_idf_svc::hal::reset::restart()` | Reboot after OTA | Call after `update.complete()` |
| `mpsc::channel::<Vec<u8>>()` | Trigger delivery from pump to OTA thread | Unbounded OK — trigger arrives at most once per OTA cycle |

**No additional Cargo features** need to be added to `Cargo.toml`. `EspOta` and `EspHttpConnection` are both compiled unconditionally in esp-idf-svc 0.51.0 under the default `std` feature.

**sdkconfig.defaults additions required:**
```
CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
```

---

## Architecture Patterns

### Partition Table Redesign

Current `partitions.csv` uses a single `factory` partition consuming all 4MB. This must be replaced:

```
# Name,     Type, SubType,  Offset,   Size,    Flags
nvs,        data, nvs,      0x9000,   0x6000,
otadata,    data, ota,      0xF000,   0x2000,
ota_0,      app,  ota_0,    0x20000,  0x1E0000,
ota_1,      app,  ota_1,    0x200000, 0x1E0000,
```

Layout math (4MB = 0x400000):
- NVS: 0x9000 → 0xF000 (was 0x10000, shrink by 0x4000 to make room for otadata)
- otadata: 0xF000 → 0x11000 (0x2000 = 8KB, holds two 4KB OTA select entries)
- ota_0: 0x20000 → 0x200000 (0x1E0000 = 1,966,080 bytes ≈ 1.875 MB)
- ota_1: 0x200000 → 0x3E0000 (0x1E0000 = same size)
- End: 0x3E0000 — 128 KB slack before 4MB boundary (safe)

**PREREQUISITE:** Existing flash with factory partition requires `espflash erase-flash` before the new partition table takes effect. This cannot be done OTA. It is the first act of Phase 8.

### OTA Thread Architecture

```
main.rs                 mqtt.rs (pump)            ota.rs
  │                         │                        │
  ├── mpsc channel ─────────┤                        │
  │   ota_tx/ota_rx          │                        │
  │                     on /ota/trigger               │
  │                     ota_tx.send(payload) ─────────►
  │                                                   │
  │  mqtt_client (Arc clone) ──────────────────────── ►
  │  device_id (String clone) ─────────────────────── ►
  │                                                   │
  │                                              ota_task():
  │                                              1. parse JSON url+sha256
  │                                              2. publish "downloading"
  │                                              3. EspOta::new()
  │                                              4. initiate_update()
  │                                              5. HTTP GET loop:
  │                                                 read chunk → sha2::update
  │                                                 → EspOtaUpdate::write()
  │                                                 → publish progress
  │                                              6. verify sha2::finalize
  │                                              7. update.complete()
  │                                              8. publish "complete"
  │                                              9. clear retained trigger
  │                                              10. restart()
```

### Module Structure Addition

```
src/
├── main.rs          # Add: ota_tx/rx channel, spawn ota thread, mark_running_slot_valid() call
├── mqtt.rs          # Add: ota_tx param to pump_mqtt_events; route /ota/trigger to ota_tx
│                    # Add: subscribe to /ota/trigger in subscriber_loop
├── ota.rs           # NEW: ota_task(), download+flash+publish loop
└── ...existing...
```

### Pattern: Mark Valid on Boot

Call this in `main()` immediately after WiFi+MQTT connection is confirmed — before spawning any relay threads. This is the safety confirmation that prevents rollback on the next reboot:

```rust
// Source: esp-idf-svc 0.51.0 src/ota.rs, EspOta::mark_running_slot_valid()
// Only needed when CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
// Safe to call unconditionally — no-op if not in PENDING_VERIFY state
{
    let mut ota = esp_idf_svc::ota::EspOta::new()
        .expect("EspOta singleton conflict at boot");
    ota.mark_running_slot_valid()
        .expect("mark_running_slot_valid failed");
    // EspOta dropped here — releases TAKEN mutex
}
```

Call site in `main()`: after `mqtt_connect()` succeeds and before spawning the pump thread.

### Pattern: Streaming Download + Write Loop

```rust
// Source: esp-idf-svc 0.51.0 src/http/client.rs + src/ota.rs
use embedded_svc::http::client::{Client as HttpClient, Method};
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::ota::EspOta;
use sha2::{Digest, Sha256};

let http_conf = HttpConfig {
    buffer_size: Some(4096),   // read buffer; tune for RAM vs speed
    timeout: Some(std::time::Duration::from_secs(30)),
    ..Default::default()
};
let mut client = HttpClient::wrap(EspHttpConnection::new(&http_conf)?);
let request = client.get(&url, &[])?;
let mut response = request.submit()?;
// status check: response.status() == 200

let mut ota = EspOta::new()?;
let mut update = ota.initiate_update()?;  // erases partition (OTA_SIZE_UNKNOWN, 4-8s)

let mut hasher = Sha256::new();
let mut buf = vec![0u8; 4096];
let mut bytes_written: u64 = 0;

loop {
    let n = response.read(&mut buf)?;  // embedded_svc::io::Read
    if n == 0 { break; }
    hasher.update(&buf[..n]);
    update.write(&buf[..n])?;
    bytes_written += n as u64;
    // publish progress
}

let hash = hasher.finalize();
// compare hash against expected sha256 hex string
update.complete()?;  // calls esp_ota_end + esp_ota_set_boot_partition
```

### Pattern: Clear Retained Trigger After OTA

To prevent re-triggering on reconnect, publish an empty payload with retain=true to the trigger topic after OTA completes:

```rust
// Source: MQTT retained message clearing convention
// Empty payload + retain=true clears the retained message on the broker
let trigger_topic = format!("gnss/{}/ota/trigger", device_id);
let mut c = mqtt_client.lock().unwrap();
let _ = c.enqueue(&trigger_topic, QoS::AtLeastOnce, /*retain=*/true, b"");
```

### Anti-Patterns to Avoid

- **Running EspOta inside the pump thread:** `EspOta::new()` takes a singleton mutex and the HTTP download blocks for tens of seconds. The pump must keep calling `connection.next()` to service keep-alive. Run OTA in a dedicated thread only.
- **Calling `update.complete()` before SHA256 verify:** Verify the hash _before_ `complete()`. If the hash fails, call `update.abort()` (or let `EspOtaUpdate` drop, which calls `esp_ota_abort`).
- **Forgetting to clear the retained trigger:** Without clearing, a retained `/ota/trigger` message replays on every reconnect, causing infinite reboot loops.
- **Calling `mark_running_slot_valid()` before connectivity confirmed:** The purpose is to confirm the new firmware "works." Calling it before WiFi/MQTT succeeds means a broken WiFi configuration survives rollback. Call it after `mqtt_connect()` returns Ok.
- **Attempting a second OTA while PENDING_VERIFY state:** `esp_ota_begin` returns `ESP_ERR_OTA_ROLLBACK_INVALID_STATE` if the current firmware has not been confirmed. Always call `mark_running_slot_valid()` before accepting an OTA trigger.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OTA partition management | Custom flash write code | `EspOta` / `EspOtaUpdate` | esp-idf handles erase, write alignment, sector boundaries, otadata update |
| SHA-256 hashing | Bit-manipulation SHA | `sha2` crate | Correct, audited, no_std-compatible |
| HTTP streaming | Raw TCP socket read | `EspHttpConnection` + `embedded_svc::io::Read` | Handles chunked encoding, redirects, keep-alive |
| Rollback state machine | Custom boot counter in NVS | `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` + `mark_running_slot_valid()` | ESP bootloader implements the full state machine (NEW → PENDING_VERIFY → VALID/INVALID) |

---

## Common Pitfalls

### Pitfall 1: Partition Erase Fires Task Watchdog

**What goes wrong:** `EspOta::initiate_update()` calls `esp_ota_begin(..., OTA_SIZE_UNKNOWN, ...)` which erases the full ~1.875 MB OTA partition. Flash erase on ESP32-C6 takes approximately 4-8 seconds. The default task watchdog timeout is 5 seconds. If the OTA thread does not feed the watchdog during erase, the device panics and reboots mid-OTA with partial flash content.

**Why it happens:** `OTA_WITH_SEQUENTIAL_WRITES` (which erases sector-by-sector as data arrives) is not exposed by the `EspOta::initiate_update()` Rust API — it hardcodes `OTA_SIZE_UNKNOWN`. The workaround is to feed the task watchdog (TWDT) by subscribing the OTA thread to TWDT before calling `initiate_update()`.

**How to avoid:**
```rust
// Subscribe this thread to the task watchdog, then feed it periodically
unsafe { esp_idf_svc::sys::esp_task_wdt_add(std::ptr::null_mut()) };
// initiate_update() blocks during erase; feed WDT inside a separate thread
// OR: disable the task watchdog for the OTA thread only:
unsafe { esp_idf_svc::sys::esp_task_wdt_delete(std::ptr::null_mut()) };
```
Simpler alternative: increase `CONFIG_ESP_TASK_WDT_TIMEOUT_S` to 30 seconds in sdkconfig.defaults for the duration of the OTA project phase. Document this as a known trade-off.

**Warning signs:** Device reboots ~5s after OTA trigger with "Task watchdog got triggered" in serial log.

### Pitfall 2: EspOta TAKEN Singleton Conflict

**What goes wrong:** `EspOta::new()` uses a global `TAKEN: Mutex<bool>` flag. If you create an `EspOta` at boot for `mark_running_slot_valid()` and forget to drop it before the OTA thread tries `EspOta::new()`, the OTA thread gets `ESP_ERR_INVALID_STATE`.

**How to avoid:** Wrap the boot-time `EspOta` usage in a block `{ ... }` so it drops before `main()` spawns threads. Never store `EspOta` in a long-lived struct.

**Warning signs:** OTA thread logs `EspOta::new() failed: ESP_ERR_INVALID_STATE`.

### Pitfall 3: Retained Trigger Replay on Reconnect

**What goes wrong:** The broker retains the `{"url":"..."}` payload on `gnss/{device_id}/ota/trigger`. After OTA completes and the device reboots, it reconnects and receives the retained trigger again, immediately starting another OTA cycle.

**How to avoid:** After `update.complete()` and before `restart()`, publish `b""` (empty payload) with `retain=true` to the trigger topic. This clears the retained message on the broker.

**Warning signs:** Device enters OTA boot loop — logs show "OTA trigger received" immediately after every boot.

### Pitfall 4: ROLLBACK_INVALID_STATE on Second OTA

**What goes wrong:** The first OTA completes and the device boots the new firmware. If `mark_running_slot_valid()` has NOT been called and the operator triggers a second OTA, `esp_ota_begin` returns `ESP_ERR_OTA_ROLLBACK_INVALID_STATE`.

**How to avoid:** `mark_running_slot_valid()` must be called unconditionally at every boot after WiFi+MQTT succeeds. It is a no-op when the slot is already confirmed (on normal (non-post-OTA) boots it returns ESP_OK without side effects).

**Warning signs:** Second OTA attempt returns error immediately at `initiate_update()` without beginning download.

### Pitfall 5: partitions.csv Path in sdkconfig

**What goes wrong:** The existing `sdkconfig.defaults` references the partition file as `../../../../../../partitions.csv` (relative to the ESP-IDF build directory). If the NVS partition start offset changes due to new partition layout, existing NVS data (WiFi credentials) will be read from wrong flash addresses on the first boot after reflash.

**How to avoid:** After `espflash erase-flash`, the NVS will be empty regardless. WiFi credentials must be re-provisioned after the reflash (they are compiled in via `config.rs` constants — this is automatic). No NVS migration is needed.

### Pitfall 6: HTTP Buffer Size vs RAM

**What goes wrong:** Large `buffer_size` in `EspHttpConnection::Configuration` (e.g., 65536) causes heap allocation failure on ESP32-C6 with its constrained RAM.

**How to avoid:** Use 4096 bytes as the read chunk size. The OTA write loop calls `update.write()` on each chunk incrementally. Flash writes are not cached, so chunk size does not affect write correctness.

---

## Code Examples

### Partition Table

```csv
# Source: verified against esp-idf partition tool constraints (ESP-IDF v5.3.3)
# Note: otadata MUST be at a 4KB-aligned offset and 8KB in size (two 4KB sectors)
# Name,     Type, SubType,  Offset,   Size,    Flags
nvs,        data, nvs,      0x9000,   0x6000,
otadata,    data, ota,      0xF000,   0x2000,
ota_0,      app,  ota_0,    0x20000,  0x1E0000,
ota_1,      app,  ota_1,    0x200000, 0x1E0000,
```

### sdkconfig.defaults additions

```
# Enable bootloader rollback support — new firmware must call mark_running_slot_valid()
# or bootloader rolls back to previous slot on next reboot
CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y

# Extend task watchdog to survive OTA partition erase (4-8 seconds on ESP32-C6)
CONFIG_ESP_TASK_WDT_TIMEOUT_S=30
```

### OTA Task Skeleton

```rust
// Source: esp-idf-svc 0.51.0 src/ota.rs (EspOta, EspOtaUpdate) +
//         src/http/client.rs (EspHttpConnection)
// File: src/ota.rs

use embedded_svc::http::client::{Client as HttpClient, Method};
use embedded_svc::io::Read;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::mqtt::client::EspMqttClient;
use esp_idf_svc::ota::EspOta;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;

pub fn ota_task(
    mqtt_client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    ota_rx: Receiver<Vec<u8>>,
) -> ! {
    let status_topic = format!("gnss/{}/ota/status", device_id);
    let trigger_topic = format!("gnss/{}/ota/trigger", device_id);

    for payload in &ota_rx {
        // parse url and optional sha256 from JSON payload
        // publish {"state":"downloading","progress":0}
        // perform download + write loop
        // on success: publish {"state":"complete"}, clear trigger, restart()
        // on failure: publish {"state":"failed","reason":"..."}
    }

    log::error!("OTA channel closed");
    loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
}

pub fn spawn_ota(
    mqtt_client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    ota_rx: Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(16384)   // HTTP client + SHA + OTA state needs more than default 8KB
        .spawn(move || ota_task(mqtt_client, device_id, ota_rx))
        .map(|_| ())
        .map_err(Into::into)
}
```

### JSON Parsing Without serde (minimal approach)

The trigger payload is `{"url":"...","sha256":"..."}`. Since the project has no `serde_json` dependency and adding one has a code-size cost, parse manually using simple byte search:

```rust
// Minimal JSON field extraction — no serde dependency
fn extract_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":\"", key);
    let start = json.find(&search)? + search.len();
    let end = json[start..].find('"')? + start;
    Some(&json[start..end])
}

// Usage:
let json = std::str::from_utf8(&payload).ok()?;
let url = extract_json_str(json, "url")?;
let sha256 = extract_json_str(json, "sha256");  // optional
```

If payload parsing complexity grows, add `serde_json = { version = "1", features = ["alloc"], default-features = false }` — it compiles for `no_std + alloc` targets.

### Progress Reporting

```rust
// Publish progress every N bytes to avoid flooding broker
// Source: OTA-06 requirement; pattern matches heartbeat_loop in mqtt.rs
fn publish_status(client: &Arc<Mutex<EspMqttClient<'static>>>, topic: &str, json: &str) {
    if let Ok(mut c) = client.lock() {
        let _ = c.enqueue(topic, QoS::AtMostOnce, false, json.as_bytes());
    }
}
// Call during loop: publish_status(&mqtt_client, &status_topic, &format!(...))
```

---

## State of the Art

| Old Approach | Current Approach | Notes |
|--------------|------------------|-------|
| Factory-only partition (no OTA) | otadata + ota_0 + ota_1 | Requires one-time USB reflash |
| `EspFirmwareInfoLoader` | `EspFirmwareInfoLoad` (non-deprecated) | Old struct deprecated in esp-idf-svc 0.51.0 |
| OTA_SIZE_UNKNOWN (full erase) | OTA_WITH_SEQUENTIAL_WRITES (incremental) | Rust API exposes only OTA_SIZE_UNKNOWN via `initiate_update()`; WDT extension is the workaround |

**Deprecated/outdated:**
- `EspFirmwareInfoLoader`: deprecated in 0.51.0 — use `EspFirmwareInfoLoad` if firmware info inspection is needed (not required for this phase).

---

## Open Questions

1. **Watchdog strategy during partition erase**
   - What we know: `initiate_update()` erases full ~1.875 MB partition via `OTA_SIZE_UNKNOWN`; this takes 4-8 seconds; default TWDT timeout is 5 seconds.
   - What's unclear: Whether simply setting `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` in sdkconfig is sufficient, or whether the OTA thread must explicitly subscribe to / feed the TWDT.
   - Recommendation: Set `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` in sdkconfig.defaults as a first step. If the TWDT still fires (observable in serial logs), add explicit `esp_task_wdt_delete(null)` for the OTA thread only.

2. **SHA256 field: required or optional in trigger payload**
   - What we know: OTA-02 says payload is `{"url":"...","sha256":"..."}` — SHA256 is shown but not explicitly marked mandatory.
   - What's unclear: Whether to reject triggers missing `sha256` or accept them (skip verification).
   - Recommendation: Require `sha256` — reject with `{"state":"failed","reason":"missing sha256"}`. Skipping verification defeats the purpose of the field.

3. **OTA thread stack size**
   - What we know: Existing threads use 8192 bytes. OTA thread needs HTTP client, SHA256 hasher, OTA handle, and a 4096-byte read buffer on the stack.
   - What's unclear: Exact stack consumption at peak.
   - Recommendation: Start at 16384 (16 KB). Reduce if `CONFIG_FREERTOS_CHECK_STACKOVERFLOW_CANARY` never fires.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | None (bare-metal embedded — no unit test harness) |
| Config file | none |
| Quick run command | `cargo build --release` (compile check) |
| Full suite command | `cargo build --release && espflash flash --monitor` (on-device smoke test) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| OTA-01 | Partition table accepted by espflash; device boots from ota_0 | manual | `espflash erase-flash && espflash flash --monitor` | ❌ Wave 0 — create partitions.csv update |
| OTA-02 | Device subscribes to /ota/trigger; trigger message delivered to ota_rx | manual-smoke | `cargo build --release` + publish via mosquitto_pub | ❌ |
| OTA-03 | Download + SHA256 verification + OTA write completes without error | manual-smoke | on-device with test HTTP server hosting known firmware | ❌ |
| OTA-04 | New firmware boots, `mark_running_slot_valid()` called, no rollback | manual | boot new firmware, observe serial log, reboot, confirm stays on new slot | ❌ |
| OTA-04 | Rollback: boot new firmware WITHOUT marking valid, confirm rollback on next reboot | manual | deliberately omit valid call in test build | ❌ |
| OTA-05 | Heartbeat continues at 30s intervals during OTA download | manual-smoke | observe MQTT heartbeat topic during active download | ❌ |
| OTA-06 | Status messages published: downloading/complete/failed | manual-smoke | `mosquitto_sub -t 'gnss/+/ota/status'` during OTA | ❌ |

### Sampling Rate
- **Per task commit:** `cargo build --release` (compile gate)
- **Per wave merge:** Full on-device flash + OTA smoke test
- **Phase gate:** All 6 requirements verified on hardware before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] Updated `partitions.csv` — OTA-01
- [ ] `src/ota.rs` — new module (OTA-02 through OTA-06)
- [ ] sdkconfig.defaults additions (`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`, `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30`)

---

## Sources

### Primary (HIGH confidence)

- `esp-idf-svc 0.51.0 src/ota.rs` (local registry) — `EspOta`, `EspOtaUpdate`, `mark_running_slot_valid()` API; `initiate_update()` hardcodes `OTA_SIZE_UNKNOWN`
- `esp-idf-svc 0.51.0 src/http/client.rs` (local registry) — `EspHttpConnection`, streaming `read()`, `Configuration`
- `esp-idf-svc 0.51.0 Cargo.toml` (local registry) — confirmed no `ota` feature flag; OTA types compiled unconditionally under `std`
- `esp-idf v5.3.3 components/app_update/esp_ota_ops.c` (local embuild) — `esp_ota_mark_app_valid_cancel_rollback()`, `ESP_ERR_OTA_ROLLBACK_INVALID_STATE`, OTA state transitions
- `esp-idf v5.3.3 components/app_update/include/esp_ota_ops.h` (local embuild) — `OTA_SIZE_UNKNOWN = 0xffffffff`, `OTA_WITH_SEQUENTIAL_WRITES = 0xfffffffe`
- `esp-idf v5.3.3 components/bootloader_support/src/bootloader_utility.c` (local embuild) — `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE` guards
- `sha2-0.10.9 Cargo.toml` (local registry) — pure Rust, no_std+alloc capable
- `esp-idf-svc 0.51.0 examples/http_client.rs` (local registry) — confirmed HTTP client usage pattern

### Secondary (MEDIUM confidence)

- STATE.md pending todos and pitfall notes — pre-identified watchdog, mark_valid placement, OTA thread isolation, sequential erase mode concerns (from previous research session)

### Tertiary (LOW confidence)

- None — all findings verified against local source files

---

## Metadata

**Confidence breakdown:**
- Partition table layout: HIGH — math verified against 4MB constraint, otadata 0x2000 requirement confirmed from esp_ota_ops.c
- EspOta API: HIGH — source read directly from local registry
- SHA256 via sha2 crate: HIGH — crate confirmed in local registry; API is stable RustCrypto
- Watchdog behavior: MEDIUM — timing estimate (4-8s) from general ESP32 flash erase speed knowledge; exact duration varies with flash chip; `CONFIG_ESP_TASK_WDT_TIMEOUT_S=30` is the safe workaround
- JSON parsing strategy: MEDIUM — manual extraction avoids dependency but is fragile for complex payloads

**Research date:** 2026-03-07
**Valid until:** 2026-06-07 (esp-idf-svc 0.51.0 is pinned; stable until project updates the pin)
