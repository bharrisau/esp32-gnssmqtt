//! MQTT client, LWT, connection pump, and heartbeat. Uses EspMqttClient from esp-idf-svc.

use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttConnection,
    LwtConfiguration, MqttClientConfiguration,
};
use embedded_svc::mqtt::client::{EventPayload, QoS};
use std::sync::{Arc, Mutex};

/// Create an MQTT client with LWT configured.
///
/// Returns `(client, connection)` where:
/// - `client` is wrapped in `Arc<Mutex<>>` for sharing across threads
/// - `connection` MUST be moved into the pump thread before any publish/subscribe call
///
/// LWT: publishes `offline` to `gnss/{device_id}/status` with retain=true on unexpected disconnect.
pub fn mqtt_connect(
    device_id: &str,
) -> anyhow::Result<(Arc<Mutex<EspMqttClient<'static>>>, EspMqttConnection)> {
    let broker_url = format!(
        "mqtt://{}:{}",
        crate::config::MQTT_HOST,
        crate::config::MQTT_PORT
    );

    // IMPORTANT: lwt_topic MUST be declared BEFORE conf in the same scope.
    // LwtConfiguration.topic is &'a str — it must outlive the MqttClientConfiguration.
    // See research pitfall 1.
    let lwt_topic = format!("gnss/{}/status", device_id);

    let conf = MqttClientConfiguration {
        client_id: Some(device_id),
        username: if crate::config::MQTT_USER.is_empty() {
            None
        } else {
            Some(crate::config::MQTT_USER)
        },
        password: if crate::config::MQTT_PASS.is_empty() {
            None
        } else {
            Some(crate::config::MQTT_PASS)
        },
        lwt: Some(LwtConfiguration {
            topic: &lwt_topic,
            payload: b"offline",
            qos: QoS::AtLeastOnce,
            retain: true,
        }),
        keep_alive_interval: Some(std::time::Duration::from_secs(60)),
        reconnect_timeout: Some(std::time::Duration::from_secs(5)),
        ..Default::default()
    };

    let (client, connection) = EspMqttClient::new(&broker_url, &conf)?;
    Ok((Arc::new(Mutex::new(client)), connection))
}

/// Drive the MQTT event loop forever.
///
/// This function MUST be called in a dedicated thread BEFORE any `client.publish()` or
/// `client.subscribe()` call. Without this pump running, all client operations block
/// indefinitely (see research pitfall 2).
///
/// On every `Connected` event, re-subscribes to the device config topic. This handles
/// broker restarts where session state is lost (see research pitfall 4).
pub fn pump_mqtt_events(
    mut connection: EspMqttConnection,
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
) -> ! {
    while let Ok(event) = connection.next() {
        match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("MQTT connected — re-subscribing");
                if let Ok(mut c) = client.lock() {
                    let topic = format!("gnss/{}/config", device_id);
                    let _ = c.subscribe(&topic, QoS::AtLeastOnce);
                }
            }
            EventPayload::Disconnected => {
                log::warn!("MQTT disconnected");
            }
            EventPayload::Error(e) => {
                log::error!("MQTT error: {:?}", e);
            }
            _ => {}
        }
    }

    // Connection closed — should not happen in normal operation.
    // Loop forever to keep the thread alive rather than returning.
    log::error!("MQTT pump exited — connection closed");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Publish a retained heartbeat to `gnss/{device_id}/heartbeat` every 30 seconds.
///
/// Waits 5 seconds before the first publish to give the MQTT stack time to fully connect
/// and for the pump thread to process the initial `Connected` event.
///
/// Uses `client.publish()` (blocking) — acceptable here because this runs in its own
/// dedicated thread and the pump thread keeps the outbox moving.
pub fn heartbeat_loop(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
) -> ! {
    let topic = format!("gnss/{}/heartbeat", device_id);

    // Initial delay — give MQTT time to fully connect before first heartbeat.
    std::thread::sleep(std::time::Duration::from_secs(5));

    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));

        // Use if-let to avoid panicking on mutex poison.
        if let Ok(mut c) = client.lock() {
            if let Err(e) = c.publish(&topic, QoS::AtMostOnce, true, b"online") {
                log::warn!("Heartbeat publish failed: {:?}", e);
            }
        }
    }
}
