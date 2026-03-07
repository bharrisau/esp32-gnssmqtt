# Phase 14: Quick Additions - Research

**Researched:** 2026-03-08
**Domain:** ESP-IDF SNTP, MQTT topic routing, esp_restart
**Confidence:** HIGH

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| MAINT-01 | Device reboots when `gnss/{device_id}/ota/trigger` payload is `"reboot"` | OTA task already receives all trigger payloads; add early-exit check before JSON parse |
| MAINT-02 | Device syncs wall-clock time via SNTP on WiFi connect; timestamps appear in log output | `EspSntp::new_default()` confirmed in esp-idf-svc 0.51.0; sdkconfig needs `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` |
| CMD-01 | Device subscribes to `gnss/{device_id}/command` and forwards each message as a raw UM980 command over UART | New subscription in `subscriber_loop`; new channel from callback to a command-relay thread; uses existing `gnss_cmd_tx` |
| CMD-02 | Command topic is non-retained; each publish triggers exactly one command send with no deduplication | Subscribe at QoS 0; do NOT subscribe with retain; no hash check in command relay (unlike config_relay) |
</phase_requirements>

## Summary

Phase 14 adds four small features that each touch one focused code path. All three capabilities — SNTP time sync, command relay, and reboot trigger — reuse existing infrastructure. No new crate dependencies are needed.

**SNTP** is already in `esp-idf-svc 0.51.0` at `esp_idf_svc::sntp::EspSntp`. Calling `EspSntp::new_default()` after WiFi is up starts polling `pool.ntp.org` servers. Once the first sync completes (typically 1-3 seconds), `std::time::SystemTime::now()` returns real wall-clock time. However, timestamps in log output are controlled by a separate sdkconfig Kconfig option (`CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM`). The current `sdkconfig.defaults` only sets `CONFIG_LOG_DEFAULT_LEVEL_INFO`; the timestamp source is not overridden and defaults to `CONFIG_LOG_TIMESTAMP_SOURCE_RTOS` (milliseconds since boot). To get ISO timestamps in log output, `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` must be added to `sdkconfig.defaults`.

**Command relay** (CMD-01, CMD-02) follows the exact same wiring pattern as config relay, but without the djb2 deduplication. A new `SyncSender<Vec<u8>>` is added to `mqtt_connect`'s signature, the callback's `Received` arm dispatches `/command` payloads to it, `subscriber_loop` subscribes the new topic at QoS 0 (no retain replay), and a small relay function forwards each payload as a single `gnss_cmd_tx.send()` call.

**Reboot trigger** (MAINT-01) is a two-line change at the top of `ota_task`'s payload processing block: if the UTF-8 payload is exactly `"reboot"`, call `restart()` (already imported in ota.rs) and skip the OTA download path entirely.

**Primary recommendation:** Implement in two plans — Plan 14-01: SNTP (sdkconfig change + `EspSntp` initialization in main.rs), Plan 14-02: command relay topic + reboot trigger (mqtt.rs + subscriber_loop + new relay fn + ota.rs).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp_idf_svc::sntp::EspSntp | 0.51.0 (already in Cargo.toml) | SNTP client wrapping ESP-IDF sntp component | Already a dependency; no new crate needed |
| std::time::SystemTime | std | Wall-clock timestamps after SNTP sync | Works on ESP32 Rust std target after time sync |
| esp_idf_svc::hal::reset::restart | 0.45.2 (already imported in ota.rs) | Immediate device restart | Already used in OTA success path |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `unsafe { esp_idf_svc::sys::esp_restart() }` | (sys crate, already in Cargo.toml) | Alternative restart call | Used in watchdog.rs and wifi.rs; either form works |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `EspSntp::new_default()` | Custom NTP server config via `SntpConf` | Default uses `0-3.pool.ntp.org` which is fine for this use case; custom config adds complexity with no benefit |

**Installation:**
No new packages needed. All required types are already in the dependency tree.

## Architecture Patterns

### Recommended Project Structure
No new files needed. Changes are confined to:
```
src/
├── main.rs          # add EspSntp init after wifi_connect; add cmd_tx channel
├── mqtt.rs          # add cmd_tx param to mqtt_connect; dispatch /command in callback; subscribe in subscriber_loop
├── ota.rs           # add "reboot" early-exit before JSON parse
└── sdkconfig.defaults  # add CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y
```

### Pattern 1: EspSntp Initialization (MAINT-02)

**What:** Create EspSntp after WiFi connects; keep handle alive for the firmware lifetime.
**When to use:** Exactly once, after `wifi_connect` returns successfully, before spawning MQTT.

