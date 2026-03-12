mod config;
mod epoch;
mod mqtt;
mod observation;
mod rinex_writer;
mod rtcm_decode;

use std::sync::Arc;

use clap::Parser;
use tokio::sync::{mpsc, watch};

#[derive(Parser)]
#[command(name = "gnss-server", about = "GNSS MQTT server")]
struct Cli {
    /// Path to configuration TOML file
    #[arg(long)]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let config = config::load_config(&cli.config)?;
    log::info!(
        "Starting gnss-server for device {} (broker {}:{})",
        config.device_id,
        config.mqtt.broker,
        config.mqtt.port
    );

    let (msg_tx, msg_rx) = mpsc::channel::<mqtt::MqttMessage>(256);
    let (state_tx, mut state_rx) = watch::channel(false);

    let config_arc = Arc::new(config);

    tokio::spawn(mqtt::mqtt_supervisor(
        config_arc,
        msg_tx,
        state_tx,
    ));

    // Spawn RTCM decode task — reads MqttMessage::Rtcm from channel and decodes frames.
    // Phase 24 RINEX writer will consume EpochGroup events; for now they are logged and discarded.
    tokio::spawn(run_decode_task(msg_rx));

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Shutting down");
                break;
            }
            Ok(()) = state_rx.changed() => {
                let connected = *state_rx.borrow();
                log::info!("MQTT connection state: {connected}");
            }
        }
    }

    Ok(())
}

/// RTCM3 decode task.
///
/// Reads MqttMessage from the channel, calls decode_rtcm_payload() for Rtcm variants,
/// and logs epoch boundary events. EpochGroup events are discarded here; Phase 24 will
/// wire them to the RINEX writer.
async fn run_decode_task(mut msg_rx: mpsc::Receiver<mqtt::MqttMessage>) {
    let mut epoch_buf = epoch::EpochBuffer::new();
    while let Some(msg) = msg_rx.recv().await {
        if let mqtt::MqttMessage::Rtcm(payload) = msg {
            let events = rtcm_decode::decode_rtcm_payload(&payload, &mut epoch_buf);
            for event in events {
                match event {
                    observation::RtcmEvent::Epoch(group) => {
                        // Epoch log line is emitted inside EpochBuffer::build_group().
                        // Discard group here; Phase 24 RINEX writer will consume it.
                        log::debug!(
                            "Epoch flushed: epoch_ms={} gps={} glo={} gal={} bds={}",
                            group.epoch_ms,
                            group.gps_count,
                            group.glo_count,
                            group.gal_count,
                            group.bds_count,
                        );
                    }
                    observation::RtcmEvent::Ephemeris(_eph) => {
                        log::debug!("Ephemeris message received (discarded — Phase 24 will store)");
                    }
                }
            }
        }
    }
}
