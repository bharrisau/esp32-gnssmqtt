# Phase 18: Telemetry and OTA Validation - Research

**Researched:** 2026-03-09
**Domain:** NMEA GGA parsing, MQTT heartbeat extension, OTA hardware validation, project README
**Confidence:** HIGH

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| TELEM-01 | Health heartbeat includes GNSS fix type, satellite count, and HDOP parsed from the most recent GGA sentence | GGA field layout documented below; atomic sharing pattern established by NTRIP_STATE precedent |
| MAINT-03 | OTA firmware update validated on hardware (device FFFEB5) as explicit sign-off gate before v2.0 milestone is marked complete | OTA pipeline fully implemented in ota.rs (Phase 8); validation is procedural, not code change |
</phase_requirements>

---

## Summary

Phase 18 completes the v2.0 milestone with two distinct work streams: a code change (GGA parsing for heartbeat telemetry) and a hardware validation procedure (OTA sign-off on device FFFEB5). A third deliverable is the project README, explicitly requested by the user.

The GGA parsing task is self-contained: the GNSS RX thread already forwards every NMEA sentence as `(sentence_type, raw)` tuples through `nmea_rx`. The `nmea_relay` thread currently consumes `nmea_rx` exclusively and publishes to MQTT. To share GGA data with the heartbeat, a small shared state structure (three atomics or a Mutex-guarded struct) must be inserted between the GNSS pipeline and the heartbeat loop — no new channels needed, no changes to the GNSS RX or TX threads.

The OTA hardware validation is purely procedural: build a firmware image, serve it over HTTP, publish the JSON trigger to MQTT, and observe the device reboot into the new image and mark its slot valid. The pipeline already handles SHA-256 verification and slot validation (`mark_running_slot_valid` is called in main.rs after WiFi+MQTT connect). The README task requires surveying all subsystems and documenting them accessibly for open-source users.

**Primary recommendation:** Use three atomics (`AtomicU8` for fix_type, `AtomicU8` for satellites, `AtomicU32` for hdop as scaled integer) in a new `gnss_state.rs` module. The NMEA relay thread writes them after parsing GGA; the heartbeat reads them. This follows the established `NTRIP_STATE` AtomicU8 pattern exactly. No mutex, no new channel, no architecture change.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-idf-svc | =0.51.0 | EspOta, EspMqttClient — already in use | Project dependency; OTA already implemented |
| sha2 | 0.10 | SHA-256 verification in OTA | Already in Cargo.toml |
| std::sync::atomic | stdlib | Shared fix-quality state between threads | No-dependency, no-alloc, established pattern |

### No New Dependencies Needed
This phase adds no new crate dependencies. All required building blocks are already present:
- NMEA parsing is string splitting (no nmea crate needed — only 3 fields from GGA)
- OTA pipeline is fully implemented
- README is a documentation task

---

## Architecture Patterns

### NMEA GGA Sentence Format
The UM980 emits standard NMEA 0183 GGA sentences. Field layout (comma-delimited):

```
$GNGGA,hhmmss.ss,lat,N/S,lon,E/W,fix,sats,hdop,alt,M,sep,M,age,ref*cs
Index:  0         1         2 3    4   5    6    7    8   9   10  11  12  13  14
```

Fields relevant to TELEM-01:
- **Field 6** — Fix quality indicator:
  - `0` = No fix
  - `1` = GPS fix (SPS)
  - `2` = DGPS fix
  - `4` = RTK Fixed
  - `5` = RTK Float
  - `6` = Estimated (dead reckoning)
- **Field 7** — Number of satellites in use (00–12+)
- **Field 8** — HDOP (horizontal dilution of precision), e.g. `1.2`

The sentence type in the GNSS pipeline is the string between `$` and the first `,`. For GGA sentences from the UM980, the sentence type is `"GNGGA"` (multi-constellation). May also appear as `"GPGGA"` (GPS-only mode). Both should be handled.

### Pattern 1: Shared Atomic Fix-Quality State

**What:** Three module-level atomics in a new `gnss_state.rs` (or added to `gnss.rs`) store the most recent GGA values. Written by the NMEA relay (or a GGA parser called from it); read by `heartbeat_loop`.

**When to use:** When two threads need to share small integer values without a channel or mutex. Exactly the pattern used for `NTRIP_STATE` in `ntrip_client.rs` and `NMEA_DROPS`/`RTCM_DROPS` in `gnss.rs`.

