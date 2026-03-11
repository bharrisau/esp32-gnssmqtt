//! Provisioning module — SoftAP web portal and NVS credential storage.
//!
//! Boot-path logic: main.rs reads NVS before WiFi init; if no credentials (or force_softap set),
//! calls run_softap_portal() instead of wifi_connect().

use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, Configuration};
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::Write;
use std::net::UdpSocket;

const PROV_HTML: &str = "<!DOCTYPE html>\
<html><head><title>GNSS Setup</title></head><body>\
<h2>GNSS Device Setup</h2>\
<p>Connect to this hotspot, fill in the form below, and press Save.</p>\
<p><em>Note: WiFi passwords with &amp;, %, +, = characters are not supported.</em></p>\
<form method=\"POST\" action=\"/save\">\
  <h3>WiFi Network 1 (required)</h3>\
  SSID: <input name=\"ssid0\" required><br>\
  Password: <input name=\"pass0\" type=\"password\"><br>\
  <h3>WiFi Network 2 (optional)</h3>\
  SSID: <input name=\"ssid1\"><br>\
  Password: <input name=\"pass1\" type=\"password\"><br>\
  <h3>WiFi Network 3 (optional)</h3>\
  SSID: <input name=\"ssid2\"><br>\
  Password: <input name=\"pass2\" type=\"password\"><br>\
  <h3>MQTT Broker</h3>\
  Host: <input name=\"mqtt_host\" required><br>\
  Port: <input name=\"mqtt_port\" value=\"1883\"><br>\
  Username: <input name=\"mqtt_user\"><br>\
  Password: <input name=\"mqtt_pass\" type=\"password\"><br>\
  <br>\
  <input type=\"submit\" value=\"Save and Reboot\">\
</form></body></html>";

/// Returns true if at least one WiFi network has been stored in NVS.
///
/// Opens NVS namespace "prov" read-only. Returns false if namespace does not exist yet
/// or if wifi_count is zero.
pub fn has_wifi_credentials(nvs_partition: &EspNvsPartition<NvsDefault>) -> bool {
    match EspNvs::new(nvs_partition.clone(), "prov", false) {
        Err(_) => false,
        Ok(nvs) => nvs.get_u8("wifi_count").unwrap_or(None).unwrap_or(0) > 0,
    }
}

/// Checks the force_softap NVS flag. If set, clears it and returns true.
///
/// Returns false if the flag is not set or on any NVS error.
pub fn check_and_clear_force_softap(nvs_partition: &EspNvsPartition<NvsDefault>) -> bool {
    match EspNvs::new(nvs_partition.clone(), "prov", true) {
        Err(_) => false,
        Ok(nvs) => match nvs.get_u8("force_softap") {
            Ok(Some(1)) => {
                let _ = nvs.set_u8("force_softap", 0);
                true
            }
            _ => false,
        },
    }
}

/// Loads all stored WiFi networks from NVS.
///
/// Returns a Vec of (ssid, password) pairs. Returns an empty Vec on NVS error.
pub fn load_wifi_networks(nvs_partition: &EspNvsPartition<NvsDefault>) -> Vec<(String, String)> {
    match EspNvs::new(nvs_partition.clone(), "prov", false) {
        Err(_) => Vec::new(),
        Ok(nvs) => {
            let count = nvs.get_u8("wifi_count").unwrap_or(None).unwrap_or(0) as usize;
            let mut networks = Vec::new();
            let mut ssid_buf = [0u8; 65];
            let mut pass_buf = [0u8; 65];
            for i in 0..count.min(3) {
                let ssid_key = format!("wifi_ssid_{}", i);
                let pass_key = format!("wifi_pass_{}", i);
                if let (Ok(Some(ssid)), Ok(Some(pass))) = (
                    nvs.get_str(&ssid_key, &mut ssid_buf),
                    nvs.get_str(&pass_key, &mut pass_buf),
                ) {
                    networks.push((ssid.to_string(), pass.to_string()));
                }
            }
            networks
        }
    }
}

