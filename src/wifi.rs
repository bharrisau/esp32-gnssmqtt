//! WiFi connection and reconnect supervisor. Uses BlockingWifi<EspWifi> from esp-idf-svc.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use embedded_svc::wifi::AuthMethod;

use crate::led::LedState;

/// Connect to the configured WiFi network.
///
/// Consumes the modem peripheral, wraps it in BlockingWifi, sets client configuration,
/// and performs the full start → connect → wait_netif_up sequence.
///
/// The returned `BlockingWifi` handle must be kept alive (e.g. passed to `wifi_supervisor`)
/// for the WiFi driver to remain active.
pub fn wifi_connect(
    modem: impl esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: crate::config::WIFI_SSID.try_into().unwrap(),
        password: crate::config::WIFI_PASS.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    log::info!("WiFi connected to SSID: {}", crate::config::WIFI_SSID);

    Ok(wifi)
}

/// WiFi reconnect supervisor.
///
/// Runs forever in a dedicated thread. Polls connection state every 5 seconds.
/// On disconnect, writes LedState::Connecting, waits `backoff_secs`, calls `wifi.connect()`,
/// and on success resets the backoff and error counters.  On failure, doubles the backoff
/// (capped at 60 seconds).  After 3 consecutive failures at maximum backoff, writes
/// LedState::Error.
///
/// Note: `wifi.start()` is NOT called here — the driver is already started by
/// `wifi_connect`. Only `wifi.connect()` is retried on reconnect.
///
/// Note: LedState::Connected is NOT written here — that is the MQTT pump's responsibility.
/// WiFi up != MQTT up; writing Connected here would show a false green before MQTT is ready.
pub fn wifi_supervisor(mut wifi: BlockingWifi<EspWifi<'static>>, led_state: Arc<AtomicU8>) -> ! {
    let mut backoff_secs: u64 = 1;
    let mut consecutive_failures: u32 = 0;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));

        let connected = wifi.is_connected().unwrap_or(false);

        if !connected {
            log::warn!(
                "WiFi disconnected. Reconnecting in {}s (attempt {}/{})...",
                backoff_secs,
                consecutive_failures + 1,
                crate::config::MAX_WIFI_RECONNECT_ATTEMPTS
            );

            // Signal "working on reconnect" immediately before the backoff sleep.
            led_state.store(LedState::Connecting as u8, Ordering::Relaxed);

            std::thread::sleep(std::time::Duration::from_secs(backoff_secs));

            let reconnect_ok = match wifi.connect() {
                Err(e) => {
                    log::error!("WiFi reconnect failed: {:?}", e);
                    false
                }
                Ok(_) => match wifi.wait_netif_up() {
                    Ok(_) => true,
                    Err(e) => {
                        log::error!("WiFi netif up failed after reconnect: {:?}", e);
                        false
                    }
                }
            };

            if reconnect_ok {
                log::info!("WiFi reconnected");
                backoff_secs = 1;
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
                if consecutive_failures >= crate::config::MAX_WIFI_RECONNECT_ATTEMPTS {
                    // Phase 12 (RESIL-01) will call esp_restart() here.
                    // For now, log error and set LED error state.
                    log::error!(
                        "WiFi: {} consecutive reconnect failures — giving up (will restart in Phase 12)",
                        consecutive_failures
                    );
                    led_state.store(LedState::Error as u8, Ordering::Relaxed);
                }
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
        // If connected: continue — nothing to do this iteration
    }
}
