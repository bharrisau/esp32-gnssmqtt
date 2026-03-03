//! esp32-gnssmqtt firmware entry point.
//!
//! Initialization order is MANDATORY:
//! 1. esp_idf_svc::sys::link_patches() — MUST be first, applies linker patches
//! 2. EspLogger::initialize_default() — MUST be before any log:: calls
//! 3. Everything else

use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::log::EspLogger;

mod config;
mod device_id;
mod uart_bridge;

fn main() {
    // Step 1: Apply ESP-IDF linker patches — MUST be called before anything else.
    // Omitting this causes a hard fault at boot.
    esp_idf_svc::sys::link_patches();

    // Step 2: Initialize the ESP-IDF logging backend.
    // MUST be called before any log::info!/warn!/error! calls.
    EspLogger::initialize_default();

    let id = device_id::get();
    log::info!("=== esp32-gnssmqtt booting ===");
    log::info!("Device ID: {}", id);
    log::info!("Build: {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // Phase 1 scaffold: idle loop.
    // Phase 2 will replace this with WiFi + MQTT initialization.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        log::info!("Heartbeat — Device ID: {}", id);
    }
}
