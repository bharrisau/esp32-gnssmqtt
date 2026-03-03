# Feature Research

**Domain:** Embedded GNSS-to-MQTT bridge firmware (ESP32-C6, Rust, UM980, WiFi/BLE)
**Researched:** 2026-03-03
**Confidence:** MEDIUM — no web access available; based on domain knowledge of embedded IoT telemetry, NMEA processing, MQTT protocols, ESP32 provisioning patterns, and Rust embedded ecosystem. Confidence is MEDIUM (not LOW) because these features are well-established across the embedded IoT domain and the project requirements in PROJECT.md align directly with observed industry patterns. Flag for validation before roadmap finalization.

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features the device must have or it is operationally unusable. A GNSS-to-MQTT bridge without any of these either cannot function or is indistinguishable from a broken device.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| UART RX from UM980 at 115200 baud 8N1 | Without this there is no GNSS data — the device does nothing | LOW | Hardware UART peripheral on ESP32-C6; `esp-idf-hal` UART driver. Baud is fixed by the UM980 hardware. |
| NMEA sentence framing and line extraction | Raw bytes from UART must be split into discrete NMEA sentences; half-sentences must not be published | LOW | Read until `\n`, accumulate in a line buffer. Buffer must handle longest NMEA sentence (max 82 chars per NMEA 0183 spec). |
| NMEA sentence type parsing (header extraction) | Topic routing requires knowing sentence type (GNGLL, GPGGA, etc.) before publishing | LOW | Parse `$<TYPE>,` prefix; no need to decode field values in v1. |
| WiFi connection on boot using stored credentials | Device is useless without network connectivity | LOW | `esp-idf-hal` WiFi station mode; connect with SSID + password from NVS. |
| MQTT client connect to broker | Core function of the device | LOW | `esp-idf-mqtt` or `rumqttc`; username + password auth; broker IP/port from NVS. |
| MQTT publish NMEA sentences to typed topics | This is the primary product function | LOW | Publish to `gnss/{device_id}/nmea/{SENTENCE_TYPE}`; QoS 0 is sufficient for real-time telemetry. |
| Persistent credential storage in NVS flash | WiFi SSID/password and MQTT broker/credentials must survive reboots | LOW | ESP32 NVS partition; `esp-idf-sys` NVS API or a Rust NVS wrapper. Credentials are lost if NVS is wiped. |
| BLE provisioning on first boot | Without provisioning, the device cannot receive its credentials and cannot connect to anything | HIGH | ESP-IDF WiFi provisioning component uses BLE transport. This is the highest-complexity table-stakes feature. Requires BLE GATT server, custom provisioning protocol or use of `esp_prov` component. |
| Auto-reconnect for WiFi and MQTT | Real-world networks drop; a device that does not reconnect is useless after the first dropout | MEDIUM | Requires a supervisor loop / state machine. WiFi reconnect events from esp-idf event loop; MQTT reconnect with exponential backoff. |
| Device ID derived from ESP32 hardware MAC/eFuse | Per-device topic namespacing requires unique stable IDs | LOW | `esp_read_mac` or eFuse serial via esp-idf. MAC is factory-burned, guaranteed unique. |
| MQTT subscribe to config topic on connect | Remote GNSS reconfiguration is a stated core value; without it UM980 init is hardcoded | LOW | Subscribe to `gnss/{device_id}/config` with QoS 1 for reliable delivery of retained config. |
| Send received config payload as UART TX to UM980 | Config topic payload must reach the UM980 as UART command bytes | LOW | UART TX write after receiving MQTT config message. Handle multi-command payloads (newline-delimited). |
| Status LED reflecting connectivity state | Without visual feedback, field debugging is impossible; no one can tell if device is running | LOW | Single RGB or multi-color LED; at minimum: provisioning mode, connecting, connected, error. GPIO output, trivial to implement. |
| Heartbeat publish on timer | Without a heartbeat, broker consumers cannot distinguish "no GNSS data" from "device offline" | LOW | Publish to `gnss/{device_id}/heartbeat` with timestamp or uptime; every 30–60 seconds is conventional. |

