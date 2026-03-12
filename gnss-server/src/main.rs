mod config;
mod mqtt;

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

    let (msg_tx, mut msg_rx) = mpsc::channel::<mqtt::MqttMessage>(256);
    let (state_tx, mut state_rx) = watch::channel(false);

    let config_arc = Arc::new(config);

    tokio::spawn(mqtt::mqtt_supervisor(
        config_arc,
        msg_tx,
        state_tx,
    ));

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Shutting down");
                break;
            }
            Some(msg) = msg_rx.recv() => {
                log::debug!("Received MQTT message: {msg:?}");
            }
            Ok(()) = state_rx.changed() => {
                let connected = *state_rx.borrow();
                log::info!("MQTT connection state: {connected}");
            }
        }
    }

    Ok(())
}
