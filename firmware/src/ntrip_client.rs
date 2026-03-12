//! NTRIP client — streams RTCM3 correction bytes from a caster to the UM980 UART.
//!
//! # Protocol
//! NTRIP v1 is a simple HTTP/1.0-like protocol over a persistent TCP socket.
//! The client sends a single GET request with optional Basic Auth and receives
//! an `ICY 200 OK\r\n\r\n` header followed by a raw RTCM3 byte stream.
//!
//! # Thread lifecycle
//! `spawn_ntrip_client` launches a single dedicated thread that:
//! 1. Loads config from NVS on startup (persists across reboots).
//! 2. If no config present, waits on `ntrip_config_rx` for MQTT delivery.
//! 3. Connects, validates response, and forwards RTCM bytes directly to UART.
//! 4. On TCP drop or 60s read timeout, resets `NTRIP_STATE` to 0 and reconnects
//!    with exponential backoff (5s → 10s → 20s → 40s cap).
//! 5. Config updates received during backoff are applied immediately without
//!    waiting for the full backoff period.
//!
//! # Known race
//! `UartDriver::write(&self)` is not mutex-protected.  The GNSS TX thread and
//! this thread can write concurrently.  UM980 config commands are rare and brief;
//! RTCM bytes are continuous.  Practical corruption risk is low.
//! KNOWN-RACE: see RESEARCH.md Pitfall 3.

use esp_idf_svc::hal::uart::UartDriver;
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use esp_idf_svc::tls::{Config as TlsConfig, EspTls, InternalSocket};
use std::io::{Read, Write as IoWrite};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// NTRIP connection state: 0 = disconnected, 1 = connected.
///
/// Read by `heartbeat_loop` to include `"ntrip"` field in the health JSON.
/// Written by `run_ntrip_session` on connect/disconnect events.
pub static NTRIP_STATE: AtomicU8 = AtomicU8::new(0);

/// Exponential backoff delay steps (seconds) used after a session error.
/// Caps at 40s.  Reset to index 0 on a clean session close or new config.
const NTRIP_BACKOFF_STEPS: &[u64] = &[5, 10, 20, 40];

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

/// NTRIP caster connection parameters.
///
/// Loaded from NVS on startup; updated via MQTT `gnss/{id}/ntrip/config` topic.
#[derive(Clone)]
pub struct NtripConfig {
    pub host:       String,
    pub port:       u16,       // default 2101 (standard NTRIP TCP port)
    pub mountpoint: String,
    pub user:       String,
    pub pass:       String,
    pub tls:        bool,      // true = use EspTls (port 443); false = plain TCP (default)
}

impl Default for NtripConfig {
    fn default() -> Self {
        NtripConfig {
            host:       String::new(),
            port:       2101,
            mountpoint: String::new(),
            user:       String::new(),
            pass:       String::new(),
            tls:        false,  // default: plain TCP
        }
    }
}

impl NtripConfig {
    /// Returns true if the minimum required fields (host + mountpoint) are set.
    pub fn is_valid(&self) -> bool {
        !self.host.is_empty() && !self.mountpoint.is_empty()
    }
}

// ---------------------------------------------------------------------------
// NVS persistence
// ---------------------------------------------------------------------------

/// Load NTRIP config from NVS namespace `"ntrip"`.
///
/// Returns a default `NtripConfig` if the namespace is absent or keys are
/// missing.  Individual key errors are silently ignored (unwrap_or_default).
pub fn load_ntrip_config(nvs_partition: &EspNvsPartition<NvsDefault>) -> NtripConfig {
    let nvs = match EspNvs::new(nvs_partition.clone(), "ntrip", false) {
        Ok(n) => n,
        Err(_) => return NtripConfig::default(),
    };

    let mut config = NtripConfig::default();

    // String fields — NVS get_str requires a caller-supplied buffer.
    let mut buf = [0u8; 128];

    if let Ok(Some(v)) = nvs.get_str("ntrip_host", &mut buf) {
        config.host = v.to_string();
    }
    if let Ok(Some(v)) = nvs.get_str("ntrip_mount", &mut buf) {
        config.mountpoint = v.to_string();
    }
    if let Ok(Some(v)) = nvs.get_str("ntrip_user", &mut buf) {
        config.user = v.to_string();
    }
    if let Ok(Some(v)) = nvs.get_str("ntrip_pass", &mut buf) {
        config.pass = v.to_string();
    }

    // Port stored as two u8 keys — no set_u16 in EspNvs (same pattern as provisioning.rs).
    // Keys: "ntrip_port_hi" (13 chars) and "ntrip_port_lo" (13 chars) — within 15-char limit.
    let port_hi = nvs.get_u8("ntrip_port_hi").ok().flatten().unwrap_or(0x08); // 0x0835 = 2101
    let port_lo = nvs.get_u8("ntrip_port_lo").ok().flatten().unwrap_or(0x35);
    config.port = u16::from_be_bytes([port_hi, port_lo]);
    if config.port == 0 {
        config.port = 2101;
    }

    config.tls = nvs.get_u8("ntrip_tls").ok().flatten().unwrap_or(0) != 0;

    config
}

