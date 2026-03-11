# Phase 20: Field testing fixes — Context

**Gathered:** 2026-03-11
**Status:** Ready for planning
**Source:** User field testing session + gnss.log analysis

<domain>
## Phase Boundary

Fix bugs found during first field deployment session on device FFFEB5. Four areas: (1) Windows captive portal not triggering, (2) MQTT throughput too low for high-rate GNSS output, (3) UM980 reset config re-apply deferred from Phase 17, (4) AUSCORS NTRIP requires HTTPS/443 — evaluate feasibility or design MQTT relay path. Also includes any remaining outstanding bugs from testing.md.

</domain>

<decisions>
## Implementation Decisions

### Windows Captive Portal (BUG-5)
- Windows 10/11 probes `http://www.msftconnecttest.com/connecttest.txt` and expects exact response `Microsoft Connect Test`
- Windows also probes `http://www.msftncsi.com/ncsi.txt` (older versions) expecting `Microsoft NCSI`
- Portal must respond to these URLs with the correct body (200 OK, no redirect) OR redirect to the portal page
- DNS hijack (BUG-1) must be fixed first — Windows also relies on DNS resolution
- BUG-1 fix (EspNetif pre-configured DNS) is already merged; need to verify it works with Windows probe flow
- iOS detection: `http://captive.apple.com/hotspot-detect.html` expects `<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>`
- Add handlers for all three platforms: Windows msftconnecttest, Windows msftncsi, iOS captive.apple.com

### MQTT Throughput (PERF-1)
- gnss.log shows GGA every 10 seconds (config `GPGGA 10`) — RTCM also at 10s intervals
- User wants to run at 5 Hz GNSS output → need MQTT pipeline capable of sustaining at least that rate
- Target: 100 msg/s peak (covers 5 Hz GNSS × multiple sentence types + RTCM bursts)
- Current architecture: nmea_relay → channel → mqtt task (sequential publish per message)
- Investigate: is `client.publish()` blocking? What is channel capacity? Any artificial delay?
- Do NOT redesign architecture speculatively — profile and fix the specific bottleneck
- esp-idf MQTT client has its own internal queue; large messages (RTCM 148 bytes) may stall if queue full
- Consider: increase MQTT outbox size via `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS` / `CONFIG_MQTT_OUTBOX_SIZE`
- If publish is blocking: switch to non-blocking enqueue + error on full; or increase MQTT task priority
- The user will need to update the UM980 config to `GPGGA 0.2` (5 Hz) to test; firmware change may not be needed

### UM980 Config Re-apply on Reset (FEAT-2)
- From Phase 17-03 deferred decision: "UM980 reboot monitor uses warning fallback — NVS-backed gnss config re-apply deferred"
- When UM980 reboots (detected via `um980_reboot` channel), need to re-send the last known config
- Config arrives over MQTT `/config` topic as `{"delay_ms": N, "commands": [...]}` — store this in NVS ("gnss" namespace)
- On UM980 reboot detection: read config from NVS, re-send commands via `gnss_cmd_tx`
- If no NVS config exists (first boot): skip silently — default UM980 state is acceptable
- NVS key: `gnss_config` as a JSON string (or individual keys) — JSON string simpler, fits in NVS blob
- Config size: the sample config is 117 bytes — fits in NVS value (max 4000 bytes for blob)
- Use same namespace as GNSS config or create new "gnss" namespace

