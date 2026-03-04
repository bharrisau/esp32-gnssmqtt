//! MQTT client, LWT, connection pump, and heartbeat. Uses EspMqttClient from esp-idf-svc.

use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttConnection,
    LwtConfiguration, MqttClientConfiguration,
};
use embedded_svc::mqtt::client::{EventPayload, QoS};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use crate::led::LedState;

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
        disable_clean_session: true,
        ..Default::default()
    };

    let (client, connection) = EspMqttClient::new(&broker_url, &conf)?;
    Ok((Arc::new(Mutex::new(client)), connection))
}

/// Drive the MQTT event loop forever.
///
/// On every `Connected` event, writes LedState::Connected to the shared LED state and
/// sends a signal through `subscribe_tx` so that the subscriber thread can (re-)subscribe.
/// On `Disconnected`, writes LedState::Connecting.
///
/// The pump itself NEVER calls any client method — doing so would deadlock because the
/// C MQTT task holds its internal mutex while dispatching the event callback (see research
/// pitfall 2). Atomic stores are NOT client method calls and are safe here.
pub fn pump_mqtt_events(
    mut connection: EspMqttConnection,
    subscribe_tx: Sender<()>,
    led_state: Arc<AtomicU8>,
) -> ! {
    while let Ok(event) = connection.next() {
        match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("MQTT connected");
                led_state.store(LedState::Connected as u8, Ordering::Relaxed);
                let _ = subscribe_tx.send(());
            }
            EventPayload::Disconnected => {
                log::warn!("MQTT disconnected");
                led_state.store(LedState::Connecting as u8, Ordering::Relaxed);
            }
            EventPayload::Error(e) => {
                log::error!("MQTT error: {:?}", e);
            }
            m @ _ => {
                log::warn!("Unhandled message: {:?}", m);
            }
        }
    }

    // Connection closed — should not happen in normal operation.
    log::error!("MQTT pump exited — connection closed");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// Subscribe to the device config topic on every Connected signal from the pump.
///
/// By the time this thread receives a signal, the pump has already called
/// `connection.next()` again, which releases the C MQTT internal mutex — so
/// `subscribe()` here is safe (no deadlock).
///
/// Handles both initial connection and broker restarts (CONN-04).
pub fn subscriber_loop(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
    subscribe_rx: Receiver<()>,
) -> ! {
    let topic = format!("gnss/{}/config", device_id);
    for () in &subscribe_rx {
        match client.lock() {
            Err(e) => log::warn!("Subscriber mutex poisoned: {:?}", e),
            Ok(mut c) => match c.subscribe(&topic, QoS::AtLeastOnce) {
                Ok(_) => log::info!("Subscribed to {}", topic),
                Err(e) => log::warn!("Subscribe failed: {:?}", e),
            },
        }
    }
    log::error!("Subscriber channel closed");
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
    log::info!("Heartbeat thread started, topic: {}", topic);

    // Initial delay — give MQTT time to fully connect before first heartbeat.
    std::thread::sleep(std::time::Duration::from_secs(5));

    loop {
        log::info!("Heartbeat attempting publish...");

        match client.lock() {
            Err(e) => log::warn!("Heartbeat mutex poisoned: {:?}", e),
            Ok(mut c) =>  match c.enqueue(&topic, QoS::AtMostOnce, true, b"online") {
                Ok(_) => log::info!("Heartbeat published to {}", topic),
                Err(e) => log::warn!("Heartbeat publish failed: {:?}", e),
            },
        }

        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
