use std::sync::Arc;

use bytes::Bytes;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::{mpsc, watch};
use tokio::time::Duration;

use crate::config::ServerConfig;

/// Messages received from MQTT broker, tagged by topic.
#[derive(Debug)]
#[allow(dead_code)]
pub enum MqttMessage {
    Rtcm(Bytes),
    Nmea(Bytes),
    Heartbeat(Bytes),
}

/// Reconnect backoff steps in seconds: [1, 2, 5, 10, 30].
const BACKOFF_STEPS: [u64; 5] = [1, 2, 5, 10, 30];

/// Map a topic string to an MqttMessage variant.
///
/// Returns None if the topic does not match a known suffix.
fn topic_to_message(topic: &str, payload: Bytes) -> Option<MqttMessage> {
    if topic.ends_with("/rtcm") {
        Some(MqttMessage::Rtcm(payload))
    } else if topic.ends_with("/nmea") {
        Some(MqttMessage::Nmea(payload))
    } else if topic.ends_with("/heartbeat") {
        Some(MqttMessage::Heartbeat(payload))
    } else {
        None
    }
}

/// MQTT supervisor task.
///
/// Owns the EventLoop. Reconnects with exponential backoff [1, 2, 5, 10, 30]s.
/// Broadcasts connection state via `state_tx` (true on ConnAck, false on error).
/// Forwards incoming publish payloads to `msg_tx` tagged by topic.
pub async fn mqtt_supervisor(
    config: Arc<ServerConfig>,
    msg_tx: mpsc::Sender<MqttMessage>,
    state_tx: watch::Sender<bool>,
) {
    let mut fail_count: usize = 0;

    loop {
        let mut opts = MqttOptions::new(
            config.mqtt.client_id.clone(),
            config.mqtt.broker.clone(),
            config.mqtt.port,
        );
        if let (Some(user), Some(pass)) = (
            config.mqtt.username.as_deref(),
            config.mqtt.password.as_deref(),
        ) {
            opts.set_credentials(user, pass);
        }
        opts.set_keep_alive(Duration::from_secs(60));

        let (client, mut eventloop) = AsyncClient::new(opts, 64);

        let device_id = &config.device_id;
        let topics = [
            format!("gnss/{device_id}/rtcm"),
            format!("gnss/{device_id}/nmea"),
            format!("gnss/{device_id}/heartbeat"),
        ];

        // Enqueue subscriptions before poll — they will be sent when the first poll() runs
        for topic in &topics {
            if let Err(e) = client.subscribe(topic, QoS::AtMostOnce).await {
                log::warn!("Failed to enqueue subscription for {topic}: {e}");
            }
        }

        // Inner poll loop
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    log::info!("MQTT connected (device {device_id})");
                    let _ = state_tx.send(true);
                    fail_count = 0;
                }
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    if let Some(msg) = topic_to_message(&p.topic, p.payload.clone()) {
                        // Non-blocking: drop message if channel is full
                        if msg_tx.try_send(msg).is_err() {
                            log::warn!("msg_tx full, dropping MQTT message on topic {}", p.topic);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("MQTT connection error: {e}");
                    let _ = state_tx.send(false);
                    break;
                }
                _ => {}
            }
        }

        // Exponential backoff before reconnect
        let delay = BACKOFF_STEPS[fail_count.min(BACKOFF_STEPS.len() - 1)];
        fail_count += 1;
        log::info!("MQTT reconnect in {delay}s (attempt {fail_count})");
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_backoff_sequence() {
        // Verify the backoff steps are [1, 2, 5, 10, 30]
        assert_eq!(BACKOFF_STEPS, [1, 2, 5, 10, 30]);
    }

    #[test]
    fn test_backoff_capped_at_30() {
        // fail_count beyond array length should stay at 30s
        let idx_4 = 4_usize.min(BACKOFF_STEPS.len() - 1);
        let idx_5 = 5_usize.min(BACKOFF_STEPS.len() - 1);
        let idx_100 = 100_usize.min(BACKOFF_STEPS.len() - 1);
        assert_eq!(BACKOFF_STEPS[idx_4], 30);
        assert_eq!(BACKOFF_STEPS[idx_5], 30);
        assert_eq!(BACKOFF_STEPS[idx_100], 30);
    }

    #[test]
    fn test_mqtt_message_variants() {
        let rtcm = MqttMessage::Rtcm(Bytes::new());
        let nmea = MqttMessage::Nmea(Bytes::new());
        let heartbeat = MqttMessage::Heartbeat(Bytes::new());

        // Verify Debug output contains variant name
        assert!(format!("{rtcm:?}").contains("Rtcm"));
        assert!(format!("{nmea:?}").contains("Nmea"));
        assert!(format!("{heartbeat:?}").contains("Heartbeat"));
    }

    #[test]
    fn test_topic_to_variant_rtcm() {
        let payload = Bytes::from_static(b"\xd3\x00\x13");
        let msg = topic_to_message("gnss/FFFEB5/rtcm", payload.clone());
        assert!(matches!(msg, Some(MqttMessage::Rtcm(_))));
    }

    #[test]
    fn test_topic_to_variant_nmea() {
        let payload = Bytes::from_static(b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n");
        let msg = topic_to_message("gnss/FFFEB5/nmea", payload.clone());
        assert!(matches!(msg, Some(MqttMessage::Nmea(_))));
    }

    #[test]
    fn test_topic_to_variant_heartbeat() {
        let payload = Bytes::from_static(b"{}");
        let msg = topic_to_message("gnss/FFFEB5/heartbeat", payload.clone());
        assert!(matches!(msg, Some(MqttMessage::Heartbeat(_))));
    }

    #[test]
    fn test_topic_to_variant_unknown() {
        let payload = Bytes::new();
        let msg = topic_to_message("gnss/FFFEB5/unknown", payload);
        assert!(msg.is_none());
    }
}