**Example (established project pattern):**
```rust
// In gnss_state.rs (new module) or appended to gnss.rs
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// Most recent GGA fix quality: 0=no fix, 1=SPS, 2=DGPS, 4=RTK Fixed, 5=RTK Float
/// 0xFF = sentinel "no GGA received yet"
pub static GGA_FIX_TYPE: AtomicU8 = AtomicU8::new(0xFF);

/// Most recent GGA satellite count. 0xFF = sentinel "no GGA received yet"
pub static GGA_SATELLITES: AtomicU8 = AtomicU8::new(0xFF);

/// Most recent GGA HDOP × 10 (e.g. HDOP 1.2 → stored as 12).
/// 0xFFFF = sentinel "no GGA received yet"
pub static GGA_HDOP_X10: AtomicU32 = AtomicU32::new(0xFFFF);
```

**Sentinel values:** Use out-of-range values (0xFF, 0xFFFF) to distinguish "never received" from a valid zero reading. Heartbeat serializes sentinel as JSON `null`.

**Source:** Established project pattern — `NTRIP_STATE: AtomicU8` in `src/ntrip_client.rs`; `NMEA_DROPS: AtomicU32` in `src/gnss.rs`.

### Pattern 2: GGA Parsing in the NMEA Relay Thread

**What:** The `nmea_relay` thread already receives every `(sentence_type, raw)` tuple. Add a branch: if `sentence_type == "GNGGA"` or `"GPGGA"`, parse the raw string and update the three atomics before (or after) publishing to MQTT.

**When to use:** No new thread needed; no changes to GNSS RX thread; no new channel. The relay thread is the natural place because it already inspects `sentence_type`.

**Example:**
```rust
// In nmea_relay.rs, inside the Ok((sentence_type, raw)) match arm:
if sentence_type == "GNGGA" || sentence_type == "GPGGA" {
    parse_gga_into_atomics(&raw);
}
```

```rust
fn parse_gga_into_atomics(raw: &str) {
    let fields: Vec<&str> = raw.split(',').collect();
    if fields.len() < 9 {
        return; // malformed
    }
    // Field 6: fix type
    if let Ok(fix) = fields[6].parse::<u8>() {
        crate::gnss_state::GGA_FIX_TYPE.store(fix, Ordering::Relaxed);
    }
    // Field 7: satellites
    if let Ok(sats) = fields[7].parse::<u8>() {
        crate::gnss_state::GGA_SATELLITES.store(sats, Ordering::Relaxed);
    }
    // Field 8: HDOP (float, store as ×10 integer to avoid float formatting in heartbeat)
    if let Ok(hdop) = fields[8].parse::<f32>() {
        crate::gnss_state::GGA_HDOP_X10.store((hdop * 10.0) as u32, Ordering::Relaxed);
    }
}
```

**Confidence:** HIGH — field indices verified against NMEA 0183 standard GGA definition.

### Pattern 3: Heartbeat JSON Extension

**What:** In `mqtt.rs::heartbeat_loop`, read the three atomics and append fields to the JSON string. Use sentinel check to emit `null` when no GGA has been received.

**Example:**
```rust
// TELEM-01: include GGA fix quality fields in heartbeat JSON
let fix_type_raw = crate::gnss_state::GGA_FIX_TYPE.load(Ordering::Relaxed);
let sats_raw = crate::gnss_state::GGA_SATELLITES.load(Ordering::Relaxed);
let hdop_raw = crate::gnss_state::GGA_HDOP_X10.load(Ordering::Relaxed);

let fix_type_json = if fix_type_raw == 0xFF { "null".to_string() } else { fix_type_raw.to_string() };
let sats_json = if sats_raw == 0xFF { "null".to_string() } else { sats_raw.to_string() };
let hdop_json = if hdop_raw == 0xFFFF { "null".to_string() } else {
    format!("{:.1}", hdop_raw as f32 / 10.0)
};

let json = format!(
    "{{\"uptime_s\":{},\"heap_free\":{},\"nmea_drops\":{},\"rtcm_drops\":{},\
     \"uart_tx_errors\":{},\"ntrip\":\"{}\",\
     \"fix_type\":{},\"satellites\":{},\"hdop\":{}}}",
    uptime_s, heap_free, nmea_drops, rtcm_drops, uart_tx_errors, ntrip_str,
    fix_type_json, sats_json, hdop_json
);
```