/// Persist NTRIP config to NVS namespace `"ntrip"`.
///
/// Errors are logged but not propagated — a failed save means the next reboot
/// will wait for MQTT delivery again, which is acceptable.
pub fn save_ntrip_config(config: &NtripConfig, nvs_partition: &EspNvsPartition<NvsDefault>) {
    let mut nvs = match EspNvs::new(nvs_partition.clone(), "ntrip", true) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("NTRIP: NVS open for write failed: {:?}", e);
            return;
        }
    };

    if let Err(e) = nvs.set_str("ntrip_host", &config.host) {
        log::warn!("NTRIP: NVS set ntrip_host failed: {:?}", e);
    }
    if let Err(e) = nvs.set_str("ntrip_mount", &config.mountpoint) {
        log::warn!("NTRIP: NVS set ntrip_mount failed: {:?}", e);
    }
    if let Err(e) = nvs.set_str("ntrip_user", &config.user) {
        log::warn!("NTRIP: NVS set ntrip_user failed: {:?}", e);
    }
    if let Err(e) = nvs.set_str("ntrip_pass", &config.pass) {
        log::warn!("NTRIP: NVS set ntrip_pass failed: {:?}", e);
    }

    let port_bytes = config.port.to_be_bytes();
    if let Err(e) = nvs.set_u8("ntrip_port_hi", port_bytes[0]) {
        log::warn!("NTRIP: NVS set ntrip_port_hi failed: {:?}", e);
    }
    if let Err(e) = nvs.set_u8("ntrip_port_lo", port_bytes[1]) {
        log::warn!("NTRIP: NVS set ntrip_port_lo failed: {:?}", e);
    }
    if let Err(e) = nvs.set_u8("ntrip_tls", if config.tls { 1 } else { 0 }) {
        log::warn!("NTRIP: NVS set ntrip_tls failed: {:?}", e);
    }

    log::info!("NTRIP: config saved to NVS (host={}, port={}, mount={}, tls={})",
        config.host, config.port, config.mountpoint, config.tls);
}

// ---------------------------------------------------------------------------
// JSON parsing
// ---------------------------------------------------------------------------

/// Parse a JSON NTRIP config payload.
///
/// Expected format:
/// `{"host":"...","port":2101,"mountpoint":"...","user":"...","pass":"..."}`
///
/// Uses manual string extraction (no serde — project has none).
/// Port defaults to 2101 if absent or unparseable.
/// Returns `None` if host is absent/empty (minimum required field).
pub fn parse_ntrip_config_payload(data: &[u8]) -> Option<NtripConfig> {
    let text = std::str::from_utf8(data).ok()?;

    let host       = extract_json_str(text, "host").unwrap_or_default().to_string();
    let mountpoint = extract_json_str(text, "mountpoint").unwrap_or_default().to_string();
    let user       = extract_json_str(text, "user").unwrap_or_default().to_string();
    let pass       = extract_json_str(text, "pass").unwrap_or_default().to_string();
    let port       = extract_json_number(text, "port").unwrap_or(2101) as u16;
    // "tls": true or "tls": 1 — support both forms
    let tls        = extract_json_bool(text, "tls").unwrap_or(false);

    if host.is_empty() {
        return None;
    }

    Some(NtripConfig { host, port, mountpoint, user, pass, tls })
}

