# Phase 20: Field Testing Fixes — Research

**Researched:** 2026-03-11
**Domain:** ESP32 firmware (Rust/esp-idf-svc): captive portal probes, MQTT throughput, NVS-backed GNSS config, TLS NTRIP
**Confidence:** HIGH (source code analysis) / MEDIUM (heap feasibility for TLS)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Windows captive portal: respond to `/connecttest.txt` with exact body `Microsoft Connect Test` (200 OK) and `/ncsi.txt` with `Microsoft NCSI`; iOS `/hotspot-detect.html` with exact Apple HTML; all as 200 OK, not redirect
- MQTT throughput: do NOT redesign architecture speculatively — profile and fix the specific bottleneck
- UM980 config re-apply: NVS key `gnss_config` as JSON blob in "gnss" namespace; on UM980 reboot signal re-send via `gnss_cmd_tx`
- AUSCORS NTRIP: investigate TLS first; if heap insufficient → document MQTT relay path and add to backlog, do NOT implement both
- BUG-1 DNS fix: already landed (Phase 19-01); verify working end-to-end
- BUG-2 Android captive portal: retest after BUG-1 confirmed
- BUG-3/BUG-4: already fixed (Phase 19-02); closed
- FEAT-1 boot button: already done (Phase 19-03); closed
- POST-1: out of scope

### Claude's Discretion
- Specific NVS key names for gnss config storage
- How to structure the MQTT relay path documentation (if TLS NTRIP not feasible)
- Which platform detection URLs to handle (at minimum: Windows msftconnecttest, iOS captive.apple.com)
- Channel size tuning values if MQTT throughput fix requires them
- Whether to add a portal UI TLS toggle for NTRIP or just NVS field

### Deferred Ideas (OUT OF SCOPE)
- POST-1: espflash OTA boot selection — post-v2.0 backlog
- BLE provisioning — post-v2.0
- MQTT relay path implementation (server-side, out of firmware scope)
- IPv6 captive portal support
</user_constraints>

---

## Summary

Phase 20 fixes four issues found during field deployment of device FFFEB5. Two are additions to existing modules (`provisioning.rs` for Windows/iOS probe handlers, `ntrip_client.rs` for optional TLS), one is a new NVS persistence feature (`config_relay.rs` + new NVS blob for GNSS config re-apply on UM980 reset), and one is a throughput investigation with targeted fixes.

The NMEA relay and MQTT pipeline are already non-blocking (`enqueue()` not `publish()`), and the NMEA channel capacity is 64 slots. The likely bottleneck is mutex contention on `Arc<Mutex<EspMqttClient>>` — every NMEA sentence acquires the mutex per sentence. At 5 Hz × multiple sentence types this should still be manageable, but the investigation should confirm whether `enqueue()` returns quickly or blocks internally when the outbox fills.

`EspTls` is available in `esp-idf-svc 0.51.0` as `esp_idf_svc::tls::EspTls<InternalSocket>` with `connect(host, port, &Config)` — API verified from source. The key risk is heap: mbedTLS needs ~50–80 KB for a TLS session; current heap_free is 125 KB. This is borderline and must be tested before committing to the implementation path.

**Primary recommendation:** Implement the four fixes as separate plans: (1) Windows/iOS captive probe handlers, (2) MQTT throughput investigation + targeted fix, (3) UM980 NVS config re-apply, (4) TLS NTRIP feasibility probe then either implement or document the MQTT relay path.

---

## Standard Stack

### Core (all already in use)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| esp-idf-svc | 0.51.0 (pinned) | WiFi, MQTT, TLS, HTTP server, NVS | Project's existing abstraction layer |
| embedded-svc | 0.28.1 (pinned) | HTTP, MQTT traits | Matches esp-idf-svc version |
| esp-idf-hal | 0.45.2 (pinned) | UART, GPIO drivers | Same |

### New APIs Required This Phase

| API | Location | Notes |
|-----|----------|-------|
| `EspTls::<InternalSocket>::new()` | `esp_idf_svc::tls` | Gate-controlled by cfg flags; available when mbedTLS enabled |
| `tls::Config { use_crt_bundle_attach: true, .. }` | `esp_idf_svc::tls` | Uses embedded CA bundle — no manual cert needed |
| `EspNvs::set_blob` / `get_blob` | `esp_idf_svc::nvs` | For gnss_config JSON blob storage |
| `MqttClientConfiguration { task_prio, buffer_size }` | `esp_idf_svc::mqtt::client` | Throughput tuning if needed |