### AUSCORS HTTPS NTRIP (FEAT-3)
- AUSCORS (Australian CORS network) uses port 443 with TLS for NTRIP connections
- Current ntrip_client.rs uses plain TCP (TcpStream / EspTcp)
- TLS NTRIP feasibility on ESP32:
  - `esp-idf-svc` has `EspTls` (wraps esp_tls) supporting client TLS — likely usable
  - AUSCORS uses a real certificate (Let's Encrypt or similar) — standard CA validation possible
  - mbedTLS is already compiled in for MQTT TLS — same stack can serve NTRIP
  - Risk: heap usage — mbedTLS session ~50-100KB, current heap_free ~125KB. May be tight.
  - Alternative: MQTT relay — server subscribes AUSCORS NTRIP over TLS, pushes RTCM to device over MQTT
- Decision: **Investigate TLS NTRIP first**. If heap is insufficient → document MQTT relay path as the recommended approach and add to backlog. Do NOT implement both — pick one.
- The NTRIP config UI in the portal already has host/port/user/pass — just change port default to 443 and add TLS flag in NVS

### Outstanding Bugs from testing.md
- BUG-1: DNS override fix already landed (Phase 19-01); verify it works end-to-end with Windows
- BUG-2: Android captive portal — blocked by BUG-1; retest after BUG-1 confirmed working
- BUG-3/BUG-4: NVS versioning — already fixed (Phase 19-02); closed
- FEAT-1: Boot button rework — already done (Phase 19-03); closed
- POST-1: espflash OTA boot selection — post-v2.0 backlog; do not include in this phase

### Claude's Discretion
- Specific NVS key names for gnss config storage
- How to structure the MQTT relay path documentation (if TLS NTRIP not feasible)
- Which platform detection URLs to handle (at minimum: Windows msftconnecttest, iOS captive.apple.com)
- Channel size tuning values if MQTT throughput fix requires them
- Whether to add a portal UI TLS toggle for NTRIP or just NVS field

</decisions>

<specifics>
## Specific Ideas

**gnss.log analysis:**
- GGA timestamps: 094302, 094312, 094322... → 10 second intervals (matches `GPGGA 10` config)
- RTCM 1006/1033/1074 each at 10s — all defined in stored config
- nmea_drops=0, rtcm_drops=0 at current rate — no drops at this output rate
- heap_free=125132 constant — no memory leak, but ~125KB available for TLS session
- fix_type=7 in UM980 terms — check what value 7 means in UM980 GGA quality field
- ntrip=disconnected throughout (expected, no creds configured)

**Windows captive portal probes:**
- `GET /connecttest.txt HTTP/1.1\r\nHost: www.msftconnecttest.com\r\n...` → must return `Microsoft Connect Test` (exact, no newline)
- `GET /ncsi.txt HTTP/1.1\r\nHost: www.msftncsi.com\r\n...` → must return `Microsoft NCSI`
- Windows also checks for `http://ipv6.msftconnecttest.com/connecttest.txt` (IPv6, not relevant here)
- After getting 200 with correct body, Windows marks as "Internet access" (not just "connected")
- Captive portal interception on Windows: redirect unknown hosts to portal (DNS hijack already handles this)

**MQTT throughput investigation targets:**
- `src/mqtt.rs`: nmea publish loop — is it blocking per message?
- `src/nmea_relay.rs`: channel capacity (current: check code)
- `src/log_relay.rs`: log channel capacity 128 (known)
- `CONFIG_MQTT_OUTBOX_SIZE`: check sdkconfig.defaults
- UM980 at 5Hz produces ~5 NMEA sentences/s + RTCM bursts — need sustained ~10-20 msg/s at minimum

**AUSCORS NTRIP TLS:**
- AUSCORS host: `auscors.ga.gov.au` port 2101 historically, now `ntrip.data.gnss.ga.gov.au` port 443
- `esp-idf-svc` EspTls type: check if it provides read/write compatible with current ntrip_client.rs TcpStream usage
- Alternative ntrip crate or raw socket: not preferred, stay with esp-idf-svc patterns

</specifics>

<deferred>
## Deferred Ideas

- POST-1: espflash OTA boot selection fix — post-v2.0 backlog, not in this phase
- BLE provisioning — already deferred to post-v2.0
- MQTT relay path for NTRIP if TLS on device proves infeasible — document as design decision, implementation is server-side (out of scope for firmware)
- IPv6 captive portal support — not needed for field deployment

</deferred>

---

*Phase: 20-field-testing-fixes*
*Context gathered: 2026-03-11 from user field testing session*