/// Extract a JSON string field value by key name.
///
/// Handles `"key":"value"` patterns without escape sequences.
/// Returns `None` if key not found or value is not a quoted string.
fn extract_json_str<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{}\"", key);
    let key_pos = text.find(needle.as_str())?;
    let after_key = &text[key_pos + needle.len()..];
    let colon_pos = after_key.find(':')? + 1;
    let after_colon = after_key[colon_pos..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let inner = &after_colon[1..]; // skip opening quote
    let end = inner.find('"')?;
    Some(&inner[..end])
}

/// Extract a JSON numeric field value by key name.
///
/// Returns `None` if key not found or value is not parseable as u64.
fn extract_json_number(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\"", key);
    let key_pos = text.find(needle.as_str())?;
    let after_key = &text[key_pos + needle.len()..];
    let colon_pos = after_key.find(':')? + 1;
    let after_colon = after_key[colon_pos..].trim_start();
    let end = after_colon.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_colon.len());
    after_colon[..end].parse::<u64>().ok()
}

/// Extract a JSON boolean field value by key name.
///
/// Accepts `true`/`false` or `1`/`0` as values.
/// Returns `None` if key not found or value is unrecognised.
fn extract_json_bool(text: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{}\"", key);
    let key_pos = text.find(needle.as_str())?;
    let after_key = &text[key_pos + needle.len()..];
    let colon_pos = after_key.find(':')? + 1;
    let after_colon = after_key[colon_pos..].trim_start();
    if after_colon.starts_with("true") || after_colon.starts_with("1") {
        Some(true)
    } else if after_colon.starts_with("false") || after_colon.starts_with("0") {
        Some(false)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Base64 encoder
// ---------------------------------------------------------------------------

/// Minimal RFC 4648 §4 standard base64 encoder for NTRIP Basic Auth credentials.
///
/// Encodes `"user:pass"` into the Authorization header value.
/// No external crate — credentials are always short ASCII strings.
/// The standard base64 alphabet is used (not URL-safe).
fn base64_encode(input: &str) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(ALPHABET[b0 >> 2] as char);
        out.push(ALPHABET[((b0 & 0x3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((b1 & 0xf) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[b2 & 0x3f] as char } else { '=' });
    }
    out
}

// ---------------------------------------------------------------------------
// NTRIP request and response
// ---------------------------------------------------------------------------

/// Build the NTRIP v1 GET request string.
///
/// Includes `Authorization: Basic` header only when `user` is non-empty.
/// Credentials are base64-encoded as `"user:pass"` per RFC 4648 §4.
fn build_ntrip_request(config: &NtripConfig) -> String {
    let mut req = format!(
        "GET /{} HTTP/1.0\r\nUser-Agent: NTRIP esp32-gnssmqtt/1.0\r\nAccept: */*\r\n",
        config.mountpoint
    );
    if !config.user.is_empty() {
        let credentials = base64_encode(&format!("{}:{}", config.user, config.pass));
        req.push_str(&format!("Authorization: Basic {}\r\n", credentials));
    }
    req.push_str("\r\n");
    req
}

/// Read NTRIP server response headers byte-by-byte until `\r\n\r\n`.
///
/// Returns `Ok(true)` if the first response line is `ICY 200 OK`.
/// Returns `Ok(false)` for other responses (e.g. `401 Unauthorized`).
/// Returns `Err` on I/O error or unexpected EOF before headers complete.
///
/// Buffer is 512 bytes — sufficient for NTRIP v1 headers (no cookies, short lines).
fn read_ntrip_headers(stream: &mut TcpStream) -> Result<bool, std::io::Error> {
    let mut header_buf = [0u8; 512];
    let mut header_len = 0usize;
    let mut byte_buf = [0u8; 1];

    loop {
        match stream.read(&mut byte_buf)? {
            0 => return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "NTRIP: EOF before headers complete",
            )),
            _ => {
                if header_len < header_buf.len() {
                    header_buf[header_len] = byte_buf[0];
                    header_len += 1;
                }
                // Detect end-of-headers marker
                if header_len >= 4
                    && &header_buf[header_len - 4..header_len] == b"\r\n\r\n"
                {
                    break;
                }
            }
        }
    }

    let header_str = std::str::from_utf8(&header_buf[..header_len]).unwrap_or("");
    let ok = header_str.starts_with("ICY 200 OK")
        || header_str.starts_with("HTTP/1.1 200")
        || header_str.starts_with("HTTP/1.0 200");
    if !ok {
        // Log the first line for diagnostics
        let first_line = header_str.split("\r\n").next().unwrap_or("(empty)");
        log::warn!("NTRIP: unexpected response: {}", first_line);
    }
    Ok(ok)
}

