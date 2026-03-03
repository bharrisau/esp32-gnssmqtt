//! Device ID module — reads factory-programmed MAC address from hardware eFuse.
//!
//! The ID is derived from the base MAC address via `esp_efuse_mac_get_default`.
//! This reads OTP (one-time programmable) eFuse — stable forever, unique per device.
//! Format: last 3 bytes of the 6-byte MAC, uppercase hex (6 characters, e.g. "A1B2C3").
//! The first 3 bytes are the Espressif OUI (same for all devices); only the last 3 are unique.

use esp_idf_svc::sys::{esp_efuse_mac_get_default, ESP_OK};

/// Returns the last 3 bytes of the factory MAC as a 6-char uppercase hex string.
///
/// This string is stable across power cycles — it reads from hardware eFuse, not RAM.
/// Example output: "A1B2C3"
///
/// # Panics
/// Panics if `esp_efuse_mac_get_default` returns a non-OK error code.
/// This indicates eFuse CRC corruption — a hardware manufacturing defect that
/// should never occur on a normally functioning device.
pub fn get() -> String {
    let mut mac = [0u8; 6];
    // SAFETY: mac.as_mut_ptr() points to a valid 6-byte stack buffer.
    // esp_efuse_mac_get_default is a pure read of OTP eFuse with no side effects.
    let ret = unsafe { esp_efuse_mac_get_default(mac.as_mut_ptr()) };
    assert_eq!(
        ret,
        ESP_OK as i32,
        "esp_efuse_mac_get_default failed: err={}",
        ret
    );
    format!("{:02X}{:02X}{:02X}", mac[3], mac[4], mac[5])
}