### No New Cargo Dependencies Required

All needed functionality is in `esp-idf-svc 0.51.0`. No new crates needed.

---

## Architecture Patterns

### Pattern 1: Windows/iOS Captive Portal Probe Handlers (BUG-5)

**What:** Add exact-response handlers for OS-specific captive portal detection URLs in `provisioning.rs` `run_softap_portal()`.

**Current state of `provisioning.rs`:**
- Already handles Android `/generate_204` (302 redirect), `/connectivitycheck` (302 redirect)
- iOS `/hotspot-detect.html` — WRONG: currently returns `redirect_html` (meta-refresh), should return exact Apple HTML
- iOS `/success.html` — same issue
- Windows `/ncsi.txt` — currently returns `redirect_html`, should return exact `Microsoft NCSI` text

**Required changes:**
- Windows `/connecttest.txt` — new handler, must return `Microsoft Connect Test` (exact, 200 OK, no trailing newline)
- Windows `/ncsi.txt` — change body from `redirect_html` to exact `Microsoft NCSI` text (200 OK)
- iOS `/hotspot-detect.html` — change body to exact Apple success HTML (200 OK)

**Windows probe flow (locked decision):**
- Windows 10/11 probes `GET /connecttest.txt` with `Host: www.msftconnecttest.com`
- Expects response `200 OK` with body exactly `Microsoft Connect Test`
- Also probes `GET /ncsi.txt` with `Host: www.msftncsi.com` (older Windows versions)
- Expects body exactly `Microsoft NCSI`
- DNS hijack resolves both hostnames to 192.168.71.1 (BUG-1 fix already in place)
- If exact body is returned, Windows marks as "Internet access" and triggers captive portal prompt

**iOS probe flow (locked decision):**
- iOS probes `GET /hotspot-detect.html` with `Host: captive.apple.com`
- Expects exact body: `<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>`
- Must be 200 OK, not a redirect — meta-refresh redirect causes iOS to miss detection

**Handler pattern from existing code:**
```rust
// Source: src/provisioning.rs existing handlers
server.fn_handler("/connecttest.txt", Method::Get, |req| {
    req.into_ok_response()?.write_all(b"Microsoft Connect Test")
})?;
```

**Why exact response, not redirect:**
- Windows: redirect causes probe to follow redirect chain, then recheck on next probe cycle — inconsistent
- iOS: meta-refresh causes page load to "succeed" without triggering captive portal notification
- Both OSes expect the probe to either succeed (correct body) or fail (wrong body / no route) — redirect is ambiguous

### Pattern 2: MQTT Throughput Investigation (PERF-1)

**Current architecture (from code):**
- GNSS RX thread → `nmea_tx.try_send()` → nmea channel (capacity 64) → `nmea_rx`
- NMEA relay thread: `recv_timeout(5s)` → `client.lock()` → `c.enqueue()` → releases lock
- `enqueue()` is non-blocking (adds to MQTT outbox without waiting for PUBACK)
- Mutex acquired per sentence, released before next iteration — heartbeat/subscriber not starved

**Throughput bottleneck candidates (in probability order):**

1. **Mutex contention:** At 5 Hz × 5–8 sentence types = 25–40 enqueue calls/second. Each call acquires `Arc<Mutex<EspMqttClient>>`. The MQTT internal C task also holds this mutex during processing. Contention is likely benign at 5 Hz but worth confirming.

2. **MQTT outbox size:** `out_buffer_size: 2048` is set in `mqtt_connect`. This is the outgoing TCP send buffer size, NOT the outbox message count. Outbox message count is controlled by ESP-IDF Kconfig `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS` (default: 30s before discard). No outbox COUNT limit is exposed via Kconfig — the outbox uses a linked list limited only by heap.

3. **enqueue() internal blocking:** `enqueue()` maps to `esp_mqtt_client_enqueue()` with `store=1`, which adds to the outbox without waiting for the network. If the outbox is too large, memory pressure could slow allocation. At QoS 0, messages are not stored in outbox after send — outbox drain should be fast.