### Differentiators (Competitive Advantage)

Features that exceed baseline expectations and add real operational value. None are required for the device to function, but several significantly improve reliability and operator experience.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| MQTT Last Will and Testament (LWT) | Broker automatically publishes an "offline" message if device disconnects ungracefully; consumers know device is down without waiting for heartbeat timeout | LOW | Set at MQTT connect time; zero ongoing cost. Use `gnss/{device_id}/status` with payload `online`/`offline`. |
| Per-sentence topic routing (not single topic) | Consumers subscribe only to sentence types they care about (e.g. only `GNGGA` for position); reduces traffic and processing on consumer side | LOW | Already in project design. Routing is trivially derived from NMEA sentence type header — `$GNGGA` → topic suffix `GNGGA`. |
| Web portal fallback provisioning | Covers environments without BLE capability (headless servers, older devices) | MEDIUM | ESP-IDF SoftAP + HTTP server serving a config form. Requires WiFi in AP mode. Lower priority than BLE path but valuable for operators. Already in PROJECT.md requirements. |
| MQTT QoS 1 for config topic subscription | Ensures UM980 initialization commands are delivered exactly-at-least-once; device won't start with wrong or missing config | LOW | QoS 1 on subscribe to config topic only. NMEA telemetry remains QoS 0 (fire-and-forget is appropriate for real-time sensor data). |
| NMEA checksum validation before publish | Filters corrupt UART bytes; prevents publishing malformed sentences that will confuse consumers | LOW | NMEA 0183 checksum is XOR of bytes between `$` and `*`; trivial to compute. Drop sentence if checksum fails. |
| Sentence-type allow/deny filter (config-driven) | Operator can suppress unwanted sentence types (e.g. suppress `GPGSV` satellite info noise) via retained config topic | MEDIUM | Requires parsing a filter list from config topic; apply per-sentence before publish. Not required for v1 but reduces broker load in high-rate GNSS scenarios. |
| MQTT retained publish for heartbeat/status | Broker retains last status; consumers connecting after device reboot immediately know current state | LOW | Set `retain=true` on heartbeat and status publishes. Zero implementation cost beyond a flag. |
| Structured heartbeat payload (JSON with uptime, GNSS fix status) | Richer diagnostics than a bare "alive" pulse; allows monitoring systems to surface device health metrics | LOW | Serialize a small JSON struct: `{"uptime_s": 1234, "fix": true, "satellites": 8}`. Requires GNSS fix state tracking (available from NMEA GGA sentence). |
| NVS partition wipe + re-provisioning via button or MQTT command | Lets operators reset credentials without firmware reflash | LOW | GPIO button hold-to-reset, or subscribe to `gnss/{device_id}/reset` topic. NVS erase via esp-idf NVS API. |
| Panic/error reporting to MQTT | Surfaces firmware panics or error states to the broker for remote diagnostics | MEDIUM | Set up a panic handler that publishes to `gnss/{device_id}/error` before restarting. Requires careful design to avoid infinite panic loops. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem like good ideas but introduce complexity, risk, or scope that is inappropriate for v1 of this firmware.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| TLS/mTLS for MQTT | Security best practice | In v1, adds certificate management complexity (provisioning, rotation, storage) that dwarfs the firmware itself; correct mTLS on embedded targets requires significant NVS/flash management and mbedTLS tuning; out of scope per PROJECT.md | Username + password auth is sufficient for v1 on trusted networks. Add TLS in v2 as a separate milestone with proper certificate provisioning design. |
| Local NMEA buffering across power cycles | "Don't lose data" instinct | The UM980 is a real-time sensor; stale buffered NMEA positions are misleading, not helpful. Flash write amplification from buffering high-rate NMEA (10Hz = 600+ sentences/minute) degrades NVS lifespan. Real-time relay is the correct model. | Accept QoS 0 data loss as the defined behavior; if loss tolerance is needed later, use an SD card with a dedicated logging feature — not NVS. |
| OTA firmware update over MQTT or WiFi | Convenient for field updates | OTA on ESP32 requires dual-partition layout, rollback logic, image verification, and handling partial transfers; adds 20–30% firmware complexity for a v1. Wrong OTA implementation bricks devices in the field. | Defer to a dedicated v2 milestone. Use esptool + USB for v1 updates. |
| Full NMEA sentence parsing (lat/lon decode, field extraction) | "It would be useful to have parsed data" | Firmware's job is relay, not parse; adding a full NMEA parser adds 500–2000 lines of code, edge cases (NMEA variants, proprietary sentences), and test burden. Consumers can parse NMEA trivially in any language. | Publish raw NMEA strings. Consumer-side parsing is the right separation of concerns. |
| Mobile app for provisioning | Better UX than BLE CLI | Building a companion mobile app is a separate product; it is out of scope for firmware v1. BLE provisioning via standard tools (nRF Connect, esp-idf-prov CLI) covers all operator use cases. | BLE GATT server that standard BLE client tools can talk to. |
| Multi-broker publishing | "Publish to multiple endpoints" | Multiple simultaneous MQTT connections multiply state management complexity and memory pressure on a constrained MCU; connection failure handling becomes combinatorial. | One broker per device. If fan-out is needed, implement it at the broker level (MQTT bridge, topic mirroring). |
| GNSS config stored in device NVS (not from broker) | "What if the broker is offline at boot?" | Defeats the stated design goal: remote reconfiguration without reflash. Also creates a dual-source-of-truth problem. | Use MQTT retained config as the single source of truth; on first boot without config, send a safe default init or wait for config before starting NMEA relay. |
| Parsed geofence or alert logic in firmware | "Alert when device leaves an area" | Application logic in firmware: couples sensor relay to a specific use case, makes firmware brittle to changing requirements, and is better done in the MQTT consumer pipeline. | Implement geofence logic in the consumer application subscribing to NMEA topics. |