// ---------------------------------------------------------------------------
// Session loop
// ---------------------------------------------------------------------------

/// Dispatch to TCP or TLS session based on config.tls.
///
/// Returns `Ok(new_config)` if the caller should reconnect with updated config.
/// Returns `Err` on connection error, timeout, or bad server response — caller applies backoff.
fn run_ntrip_session(
    config: &NtripConfig,
    uart: &Arc<UartDriver<'static>>,
    config_rx: &Receiver<Vec<u8>>,
) -> Result<Option<NtripConfig>, std::io::Error> {
    if config.tls {
        run_ntrip_session_tls(config, uart, config_rx)
    } else {
        run_ntrip_session_tcp(config, uart, config_rx)
    }
}

/// Run a single plain-TCP NTRIP session: connect → validate → stream → disconnect.
///
/// Writes RTCM correction bytes directly to `uart.write()`.
/// Returns `Ok(new_config)` if the caller should reconnect with updated config
/// (config payload received via `config_rx` during the session).
/// Returns `Err` on TCP error, timeout, or bad server response — caller applies backoff.
///
/// # KNOWN-RACE
/// `uart.write(&self)` is not mutex-protected.  GNSS TX thread and this thread
/// can write concurrently.  Commands are rare; risk is low.
/// KNOWN-RACE: see RESEARCH.md Pitfall 3.
fn run_ntrip_session_tcp(
    config: &NtripConfig,
    uart: &Arc<UartDriver<'static>>,
    config_rx: &Receiver<Vec<u8>>,
) -> Result<Option<NtripConfig>, std::io::Error> {
    log::info!("NTRIP: connecting to {}:{} mount={}",
        config.host, config.port, config.mountpoint);

    let addr = format!("{}:{}", config.host, config.port);
    let mut stream = TcpStream::connect(&addr)?;

    // Read timeout: 60s — caster silence triggers reconnect.
    // Write timeout: 10s — stalled connection detected quickly.
    // See RESEARCH.md Pitfall 7.
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    // Send NTRIP v1 GET request
    stream.write_all(build_ntrip_request(config).as_bytes())?;

    // Validate server response
    if !read_ntrip_headers(&mut stream)? {
        return Err(std::io::Error::other("NTRIP: server did not respond with ICY 200 OK"));
    }

    NTRIP_STATE.store(1, Ordering::Relaxed);
    log::info!("NTRIP: connected — streaming RTCM corrections to UM980 UART");

    // Log HWM at session start (project standard).
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] NTRIP session start: {} words ({} bytes) stack remaining",
        hwm_words, hwm_words * 4);

    // RTCM streaming loop — forward bytes directly to UART.
    let mut buf = [0u8; 512];
    loop {
        // Check for a config update (non-blocking — don't stall the stream).
        if let Ok(payload) = config_rx.try_recv() {
            log::info!("NTRIP: config update received during session — reconnecting");
            NTRIP_STATE.store(0, Ordering::Relaxed);
            // Return the parsed config for the caller to use on reconnect.
            if let Some(new_cfg) = parse_ntrip_config_payload(&payload) {
                return Ok(Some(new_cfg));
            }
            // Payload invalid — reconnect with existing config.
            return Ok(None);
        }

        match stream.read(&mut buf) {
            Ok(0) => {
                log::warn!("NTRIP: connection closed by caster");
                break;
            }
            Ok(n) => {
                // KNOWN-RACE: see module-level doc and RESEARCH.md Pitfall 3.
                if let Err(e) = uart.write(&buf[..n]) {
                    log::warn!("NTRIP: UART write error: {:?}", e);
                    // Continue — UART errors are transient; stream is still live.
                }
            }
            Err(e) => {
                // WouldBlock / TimedOut = read timeout exceeded (60s of silence).
                // Other errors = TCP connection dropped.
                log::warn!("NTRIP: stream read error: {:?}", e);
                break;
            }
        }
    }

    NTRIP_STATE.store(0, Ordering::Relaxed);
    // Clean exit — caller will apply backoff before reconnecting.
    Err(std::io::Error::other("NTRIP: session ended"))
}

