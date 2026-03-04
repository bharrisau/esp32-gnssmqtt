//! USB CDC (USB-Serial-JTAG) <-> UM980 (UART0, GPIO16 TX / GPIO17 RX) bidirectional debug bridge.
//! Forwards USB input to UM980 and echoes UM980 output back to USB.
//! Development use only.

use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use std::io::{Read, Write};
use std::sync::Arc;

/// Spawn two bridge threads connecting USB serial (stdin/stdout) to the UM980 UART.
///
/// Thread A: UM980 → USB — reads from UartDriver (UART1) and writes to stdout.
/// Thread B: USB → UM980 — reads lines from stdin and writes to UartDriver (UART1).
///
/// Both threads use an 8 KiB FreeRTOS stack to avoid stack overflow.
/// Returns Ok(()) immediately after spawning; threads run indefinitely.
pub fn spawn_bridge(
    uart: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
) -> anyhow::Result<()> {
    // Initialise UART0 at 115 200 baud with a 4 KiB receive ring buffer.
    let um980 = UartDriver::new(
        uart,
        tx_pin,
        rx_pin,
        Option::<AnyIOPin>::None,
        Option::<AnyIOPin>::None,
        &Config::new()
            .baudrate(Hertz(115_200))
            .rx_fifo_size(crate::config::UART_RX_BUF_SIZE as usize),
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
                        //log::info!("UM980→USB: {:?}", &buf[..n]);
                        let _ = std::io::stdout().write_all(&buf[..n]);
                        let _ = std::io::stdout().flush();
                    }
                    _ => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        })
        .unwrap();

    // Thread B — USB → UM980: line-editing with local echo and backspace support.
    //
    // espflash monitor (host) is line-buffered — it only renders received bytes when it
    // sees \n. To force immediate display, every redraw ends with \n then ANSI cursor-up
    // (\x1b[A) to keep the prompt on a single line visually.
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let mut line = [0u8; 256];
            let mut line_len: usize = 0;
            let mut buf = [0u8; 64];

            // Reprint the current line buffer in-place. The \n forces espflash to flush
            // its host stdout; \x1b[A moves the cursor back up so the next character
            // appears on the same line.
            let redraw = |line: &[u8], len: usize| {
                let mut out = std::io::stdout();
                let _ = out.write_all(b"\r\x1b[K"); // CR + erase to end of line
                let _ = out.write_all(&line[..len]);
                let _ = out.write_all(b"\n\x1b[A"); // newline (flush host buffer) + cursor up
                let _ = out.flush();
            };

            loop {
                match std::io::stdin().read(&mut buf) {
                    Ok(0) => break, // EOF — connection closed
                    Ok(n) => {
                        for &byte in &buf[..n] {
                            match byte {
                                b'\r' | b'\n' => {
                                    // Move to new line and send buffered command to UM980.
                                    let _ = std::io::stdout().write_all(b"\r\n");
                                    let _ = std::io::stdout().flush();
                                    if line_len > 0 {
                                        let _ = um980.write(&line[..line_len]);
                                        let _ = um980.write(b"\r\n");
                                        line_len = 0;
                                    }
                                }
                                0x7F | 0x08 => {
                                    // DEL or backspace — erase last character and redraw.
                                    if line_len > 0 {
                                        line_len -= 1;
                                        redraw(&line, line_len);
                                    }
                                }
                                0x20..=0x7E => {
                                    // Printable ASCII — buffer and redraw.
                                    if line_len < line.len() {
                                        line[line_len] = byte;
                                        line_len += 1;
                                        redraw(&line, line_len);
                                    }
                                }
                                _ => {} // ignore other control characters
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
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