4. **NMEA channel capacity:** Currently 64 slots (set in `gnss.rs` line 150). At 5 Hz × 8 sentences = 40/sec. With 5s timeout in relay, 200 sentences could queue. 64 is the limit before drops. This may need to increase to 128 if GNSS rate increases to 5 Hz.

**Investigation plan (from locked decision: profile first):**
- Add timing log around `client.lock()` + `c.enqueue()` in `nmea_relay.rs` to detect mutex contention
- Check if `nmea_drops` counter increments at 5 Hz rate
- Check heap_free trend (shrinking = outbox backlog)

**Targeted fixes (if investigation reveals specific bottleneck):**
- If channel fills: increase nmea channel capacity from 64 to 128 in `gnss.rs`
- If mutex contention: increase MQTT task priority via `MqttClientConfiguration { task_prio: 6, .. }` (default is 5)
- If outbox timeout: add `CONFIG_MQTT_OUTBOX_EXPIRED_TIMEOUT_MS=5000` to `sdkconfig.defaults`

**Note:** `MqttClientConfiguration` fields `task_prio` and `task_stack` are set to 0 in the current code (default). Setting `task_prio: 6` bumps the MQTT C task above default FreeRTOS priority.

### Pattern 3: UM980 NVS Config Re-apply (FEAT-2)

**Current state:**
- `config_relay.rs` receives MQTT `/config` payloads, applies to UM980 via `gnss_cmd_tx`
- `config_relay.rs` hash-deduplicates but does NOT persist to NVS
- `main.rs` UM980 reboot monitor: detects `$devicename` banner, logs warning, does nothing else
- Payload format: `{"delay_ms": N, "commands": ["CMD1", ...]}`

**Required change:**
1. `config_relay.rs` `apply_config()`: after successful dispatch, save raw payload bytes to NVS blob key `gnss_config` in namespace `"gnss"`
2. `main.rs` UM980 reboot monitor: on signal, read NVS blob, if present re-apply via `gnss_cmd_tx` using same `apply_config()` logic

**NVS blob pattern (no existing blob usage in project — establish pattern):**
```rust
// Source: esp-idf-svc NVS API (verified from codebase)
// Namespace "gnss", key "gnss_config"
// Store:
let mut nvs = EspNvs::new(nvs_partition.clone(), "gnss", true)?;
nvs.set_blob("gnss_config", payload)?;

// Load:
let nvs = EspNvs::new(nvs_partition.clone(), "gnss", false)?;
let mut buf = vec![0u8; 512]; // sample config is 117 bytes; 512 is safe headroom
if let Ok(Some(size)) = nvs.get_blob("gnss_config", &mut buf) {
    // buf[..size] contains the saved JSON
}
```

