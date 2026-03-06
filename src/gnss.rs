//! GNSS UART hub for the UM980 receiver.
//!
//! This module exclusively owns the `UartDriver` connected to the UM980 on
//! UART0 (GPIO16 TX / GPIO17 RX, 115 200 baud 8N1).  It exposes a clean
//! channel-based interface to the rest of the firmware:
//!
//! * **RX thread** — polls the UART with `NON_BLOCK`, assembles raw bytes
//!   into complete NMEA sentences (newline-terminated), mirrors every raw
//!   sentence to `stdout` for `espflash monitor` visibility, and forwards a
//!   `(sentence_type, raw_sentence)` tuple to the caller via an
//!   `mpsc::SyncSender<(String, String)>` (bounded, 64 slots).
//!
//! * **TX thread** — blocks on an `mpsc::Receiver<String>` and writes each
//!   received command line to the UART followed by `\r\n` (UM980 protocol
//!   requirement).
//!
//! `spawn_gnss` is the sole public entry point.  It creates the `UartDriver`,
//! wraps it in an `Arc` (no `Mutex` needed — `UartDriver::read` and `write`
//! both take `&self`), spawns both threads, and returns
//! `(cmd_tx, nmea_rx)` to the caller.

use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TrySendError};

/// Spawn the GNSS UART hub.
///
/// Initialises `UartDriver` at 115 200 baud with a 4 KiB receive ring buffer,
/// then spawns an RX thread and a TX thread.  Returns `(cmd_tx, nmea_rx)`:
///
/// * `cmd_tx: Sender<String>` — send an ASCII command string here and the TX
///   thread will write it to the UM980 with a trailing `\r\n`.
/// * `nmea_rx: Receiver<(String, String)>` — receive `(sentence_type, raw)`
///   tuples; `sentence_type` is the field between `$` and the first `,` (e.g.
///   `"GNGGA"`, `"GNRMC"`).
pub fn spawn_gnss(
    uart: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
) -> anyhow::Result<(Sender<String>, Receiver<(String, String)>)> {
    // Initialise UART0 at 115 200 baud.  rx_fifo_size must be set at driver
    // creation time — there is no sdkconfig option for this in ESP-IDF v5.
    let uart = UartDriver::new(
        uart,
        tx_pin,
        rx_pin,
        Option::<AnyIOPin>::None,
        Option::<AnyIOPin>::None,
        &Config::new()
            .baudrate(Hertz(115_200))
            .rx_fifo_size(crate::config::UART_RX_BUF_SIZE as usize),
    )?;

    // Arc allows both threads to share the driver without a Mutex.
    // UartDriver::read / write take &self, so &-access from two threads is sound.
    let uart = Arc::new(uart);

    // Channel: NMEA sentences from RX thread to caller (Phase 5 consumer).
    // Bounded to 64 so the RX thread can drop sentences without blocking UART reads.
    let (nmea_tx, nmea_rx) = mpsc::sync_channel::<(String, String)>(64);

    // Channel: command strings from caller to TX thread.
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();

    // -------------------------------------------------------------------------
    // RX thread — NON_BLOCK polling + sentence assembly
    // -------------------------------------------------------------------------
    let uart_rx = Arc::clone(&uart);
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let mut read_buf = [0u8; 256];
            // 512-byte accumulator covers the longest UM980 proprietary sentences.
            let mut line_buf = [0u8; 512];
            let mut line_len: usize = 0;

            loop {
                match uart_rx.read(&mut read_buf, NON_BLOCK) {
                    Ok(n) if n > 0 => {
                        for &byte in &read_buf[..n] {
                            if byte == b'\n' {
                                // ----- newline: process the accumulated line -----

                                // Strip optional trailing \r.
                                let end = if line_len > 0 && line_buf[line_len - 1] == b'\r' {
                                    line_len - 1
                                } else {
                                    line_len
                                };

                                // Mirror raw bytes to stdout so espflash monitor shows them.
                                let _ = std::io::stdout().write_all(&line_buf[..end]);
                                let _ = std::io::stdout().write_all(b"\n");

                                if line_buf[..end].first() == Some(&b'$') && end > 1 {
                                    // Valid NMEA sentence — extract type and forward.
                                    if let Ok(s) = std::str::from_utf8(&line_buf[..end]) {
                                        let sentence_type = s[1..]
                                            .split(',')
                                            .next()
                                            .unwrap_or("UNKNOWN")
                                            .to_string();
                                        match nmea_tx.try_send((sentence_type, s.to_string())) {
                                            Ok(_) => {}
                                            Err(TrySendError::Full(_)) => {
                                                log::warn!("NMEA: relay channel full — sentence dropped");
                                            }
                                            Err(TrySendError::Disconnected(_)) => {
                                                log::error!("NMEA: relay channel disconnected");
                                            }
                                        }
                                    }
                                } else if end > 0 {
                                    // Non-NMEA, non-empty line — log and drop.
                                    log::warn!(
                                        "GNSS: non-NMEA line dropped: {:?}",
                                        std::str::from_utf8(&line_buf[..end])
                                    );
                                }
                                // end == 0: empty line after stripping — silently skip.

                                line_len = 0;
                            } else if line_len < line_buf.len() {
                                line_buf[line_len] = byte;
                                line_len += 1;
                            } else {
                                // Accumulator full without seeing a newline — discard.
                                log::warn!(
                                    "GNSS: RX line buffer overflow, discarding {} bytes",
                                    line_len
                                );
                                line_len = 0;
                            }
                        }
                    }
                    // No data available or read error — yield to other tasks.
                    _ => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        })
        .expect("gnss rx spawn failed");

    // -------------------------------------------------------------------------
    // TX thread — blocking mpsc drain → UART write
    // -------------------------------------------------------------------------
    let uart_tx = uart; // moves the original Arc; RX thread holds the clone
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // `for line in &cmd_rx` blocks until a command arrives — correct for
            // infrequent TX (GNSS commands are sent rarely, not in a tight loop).
            for line in &cmd_rx {
                let _ = uart_tx.write(line.as_bytes());
                let _ = uart_tx.write(b"\r\n");
            }
            // All Senders were dropped — no more commands can arrive.
            log::error!("GNSS TX channel closed — TX thread exiting");
        })
        .expect("gnss tx spawn failed");

    Ok((cmd_tx, nmea_rx))
}
