//! Compile-time configuration constants.
//!
//! Phase 1: All connectivity constants are empty-string stubs.
//! Phase 2 will fill these in with real values (WiFi SSID/pass, MQTT host/port/credentials).
//!
//! DO NOT use todo!() — that panics at runtime. Empty strings are safe no-ops for Phase 1.

/// WiFi credentials (filled by Phase 2)
pub const WIFI_SSID: &str = "";
pub const WIFI_PASS: &str = "";

/// MQTT broker connection (filled by Phase 2)
pub const MQTT_HOST: &str = "";
pub const MQTT_PORT: u16 = 1883;
pub const MQTT_USER: &str = "";
pub const MQTT_PASS: &str = "";

/// UART RX ring buffer size for the UM980 UART driver (Phase 2).
/// This value MUST be passed as rx_buffer_size in uart_driver_install().
/// There is no sdkconfig Kconfig option for this in ESP-IDF v5 — it is runtime-only.
pub const UART_RX_BUF_SIZE: usize = 4096;

/// Blocking channel receive timeout for hot-path relay threads (NMEA, RTCM, GNSS TX).
/// At 10 Hz NMEA rate, timeouts occur only if GNSS pipeline stalls — used as a liveness check.
pub const RELAY_RECV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Blocking channel receive timeout for low-rate threads (config relay, OTA, subscriber).
/// These threads receive events rarely; timeout simply prevents indefinite hang if producer dies.
pub const SLOW_RECV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum consecutive WiFi reconnect failures before logging an error.
/// After this many consecutive failures, LedState::Error is set.
/// Phase 12 (RESIL-01) will add esp_restart() at this threshold.
pub const MAX_WIFI_RECONNECT_ATTEMPTS: u32 = 20;
