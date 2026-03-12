//! Server configuration loading via figment (TOML + environment variable overrides).
//!
//! # Example config.toml
//!
//! ```toml
//! # device_id = "FFFEB5"
//! # [mqtt]
//! # broker = "localhost"
//! # port = 1883
//! # client_id = "gnss-server-1"
//! # Override with: GNSS_MQTT__PASSWORD=secret ./gnss-server --config config.toml
//! ```
//!
//! Environment variables prefixed `GNSS_` override TOML values.
//! Double underscores `__` represent nesting separators.
//! Example: `GNSS_MQTT__PASSWORD=secret` overrides `mqtt.password`.

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};

/// MQTT broker connection settings.
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub struct MqttConfig {
    /// Hostname or IP address of the MQTT broker, e.g. "localhost"
    pub broker: String,
    /// Port number, e.g. 1883
    pub port: u16,
    /// Unique client identifier for this server instance
    pub client_id: String,
    /// Optional MQTT username
    pub username: Option<String>,
    /// Optional MQTT password
    pub password: Option<String>,
}

/// Top-level server configuration.
#[derive(Debug, serde::Deserialize)]
pub struct ServerConfig {
    /// Device identifier, e.g. "FFFEB5"
    pub device_id: String,
    /// MQTT broker connection settings
    pub mqtt: MqttConfig,
}

/// Load server configuration from a TOML file, with environment variable overrides.
///
/// TOML file is read first; environment variables prefixed `GNSS_` are merged on top.
/// Double underscores `__` in env var names map to nested config keys.
pub fn load_config(path: &str) -> anyhow::Result<ServerConfig> {
    let config = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("GNSS_").split("__"))
        .extract::<ServerConfig>()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::{providers::Format, Figment};

    #[test]
    fn test_deserialize_minimal_toml() {
        let toml = r#"
device_id = "TESTDEV"

[mqtt]
broker = "mqtt.example.com"
port = 1883
client_id = "test-server"
"#;
        let config: ServerConfig = Figment::new()
            .merge(Toml::string(toml))
            .extract()
            .expect("should deserialize minimal TOML");

        assert_eq!(config.device_id, "TESTDEV");
        assert_eq!(config.mqtt.broker, "mqtt.example.com");
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.mqtt.client_id, "test-server");
        assert!(config.mqtt.username.is_none());
        assert!(config.mqtt.password.is_none());
    }

    #[test]
    fn test_deserialize_with_credentials() {
        let toml = r#"
device_id = "FFFEB5"

[mqtt]
broker = "broker.example.com"
port = 8883
client_id = "gnss-server-1"
username = "user"
password = "secret"
"#;
        let config: ServerConfig = Figment::new()
            .merge(Toml::string(toml))
            .extract()
            .expect("should deserialize TOML with credentials");

        assert_eq!(config.mqtt.username, Some("user".to_string()));
        assert_eq!(config.mqtt.password, Some("secret".to_string()));
    }

    #[test]
    fn test_env_override() {
        // Test that figment merge order: TOML first, env second (env wins)
        let toml = r#"
device_id = "ORIGINAL"

[mqtt]
broker = "localhost"
port = 1883
client_id = "test"
"#;
        // We test the merge logic works by layering two TOML sources
        // (env var testing would require setting process env, which is fragile in tests)
        let override_toml = r#"
device_id = "OVERRIDDEN"
"#;
        let config: ServerConfig = Figment::new()
            .merge(Toml::string(toml))
            .merge(Toml::string(override_toml))
            .extract()
            .expect("should allow override");

        assert_eq!(config.device_id, "OVERRIDDEN");
        // mqtt fields from base TOML should still be present
        assert_eq!(config.mqtt.broker, "localhost");
    }
}
