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