**Note:** HDOP stored as ×10 integer avoids f32 in the static and avoids format!("{:.1}", f32) precision surprises. Reconverting for JSON output is safe.

### Pattern 4: OTA Hardware Validation Procedure

**What:** The OTA pipeline (ota.rs) is fully implemented. MAINT-03 requires a documented, repeatable validation procedure run on device FFFEB5. This is a procedural task, not a code change.

**Procedure outline:**
1. Build a "canary" firmware image (increment version string or add a log line) to prove the new image is distinct from current
2. Compute SHA-256 of the `.bin` file: `sha256sum build/esp32-gnssmqtt.bin`
3. Serve the binary over HTTP on a machine reachable from the device (e.g. `python3 -m http.server 8080`)
4. Publish OTA trigger to MQTT:
   ```
   Topic:   gnss/FFFEB5/ota/trigger
   Payload: {"url":"http://192.168.x.x:8080/esp32-gnssmqtt.bin","sha256":"<hex>"}
   Retain:  true (so device gets it on reconnect if needed)
   ```
5. Observe MQTT `/ota/status` for `downloading` → `complete` progression
6. Observe device reboot into new image (check log for new version string)
7. Observe `mark_running_slot_valid` log line — confirms slot marked VALID (cancels rollback)
8. Confirm device stays up through next heartbeat cycle (no rollback occurred)

**Captive portal hardware verify:** Also deferred from Phase 17 — validate SoftAP + mobile device detection alongside Phase 18 hardware sign-off.

### Recommended Module Structure

```
src/
├── gnss_state.rs    # NEW — shared GGA atomics (GGA_FIX_TYPE, GGA_SATELLITES, GGA_HDOP_X10)
├── nmea_relay.rs    # MODIFY — add GGA parser call in Ok() arm
├── mqtt.rs          # MODIFY — heartbeat_loop reads gnss_state atomics, extends JSON
└── main.rs          # MODIFY — add `mod gnss_state;`
```

README location: project root `README.md`.

### Anti-Patterns to Avoid

- **Adding a new channel for GGA data:** Unnecessary complexity. Atomics are sufficient for three scalar values updated at NMEA rate (1 Hz GGA typical). A channel would require a new receiver in heartbeat_loop and a new sender in nmea_relay.
- **Using f32 atomic:** No `AtomicF32` in stable Rust std. Store HDOP as `u32` (×10 scaled integer). `AtomicU32` is available.
- **Parsing GGA in the GNSS RX thread:** The RX thread is the hot path (NON_BLOCK polling). Keep it minimal. Parse in the relay thread.
- **Using a crate for NMEA parsing:** Adds embedded dependency complexity, linker overhead, and potential `no_std` compatibility issues. Splitting on commas and indexing is three lines.
- **Stale-data risk:** If GGA parsing writes before checking for empty fields (e.g., empty field 6 during no-fix), the sentinel gets overwritten. Guard: only write atomics if `!fields[N].is_empty()`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OTA flash + rollback | Custom partition writer | `EspOta` (already used) | Handles partition selection, CRC, bootloader slot management |
| SHA-256 | Custom hash | `sha2` crate (already in Cargo.toml) | Audited, correct, already dependency |
| NMEA full parse | Custom NMEA library | Field indexing with `split(',')` | Only 3 fields needed; crate adds 10KB+ to binary |

**Key insight:** All complex infrastructure is already in place. This phase is targeted additions to existing modules.

---

## Common Pitfalls

### Pitfall 1: Empty GGA Fields During No-Fix
**What goes wrong:** When the UM980 has no GNSS fix, GGA field 6 is `"0"` but fields 7 and 8 may be empty strings (`""`). Calling `.parse::<u8>()` on `""` returns `Err`, which is safe, but calling it without checking field count or emptiness panics on out-of-bounds access.
**Why it happens:** NMEA spec allows empty fields; UM980 honors this.
**How to avoid:** Check `fields.len() >= 9` first. Then check `!fields[N].is_empty()` before parsing. Only write atomics on successful parse.
**Warning signs:** Heartbeat shows `null` for satellites/HDOP even when fix_type shows `1`.

### Pitfall 2: GGA Sentence Type Variants
**What goes wrong:** Filtering only on `"GNGGA"` misses sentences when UM980 is configured for GPS-only mode, which emits `"GPGGA"`.
**Why it happens:** NMEA talker prefix changes with constellation mode: GN = multi-constellation, GP = GPS-only, GL = GLONASS-only.
**How to avoid:** Match on `sentence_type.ends_with("GGA")` or explicitly handle both `"GNGGA"` and `"GPGGA"`.
**Warning signs:** fix_type stays at sentinel while NMEA log shows `$GPGGA,...` sentences.