```rust
// Source: esp-idf-svc 0.51.0 src/sntp.rs + examples/sntp.rs
use esp_idf_svc::sntp;

// After wifi_connect, before mqtt_connect:
let _sntp = sntp::EspSntp::new_default().expect("SNTP init failed");
log::info!("SNTP initialized");
// _sntp must stay alive — dropping it calls sntp_stop()
```

EspSntp is a singleton (guarded by a static mutex `TAKEN`). Calling `new_default()` a second time returns `ESP_ERR_INVALID_STATE`. The handle must not be dropped; move it into the idle loop or keep it in main scope.

**Sync timing:** The first NTP response arrives within 1-5 seconds of WiFi being up (network-dependent). There is no blocking wait in `new_default()` — it starts the background SNTP task. Subsequent log lines will show real timestamps once the kernel clock updates; lines emitted immediately after `sntp_init()` may still show boot-relative times if the sync has not completed yet.

### Pattern 2: Log Timestamp Source (MAINT-02)

**What:** Set `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` in `sdkconfig.defaults`.
**When to use:** This is a build-time Kconfig setting; SNTP sync at runtime populates the underlying clock.

The EspLogger in esp-idf-svc 0.51.0 has two timestamp modes (confirmed in `src/log.rs`):
- `cfg!(esp_idf_log_timestamp_source_rtos)` → `esp_log_timestamp()` → milliseconds since boot (CURRENT setting)
- `cfg!(esp_idf_log_timestamp_source_system)` → `esp_log_system_timestamp()` → `"HH:MM:SS.mmm"` string format

`esp_log_system_timestamp()` returns a wall-clock time string once SNTP has synced. Before sync, it shows `"00:00:00.000"` or a similar sentinel. This is acceptable per the success criterion ("after WiFi connects").

Add to `sdkconfig.defaults`:
```
# Log timestamps: use system time (wall clock) instead of RTOS ticks.
# After SNTP syncs, log lines show HH:MM:SS.mmm instead of uptime ms.
CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y
```

This requires a clean rebuild (sdkconfig changes invalidate the ESP-IDF CMake cache). Build command: `cargo build --release` (embuild handles the CMake reconfiguration).

