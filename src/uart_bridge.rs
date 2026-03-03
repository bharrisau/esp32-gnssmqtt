//! USB CDC (UART0) <-> UM980 (UART1) bidirectional debug bridge.
//! Forwards USB input to UM980 and echoes UM980 output back to USB.
//! Development use only.

use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

/// Spawn two bridge threads connecting USB serial (stdin/stdout) to the UM980 UART.
///
/// Thread A: UM980 → USB — reads from UartDriver (UART1) and writes to stdout.
/// Thread B: USB → UM980 — reads lines from stdin and writes to UartDriver (UART1).
///
/// Both threads use an 8 KiB FreeRTOS stack to avoid stack overflow.
/// Returns Ok(()) immediately after spawning; threads run indefinitely.
pub fn spawn_bridge(
    uart1: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
) -> anyhow::Result<()> {
    // Initialise UART1 at 115 200 baud with a 4 KiB receive ring buffer.
    let um980 = UartDriver::new(
        uart1,
        tx_pin,
        rx_pin,
        Option::<AnyIOPin>::None,
        Option::<AnyIOPin>::None,
        &Config::new()
            .baudrate(Hertz(115_200))
            .rx_buffer_size(crate::config::UART_RX_BUF_SIZE as u32),
    )?;

    // Wrap in Arc so both threads share the same driver without lifetime issues.
    let um980 = Arc::new(um980);

    // Thread A — UM980 → USB: poll UART1 and echo bytes to USB stdout.
    let um980_rx = Arc::clone(&um980);
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let mut buf = [0u8; 256];
            loop {
                match um980_rx.read(&mut buf, NON_BLOCK) {
                    Ok(n) if n > 0 => {
                        let _ = std::io::stdout().write_all(&buf[..n]);
                        let _ = std::io::stdout().flush();
                    }
                    _ => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        })
        .unwrap();

    // Thread B — USB → UM980: read lines from USB stdin and forward to UART1.
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = BufReader::new(stdin.lock());
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF — connection closed
                    Ok(_) => {
                        let _ = um980.write(line.as_bytes());
                    }
                    Err(e) => {
                        log::warn!("UART bridge stdin read error: {:?}", e);
                    }
                }
            }
            log::error!("UART bridge USB->UM980 thread exited");
        })
        .unwrap();

    Ok(())
}
