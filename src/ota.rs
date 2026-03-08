//! OTA firmware update task.
//!
//! Receives trigger payloads via mpsc channel from the MQTT pump, HTTP-streams
//! firmware into the inactive OTA partition with concurrent SHA-256 verification,
//! publishes progress to MQTT, and reboots on success.
//!
//! The OTA thread runs independently of the MQTT pump thread. Running EspOta inside
//! the pump thread would block connection.next() calls, causing keep-alive timeouts.

use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::hal::reset::restart;
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::mqtt::client::EspMqttClient;
use esp_idf_svc::nvs::{EspNvsPartition, NvsDefault};
use esp_idf_svc::ota::EspOta;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Publish a JSON status string to the OTA status topic.
///
/// Uses enqueue (non-blocking) to avoid stalling the OTA thread.
/// Mirrors the heartbeat_loop pattern from mqtt.rs.
fn publish_status(client: &Arc<Mutex<EspMqttClient<'static>>>, topic: &str, json: &str) {
    match client.lock() {
        Err(e) => log::warn!("OTA: status mutex poisoned: {:?}", e),
        Ok(mut c) => match c.enqueue(topic, QoS::AtMostOnce, false, json.as_bytes()) {
            Ok(_) => {}
            Err(e) => log::warn!("OTA: status enqueue failed: {:?}", e),
        },
    }
}

/// Extract a JSON string field value without a serde dependency.
///
/// Searches for `"key":"` and returns the content up to the next `"`.
/// Only works for simple string values with no escaping.
fn extract_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":\"", key);
    let start = json.find(&search)? + search.len();
    let end = json[start..].find('"')? + start;
    Some(&json[start..end])
}

