//! esp32-gnssmqtt firmware entry point.
//!
//! Initialization order is MANDATORY:
//! 1. esp_idf_svc::sys::link_patches() — MUST be first, applies linker patches
//! 2. EspLogger::initialize_default() — MUST be before any log:: calls
//! 3. Peripherals::take() — take hardware ownership
//! 4. EspSystemEventLoop::take() — required by WiFi
//! 5. EspDefaultNvsPartition::take() — required by WiFi
//! 6. wifi::wifi_connect — WiFi BEFORE MQTT (IP required for TCP)
//! 7. uart_bridge::spawn_bridge — UART bridge (independent, after WiFi)
//! 8. mqtt::mqtt_connect — MQTT AFTER WiFi (TCP must be up)
//! 9. Create subscribe_tx/rx channel
//! 10. Spawn pump thread (signals subscriber on Connected)
//! 11. Spawn subscriber thread (subscribes on Connected signal)
//! 12. Spawn heartbeat thread
//! 13. Spawn wifi supervisor thread
//! 14. Main thread: idle loop

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

mod config;
mod device_id;
mod led;
mod mqtt;
mod uart_bridge;
mod wifi;

fn main() {
    // Step 1: Apply ESP-IDF linker patches — MUST be called before anything else.
    // Omitting this causes a hard fault at boot.
    esp_idf_svc::sys::link_patches();

    // Step 2: Initialize the ESP-IDF logging backend.
    // MUST be called before any log::info!/warn!/error! calls.
    EspLogger::initialize_default();

    let device_id = device_id::get();
    log::info!("=== esp32-gnssmqtt booting ===");
    log::info!("Device ID: {}", device_id);
    log::info!("Build: {} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // Step 3: Take hardware peripherals
    let peripherals = Peripherals::take().expect("peripherals already taken");

    // Step 4: System event loop (required by WiFi)
    let sysloop = EspSystemEventLoop::take().expect("sysloop already taken");

    // Step 5: NVS partition (required by WiFi)
    let nvs = EspDefaultNvsPartition::take().expect("NVS already taken");

    // Step 6: WiFi — must be before MQTT (TCP requires IP)
    log::info!("Connecting to WiFi...");
    let wifi = wifi::wifi_connect(peripherals.modem, sysloop.clone(), nvs)
        .expect("WiFi connect failed");
    log::info!("WiFi connected");

    // Step 7: UART bridge (UM980 on UART0, GPIO16 TX / GPIO17 RX)
    uart_bridge::spawn_bridge(
        peripherals.uart0,
        peripherals.pins.gpio16, // TX to UM980
        peripherals.pins.gpio17, // RX from UM980
    )
    .expect("UART bridge init failed");
    log::info!("UART bridge started");

    // Step 8: MQTT — after WiFi (IP must be up)
    log::info!("Connecting to MQTT broker...");
    let (mqtt_client, mqtt_connection) = mqtt::mqtt_connect(&device_id)
        .expect("MQTT connect failed");
    log::info!("MQTT client created");

    // Step 9: mpsc channel — pump signals subscriber on every Connected event
    let (subscribe_tx, subscribe_rx) = std::sync::mpsc::channel::<()>();

    // Step 10: Pump thread — drives connection.next(), never touches client
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::pump_mqtt_events(mqtt_connection, subscribe_tx))
        .expect("pump thread spawn failed");

    // Step 11: Subscriber thread — subscribes on Connected (initial + broker restart)
    let sub_client = mqtt_client.clone();
    let sub_device_id = device_id.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::subscriber_loop(sub_client, sub_device_id, subscribe_rx))
        .expect("subscriber thread spawn failed");

    // Step 12: Heartbeat thread
    let hb_client = mqtt_client.clone();
    let hb_device_id = device_id.clone();
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || mqtt::heartbeat_loop(hb_client, hb_device_id))
        .expect("heartbeat thread spawn failed");

    // Step 13: WiFi supervisor thread (reconnect on drop)
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || wifi::wifi_supervisor(wifi))
        .expect("wifi supervisor spawn failed");

    // Step 14: Main thread parks — all work is done in spawned threads
    log::info!("All subsystems started — device operational");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
