# esp32-gnssmqtt

ESP32 firmware in Rust that streams GNSS data (NMEA + RTCM3) to an MQTT broker in real time with remote configuration, OTA updates, and automatic recovery.

## Overview

esp32-gnssmqtt turns an ESP32 development board paired with a UniCore UM980 GNSS receiver into a remote telemetry station. The device continuously reads NMEA sentences and RTCM3 frames from the UM980 and publishes them to an MQTT broker, allowing any subscriber (GIS software, base station controller, custom logger) to receive live position data over a LAN or internet connection.

Version 2.0 adds everything needed for unattended outdoor RTK operation: a web-based provisioning portal for first-time WiFi and MQTT credential entry (no recompile required), an NTRIP client that streams RTK correction data from a caster to the UM980 for centimetre-level accuracy, remote log forwarding so you can diagnose a deployed device without physical access, and OTA firmware updates triggered via a single MQTT publish.

The firmware is designed to run without supervision. Watchdog threads detect WiFi and MQTT outages and trigger timed reboots if connectivity is not restored within the configured window. All critical settings (WiFi credentials, MQTT credentials, NTRIP caster config) are stored in non-volatile storage and survive power cycles.

## Hardware

- ESP32 development board (tested with standard 38-pin devkit)
- UniCore UM980 GNSS receiver connected via UART (3.3 V logic)
- **GPIO9** — SoftAP reset button (hold low for 3 seconds to re-enter provisioning mode)
- **GPIO15** — Status LED (active-low; see [LED States](#led-states))

## Features

- **GNSS Pipeline** — NMEA sentences are published to MQTT one topic per sentence type (GNGGA, GNRMC, etc.); RTCM3 frames are published as binary for external RTK consumers.
- **Health Heartbeat** — JSON status message published every 60 seconds with uptime, free heap, drop counters, NTRIP connection state, and GNSS fix quality (fix type, satellites in use, HDOP).
- **Remote Logging** — All ESP-IDF log output is forwarded to an MQTT topic in real time; the log forwarding threshold is configurable at runtime without a reboot.
- **NTRIP Corrections** — Built-in NTRIP v1 client connects to a caster and streams RTCM3 correction data directly to the UM980 UART, enabling RTK Fixed and RTK Float modes. Settings are persisted to NVS and survive reboots.
- **OTA Firmware Updates** — Triggered via a single MQTT publish containing a URL and SHA-256 hash; the device downloads the binary, verifies the hash, reboots into the new slot, and marks it valid.
- **Provisioning / SoftAP** — On first boot (or on demand), the device broadcasts a `GNSS-Setup` WiFi hotspot with a captive portal web UI for entering WiFi SSIDs and MQTT broker details. No recompile, no serial cable required.
- **Command Relay** — Publish any UM980 command string to MQTT and it is forwarded over UART; useful for changing receiver output rates or resetting the UM980 remotely.
- **Resilience** — Watchdog threads monitor WiFi and MQTT connectivity; automatic reboot on sustained failure; all NVS-stored config survives the reboot.

## MQTT Topic Reference

All topics use `gnss/{device_id}/...` where `device_id` is your device's MAC-derived identifier shown in the boot log (e.g. `gnss/a4cf1234abcd/heartbeat`).

| Topic | Direction | Payload | QoS | Retain | Notes |
|---|---|---|---|---|---|
| `gnss/{device_id}/nmea/{type}` | publish | Raw NMEA sentence (e.g. `$GNGGA,...`) | 0 | false | One topic per sentence type (GNGGA, GNRMC, etc.) |
| `gnss/{device_id}/rtcm` | publish | Raw RTCM3 frame bytes | 0 | false | Binary payload |
| `gnss/{device_id}/heartbeat` | publish | JSON: `uptime_s`, `heap_free`, `nmea_drops`, `rtcm_drops`, `uart_tx_errors`, `ntrip`, `fix_type`, `satellites`, `hdop` | 0 | false | Published every 60 s |
| `gnss/{device_id}/status` | publish | `"online"` on connect; `"offline"` as LWT | 1 | true | Last Will and Testament topic |
| `gnss/{device_id}/log` | publish | Plain text log line | 0 | false | Forwarded from ESP-IDF log |
| `gnss/{device_id}/log/level` | subscribe | `"error"` / `"warn"` / `"info"` / `"debug"` / `"verbose"` | 1 | true | Set log forwarding threshold at runtime |
| `gnss/{device_id}/config` | subscribe | JSON UM980 config command | 1 | true | Forwarded to UM980 UART |
| `gnss/{device_id}/command` | subscribe | Raw UM980 command string | 0 | false | Non-retained; each publish = one command sent |
| `gnss/{device_id}/ntrip/config` | subscribe | JSON: `host`, `port`, `mountpoint`, `username`, `password` | 1 | true | NTRIP caster settings; persisted to NVS |
| `gnss/{device_id}/ota/trigger` | subscribe | JSON: `{"url":"...","sha256":"..."}` or `"reboot"` or `"softap"` | 1 | true | OTA trigger, or reboot / SoftAP command |
| `gnss/{device_id}/ota/status` | publish | `"downloading"` / `"complete"` / `"failed"` | 0 | false | OTA progress |

## First-Time Setup (Provisioning)

1. Flash firmware to the ESP32 (see [Building and Flashing](#building-and-flashing)).
2. On first boot — or when no WiFi credentials are stored in NVS — the device broadcasts a WiFi hotspot called **`GNSS-Setup`** (open network, no password).
3. Connect to `GNSS-Setup` from any phone, tablet, or laptop.
4. A captive portal redirect should open automatically; if it does not, open a browser and navigate to `192.168.4.1`.
5. Fill in up to three WiFi SSIDs and their passwords.
6. Fill in your MQTT broker hostname, port, and optional username/password.
7. Click **Save** — the device stores all credentials in NVS and reboots into station mode.
8. The device connects to WiFi, then to the MQTT broker, and begins publishing GNSS data.

**To re-enter provisioning mode at any time:**
- Hold GPIO9 low for 3 seconds (hardware button), or
- Publish `"softap"` to `gnss/{device_id}/ota/trigger`.

The SoftAP portal automatically shuts down after 300 seconds if no client connects, and the device retries station mode with the stored credentials.

## Building and Flashing

### Prerequisites

- Rust toolchain with the Xtensa target, installed via [espup](https://github.com/esp-rs/espup):
  ```
  cargo install espup
  espup install
  source ~/export-esp.sh   # add to ~/.bashrc or run each session
  ```
- `ldproxy` linker shim:
  ```
  cargo install ldproxy
  ```
- ESP-IDF v5.x (fetched automatically by the `esp-idf-sys` build script on first build; allow extra time).
- `cargo-espflash`:
  ```
  cargo install cargo-espflash
  ```

### First Build

```
git clone https://github.com/bharrisau/esp32-gnssmqtt
cd esp32-gnssmqtt
cargo build --release
```

The first build downloads and compiles ESP-IDF; expect 10–20 minutes. Subsequent builds are incremental.

### Configuration

WiFi and MQTT credentials can be entered via the SoftAP provisioning portal (recommended for deployed devices). For development builds you can also create `src/config.rs` with compile-time defaults — see the docstring at the top of `src/main.rs` for the expected structure. `src/config.rs` is gitignored and will not be committed.

### Flash and Monitor

```
cargo espflash flash --release --monitor
```

This flashes the release binary and opens a serial monitor. The device ID (`gnss/{device_id}`) is printed in the boot log.

## NTRIP Configuration

Publish a **retained** JSON payload to `gnss/{device_id}/ntrip/config`:

```json
{
  "host": "caster.example.com",
  "port": 2101,
  "mountpoint": "MOUNTPOINT",
  "username": "user",
  "password": "pass"
}
```

The device connects immediately and streams RTCM3 corrections to the UM980. On connection loss the client reconnects with exponential backoff. Settings are written to NVS and survive a reboot.

To disconnect from the caster and clear the stored config, publish an **empty payload** to the same topic (this clears the retained message and the NVS entry).

NTRIP connection state is reported in the [heartbeat](#health-heartbeat) (`"connected"` or `"disconnected"`).

## OTA Firmware Update

1. Build the new firmware:
   ```
   cargo build --release
   ```

2. Locate the binary. `cargo-espflash` produces `target/xtensa-esp32-espidf/release/esp32-gnssmqtt` (ELF); convert to a flashable binary if your HTTP server will serve it directly:
   ```
   esptool.py --chip esp32 elf2image target/xtensa-esp32-espidf/release/esp32-gnssmqtt
   ```
   This creates `esp32-gnssmqtt.bin`.

3. Compute the SHA-256 hash:
   ```
   sha256sum esp32-gnssmqtt.bin
   ```

4. Serve the binary over HTTP from the same network as the device:
   ```
   python3 -m http.server 8080
   ```

5. Publish the OTA trigger (retained, QoS 1):
   ```
   Topic:   gnss/{device_id}/ota/trigger
   Payload: {"url":"http://192.168.1.X:8080/esp32-gnssmqtt.bin","sha256":"<hex>"}
   Retain:  true
   ```

6. Monitor `gnss/{device_id}/ota/status` and `gnss/{device_id}/log`. The device downloads the binary, verifies the SHA-256 hash, writes it to the inactive OTA slot, reboots, and marks the new slot valid on successful startup.

7. After the update completes, **clear the retained trigger** by publishing an empty payload to the OTA trigger topic. This prevents re-flashing on the next reconnect.

To trigger a simple reboot (no firmware change), publish `"reboot"` to the OTA trigger topic.

## LED States

The status LED on GPIO15 is active-low. Patterns:

| State | Pattern | Period |
|---|---|---|
| Connecting (WiFi or MQTT) | 200 ms on / 200 ms off — fast blink | 400 ms |
| Connected (MQTT up) | Steady on | — |
| Error | Triple rapid pulse (100 ms on / 100 ms off × 3) then 700 ms off | 1300 ms |
| SoftAP provisioning mode | 500 ms on / 500 ms off — slow blink | 1000 ms |

The four patterns are visually distinct: fast blink (connecting), solid (running), triple pulse (error), and medium blink (SoftAP).

## Health Heartbeat

The device publishes a JSON heartbeat every 60 seconds to `gnss/{device_id}/heartbeat`:

```json
{
  "uptime_s": 3600,
  "heap_free": 180000,
  "nmea_drops": 0,
  "rtcm_drops": 0,
  "uart_tx_errors": 0,
  "ntrip": "connected",
  "fix_type": 4,
  "satellites": 12,
  "hdop": 0.9
}
```

Field reference:

| Field | Type | Description |
|---|---|---|
| `uptime_s` | integer | Seconds since boot |
| `heap_free` | integer | Free heap bytes |
| `nmea_drops` | integer | NMEA sentences dropped (channel full) |
| `rtcm_drops` | integer | RTCM3 frames dropped (channel full) |
| `uart_tx_errors` | integer | Failed UART writes to UM980 |
| `ntrip` | string | `"connected"` or `"disconnected"` |
| `fix_type` | integer or null | GNSS fix quality code (see below); `null` if no GGA received |
| `satellites` | integer or null | Number of satellites in use; `null` if no GGA received |
| `hdop` | float or null | Horizontal dilution of precision (lower = better); `null` if no GGA received |

**fix_type values** (from GGA sentence field 6):

| Value | Meaning |
|---|---|
| 0 | No fix |
| 1 | GPS SPS (standard positioning) |
| 2 | DGPS / differential |
| 4 | RTK Fixed (centimetre-level) |
| 5 | RTK Float (sub-metre) |
| 6 | Estimated (dead reckoning) |

`fix_type`, `satellites`, and `hdop` are `null` when no GGA sentence has been received since boot — this is unambiguous as a value of `0` means a valid "no fix" GGA was received.

## Troubleshooting

**Device not connecting to WiFi**
Check that the stored SSID and password are correct. Use the SoftAP portal to re-enter credentials (hold GPIO9 low for 3 s, or publish `"softap"` to the OTA trigger topic). Monitor the serial output for `[wifi]` log lines.

**MQTT connection fails**
Verify the broker hostname, port, and credentials via the SoftAP portal. Check `gnss/{device_id}/log` for `[mqtt]` error lines if the device is already online on the previous connection.

**OTA download fails with HTTP error**
Check that the HTTP server is reachable from the device's network. If running a local server, allow the port through the firewall:
```
sudo ufw allow 8080
```
Verify that the IP address in the OTA URL matches the machine serving the file.

**NTRIP not connecting**
Verify the caster hostname, port, mountpoint name, and credentials. Check `gnss/{device_id}/log` for `[ntrip]` error lines. Common issues: wrong mountpoint string (case-sensitive), firewall blocking outbound TCP on the caster port, or expired account credentials.

**Device rebooting repeatedly**
Monitor `gnss/{device_id}/log`. Look for watchdog or MQTT disconnect timer messages. Sustained WiFi instability is the most common cause — ensure the device has adequate signal strength. Reducing the distance to the access point or adding a WiFi repeater usually resolves this.

**No GNSS data being published**
Check that the UM980 is powered and that its UART TX is connected to the ESP32 RX pin. The heartbeat `fix_type` field being `null` means no GGA sentence is reaching the firmware; `0` means GGA is arriving but the receiver has no satellite fix yet (normal for the first few minutes outdoors).