/// Run a single TLS NTRIP session using EspTls (mbedTLS).
///
/// Used for NTRIP casters that require TLS (e.g. AUSCORS port 443).
/// NOTE: set_read_timeout() is not available on EspTls. The read loop relies on
/// the caster sending continuous RTCM data; silence means a real disconnect.
/// EspTls::new() may fail with ESP_ERR_NO_MEM if heap is insufficient — this is
/// caught and returned as Err to trigger exponential backoff (not a panic).
fn run_ntrip_session_tls(
    config: &NtripConfig,
    uart: &Arc<UartDriver<'static>>,
    config_rx: &Receiver<Vec<u8>>,
) -> Result<Option<NtripConfig>, std::io::Error> {
    log::info!("NTRIP: TLS connecting to {}:{} mount={}",
        config.host, config.port, config.mountpoint);

    let mut tls = EspTls::new()
        .map_err(|e| std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!("EspTls::new() failed (heap?): {:?}", e),
        ))?;

    tls.connect(
        &config.host,
        config.port,
        &TlsConfig {
            use_crt_bundle_attach: true,
            common_name: Some(&config.host),
            timeout_ms: 10_000,
            ..TlsConfig::new()
        },
    ).map_err(|e| std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        format!("TLS connect failed: {:?}", e),
    ))?;

    // Send NTRIP request using HTTP/1.1 (Host header required for virtual hosting on port 443)
    let req = build_ntrip_request_v11(config);
    tls.write_all(req.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, format!("{:?}", e)))?;

    // Validate server response
    if !read_ntrip_headers_tls(&mut tls)? {
        return Err(std::io::Error::other("NTRIP TLS: server did not respond with 200 OK"));
    }

    NTRIP_STATE.store(1, Ordering::Relaxed);
    log::info!("NTRIP: TLS connected — streaming RTCM corrections to UM980 UART");

    // RTCM streaming loop — identical to TCP version but using tls.read()
    let mut buf = [0u8; 512];
    loop {
        if let Ok(payload) = config_rx.try_recv() {
            log::info!("NTRIP: config update received during TLS session — reconnecting");
            NTRIP_STATE.store(0, Ordering::Relaxed);
            if let Some(new_cfg) = parse_ntrip_config_payload(&payload) {
                return Ok(Some(new_cfg));
            }
            return Ok(None);
        }

        match tls.read(&mut buf) {
            Ok(0) => {
                log::warn!("NTRIP TLS: connection closed by caster");
                break;
            }
            Ok(n) => {
                if let Err(e) = uart.write(&buf[..n]) {
                    log::warn!("NTRIP TLS: UART write error: {:?}", e);
                }
            }
            Err(e) => {
                log::warn!("NTRIP TLS: stream read error: {:?}", e);
                break;
            }
        }
    }

    NTRIP_STATE.store(0, Ordering::Relaxed);
    Err(std::io::Error::other("NTRIP TLS: session ended"))
}

/// Build an HTTP/1.1 NTRIP GET request string (used for TLS connections).
///
/// HTTP/1.1 requires a Host header for virtual hosting on port 443.
fn build_ntrip_request_v11(config: &NtripConfig) -> String {
    let mut req = format!(
        "GET /{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: NTRIP esp32-gnssmqtt/1.0\r\nAccept: */*\r\nConnection: close\r\n",
        config.mountpoint, config.host
    );
    if !config.user.is_empty() {
        let credentials = base64_encode(&format!("{}:{}", config.user, config.pass));
        req.push_str(&format!("Authorization: Basic {}\r\n", credentials));
    }
    req.push_str("\r\n");
    req
}

/// Read NTRIP server response headers from an EspTls connection byte-by-byte until `\r\n\r\n`.
///
/// Returns `Ok(true)` if the first response line is a 200 OK variant.
/// Returns `Ok(false)` for other responses (e.g. 401 Unauthorized).
/// Returns `Err` on I/O error or unexpected EOF before headers complete.
fn read_ntrip_headers_tls(tls: &mut EspTls<InternalSocket>) -> Result<bool, std::io::Error> {
    let mut header_buf = [0u8; 512];
    let mut header_len = 0usize;
    let mut byte_buf = [0u8; 1];

    loop {
        match tls.read(&mut byte_buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, format!("{:?}", e)))?
        {
            0 => return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "NTRIP TLS: EOF before headers complete",
            )),
            _ => {
                if header_len < header_buf.len() {
                    header_buf[header_len] = byte_buf[0];
                    header_len += 1;
                }
                if header_len >= 4 && &header_buf[header_len - 4..header_len] == b"\r\n\r\n" {
                    break;
                }
            }
        }
    }

    let header_str = std::str::from_utf8(&header_buf[..header_len]).unwrap_or("");
    let ok = header_str.starts_with("ICY 200 OK")
        || header_str.starts_with("HTTP/1.1 200")
        || header_str.starts_with("HTTP/1.0 200");
    if !ok {
        let first_line = header_str.split("\r\n").next().unwrap_or("(empty)");
        log::warn!("NTRIP TLS: unexpected response: {}", first_line);
    }
    Ok(ok)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Spawn the NTRIP client thread.