**NVS key name choice (Claude's discretion):** `gnss_config` in namespace `"gnss"` — aligns with CONTEXT.md, avoids collision with `"ntrip"` and `"prov"` namespaces.

**Max NVS blob size:** 4000 bytes (ESP-IDF NVS limit). Sample config is 117 bytes. No size concern.

**Thread safety consideration:**
- `config_relay.rs` has a `gnss_cmd_tx: SyncSender<String>` but no NVS partition
- Need to pass `nvs_partition: EspNvsPartition<NvsDefault>` clone into `spawn_config_relay`
- `EspNvsPartition<NvsDefault>` implements `Clone` cheaply (reference-counted)
- UM980 reboot monitor in `main.rs` also needs NVS access — already has `nvs` in scope

**Re-apply flow:**
```
UM980 resets → $devicename banner → gnss.rs signals um980_reboot_tx
→ main.rs UM980 reboot monitor wakes → 500ms wait → read NVS gnss_config
→ if Some: call apply_config() via gnss_cmd_tx
→ if None: log info "no GNSS config saved, skipping re-apply"
```

**Key: apply_config() must be accessible from both config_relay thread and main.rs reboot monitor thread.** Options:
- Make `apply_config()` pub in `config_relay.rs` and call from main.rs
- Or move gnss config NVS save into `config_relay.rs`, add a re-apply channel or pass `nvs` clone

Cleanest approach: pass `nvs_partition` clone into `spawn_config_relay`, save on each new config. Make the UM980 reboot monitor read NVS directly in `main.rs` and call `config_relay::apply_config()` (make it pub).

### Pattern 4: TLS NTRIP for AUSCORS (FEAT-3)

**EspTls API (verified from source `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/tls.rs`):**

```rust
// Source: esp-idf-svc-0.51.0/src/tls.rs + examples/tls.rs
use esp_idf_svc::tls::{EspTls, Config as TlsConfig};

let mut tls = EspTls::new()?;  // allocates esp_tls context
tls.connect(
    "ntrip.data.gnss.ga.gov.au",
    443,
    &TlsConfig {
        use_crt_bundle_attach: true,  // uses embedded Mozilla CA bundle — no manual cert
        common_name: Some("ntrip.data.gnss.ga.gov.au"),
        ..TlsConfig::new()
    },
)?;
// After connect: tls.read(&mut buf) and tls.write(data) replace TcpStream::read/write
```

**cfg guard required:** `EspTls` is only compiled when:
- `esp_idf_comp_esp_tls_enabled` AND
- `esp_idf_esp_tls_using_mbedtls` OR `esp_idf_esp_tls_using_wolfssl`

These are set by ESP-IDF Kconfig. mbedTLS is already compiled in for MQTT TLS (the `mqtts://` path). The cfg guards are compile-time, not runtime.

**`use_crt_bundle_attach: true`** uses `esp_crt_bundle_attach` — the bundled Mozilla CA certificates. This is the correct approach for AUSCORS (`ntrip.data.gnss.ga.gov.au`) which uses a real publicly-trusted certificate. Requires `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE=y` in sdkconfig (likely already set since MQTT TLS works).

**Interface difference from TcpStream:**
- `TcpStream::read` → `EspTls::read` (signature identical: `&mut buf` → `usize`)
- `TcpStream::write_all` → `EspTls::write_all` (available)
- `TcpStream::set_read_timeout` → NOT available on EspTls. Timeout is set via `TlsConfig { timeout_ms: 60000, .. }` (connection timeout, not per-read timeout). Read timeouts handled via the underlying socket.

**Heap feasibility:**
- heap_free from gnss.log: 125132 bytes (~122 KB)
- mbedTLS session for TLS 1.2 RSA: ~50–80 KB heap (MEDIUM confidence — depends on cert chain, cipher suite)
- MQTT TLS (if enabled) uses its own mbedTLS session when connecting mqtts://
- For this deployment, MQTT uses plain TCP (tls=false per NVS config) — so no existing TLS session
- Therefore: 125 KB available, ~50–80 KB needed → 45–75 KB remaining after TLS NTRIP session
- Assessment: BORDERLINE. If mbedTLS needs more than 80 KB, we'll see ESP_ERR_NO_MEM on `EspTls::new()`
- Decision protocol: attempt `EspTls::new()` + connect; if fails with NO_MEM → document MQTT relay path

**Adaptation to ntrip_client.rs:**
- `run_ntrip_session` uses `TcpStream` — replace with `EspTls<InternalSocket>` when `config.tls == true`
- Add `tls: bool` field to `NtripConfig`
- NVS key: `ntrip_tls` as `u8` (0 = plain, 1 = TLS) — analogous to `mqtt_tls`
- Portal UI: add `tls` checkbox to NTRIP config form (Claude's discretion — yes, include it)

**Read timeout on EspTls:**
- `TlsConfig::timeout_ms` is the connection handshake timeout (default 4000ms)
- After connection, reads are blocking with no per-read timeout unless set on the underlying socket
- For the session loop, the 60s read timeout from `TcpStream::set_read_timeout()` must be replicated
- Use `ntrip_config_rx.recv_timeout()` as a control-plane check between reads (already done)
- Alternative: set socket-level timeout via `setsockopt` on `tls.context_handle()` → lower-level, possible but complex
- Simpler: rely on NTRIP server NMEA keep-alive interval — AUSCORS sends RTCM data continuously; silence = connection drop; read will eventually return error

**NTRIP request over TLS:**
- Same HTTP/1.0 GET request format as plain TCP
- AUSCORS may require HTTP/1.1 (Host header mandatory for virtual hosts on port 443)
- Current `build_ntrip_request()` uses `HTTP/1.0` — may need to change to `HTTP/1.1` with `Connection: close` for TLS NTRIP
- AUSCORS specifically: check if `ICY 200 OK` or `HTTP/1.1 200 OK` is expected in response

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TLS session management | Custom mbedTLS bindings | `EspTls<InternalSocket>` from `esp_idf_svc::tls` | Handles session lifecycle, ALPN, cert validation |
| CA certificate validation | Embedded cert PEM | `use_crt_bundle_attach: true` | ESP-IDF bundles Mozilla CA store; already compiled in for MQTT |
| Windows probe detection | Parse Host header | Exact path matching (`/connecttest.txt`) | DNS hijack means all hostnames resolve to portal — path matching is sufficient |

---

## Common Pitfalls

### Pitfall 1: iOS hotspot-detect.html redirect vs exact response
**What goes wrong:** Current code returns `redirect_html` (meta-refresh 200 OK) for `/hotspot-detect.html`. iOS expects the exact `<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>` response body to mark the probe as successful. Any redirect causes iOS captive detection to fail silently.
**How to avoid:** Return exact static bytes as 200 OK. No trailing newline, no DOCTYPE prefix.

### Pitfall 2: Windows connecttest.txt host vs path
**What goes wrong:** Thinking Windows uses a special port or HTTPS. Windows 10/11 uses plain HTTP on port 80, relying on DNS hijack to redirect `www.msftconnecttest.com` to the portal IP.
**How to avoid:** Register handler on path `/connecttest.txt`. DNS hijack already resolves the hostname. HTTP server on port 80 serves the response.

### Pitfall 3: EspTls read timeout
**What goes wrong:** `EspTls` does not expose `set_read_timeout()` like `TcpStream`. If NTRIP caster goes silent, the read loop blocks indefinitely.
**How to avoid:** Use `TlsConfig { timeout_ms }` for connection timeout. For session-level read timeout, accept that NTRIP caster silence will eventually cause TCP keepalive failure, OR use a non-blocking poll pattern. The current architecture periodically checks `ntrip_config_rx.try_recv()` — this doesn't help if `tls.read()` blocks. Plan must address this explicitly.
**Recommendation:** Use `EspTls` connection with `non_block: false` for TLS handshake; then use a select/poll on the underlying socket fd for read timeout. `tls.context_handle()` returns `*mut esp_tls`, from which the socket fd can be extracted via `esp_tls_get_conn_sockfd`. Alternatively: accept the risk for MVP since AUSCORS sends RTCM continuously — silence means disconnection which TCP will detect via RST/FIN.

### Pitfall 4: NVS blob key character limit
**What goes wrong:** NVS key names are limited to 15 characters. `gnss_config` = 11 characters. Safe.
**How to avoid:** Count key name length. `ntrip_tls` = 9 chars (fine).

### Pitfall 5: config_relay.rs apply_config not pub
**What goes wrong:** `apply_config()` in `config_relay.rs` is currently `fn` (private). The UM980 reboot monitor in `main.rs` needs to call it.
**How to avoid:** Make `apply_config()` `pub` in `config_relay.rs`, or extract the NVS read + dispatch logic into `main.rs` directly (simpler given the monitor thread is already in main.rs).

### Pitfall 6: EspTls cfg guard compilation failure
**What goes wrong:** If `esp_idf_comp_esp_tls_enabled` or `esp_idf_esp_tls_using_mbedtls` cfg flags are not set, `use esp_idf_svc::tls::EspTls` will fail to compile (the module's contents are conditionally compiled).
**How to avoid:** Check that `CONFIG_ESP_TLS_USING_MBEDTLS=y` is in sdkconfig. Since MQTT TLS (`mqtts://`) already works, mbedTLS is compiled in — this is safe to assume but should be verified by checking the generated cfg flags or a test build.

### Pitfall 7: NTRIP over TLS — server response format
**What goes wrong:** AUSCORS port 443 may return `HTTP/1.1 200 OK` instead of `ICY 200 OK`. Current `read_ntrip_headers()` only accepts `ICY 200 OK`.
**How to avoid:** Update `read_ntrip_headers()` to accept either `ICY 200 OK` or `HTTP/1.1 200 OK` when TLS is active. NTRIP v2 over HTTPS uses standard HTTP responses.

---

## Code Examples

### Windows captive probe handler
```rust
// Source: Based on existing provisioning.rs handler pattern + Windows NCSI spec
// Returns exact probe response required by Windows 10/11 captive portal detection
server.fn_handler("/connecttest.txt", Method::Get, |req| {
    req.into_ok_response()?.write_all(b"Microsoft Connect Test")
})?;
server.fn_handler("/ncsi.txt", Method::Get, |req| {
    req.into_ok_response()?.write_all(b"Microsoft NCSI")
})?;
```

### iOS captive probe handler (fix existing)
```rust
// Source: Based on provisioning.rs existing handler + Apple captive portal spec
// Must return exact HTML, not redirect, for iOS to trigger captive portal notification
const IOS_SUCCESS_HTML: &[u8] =
    b"<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>";

server.fn_handler("/hotspot-detect.html", Method::Get, move |req| {
    req.into_ok_response()?.write_all(IOS_SUCCESS_HTML)
})?;
```

### NVS blob save (gnss config)
```rust
// Source: esp-idf-svc NVS API pattern (consistent with existing provisioning.rs NVS usage)
// Save raw config JSON payload to NVS for re-apply on UM980 reboot
fn save_gnss_config(payload: &[u8], nvs_partition: &EspNvsPartition<NvsDefault>) {
    match EspNvs::new(nvs_partition.clone(), "gnss", true) {
        Ok(mut nvs) => {
            if let Err(e) = nvs.set_blob("gnss_config", payload) {
                log::warn!("GNSS config: NVS save failed: {:?}", e);
            } else {
                log::info!("GNSS config: saved {} bytes to NVS", payload.len());
            }
        }
        Err(e) => log::warn!("GNSS config: NVS open failed: {:?}", e),
    }
}
```

### NVS blob load (gnss config)
```rust
// Source: esp-idf-svc NVS API pattern
fn load_gnss_config(nvs_partition: &EspNvsPartition<NvsDefault>) -> Option<Vec<u8>> {
    let nvs = EspNvs::new(nvs_partition.clone(), "gnss", false).ok()?;
    let mut buf = vec![0u8; 512];
    match nvs.get_blob("gnss_config", &mut buf) {
        Ok(Some(size)) => Some(buf[..size].to_vec()),
        _ => None,
    }
}
```

### EspTls connect for NTRIP
```rust
// Source: esp-idf-svc-0.51.0/examples/tls.rs + src/tls.rs
use esp_idf_svc::tls::{EspTls, Config as TlsConfig};

let mut tls = EspTls::new()?;
tls.connect(
    &config.host,
    config.port,  // 443 for AUSCORS
    &TlsConfig {
        use_crt_bundle_attach: true,
        common_name: Some(&config.host),
        timeout_ms: 10_000,
        ..TlsConfig::new()
    },
)?;
// Then: tls.read(&mut buf) and tls.write_all(request.as_bytes())
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| TcpStream for NTRIP | EspTls for TLS NTRIP | This phase | AUSCORS port 443 support |
| /ncsi.txt redirect | /ncsi.txt exact body | This phase | Windows captive portal works |
| No UM980 config persistence | NVS gnss_config blob | This phase | Config survives UM980 reboot |

---

## Open Questions

1. **Does AUSCORS respond with `ICY 200 OK` or `HTTP/1.1 200 OK` on port 443?**
   - What we know: NTRIP v1 uses `ICY 200 OK`; NTRIP over HTTPS may use standard HTTP
   - What's unclear: AUSCORS-specific response format on port 443
   - Recommendation: Update `read_ntrip_headers()` to accept both — harmless, covers both cases

2. **Heap budget for EspTls session: is 125 KB sufficient?**
   - What we know: heap_free = 125132 bytes from gnss.log; mbedTLS needs ~50–80 KB
   - What's unclear: exact allocation at runtime with all other subsystems running
   - Recommendation: Test empirically — `EspTls::new()` + `connect()` will fail with `ESP_ERR_NO_MEM` if insufficient

3. **Does `enqueue()` in nmea_relay block at high rates?**
   - What we know: `enqueue()` maps to `esp_mqtt_client_enqueue()` with non-blocking semantics for QoS 0
   - What's unclear: whether the MQTT task processes the outbox fast enough at 40 msg/s
   - Recommendation: Add timing logs around the lock-acquire + enqueue for one test run at 5 Hz

4. **EspTls per-read timeout on live session**
   - What we know: `TlsConfig::timeout_ms` is for handshake only; `set_read_timeout()` not available
   - What's unclear: whether TCP keepalive alone is sufficient to detect silent AUSCORS disconnect
   - Recommendation: Accept for MVP; AUSCORS sends continuous RTCM so extended silence = real disconnect

---

## Validation Architecture

> `workflow.nyquist_validation` key is absent from `.planning/config.json` — treat as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | None — hardware-only validation (embedded firmware) |
| Config file | none |
| Quick run command | `cargo clippy -- -D warnings` |
| Full suite command | `cargo build --release` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | Notes |
|--------|----------|-----------|-------------------|-------|
| BUG-5 | Windows captive probe returns exact body | manual | n/a | Connect Windows device to `GNSS-Setup`, verify "Internet access" notification |
| BUG-5 | iOS captive probe returns exact body | manual | n/a | Connect iPhone to `GNSS-Setup`, verify captive portal notification |
| PERF-1 | NMEA relay sustains 5 Hz rate without drops | manual | n/a | Set UM980 to `GPGGA 0.2`, monitor nmea_drops in heartbeat |
| FEAT-2 | UM980 config re-applies after reboot | manual | n/a | Power-cycle UM980, verify config re-sent in logs |
| FEAT-3 | TLS NTRIP connects to AUSCORS port 443 | manual | n/a | Configure host=ntrip.data.gnss.ga.gov.au port=443, verify NTRIP connected in heartbeat |

Build and clippy gates:
```bash
cargo clippy -- -D warnings  # must be clean
cargo build --release        # must succeed
```

### Sampling Rate
- **Per task commit:** `cargo clippy -- -D warnings`
- **Per wave merge:** `cargo build --release`
- **Phase gate:** Full build clean + hardware validation on FFFEB5

### Wave 0 Gaps
None — no test infrastructure changes needed. Hardware validation is manual.

---

## Sources

### Primary (HIGH confidence — source code read)
- `src/provisioning.rs` — current captive portal probe handlers, HTTP server pattern
- `src/ntrip_client.rs` — TcpStream-based NTRIP session, NVS persistence pattern
- `src/mqtt.rs` — mqtt_connect MqttClientConfiguration, enqueue() usage
- `src/nmea_relay.rs` — channel capacity (64), mutex per sentence, enqueue pattern
- `src/gnss.rs` — nmea channel capacity 64, NMEA channel send pattern
- `src/config_relay.rs` — apply_config() private, no NVS persistence
- `src/main.rs` — UM980 reboot monitor (logs warning only), spawn order
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/tls.rs` — EspTls API, Config struct, read/write methods
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/examples/tls.rs` — EspTls usage pattern with `use_crt_bundle_attach`
- `/home/ben/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-idf-svc-0.51.0/src/mqtt/client.rs` — MqttClientConfiguration fields (task_prio, task_stack, buffer_size, out_buffer_size)

### Secondary (MEDIUM confidence)
- ESP-IDF v5.3.3 Kconfig for MQTT (`components/mqtt/esp-mqtt/Kconfig`) — outbox config, task priority fields
- `20-CONTEXT.md` gnss.log analysis — heap_free=125132, nmea_drops=0 at 10s rate, fix_type=7

### Tertiary (LOW confidence — general knowledge)
- mbedTLS session heap estimate ~50–80 KB (unverified for this specific TLS 1.2 RSA scenario on ESP32-C6)
- Windows 10/11 captive portal probe response format (known from spec, not tested on this hardware)
- AUSCORS NTRIP response format on port 443 (unknown — needs field test)

---

## Metadata

**Confidence breakdown:**
- Windows/iOS probe handlers: HIGH — response formats are documented, implementation pattern is trivial
- MQTT throughput: HIGH for diagnosis approach; MEDIUM for fix (depends on what profiling reveals)
- UM980 NVS re-apply: HIGH — NVS blob API verified, pattern is established in codebase
- TLS NTRIP feasibility: MEDIUM — EspTls API verified, heap feasibility requires runtime test

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (stable library stack, no pending updates)