Note: `esp_log_system_timestamp()` returns a pointer to a static buffer — the comment in `esp-idf-svc/src/log.rs` explicitly notes this has a race condition. This is a known upstream issue (tracked in PR #494). For logging use it is acceptable; do NOT call `esp_log_system_timestamp()` from application code directly.

### Pattern 3: Command Relay Topic (CMD-01, CMD-02)

**What:** Wire a new `gnss/{device_id}/command` MQTT subscription to a channel that forwards each payload as one raw UM980 command. No deduplication.
**When to use:** This is an always-on subscription, like `/config` and `/ota/trigger`.

Channel sizing: bounded to 4, matching the config channel. Commands are operator-triggered and rare; 4 slots cover a burst without risk of blocking the callback.

```rust
// In main.rs — new channel before mqtt_connect:
// command payload — callback → command relay
// Bounded to 4: operator-triggered, rare. try_send() in callback never blocks.
let (cmd_relay_tx, cmd_relay_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
```

mqtt_connect signature gains one parameter; callback `Received` arm gains one branch:

```rust
} else if t.ends_with("/command") {
    match cmd_relay_tx.try_send(data.to_vec()) {
        Ok(_) => {}
        Err(TrySendError::Full(_)) => log::warn!("mqtt cb: command channel full — command dropped"),
        Err(TrySendError::Disconnected(_)) => log::warn!("mqtt cb: command channel closed"),
    }
}
```

subscriber_loop subscribes the new topic **at QoS 0** — QoS 0 means the broker does not replay retained messages on reconnect. This is intentional: CMD-02 requires that replaying the MQTT session does NOT re-execute old commands.

```rust
// In subscriber_loop, alongside /config and /ota/trigger subscriptions:
let command_topic = format!("gnss/{}/command", device_id);
match c.subscribe(&command_topic, QoS::AtMostOnce) {  // QoS 0 — no retain replay
    Ok(_) => log::info!("Subscribed to {}", command_topic),
    Err(e) => log::warn!("Subscribe /command failed: {:?}", e),
}
```

The relay function is trivial — decode UTF-8 payload, send a single string via `gnss_cmd_tx`:

```rust
fn command_relay_task(gnss_cmd_tx: SyncSender<String>, cmd_relay_rx: Receiver<Vec<u8>>) -> ! {
    loop {
        match cmd_relay_rx.recv_timeout(config::SLOW_RECV_TIMEOUT) {
            Ok(payload) => {
                match std::str::from_utf8(&payload) {
                    Ok(cmd) => {
                        log::info!("Command relay: forwarding: {:?}", cmd);
                        if let Err(e) = gnss_cmd_tx.send(cmd.to_string()) {
                            log::error!("Command relay: gnss_cmd_tx send failed: {:?}", e);
                        }
                    }
                    Err(e) => log::warn!("Command relay: payload not valid UTF-8: {:?}", e),
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("Command relay: channel closed");
                break;
            }
        }
    }
    loop { std::thread::sleep(Duration::from_secs(60)); }
}
```

This can live directly in `mqtt.rs` (alongside `subscriber_loop`) or in a new `command_relay.rs`. Given its simplicity (~30 lines), putting it in `mqtt.rs` avoids a new file.

### Pattern 4: Reboot Trigger (MAINT-01)

**What:** In `ota_task`, check payload for `"reboot"` string before parsing JSON.
**When to use:** At the top of the per-payload processing block, before any JSON parsing.

```rust
// Source: ota.rs pattern — ota_task() payload processing block
// Add at the start of the inner block, after UTF-8 decode:
let json = match std::str::from_utf8(&payload) {
    Ok(s) => s.to_owned(),
    Err(e) => { /* existing error path */ continue; }
};

// NEW: handle "reboot" before attempting OTA JSON parse
if json.trim() == "reboot" {
    log::info!("OTA: reboot command received — restarting");
    // Allow a brief moment for the log line to flush before restart
    std::thread::sleep(Duration::from_millis(200));
    restart();  // already imported: esp_idf_svc::hal::reset::restart
}
```

The `restart()` function from `esp_idf_svc::hal::reset` is already imported in ota.rs. It calls `esp_restart()` internally. The function diverges (never returns), so no `continue` or `return` is needed after it.

The 200ms sleep is optional but prevents the log line from being lost if the serial buffer hasn't been flushed before restart. The 5-second requirement in the success criteria gives ample margin.

### Anti-Patterns to Avoid

- **Deduplicating the command topic:** Config relay uses djb2 deduplication because `/config` is retained and replayed on reconnect. The command topic is non-retained (QoS 0 subscription). Adding hash deduplication would silently swallow repeated identical commands (e.g., the user sending the same UM980 command twice). Do not add a hash check.
- **Dropping EspSntp:** If `_sntp` goes out of scope, the Drop impl calls `sntp_stop()`. The handle must survive for the entire firmware lifetime. Assign it in `main()` scope before the idle loop.
- **Subscribing /command at QoS 1:** QoS 1 with `disable_clean_session: true` (current MQTT config) would cause the broker to replay unacknowledged messages on reconnect. Use QoS 0 for the command topic specifically.
- **Using esp_log_system_timestamp() from application code directly:** It returns a pointer to a static buffer — not thread-safe. The EspLogger already calls it correctly. Application code should use `std::time::SystemTime::now()` for timestamps.
- **Calling EspSntp::new() twice:** The `TAKEN` static mutex prevents double-init, but it returns an error rather than panicking. Call it once in main and handle the error.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| NTP time sync | Custom UDP NTP client | `EspSntp::new_default()` | ESP-IDF SNTP component handles server selection, retries, drift correction, and clock update atomically |
| ISO timestamp formatting | strftime-like logic | `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` Kconfig | EspLogger already calls `esp_log_system_timestamp()` — single config line enables it |
| Safe device restart | Custom reset sequence | `restart()` from `esp_idf_svc::hal::reset` or `unsafe { esp_idf_svc::sys::esp_restart() }` | Both are correct; `restart()` is already imported in ota.rs |

**Key insight:** All three capabilities in this phase are one-liners or config changes at the mechanism level. The engineering effort is in wiring (channels, subscriptions, dispatch) rather than implementation.

## Common Pitfalls

### Pitfall 1: EspSntp Handle Dropped Too Early
**What goes wrong:** Timestamps appear in logs briefly after SNTP sync, then revert to `00:00:00.000` or millisecond ticks.
**Why it happens:** The `EspSntp` Drop impl calls `sntp_stop()`. If the handle is let-bound in a block that ends before the idle loop, the SNTP client stops.
**How to avoid:** Assign `let _sntp = sntp::EspSntp::new_default()?;` in the outermost `main()` scope, before the idle loop. The leading underscore suppresses the unused-variable warning while keeping the binding alive.
**Warning signs:** Log timestamps showing boot-relative ms after appearing as wall-clock times briefly.

### Pitfall 2: Command Topic Subscribed at QoS 1 Instead of QoS 0
**What goes wrong:** After device reboot, broker replays unacknowledged `/command` messages, causing UM980 to re-execute stale commands.
**Why it happens:** With `disable_clean_session: true` (current MQTT config), QoS 1 subscriptions persist across reconnects on the broker side.
**How to avoid:** Subscribe `gnss/{device_id}/command` at `QoS::AtMostOnce` (QoS 0). Confirm in subscriber_loop.
**Warning signs:** UM980 receives commands that were not recently published — compare broker publish timestamps with device reconnect times.

### Pitfall 3: sdkconfig Change Requires Clean Build
**What goes wrong:** `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y` added but logs still show ms ticks.
**Why it happens:** ESP-IDF CMake caches the previous Kconfig configuration; `cargo build` without `--clean` may not regenerate sdkconfig.
**How to avoid:** After adding the Kconfig line, run `cargo clean` then `cargo build --release`. Alternatively, delete `target/riscv32imac-esp-espidf/release/build/esp-idf-sys-*/out/build/` to force CMake reconfiguration.
**Warning signs:** Build succeeds but `cfg!(esp_idf_log_timestamp_source_system)` still evaluates as false in EspLogger.

### Pitfall 4: SNTP Sync Delay — First Log Lines Still Show Boot Time
**What goes wrong:** Log output after SNTP init shows `00:00:00.000` for the first 1-5 seconds.
**Why it happens:** `EspSntp::new_default()` returns immediately; the background SNTP task polls and the first server response takes network round-trip time. The ESP-IDF clock updates atomically on first sync, but there is a window.
**How to avoid:** This is expected behavior. The success criterion says "after WiFi connects" — not "immediately at init." The first few log lines may show pre-sync timestamps. An optional informational log after SNTP init helps operators know sync is in progress.
**Warning signs:** All logs after reboot show `00:00:00.000` indefinitely — this indicates a DNS or network issue preventing NTP server contact.

### Pitfall 5: "reboot" Payload Requires Trim
**What goes wrong:** Publishing `"reboot\n"` or `"reboot "` (with trailing whitespace) does not trigger reboot because string comparison fails.
**Why it happens:** MQTT payloads from some clients include trailing newlines or spaces.
**How to avoid:** Use `json.trim() == "reboot"` rather than `json == "reboot"`. Confirmed pattern from ota.rs (which already trims nothing — add `.trim()` to the comparison in the reboot check).
**Warning signs:** Payload arrives (logged), reboot does not occur, OTA JSON parse then fails because "reboot" is not valid JSON.

## Code Examples

Verified patterns from source in this repo and esp-idf-svc 0.51.0:

### EspSntp Initialization in main.rs
```rust
// Source: esp-idf-svc-0.51.0/examples/sntp.rs + src/sntp.rs
// After wifi_connect, before mqtt_connect:
use esp_idf_svc::sntp;
let _sntp = sntp::EspSntp::new_default().expect("SNTP init failed");
log::info!("SNTP initialized — time will sync in background");
```

### sdkconfig.defaults Timestamp Change
```
# Source: esp-idf-svc-0.51.0/src/log.rs — cfg!(esp_idf_log_timestamp_source_system) branch
# Replaces default RTOS tick timestamps with wall-clock HH:MM:SS.mmm after SNTP sync
CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM=y
```

### Reboot Check in ota_task
```rust
// Source: ota.rs ota_task() — add after UTF-8 decode of payload
// esp_idf_svc::hal::reset::restart already imported at top of ota.rs
if json.trim() == "reboot" {
    log::info!("OTA: 'reboot' payload — restarting device");
    std::thread::sleep(Duration::from_millis(200)); // let log line flush
    restart(); // diverges — does not return
}
```

### subscriber_loop Addition
```rust
// Source: mqtt.rs subscriber_loop — mirror existing /config and /ota/trigger subscriptions
let command_topic = format!("gnss/{}/command", device_id);
match c.subscribe(&command_topic, QoS::AtMostOnce) {  // QoS 0: no retain replay (CMD-02)
    Ok(_) => log::info!("Subscribed to {}", command_topic),
    Err(e) => log::warn!("Subscribe /command failed: {:?}", e),
}
```

### Command Relay Channel Declaration in main.rs
```rust
// Mirror config channel pattern — bounded to 4, try_send in callback
let (cmd_relay_tx, cmd_relay_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `sntp_init()` (legacy C API, ESP-IDF v4) | `esp_sntp_init()` (ESP-IDF v5.1+) | ESP-IDF v5.1 | `EspSntp` wraps both; version is auto-detected at compile time via cfg flags in sntp.rs |
| `CONFIG_LOG_TIMESTAMP_SOURCE_RTOS` (ms ticks) | `CONFIG_LOG_TIMESTAMP_SOURCE_SYSTEM` (wall clock) | Always been an option; project never enabled it | One Kconfig line enables ISO-style timestamps |

**Deprecated/outdated:**
- Manual `sntp_setoperatingmode` / `sntp_setservername` / `sntp_init` call sequence: Replaced by `EspSntp::new()` in esp-idf-svc, which handles all three steps.

## Open Questions

1. **NTP server reachability in the field**
   - What we know: Default servers are `0-3.pool.ntp.org`; they require UDP 123 outbound. Most home/office networks allow this.
   - What's unclear: The target deployment network. If UDP 123 is blocked, SNTP will silently fail (no error return from `new_default()`; get_sync_status() remains `Reset`).
   - Recommendation: Not a code concern for Phase 14. If NTP is blocked in the field, it becomes a Phase 15 NVS config item. Accept default servers for now.

2. **`esp_log_system_timestamp()` race condition (upstream issue)**
   - What we know: `src/log.rs` in esp-idf-svc 0.51.0 includes a comment flagging a race (PR #494) because the function returns a pointer to a static buffer. The EspLogger acquires `EspStdout` which holds the stdout lock during the write — this provides serialization between log calls on different threads.
   - What's unclear: Whether two threads racing on `esp_log_system_timestamp()` itself (before the EspStdout lock) can produce garbled output.
   - Recommendation: Accept the known limitation. For this firmware's use (status logging, not high-frequency data), the risk of garbled timestamp text is negligible.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — embedded Rust firmware, no host test runner |
| Config file | N/A |
| Quick run command | `cargo build --release` (compilation check) |
| Full suite command | `cargo build --release` + flash + manual inspection |

This is embedded firmware running on ESP32-C6 hardware. There is no host-side test harness. Validation is compilation success + hardware observation.

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MAINT-01 | `"reboot"` to `/ota/trigger` restarts device within 5s | manual | `cargo build --release` (compile) | N/A |
| MAINT-02 | ISO timestamps in log output after WiFi connects | manual | `cargo build --release` (compile) | N/A |
| CMD-01 | `/command` payload forwarded to UM980 UART once | manual | `cargo build --release` (compile) | N/A |
| CMD-02 | No retained replay of old commands on reconnect | manual | `cargo build --release` (compile) | N/A |

### Sampling Rate
- **Per task commit:** `cargo build --release`
- **Per wave merge:** `cargo build --release` + flash + `espflash monitor` observation
- **Phase gate:** All four manual criteria observed on device before `/gsd:verify-work`

### Wave 0 Gaps
None — existing build infrastructure covers all phase requirements. No new test files needed.

## Sources

### Primary (HIGH confidence)
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/sntp.rs` — EspSntp API, SntpConf, new_default(), get_sync_status(), Drop behavior
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/examples/sntp.rs` — canonical SNTP usage pattern
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/log.rs` — EspLogger timestamp branch logic (RTOS vs system), `esp_log_system_timestamp()` usage and race condition note
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/systime.rs` — EspSystemTime::now() and gettimeofday relationship
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/ota.rs` — existing `restart()` import, payload processing pattern
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/mqtt.rs` — subscriber_loop subscription pattern, callback dispatch pattern
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/src/config_relay.rs` — deduplication pattern (to confirm command relay must NOT replicate it)
- `/home/ben/github.com/bharrisau/esp32-gnssmqtt/sdkconfig.defaults` — confirmed current timestamp source is RTOS ticks

### Secondary (MEDIUM confidence)
- None required — all critical claims verified against local source.

### Tertiary (LOW confidence)
- NTP sync timing (1-5 seconds): based on typical ESP-IDF SNTP behavior; exact timing is network-dependent.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — EspSntp API read directly from local registry source; no inference
- Architecture: HIGH — all patterns derived from existing codebase (ota.rs, mqtt.rs, config_relay.rs)
- Pitfalls: HIGH for code pitfalls (verified against source); MEDIUM for timing pitfall (network-dependent)

**Research date:** 2026-03-08
**Valid until:** 2026-06-08 (stable; ESP-IDF 5.3.3 is a fixed version in this project)
