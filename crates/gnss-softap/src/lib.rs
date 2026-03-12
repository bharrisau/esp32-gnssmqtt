#![no_std]

/// Runs a SoftAP captive portal for initial device provisioning.
///
/// The portal starts a WiFi access point, serves an HTTP configuration form,
/// and collects WiFi + MQTT + NTRIP credentials from the user.
///
/// # Implementations
///
/// - ESP-IDF: `provisioning.rs` using `EspHttpServer` + `BlockingWifi` (std only)
/// - nostd: partially unblocked — see `BLOCKER.md`
pub trait SoftApPortal {
    /// Error type for SoftAP operations.
    type Error: core::fmt::Debug;

    /// Start the SoftAP with the given SSID and WPA2-PSK password.
    ///
    /// The SSID format used by the firmware is `GNSS-{device_id}`.
    /// The WPA2 PSK is the same value as the device ID.
    fn start(&mut self, ssid: &str, password: &str) -> Result<(), Self::Error>;

    /// Block until a valid credential set is submitted via the HTTP configuration form.
    ///
    /// Returns the collected credentials on success. Implementations should handle
    /// DNS hijacking (redirecting all DNS queries to the portal IP) internally.
    fn wait_for_credentials(&mut self) -> Result<ProvisioningCredentials, Self::Error>;

    /// Stop the SoftAP and release WiFi resources.
    fn stop(&mut self) -> Result<(), Self::Error>;
}

/// Credentials collected by the SoftAP provisioning portal.
///
/// Field sizes use fixed-capacity stack buffers to remain heap-free.
/// All strings are UTF-8.
pub struct ProvisioningCredentials {
    /// WiFi SSID (up to 32 bytes per IEEE 802.11)
    pub wifi_ssid: [u8; 32],
    pub wifi_ssid_len: usize,
    /// WiFi WPA2 password (up to 63 bytes)
    pub wifi_password: [u8; 64],
    pub wifi_password_len: usize,
    /// MQTT broker hostname or IP (up to 64 bytes)
    pub mqtt_host: [u8; 64],
    pub mqtt_host_len: usize,
    /// MQTT broker port
    pub mqtt_port: u16,
    /// NTRIP caster hostname (up to 64 bytes)
    pub ntrip_host: [u8; 64],
    pub ntrip_host_len: usize,
    /// NTRIP caster port
    pub ntrip_port: u16,
    /// NTRIP mountpoint (up to 32 bytes)
    pub ntrip_mount: [u8; 32],
    pub ntrip_mount_len: usize,
}
