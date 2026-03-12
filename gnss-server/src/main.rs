mod config;
mod epoch;
mod mqtt;
mod nmea_parse;
mod observation;
mod rinex_writer;
mod rtcm_decode;
mod web_server;

use std::sync::Arc;

use clap::Parser;
use tokio::sync::{broadcast, mpsc, watch};

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
    let (ws_tx, _ws_rx_discard) = broadcast::channel::<String>(16);

    let config_arc = Arc::new(config);

    // Create output directory at startup (before spawning tasks)
    std::fs::create_dir_all(&config_arc.output_dir)?;

    tokio::spawn(web_server::run_web_server(
        config_arc.http_port,
        ws_tx.clone(),
    ));

    tokio::spawn(mqtt::mqtt_supervisor(
        Arc::clone(&config_arc),
        msg_tx,
        state_tx,
    ));

    // Spawn RTCM decode task — reads MqttMessage from channel and decodes frames.
    tokio::spawn(run_decode_task(
        msg_rx,
        config_arc.output_dir.clone(),
        config_arc.device_id.clone(),
        ws_tx,
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
/// Reads MqttMessage from the channel, decodes RTCM frames and writes RINEX,
/// fans NMEA GSV data and heartbeat messages to WebSocket clients via ws_tx.
async fn run_decode_task(
    mut msg_rx: mpsc::Receiver<mqtt::MqttMessage>,
    output_dir: String,
    station: String,
    ws_tx: broadcast::Sender<String>,
) {
    let gps_week = rinex_writer::current_gps_week();
    let mut epoch_buf = epoch::EpochBuffer::new();
    let mut obs_writer = rinex_writer::RinexObsWriter::new(&output_dir, station.clone(), gps_week);
    let mut nav_writer = rinex_writer::RinexNavWriter::new(&output_dir, station, gps_week);
    let mut gsv_acc = nmea_parse::GsvAccumulator::new();

    while let Some(msg) = msg_rx.recv().await {
        match msg {
            mqtt::MqttMessage::Rtcm(payload) => {
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
            mqtt::MqttMessage::Nmea(payload) => {
                if let Ok(s) = std::str::from_utf8(&payload) {
                    if let Some(state) = gsv_acc.feed(s) {
                        if let Ok(json) = serde_json::to_string(&state) {
                            let _ = ws_tx.send(json);
                        }
                    }
                }
            }
            mqtt::MqttMessage::Heartbeat(payload) => {
                if let Some(tagged) = nmea_parse::tag_heartbeat(&payload) {
                    let _ = ws_tx.send(tagged);
                }
            }
        }
    }
}
