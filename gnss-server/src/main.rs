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

    // Create output directory at startup (before spawning tasks)
    std::fs::create_dir_all(&config_arc.output_dir)?;

    tokio::spawn(mqtt::mqtt_supervisor(
        Arc::clone(&config_arc),
        msg_tx,
        state_tx,
    ));

    // Spawn RTCM decode task — reads MqttMessage::Rtcm from channel and decodes frames.
    tokio::spawn(run_decode_task(
        msg_rx,
        config_arc.output_dir.clone(),
        config_arc.device_id.clone(),
    ));

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
/// forwards EpochGroup events to RinexObsWriter and EphemerisMsg events to RinexNavWriter.
async fn run_decode_task(
    mut msg_rx: mpsc::Receiver<mqtt::MqttMessage>,
    output_dir: String,
    station: String,
) {
    let gps_week = rinex_writer::current_gps_week();
    let mut epoch_buf = epoch::EpochBuffer::new();
    let mut obs_writer = rinex_writer::RinexObsWriter::new(&output_dir, station.clone(), gps_week);
    let mut nav_writer = rinex_writer::RinexNavWriter::new(&output_dir, station, gps_week);

    while let Some(msg) = msg_rx.recv().await {
        if let mqtt::MqttMessage::Rtcm(payload) = msg {
            let events = rtcm_decode::decode_rtcm_payload(&payload, &mut epoch_buf);
            for event in events {
                match event {
                    observation::RtcmEvent::Epoch(group) => {
                        let epoch_utc =
                            rinex_writer::gps_tow_to_utc(gps_week, group.epoch_ms);
                        if let Err(e) = obs_writer.write_group(&epoch_utc, &group) {
                            log::warn!("RINEX obs write error: {e}");
                        }
                    }
                    observation::RtcmEvent::Ephemeris(eph) => {
                        let epoch_utc = chrono::Utc::now();
                        if let Err(e) = nav_writer.write_ephemeris(&epoch_utc, &eph) {
                            log::warn!("RINEX nav write error: {e}");
                        }
                    }
                }
            }
        }
    }
}