---

## Feature Dependencies

```
[NVS Credential Storage]
    └──required by──> [WiFi Connection on Boot]
                          └──required by──> [MQTT Client Connect]
                                                └──required by──> [NMEA Publish to Topics]
                                                └──required by──> [Config Topic Subscribe]
                                                └──required by──> [Heartbeat Publish]

[BLE Provisioning]
    └──writes to──> [NVS Credential Storage]

[Web Portal Fallback Provisioning]
    └──writes to──> [NVS Credential Storage]
    └──conflicts with──> [BLE Provisioning] (can't run both simultaneously; need mode-select logic)

[UART RX from UM980]
    └──required by──> [NMEA Sentence Framing]
                          └──required by──> [NMEA Type Parsing]
                                                └──required by──> [NMEA Publish to Topics]

[Config Topic Subscribe]
    └──required by──> [UART TX to UM980 (init commands)]

[MQTT Client Connect]
    └──required by──> [LWT registration] (must be set at connect time, not after)

[NMEA Checksum Validation]
    └──enhances──> [NMEA Publish to Topics] (filter corrupt sentences before publish)

[Auto-Reconnect (WiFi)]
    └──required by──> [Auto-Reconnect (MQTT)] (can't reconnect MQTT without WiFi)

[Device ID (MAC/eFuse)]
    └──required by──> [All topic construction] (topics include {device_id})
```

### Dependency Notes

- **NVS requires provisioning to exist first:** On a freshly flashed device with no credentials, the boot sequence must detect empty NVS and enter provisioning mode rather than attempting a connection that will always fail.
- **Config topic subscription requires WiFi+MQTT to be up:** The UM980 cannot be initialized with remote commands until the full connectivity stack is established. Design implication: device must handle "connected but no config received yet" as a valid transient state.
- **LWT must be registered at MQTT connect time:** LWT is not a post-connect subscription; it must be embedded in the CONNECT packet. This means LWT topic/payload must be known before calling `mqtt_connect`.
- **BLE and WiFi SoftAP provisioning modes conflict:** ESP32-C6 can run BLE and WiFi concurrently but SoftAP provisioning requires WiFi in AP mode. Mode selection logic (BLE primary, web portal fallback) must be explicitly coded — they cannot both be active defaults.
- **Auto-reconnect for MQTT depends on WiFi being up:** The reconnect state machine must be layered: reconnect WiFi first, then reconnect MQTT. A flat "reconnect everything" approach causes MQTT connect attempts against a non-existent network interface.

