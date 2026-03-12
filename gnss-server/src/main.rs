mod config;

use clap::Parser;

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
    tokio::signal::ctrl_c().await?;
    Ok(())
}