/// Core OTA update loop. Runs forever — receives triggers from the MQTT pump.
///
/// For each trigger payload:
/// 1. Parse url and sha256 from JSON
/// 2. Publish downloading status
/// 3. HTTP GET the firmware URL
/// 4. Initiate OTA update (erases partition — takes 4-8s; WDT extended to 30s in sdkconfig)
/// 5. Stream download: feed sha2 hasher + write each chunk to flash
/// 6. Verify SHA-256 before completing
/// 7. Finalize update, publish complete, clear retained trigger, restart
///
/// All errors publish failed status and continue to the next trigger —
/// the OTA thread does NOT restart on error.
pub fn ota_task(
    mqtt_client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    ota_rx: Receiver<Vec<u8>>,
    nvs: EspNvsPartition<NvsDefault>,
) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "OTA task", hwm_words, hwm_words * 4);
    let status_topic = format!("gnss/{}/ota/status", device_id);
    let trigger_topic = format!("gnss/{}/ota/trigger", device_id);

    loop {
    let payload = match ota_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
        Ok(p) => p,
        Err(RecvTimeoutError::Timeout) => {
            // No OTA trigger within 30s — OTA is operator-triggered and rare. Continue.
            continue;
        }
        Err(RecvTimeoutError::Disconnected) => {
            log::error!("OTA: channel closed — ota_rx hung up");
            break;
        }
    };
    {
        // --- Step 3: Parse trigger JSON ---
        let json = match std::str::from_utf8(&payload) {
            Ok(s) => s.to_owned(),
            Err(e) => {
                log::warn!("OTA: payload not valid UTF-8: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    "{\"state\":\"failed\",\"reason\":\"payload not valid UTF-8\"}",
                );
                continue;
            }
        };

        // MAINT-01: handle "reboot" payload before attempting OTA JSON parse.
        // Use .trim() to tolerate trailing whitespace/newlines from MQTT clients.
        // restart() is already imported at the top of this file and diverges (never returns).
        if json.trim() == "reboot" {
            log::info!("OTA: 'reboot' payload received — restarting device");
            std::thread::sleep(std::time::Duration::from_millis(200)); // let log line flush
            restart();
        }

        // PROV-07: handle "softap" payload — enter SoftAP mode on next boot.
        // Check before OTA JSON parse. Same short-circuit pattern as "reboot" above.
        if json.trim() == "softap" {
            log::info!("OTA: 'softap' payload received — entering SoftAP mode");
            crate::provisioning::set_force_softap(&nvs);
            std::thread::sleep(std::time::Duration::from_millis(200)); // let log flush
            restart();
        }

        let url = match extract_json_str(&json, "url") {
            Some(u) => u.to_owned(),
            None => {
                log::warn!("OTA: trigger missing url field");
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    "{\"state\":\"failed\",\"reason\":\"missing url or sha256\"}",
                );
                continue;
            }
        };

        let sha256 = match extract_json_str(&json, "sha256") {
            Some(s) => s.to_owned(),
            None => {
                log::warn!("OTA: trigger missing sha256 field");
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    "{\"state\":\"failed\",\"reason\":\"missing url or sha256\"}",
                );
                continue;
            }
        };

        log::info!("OTA: trigger received, url={}, sha256={}", url, sha256);

        // --- Step 4: Publish downloading ---
        publish_status(
            &mqtt_client,
            &status_topic,
            "{\"state\":\"downloading\",\"progress\":0}",
        );

        // --- Step 5: Build HTTP client ---
        let http_conf = HttpConfig {
            buffer_size: Some(4096),
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        };

        let mut http = match EspHttpConnection::new(&http_conf) {
            Ok(conn) => HttpClient::wrap(conn),
            Err(e) => {
                log::warn!("OTA: HTTP connection init failed: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!("{{\"state\":\"failed\",\"reason\":\"HTTP init: {:?}\"}}", e),
                );
                continue;
            }
        };

        let request = match http.get(&url) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("OTA: HTTP GET failed: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!("{{\"state\":\"failed\",\"reason\":\"HTTP GET: {:?}\"}}", e),
                );
                continue;
            }
        };

        let mut response = match request.submit() {
            Ok(r) => r,
            Err(e) => {
                log::warn!("OTA: HTTP submit failed: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!("{{\"state\":\"failed\",\"reason\":\"HTTP submit: {:?}\"}}", e),
                );
                continue;
            }
        };

        let status_code = response.status();
        if status_code != 200 {
            log::warn!("OTA: HTTP {} response", status_code);
            publish_status(
                &mqtt_client,
                &status_topic,
                &format!("{{\"state\":\"failed\",\"reason\":\"HTTP {}\"}}", status_code),
            );
            continue;
        }

        // --- Step 6: Initiate OTA (erases partition — takes 4-8s) ---
        let mut ota = match EspOta::new() {
            Ok(o) => o,
            Err(e) => {
                log::warn!("OTA: EspOta::new() failed: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!("{{\"state\":\"failed\",\"reason\":\"OTA init: {:?}\"}}", e),
                );
                continue;
            }
        };

        let mut update = match ota.initiate_update() {
            Ok(u) => u,
            Err(e) => {
                log::warn!("OTA: initiate_update() failed: {:?}", e);
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!(
                        "{{\"state\":\"failed\",\"reason\":\"initiate_update: {:?}\"}}",
                        e
                    ),
                );
                continue;
            }
        };

        // --- Step 7: Streaming download + write loop ---
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 4096];
        let mut bytes_written: u64 = 0;
        let mut last_progress: u64 = 0;
        let mut loop_error: Option<String> = None;

        loop {
            let n = match response.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    log::warn!("OTA: read error after {} bytes: {:?}", bytes_written, e);
                    loop_error = Some(format!("read error: {:?}", e));
                    break;
                }
            };

            if n == 0 {
                break;
            }

            hasher.update(&buf[..n]);

            if let Err(e) = update.write(&buf[..n]) {
                log::warn!("OTA: write error after {} bytes: {:?}", bytes_written, e);
                loop_error = Some(format!("write error: {:?}", e));
                break;
            }

            bytes_written += n as u64;

            // Publish progress every 64 KB to avoid flooding broker
            if bytes_written - last_progress >= 65536 {
                last_progress = bytes_written;
                publish_status(
                    &mqtt_client,
                    &status_topic,
                    &format!(
                        "{{\"state\":\"downloading\",\"progress\":{}}}",
                        bytes_written
                    ),
                );
            }
        }

        if let Some(err) = loop_error {
            // update drops here — EspOtaUpdate::drop() calls esp_ota_abort automatically
            publish_status(
                &mqtt_client,
                &status_topic,
                &format!("{{\"state\":\"failed\",\"reason\":\"{}\"}}", err),
            );
            continue;
        }

        log::info!("OTA: download complete — {} bytes written", bytes_written);

        // --- Step 8: SHA-256 verification ---
        let hash: [u8; 32] = hasher.finalize().into();
        let actual_hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

        if actual_hex != sha256 {
            log::warn!(
                "OTA: sha256 mismatch: expected {} got {}",
                sha256,
                actual_hex
            );
            // update drops here — esp_ota_abort called automatically
            publish_status(
                &mqtt_client,
                &status_topic,
                &format!(
                    "{{\"state\":\"failed\",\"reason\":\"sha256 mismatch: expected {} got {}\"}}",
                    sha256, actual_hex
                ),
            );
            continue;
        }

        log::info!("OTA: SHA-256 verified OK");

        // --- Step 9: Finalize ---
        if let Err(e) = update.complete() {
            log::warn!("OTA: complete() failed: {:?}", e);
            publish_status(
                &mqtt_client,
                &status_topic,
                &format!("{{\"state\":\"failed\",\"reason\":\"complete: {:?}\"}}", e),
            );
            continue;
        }

        // --- Step 10: Publish complete ---
        publish_status(&mqtt_client, &status_topic, "{\"state\":\"complete\"}");

        // --- Step 11: Clear retained trigger (empty payload + retain=true) ---
        match mqtt_client.lock() {
            Err(e) => log::warn!("OTA: trigger clear mutex poisoned: {:?}", e),
            Ok(mut c) => {
                match c.enqueue(&trigger_topic, QoS::AtLeastOnce, true, b"") {
                    Ok(_) => log::info!("OTA: retained trigger cleared"),
                    Err(e) => log::warn!("OTA: trigger clear failed: {:?}", e),
                }
            }
        }

        // --- Step 12: Sleep to allow MQTT enqueue to complete before restart ---
        std::thread::sleep(Duration::from_millis(500));

        // --- Step 13: Reboot into new partition ---
        log::info!("OTA: complete — restarting");
        restart();
    } // end inner payload processing block
    } // end outer recv_timeout loop

    // Dead-end park (pump exited; thread has nothing to do).
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Spawn the OTA task in a dedicated thread with 16384-byte stack.
///
/// The larger stack is required because the OTA thread runs:
/// - HTTP client (heap-allocated, but initialization uses stack)
/// - SHA-256 hasher state
/// - EspOtaUpdate handle
/// - 4096-byte read buffer (heap via vec!, not stack)
///
/// Returns Ok(()) immediately; the thread runs independently.
pub fn spawn_ota(
    mqtt_client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    ota_rx: Receiver<Vec<u8>>,
    nvs: EspNvsPartition<NvsDefault>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .stack_size(16384)
        .spawn(move || ota_task(mqtt_client, device_id, ota_rx, nvs))
        .map(|_| ())
        .map_err(Into::into)
}