---

## MVP Definition

### Launch With (v1)

These are the features needed for the device to deliver its core value: reliably relay NMEA to MQTT with zero-touch provisioning.

- [x] **UART RX from UM980 at 115200 baud** — without this, there is no product
- [x] **NMEA sentence framing and line extraction** — prerequisite for all NMEA processing
- [x] **NMEA sentence type parsing (header extraction)** — required for per-topic routing
- [x] **NMEA checksum validation** — prevents corrupt data reaching the broker; LOW complexity, HIGH value
- [x] **Device ID from ESP32 hardware MAC/eFuse** — required for topic namespacing
- [x] **NVS credential storage** — required for persistent WiFi and MQTT config
- [x] **BLE provisioning on first boot** — required for zero-touch field deployment
- [x] **WiFi connection on boot using stored credentials** — core connectivity
- [x] **MQTT client connect with username + password** — core connectivity
- [x] **MQTT publish NMEA to `gnss/{device_id}/nmea/{TYPE}`** — core product function
- [x] **MQTT subscribe to `gnss/{device_id}/config` (QoS 1)** — required for remote UM980 init
- [x] **UART TX to UM980 (send received config payload)** — required for remote UM980 init
- [x] **Auto-reconnect for WiFi and MQTT** — without this the device requires manual power cycle on every network blip
- [x] **Status LED (provisioning / connecting / connected / error)** — required for field diagnosis
- [x] **Heartbeat publish** — required for consumers to detect device presence
- [x] **MQTT LWT for offline status** — differentiator with LOW cost; include in v1
- [x] **MQTT retain flag on heartbeat/status publishes** — zero cost flag; include in v1

### Add After Validation (v1.x)

Features to add once the core relay pipeline is confirmed working in the field.

- [ ] **Web portal fallback provisioning** — add when BLE coverage gaps are reported; already in PROJECT.md requirements but lower priority than BLE path
- [ ] **NVS wipe + re-provisioning trigger (button or MQTT command)** — add when first operator needs to re-provision a deployed device
- [ ] **Structured JSON heartbeat (uptime, fix status, satellite count)** — add when monitoring dashboards are being built and need richer data
- [ ] **Sentence-type allow/deny filter via config** — add when operators report unwanted NMEA noise consuming broker bandwidth

### Future Consideration (v2+)

Features requiring separate design work; premature in v1.

- [ ] **TLS/mTLS** — defer until v2; requires certificate provisioning design, mbedTLS tuning, and NVS space planning
- [ ] **OTA firmware update** — requires dual-partition layout, rollback, image signing; defer to dedicated OTA milestone
- [ ] **Panic/error reporting to MQTT** — useful but requires careful panic handler design to avoid boot loops; defer until v1 is stable
- [ ] **Sentence-type filtering (advanced, regex-based)** — defer until simple allow/deny filter is validated as insufficient

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| UART RX + NMEA framing + type parse | HIGH | LOW | P1 |
| NMEA checksum validation | HIGH | LOW | P1 |
| NVS credential storage | HIGH | LOW | P1 |
| BLE provisioning | HIGH | HIGH | P1 |
| WiFi + MQTT connect | HIGH | LOW | P1 |
| NMEA publish to per-type topics | HIGH | LOW | P1 |
| Config topic subscribe + UART TX to UM980 | HIGH | LOW | P1 |
| Auto-reconnect (WiFi + MQTT) | HIGH | MEDIUM | P1 |
| Device ID from MAC/eFuse | HIGH | LOW | P1 |
| Status LED | MEDIUM | LOW | P1 |
| Heartbeat publish | MEDIUM | LOW | P1 |
| MQTT LWT (offline status) | MEDIUM | LOW | P1 — free capability, include at connect time |
| Retain flag on status/heartbeat | LOW | LOW | P1 — zero-cost flag |
| Web portal fallback provisioning | MEDIUM | MEDIUM | P2 |
| NVS wipe / re-provisioning trigger | MEDIUM | LOW | P2 |
| Structured JSON heartbeat | LOW | LOW | P2 |
| Sentence-type filter (config-driven) | LOW | MEDIUM | P2 |
| Panic/error MQTT reporting | MEDIUM | MEDIUM | P3 |
| TLS/mTLS | HIGH | HIGH | P3 — correct design requires separate milestone |
| OTA firmware update | HIGH | HIGH | P3 — correct design requires separate milestone |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