/// Loads MQTT configuration from NVS.
///
/// Returns Some((host, port, user, pass, tls)) if mqtt_host is non-empty, None otherwise.
/// Reconstructs the 16-bit port from two u8 keys. Uses 1883 as default if port is 0.
/// tls defaults to false when the key is absent — old firmware never wrote mqtt_tls,
/// so absence means plain MQTT (BUG-3 fix: avoids TLS handshake against plain broker post-OTA).
pub fn load_mqtt_config(
    nvs_partition: &EspNvsPartition<NvsDefault>,
) -> Option<(String, u16, String, String, bool)> {
    let nvs = EspNvs::new(nvs_partition.clone(), "prov", false).ok()?;
    let mut host_buf = [0u8; 65];
    let mut user_buf = [0u8; 65];
    let mut pass_buf = [0u8; 65];

    let host = nvs.get_str("mqtt_host", &mut host_buf).ok()??.to_string();
    if host.is_empty() {
        return None;
    }

    let port_hi = nvs.get_u8("mqtt_port_hi").unwrap_or(None).unwrap_or(0);
    let port_lo = nvs.get_u8("mqtt_port_lo").unwrap_or(None).unwrap_or(0);
    let mut port = (port_hi as u16) << 8 | (port_lo as u16);
    if port == 0 {
        port = 1883;
    }

    let user = nvs
        .get_str("mqtt_user", &mut user_buf)
        .unwrap_or(None)
        .unwrap_or("")
        .to_string();
    let pass = nvs
        .get_str("mqtt_pass", &mut pass_buf)
        .unwrap_or(None)
        .unwrap_or("")
        .to_string();

    // Default 0 (false = plain MQTT). TLS toggle deferred to security milestone (SEC-F01).
    // Old firmware never wrote this key — unwrap_or(0) ensures TLS is off for pre-existing configs.
    let tls = nvs.get_u8("mqtt_tls").unwrap_or(None).unwrap_or(0) != 0;

    Some((host, port, user, pass, tls))
}

/// Sets the force_softap NVS flag so the next boot enters SoftAP mode.
///
/// Used by GPIO9 monitor and MQTT "softap" trigger (Plan 15-03).
pub fn set_force_softap(nvs_partition: &EspNvsPartition<NvsDefault>) {
    match EspNvs::new(nvs_partition.clone(), "prov", true) {
        Ok(nvs) => {
            if let Err(e) = nvs.set_u8("force_softap", 1) {
                log::warn!("set_force_softap: failed to write NVS: {:?}", e);
            }
        }
        Err(e) => {
            log::warn!("set_force_softap: failed to open NVS: {:?}", e);
        }
    }
}