### Pitfall 3: HDOP Format Variations
**What goes wrong:** UM980 may emit HDOP as `"1.2"`, `"1.20"`, or `"01.2"`. Rust `f32::parse` handles all these correctly. Risk is emitting `"0.0"` JSON for an empty field.
**Why it happens:** NMEA allows leading zeros and trailing zeros in decimal fields.
**How to avoid:** Guard: only update `GGA_HDOP_X10` when parse succeeds AND field is not empty.

### Pitfall 4: OTA HTTP Server Accessibility
**What goes wrong:** Device cannot reach the HTTP server due to firewall rules or wrong IP address in trigger payload.
**Why it happens:** Development machines often have firewall rules blocking inbound port 8080.
**How to avoid:** Test HTTP accessibility from a separate device on same WiFi network before triggering OTA. Use `curl http://192.168.x.x:8080/firmware.bin` to verify. Check firewall: `sudo ufw allow 8080` if needed.
**Warning signs:** OTA status shows `failed` with `HTTP GET` error.

### Pitfall 5: Retained Trigger Causing Re-Flash Loop
**What goes wrong:** OTA trigger published with `retain=true` causes device to re-flash on every reconnect until the retained message is cleared.
**Why it happens:** `ota_tx` is subscribed with `QoS::AtLeastOnce` — retained messages are re-delivered on reconnect.
**How to avoid:** The existing `ota.rs` Step 11 already clears the retained trigger (empty payload + retain=true). Confirm this executes. If testing with repeated OTA attempts, clear manually: publish empty payload to `gnss/FFFEB5/ota/trigger` with retain=true.

### Pitfall 6: clippy -D warnings on new module
**What goes wrong:** New atomics declared `pub static` but only read/written via `crate::gnss_state::` — clippy may emit `dead_code` if the module is added but not yet wired in.
**Why it happens:** Incremental wiring across tasks — module added in task 1, used in task 2.
**How to avoid:** Wire all three references (gnss_state declaration, nmea_relay write, heartbeat read) in the same plan. Or add `#[allow(dead_code)]` during intermediate steps and remove before final commit. Run `cargo clippy -- -D warnings` before each task commit.

---

## Code Examples

### GGA Field Reference
```
$GNGGA,123519.00,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47
         field[0]   [1]    [2] [3]     [4][5][6][7] [8]  [9][10][11][12][13][14]
                           lat  N/S    lon E/W fix sats hdop
```
- `fields[0]` = `"$GNGGA"` (with the `$` prefix since sentence starts with `$`)

**IMPORTANT:** In the GNSS RX state machine, `sentence_type` is extracted as:
```rust
let sentence_type = s[1..].split(',').next().unwrap_or("UNKNOWN").to_string();
```
So `sentence_type` for `$GNGGA,...` is `"GNGGA"` (no `$`). The `raw` string is the full sentence starting with `$`.

When parsing `raw.split(',')`:
- `fields[0]` = `"$GNGGA"`
- `fields[6]` = fix quality
- `fields[7]` = satellites
- `fields[8]` = HDOP

### Atomic Module Pattern (from existing codebase)
```rust
// From src/ntrip_client.rs — the exact pattern to follow:
pub static NTRIP_STATE: AtomicU8 = AtomicU8::new(0);

// Written:
NTRIP_STATE.store(1, Ordering::Relaxed);  // connected
NTRIP_STATE.store(0, Ordering::Relaxed);  // disconnected

// Read (from mqtt.rs heartbeat_loop):
let ntrip_state = crate::ntrip_client::NTRIP_STATE.load(Ordering::Relaxed);
```

### OTA Trigger Payload (existing format)
```json
{"url":"http://192.168.1.100:8080/esp32-gnssmqtt.bin","sha256":"abcdef0123456789..."}
```
Published to: `gnss/FFFEB5/ota/trigger` with `retain=true`.

---

## README Structure

The README is an open-source project document. Based on user requirements, it must cover all features in an approachable but thorough manner. Recommended sections:

```markdown
# esp32-gnssmqtt

## Overview
## Hardware
## Features
  - GNSS Pipeline (NMEA + RTCM3)
  - MQTT Topics (all topics with payload format)
  - NTRIP Corrections
  - OTA Firmware Updates
  - Provisioning / SoftAP
  - Remote Log Relay
  - Health Heartbeat
  - RTCM Relay
## Configuration
  - First-time setup (SoftAP provisioning)
  - MQTT topic configuration
  - NTRIP configuration
## Building and Flashing
## MQTT Topic Reference (table)
## OTA Update Procedure
## LED States
## Troubleshooting
```

The MQTT topic reference table is the most valuable section for operators. Include every topic, direction (pub/sub), payload format, QoS, and retain flag.

---

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| Heartbeat: uptime + drops only | Heartbeat: + fix_type + satellites + hdop | Operators can assess RTK status remotely |
| OTA: code only (Phase 8) | OTA: hardware-validated on FFFEB5 | v2.0 milestone complete |

---

## Open Questions

1. **HDOP JSON representation**
   - What we know: HDOP is stored as `u32` (×10 integer) to avoid `AtomicF32`
   - What's unclear: Should heartbeat emit `"hdop":1.2` (float) or `"hdop":12` (raw integer)?
   - Recommendation: Emit as float string `"hdop":1.2` for human readability. Use `format!("{:.1}", hdop_raw as f32 / 10.0)` in heartbeat. Consistent with how NTRIP state is emitted as human-readable string.

2. **fix_type JSON representation**
   - What we know: NMEA GGA field 6 is an integer (0/1/2/4/5/6)
   - What's unclear: Should JSON include a human-readable label alongside the integer?
   - Recommendation: Emit as integer only (`"fix_type":4`). Operators and consumers can decode from the NMEA spec. Avoids string allocation in the hot heartbeat path.

3. **Captive portal hardware verification (deferred from Phase 17)**
   - What we know: Deferred to end-of-milestone alongside Phase 18 hardware sign-off
   - Recommendation: Plan a combined hardware session: OTA validation + captive portal mobile test together.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | None — ESP32 bare-metal firmware; no automated test runner |
| Config file | none |
| Quick run command | `cargo clippy -- -D warnings` |
| Full suite command | `cargo build --release` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TELEM-01 | Heartbeat JSON contains fix_type, satellites, hdop | manual — MQTT subscribe | `cargo clippy -- -D warnings && cargo build` | ❌ Wave 0: no test file needed; manual MQTT observation |
| MAINT-03 | OTA update completes on device FFFEB5 | hardware — manual | n/a (hardware validation) | ❌ procedural only |

### Sampling Rate
- **Per task commit:** `cargo clippy -- -D warnings`
- **Per wave merge:** `cargo build --release`
- **Phase gate:** Hardware validation of OTA + MQTT heartbeat observation before `/gsd:verify-work`

### Wave 0 Gaps
- No test files required — ESP32 firmware is validated via hardware observation
- `cargo clippy -- -D warnings` is the automated gate per project convention (MEMORY.md)

---

## Sources

### Primary (HIGH confidence)
- NMEA 0183 standard — GGA sentence field layout verified against canonical field index documentation
- `src/gnss.rs` — sentence_type extraction logic (line 233-237): confirmed `sentence_type` excludes `$` prefix
- `src/mqtt.rs` — existing heartbeat JSON format (lines 396-400): confirmed field names and format pattern
- `src/ntrip_client.rs` — `NTRIP_STATE: AtomicU8` pattern (line 37): direct precedent for GGA atomics
- `src/ota.rs` — complete OTA pipeline reviewed: SHA-256 verify, slot mark, retained trigger clear all present
- `src/nmea_relay.rs` — confirmed nmea_relay is the only consumer of nmea_rx; GGA parser can be added here

### Secondary (MEDIUM confidence)
- UM980 NMEA output: GNGGA is expected multi-constellation format; GPGGA possible in GPS-only mode (standard NMEA behavior)

### Tertiary (LOW confidence)
- None

---

## Metadata

**Confidence breakdown:**
- GGA parsing approach: HIGH — sentence format is a decades-old standard; field indices are fixed
- Atomic sharing pattern: HIGH — directly follows NTRIP_STATE pattern already in codebase
- OTA validation procedure: HIGH — pipeline fully implemented and reviewed; procedure is straightforward
- README content scope: HIGH — user explicitly enumerated all required topics

**Research date:** 2026-03-09
**Valid until:** 2026-06-09 (stable — NMEA 0183 GGA format does not change; codebase is pinned)