---

## Competitor Feature Analysis

Direct "competitors" for this firmware are open-source GNSS-to-MQTT bridge implementations (e.g. gpsd+MQTT adapters, Python-on-Pi solutions, SparkFun/Adafruit GPS+MQTT demos) and commercial GPS tracking middleware.

| Feature | gpsd + MQTT adapter (Pi/Linux) | Commercial GPS tracker (SIM7xxx) | This project |
|---------|-------------------------------|----------------------------------|--------------|
| NMEA relay to MQTT | Via gpsd daemon, sentence parsing, re-serialization | Proprietary binary or NMEA over cellular MQTT | Raw NMEA sentence relay, no intermediate parse |
| Per-sentence topic routing | Rarely; usually single topic or JSON-wrapped | No — flat topic or binary protocol | Yes — `gnss/{id}/nmea/{TYPE}` per sentence type |
| Remote GNSS module config | No — gpsd has its own config layer | Limited — AT commands via SIM module | Yes — retained MQTT config topic → UART passthrough |
| Zero-touch provisioning | Not applicable (Linux has SSH) | QR code / cellular APN config | BLE provisioning with web portal fallback |
| Auto-reconnect | OS networking handles it | Cellular modem retries | Explicit reconnect state machine in firmware |
| Heartbeat / LWT | Manual implementation | Varies | Built-in heartbeat + MQTT LWT |
| TLS | Yes (OS-managed) | Often yes (cellular TLS) | Deferred to v2 |
| OTA | apt-get / SCP | Cellular OTA | Deferred to v2 |
| Rust embedded, no Linux | No | No | Yes — key differentiator for resource-constrained deployment |

**Key insight:** The main competitive differentiator vs. Linux-based solutions is the constrained, deterministic Rust firmware with sub-second boot time and zero OS overhead. The main differentiator vs. cellular trackers is the RTK-grade UM980 GNSS capability and WiFi connectivity model.

---

## Sources

- Project requirements: `.planning/PROJECT.md` (authoritative for this project's scope)
- NMEA 0183 standard: IEC 61162-1 — sentence format, max length (82 chars), checksum algorithm (XOR between `$` and `*`)
- ESP32-C6 ESP-IDF provisioning: Espressif official docs (WiFi provisioning component, BLE transport, NVS API) — MEDIUM confidence from training data; verify against ESP-IDF v5.x docs
- MQTT 3.1.1 specification (OASIS) — LWT (Section 3.1.3.4), retained messages (Section 3.3.1.3), QoS levels — HIGH confidence, well-established standard
- Embedded Rust ESP32 ecosystem: `esp-idf-hal`, `esp-idf-sys`, `esp-idf-svc` crates — MEDIUM confidence from training data; verify against current crate versions on crates.io
- IoT firmware reconnection patterns: well-established pattern in ESP-IDF example code and embedded IoT literature — MEDIUM confidence
- UM980 UART interface: manufacturer datasheet (Unicore Communications UM980) — behavior assumed from project context; verify baud rate and command format against actual datasheet

---

*Feature research for: ESP32-C6 GNSS-to-MQTT bridge firmware (Rust)*
*Researched: 2026-03-03*