/// Starts the SoftAP captive portal and serves the provisioning web UI.
///
/// Configures WiFi in AccessPoint mode (SSID: "GNSS-Setup", open, channel 6), starts an
/// HTTP server on port 80, and blocks until either credentials are submitted (triggers reboot)
/// or 300 seconds elapse with no connected client (triggers reboot back to STA mode).
///
/// NOTE: Do NOT call wifi.connect() — AP mode only requires start() + wait_netif_up().
pub fn run_softap_portal(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    nvs_partition: EspNvsPartition<NvsDefault>,
) -> anyhow::Result<()> {
    // Step 1: Configure and start SoftAP.
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: "GNSS-Setup".try_into().unwrap(),
        ssid_hidden: false,
        auth_method: AuthMethod::None,
        channel: 6,
        max_connections: 4,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;
    // Do NOT call wifi.connect() — AP mode does not use connect()
    // DHCP DNS is pre-configured in the ap_netif passed via EspWifi::wrap_all in main.rs —
    // no post-start DNS override needed here.

    log::info!("SoftAP started — connect to 'GNSS-Setup' and navigate to 192.168.71.1");

    // Step 2: Start HTTP server with increased stack size for POST body parsing.
    let mut server = EspHttpServer::new(&HttpConfig {
        stack_size: 10240,
        ..Default::default()
    })?;

    // Step 3: GET "/" — serve provisioning HTML form.
    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?.write_all(PROV_HTML.as_bytes())
    })?;

    // Step 4: POST "/save" — parse credentials, write to NVS, then reboot.
    // Known limitation: passwords containing &, %, +, =, or non-ASCII characters
    // are not supported in v2.0. URL-encoding is not decoded.
    let nvs_for_handler = nvs_partition.clone();
    server.fn_handler::<anyhow::Error, _>("/save", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        if len > 2048 {
            req.into_status_response(413)?.write_all(b"Too large")?;
            return Ok(());
        }
        let mut buf = vec![0u8; len];
        req.read(&mut buf)?;

        let body = std::str::from_utf8(&buf).unwrap_or("");

        let ssid0 = parse_form_field(body, "ssid0").unwrap_or("");
        let pass0 = parse_form_field(body, "pass0").unwrap_or("");
        let ssid1 = parse_form_field(body, "ssid1").unwrap_or("");
        let pass1 = parse_form_field(body, "pass1").unwrap_or("");
        let ssid2 = parse_form_field(body, "ssid2").unwrap_or("");
        let pass2 = parse_form_field(body, "pass2").unwrap_or("");
        let mqtt_host = parse_form_field(body, "mqtt_host").unwrap_or("");
        let mqtt_port_str = parse_form_field(body, "mqtt_port").unwrap_or("1883");
        let mqtt_user = parse_form_field(body, "mqtt_user").unwrap_or("");
        let mqtt_pass = parse_form_field(body, "mqtt_pass").unwrap_or("");

        let mqtt_port: u16 = mqtt_port_str.parse().unwrap_or(1883);

        // Build network list (only non-empty SSIDs).
        let mut networks: Vec<(&str, &str)> = Vec::new();
        if !ssid0.is_empty() {
            networks.push((ssid0, pass0));
        }
        if !ssid1.is_empty() {
            networks.push((ssid1, pass1));
        }
        if !ssid2.is_empty() {
            networks.push((ssid2, pass2));
        }

        if let Err(e) = save_credentials(
            nvs_for_handler.clone(),
            &networks,
            mqtt_host,
            mqtt_port,
            mqtt_user,
            mqtt_pass,
        ) {
            log::error!("save_credentials failed: {:?}", e);
            req.into_status_response(500)?
                .write_all(b"Failed to save credentials")?;
            return Ok(());
        }

        req.into_ok_response()?
            .write_all(b"Saved. Rebooting in 1 second...")?;

        // Spawn thread to reboot after giving the browser time to receive the response.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            log::info!("Rebooting after credential save...");
            unsafe { esp_idf_svc::sys::esp_restart() };
        });

        Ok(())
    })?;

    // Captive portal probe URL handlers — cause Android/iOS/Windows to show the sign-in prompt.
    // Android probes: /generate_204, /connectivitycheck → 302 redirect (triggers captive detection)
    // iOS probes: /hotspot-detect.html → exact Apple success HTML (200 OK)
    //             /success.html, /library/test/success.html → meta-refresh redirect (200 OK)
    // Windows 10/11 probe: /connecttest.txt → exact "Microsoft Connect Test" (200 OK)
    // Windows older probe: /ncsi.txt → exact "Microsoft NCSI" (200 OK)
    // Mikrotik/generic: /redirect → meta-refresh redirect (200 OK)
    //
    // iOS and Windows require exact response bodies — returning redirect_html instead causes
    // the OS to skip the captive portal notification (BUG-5 fix).
    const IOS_SUCCESS_HTML: &[u8] = b"<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>";
    let redirect_html: &'static [u8] = b"<html><head><meta http-equiv='refresh' content='0;url=http://192.168.71.1/'></head></html>";

    server.fn_handler("/generate_204", Method::Get, |req| {
        req.into_response(302, Some("Found"), &[("Location", "http://192.168.71.1/")])?
            .write_all(b"")
    })?;
    server.fn_handler("/connectivitycheck", Method::Get, |req| {
        req.into_response(302, Some("Found"), &[("Location", "http://192.168.71.1/")])?
            .write_all(b"")
    })?;
    server.fn_handler("/hotspot-detect.html", Method::Get, |req| {
        req.into_ok_response()?.write_all(IOS_SUCCESS_HTML)
    })?;
    server.fn_handler("/success.html", Method::Get, move |req| {
        req.into_ok_response()?.write_all(redirect_html)
    })?;
    if let Err(e) = server.fn_handler("/library/test/success.html", Method::Get, move |req| {
        req.into_ok_response()?.write_all(redirect_html)
    }) {
        log::warn!("captive portal: failed to register /library/test/success.html: {:?}", e);
    }
    server.fn_handler("/connecttest.txt", Method::Get, |req| {
        req.into_ok_response()?.write_all(b"Microsoft Connect Test")
    })?;
    server.fn_handler("/ncsi.txt", Method::Get, |req| {
        req.into_ok_response()?.write_all(b"Microsoft NCSI")
    })?;
    server.fn_handler("/redirect", Method::Get, move |req| {
        req.into_ok_response()?.write_all(redirect_html)
    })?;

    // Captive portal DNS hijack: respond to all DNS A queries with 192.168.71.1.
    // This causes any device connected to the SoftAP to resolve all hostnames to
    // the portal IP, triggering the OS captive portal detection flow.
    // Thread is not explicitly stopped — it will exit when the ESP32 reboots after
    // credential save (the reboot happens in a spawned thread after 1s delay).
    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || {
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "DNS hijack", hwm_words, hwm_words * 4);

            let socket = match UdpSocket::bind("0.0.0.0:53") {
                Ok(s) => s,
                Err(e) => {
                    log::error!("DNS hijack: failed to bind UDP port 53: {:?}", e);
                    return;
                }
            };

            // Set read timeout so the thread can periodically yield rather than
            // blocking indefinitely.
            if let Err(e) = socket.set_read_timeout(Some(std::time::Duration::from_secs(2))) {
                log::warn!("DNS hijack: set_read_timeout failed: {:?}", e);
            }

            log::info!("DNS hijack: listening on UDP port 53");
            let mut buf = [0u8; 512];

            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, src)) => {
                        if len < 12 {
                            continue; // too short to be a valid DNS query
                        }
                        // Only respond to queries (QR=0), not responses.
                        if buf[2] & 0x80 != 0 {
                            continue; // already a response — ignore
                        }
                        // Only respond if QDCOUNT is non-zero.
                        let qdcount = u16::from_be_bytes([buf[4], buf[5]]);
                        if qdcount == 0 {
                            continue; // nothing to answer
                        }

                        // Find the end of the question section so we copy only
                        // header + question into the response — this strips any
                        // EDNS OPT record (or other additional records) from the
                        // query before echoing it back.  Leaving extra bytes in the
                        // response causes "malformed" errors and, crucially, the
                        // original code set resp[3] |= 0x04 thinking it was the AA
                        // bit — but AA lives in byte 2; byte 3 bit-2 is RCODE=4
                        // (NOTIMP), which is exactly what clients were seeing.
                        let mut q_end = 12usize;
                        // Walk QNAME labels (length-prefixed, terminated by 0x00).
                        while q_end < len {
                            let label_len = buf[q_end] as usize;
                            if label_len == 0 { q_end += 1; break; }
                            // Compression pointer — two bytes, then stop.
                            if label_len & 0xC0 == 0xC0 { q_end += 2; break; }
                            q_end += 1 + label_len;
                        }
                        q_end += 4; // QTYPE (2) + QCLASS (2)
                        if q_end > len {
                            continue; // malformed question, skip
                        }

                        // Build DNS response: header + question only (no additional
                        // section), then append one A record answer.
                        let mut resp = Vec::with_capacity(q_end + 16);
                        resp.extend_from_slice(&buf[..q_end]);

                        // DNS header flags (RFC 1035 §4.1.1):
                        //   Byte 2: QR(7) Opcode(6:3) AA(2) TC(1) RD(0)
                        //   Byte 3: RA(7) Z(6) AD(5) CD(4) RCODE(3:0)
                        resp[2] |= 0x84; // QR=1 (response), AA=1 (authoritative)
                        resp[3]  = 0x00; // RA=0, RCODE=0 (no error)
                        // ANCOUNT=1, NSCOUNT=0, ARCOUNT=0
                        resp[6] = 0x00; resp[7] = 0x01;
                        resp[8] = 0x00; resp[9] = 0x00;
                        resp[10] = 0x00; resp[11] = 0x00;

                        // Append answer RR:
                        // NAME: pointer to offset 12 (start of question QNAME)
                        resp.extend_from_slice(&[0xC0, 0x0C]);
                        // TYPE: A (1)
                        resp.extend_from_slice(&[0x00, 0x01]);
                        // CLASS: IN (1)
                        resp.extend_from_slice(&[0x00, 0x01]);
                        // TTL: 30 seconds (short: clients re-query quickly when AP disappears)
                        resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x1E]);
                        // RDLENGTH: 4 (IPv4 address)
                        resp.extend_from_slice(&[0x00, 0x04]);
                        // RDATA: 192.168.71.1
                        resp.extend_from_slice(&[192, 168, 71, 1]);

                        if let Err(e) = socket.send_to(&resp, src) {
                            log::warn!("DNS hijack: send_to {:?} failed: {:?}", src, e);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                               || e.kind() == std::io::ErrorKind::TimedOut => {
                        // Read timeout — normal idle state. Loop and wait for next query.
                    }
                    Err(e) => {
                        log::warn!("DNS hijack: recv_from error: {:?}", e);
                        // Brief sleep to avoid tight error loop on persistent socket errors.
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        })
        .expect("DNS hijack thread spawn failed");
    log::info!("DNS hijack started — all DNS queries will resolve to 192.168.71.1");

    // Step 5: 300-second no-client timeout loop. Keeps server alive in scope.
    let mut no_client_since = std::time::Instant::now();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let clients = count_softap_clients();
        if clients > 0 {
            no_client_since = std::time::Instant::now();
        } else if no_client_since.elapsed().as_secs() >= 300 {
            log::info!("SoftAP: no client for 300s — returning to STA mode via restart");
            // Do NOT set force_softap — next boot tries STA mode with stored credentials.
            unsafe { esp_idf_svc::sys::esp_restart() };
        }
    }
}