///
/// The thread owns the TCP lifecycle: config load → connect → stream → backoff → reconnect.
/// It returns immediately after spawning (non-blocking).
///
/// # Parameters
/// * `uart` — shared UART driver clone from `spawn_gnss`; used to write RTCM bytes directly.
/// * `ntrip_config_rx` — receives JSON config payloads from the MQTT pump thread.
/// * `nvs` — NVS partition for loading/saving NTRIP config across reboots.
pub fn spawn_ntrip_client(
    uart: Arc<UartDriver<'static>>,
    ntrip_config_rx: Receiver<Vec<u8>>,
    nvs: EspNvsPartition<NvsDefault>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate.
            // Value × 4 = bytes free.  Increase stack_size to 12288 if HWM < 20% free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "NTRIP client", hwm_words, hwm_words * 4);

            // Load saved config from NVS (avoids waiting for MQTT on reboot).
            let mut config = load_ntrip_config(&nvs);

            if config.is_valid() {
                log::info!("NTRIP: loaded config from NVS — host={} port={} mount={}",
                    config.host, config.port, config.mountpoint);
            } else {
                log::info!("NTRIP: no saved config — waiting for MQTT delivery");
            }

            let mut backoff_idx = 0usize;

            loop {
                // Phase 1: wait for a valid config if we don't have one yet.
                if !config.is_valid() {
                    match ntrip_config_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
                        Ok(payload) => {
                            if let Some(new_cfg) = parse_ntrip_config_payload(&payload) {
                                save_ntrip_config(&new_cfg, &nvs);
                                config = new_cfg;
                                backoff_idx = 0;
                            } else {
                                log::warn!("NTRIP: received invalid config payload, ignoring");
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            // Still no config — keep waiting.
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            log::error!("NTRIP: config channel disconnected — thread exiting");
                            break;
                        }
                    }
                    continue;
                }

                // Phase 2: attempt a session.
                match run_ntrip_session(&config, &uart, &ntrip_config_rx) {
                    Ok(Some(new_cfg)) => {
                        // Config updated during session — reconnect immediately.
                        log::info!("NTRIP: applying new config, reconnecting");
                        save_ntrip_config(&new_cfg, &nvs);
                        config = new_cfg;
                        backoff_idx = 0;
                        continue;
                    }
                    Ok(None) => {
                        // Config update received but payload invalid — reconnect with current config.
                        log::info!("NTRIP: invalid config update, reconnecting with current config");
                        backoff_idx = 0;
                        continue;
                    }
                    Err(e) => {
                        let delay = NTRIP_BACKOFF_STEPS[backoff_idx];
                        log::warn!("NTRIP: session error — {:?} — reconnecting in {}s", e, delay);
                        backoff_idx = (backoff_idx + 1).min(NTRIP_BACKOFF_STEPS.len() - 1);

                        // During backoff, check for a new config update.
                        match ntrip_config_rx.recv_timeout(Duration::from_secs(delay)) {
                            Ok(payload) => {
                                if let Some(new_cfg) = parse_ntrip_config_payload(&payload) {
                                    log::info!("NTRIP: new config received during backoff, applying");
                                    save_ntrip_config(&new_cfg, &nvs);
                                    config = new_cfg;
                                    backoff_idx = 0;
                                } else {
                                    log::warn!("NTRIP: invalid config during backoff, ignoring");
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => {
                                // Backoff expired — retry connection.
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                log::error!("NTRIP: config channel disconnected — thread exiting");
                                break;
                            }
                        }
                    }
                }
            }

            // Dead-end park (config channel closed; thread has nothing to do).
            NTRIP_STATE.store(0, Ordering::Relaxed);
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("ntrip client thread spawn failed");

    Ok(())
}