/// Writes WiFi and MQTT credentials to NVS namespace "prov".
fn save_credentials(
    nvs_partition: EspNvsPartition<NvsDefault>,
    networks: &[(&str, &str)],
    mqtt_host: &str,
    mqtt_port: u16,
    mqtt_user: &str,
    mqtt_pass: &str,
) -> anyhow::Result<()> {
    let mut nvs = EspNvs::new(nvs_partition, "prov", true)?;
    let count = networks.len().min(3) as u8;
    nvs.set_u8("wifi_count", count)?;
    for (i, (ssid, pass)) in networks.iter().enumerate().take(3) {
        nvs.set_str(&format!("wifi_ssid_{}", i), ssid)?;
        nvs.set_str(&format!("wifi_pass_{}", i), pass)?;
    }
    nvs.set_str("mqtt_host", mqtt_host)?;
    nvs.set_u8("mqtt_port_hi", (mqtt_port >> 8) as u8)?;
    nvs.set_u8("mqtt_port_lo", (mqtt_port & 0xFF) as u8)?;
    nvs.set_str("mqtt_user", mqtt_user)?;
    nvs.set_str("mqtt_pass", mqtt_pass)?;
    nvs.set_u8("mqtt_tls", 0)?;   // plain MQTT; TLS deferred to security milestone (SEC-F01)
    nvs.set_u8("config_ver", 1)?; // NVS schema v1 — written on every save for future detection
    Ok(())
}

/// Parses a single field from a URL-encoded form body.
///
/// Known limitation: does not handle percent-encoding. Passwords containing
/// &, %, +, =, or non-ASCII characters are not supported in v2.0.
fn parse_form_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("{}=", key);
    let start = body.find(&search)? + search.len();
    let end = body[start..].find('&').map(|i| start + i).unwrap_or(body.len());
    Some(&body[start..end])
}

/// Returns the number of clients currently connected to the SoftAP.
fn count_softap_clients() -> u8 {
    let mut sta_list = esp_idf_svc::sys::wifi_sta_list_t::default();
    let ret = unsafe { esp_idf_svc::sys::esp_wifi_ap_get_sta_list(&mut sta_list as *mut _) };
    if ret == 0 {
        sta_list.num as u8
    } else {
        0
    }
}
