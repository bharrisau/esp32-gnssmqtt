//! GNSS UART hub for the UM980 receiver.
//!
//! This module exclusively owns the `UartDriver` connected to the UM980 on
//! UART0 (GPIO16 TX / GPIO17 RX, 115 200 baud 8N1).  It exposes a clean
//! channel-based interface to the rest of the firmware:
//!
//! * **RX thread** — polls the UART with `NON_BLOCK`, processes each byte
//!   through a four-state `RxState` machine, mirrors every NMEA sentence to
//!   `stdout` for `espflash monitor` visibility, forwards a
//!   `(sentence_type, raw_sentence)` tuple to the caller via an
//!   `mpsc::SyncSender<(String, String)>` (bounded, 64 slots), and forwards
//!   verified RTCM frames as `(message_type, complete_frame)` tuples to an
//!   `mpsc::SyncSender<(u16, Vec<u8>)>` (bounded, 32 slots).
//!
//! * **TX thread** — blocks on an `mpsc::Receiver<String>` and writes each
//!   received command line to the UART followed by `\r\n` (UM980 protocol
//!   requirement).
//!
//! `spawn_gnss` is the sole public entry point.  It creates the `UartDriver`,
//! wraps it in an `Arc` (no `Mutex` needed — `UartDriver::read` and `write`
//! both take `&self`), spawns both threads, and returns
//! `(cmd_tx, nmea_rx, rtcm_rx)` to the caller.

use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};

/// Four-state receiver state machine for handling mixed NMEA+RTCM byte streams.
///
/// `Idle` — waiting for a frame start byte (`$` or `0xD3`).
/// `NmeaLine` — accumulating bytes for an NMEA sentence until `\n`.
/// `RtcmHeader` — collecting the 3-byte RTCM3 header to determine payload length.
/// `RtcmBody` — collecting payload+CRC bytes; Box heap-allocates the 1029-byte buffer
///              to avoid stack overflow (stack frame is 256 bytes for read_buf + overhead).
enum RxState {
    Idle,
    NmeaLine { buf: [u8; 512], len: usize },
    RtcmHeader { buf: [u8; 3], len: usize },
    // PITFALL: [u8; 1029] on stack risks overflow at 8192 stack size.
    // Use Box<[u8; 1029]> to heap-allocate once. Stack frame is 256 (read_buf)
    // + enum discriminant + len field; heap allocation is the safe approach.
    RtcmBody { buf: Box<[u8; 1029]>, len: usize, expected: usize },
}

/// Running count of UART TX write errors in the GNSS TX thread.
///
/// Incremented atomically on every failed `uart_tx.write()` call.
/// Will be read by the health telemetry subsystem (Phase 13).
static UART_TX_ERRORS: AtomicU32 = AtomicU32::new(0);

/// CRC-24Q: polynomial 0x864CFB, init 0, no reflection, no XOR out.
///
/// Input is the complete RTCM3 frame from preamble (0xD3) up to but not
/// including the final 3 CRC bytes.
///
/// Source: RTCM SC-104, confirmed in RTKLIB (tomojitakasu/RTKLIB/src/rtcm.c).
fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        crc ^= (byte as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x1000000 != 0 {
                crc ^= 0x864CFB;
            }
        }
    }
    crc & 0xFFFFFF
}

/// Spawn the GNSS UART hub.
///
/// Initialises `UartDriver` at 115 200 baud with a 4 KiB receive ring buffer,
/// then spawns an RX thread and a TX thread.  Returns `(cmd_tx, nmea_rx, rtcm_rx)`:
///
/// * `cmd_tx: SyncSender<String>` — send an ASCII command string here and the TX
///   thread will write it to the UM980 with a trailing `\r\n`.
///   Bounded to 16: a config batch is typically ≤ 16 commands (with 100ms delay
///   between sends). Capacity 16 prevents config_relay from blocking UART TX drain
///   on large batches.
/// * `nmea_rx: Receiver<(String, String)>` — receive `(sentence_type, raw)`
///   tuples; `sentence_type` is the field between `$` and the first `,` (e.g.
///   `"GNGGA"`, `"GNRMC"`).
/// * `rtcm_rx: Receiver<(u16, Vec<u8>)>` — receive `(message_type, frame)`
///   tuples; `message_type` is the 12-bit RTCM3 message type; `frame` is the
///   complete raw frame (preamble + header + payload + CRC bytes).
pub fn spawn_gnss(
    uart: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
) -> anyhow::Result<(SyncSender<String>, Receiver<(String, String)>, Receiver<(u16, Vec<u8>)>)> {
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

    // Channel: RTCM frames from RX thread to rtcm_relay (Phase 7).
    // Bounded to 32; at 1-4 frames/sec, full channel means relay is stalled.
    let (rtcm_tx, rtcm_rx) = mpsc::sync_channel::<(u16, Vec<u8>)>(32);

    // Channel: command strings from caller to TX thread.
    // Bounded to 16: a config batch is typically ≤ 16 commands (with 100ms delay between sends).
    // Capacity 16 prevents config_relay from blocking UART TX drain on large batches.
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<String>(16);

    // -------------------------------------------------------------------------
    // RX thread — NON_BLOCK polling + state machine byte processing
    // -------------------------------------------------------------------------
    let uart_rx = Arc::clone(&uart);
    std::thread::Builder::new()
        .stack_size(12288) // increased from 8192: RtcmBody buf is heap (Box) but other frame overhead warrants headroom
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "GNSS RX", hwm_words, hwm_words * 4);
            let mut state = RxState::Idle;
            let mut read_buf = [0u8; 256];

            loop {
                match uart_rx.read(&mut read_buf, NON_BLOCK) {
                    Ok(n) if n > 0 => {
                        for &byte in &read_buf[..n] {
                            state = match state {
                                RxState::Idle => {
                                    if byte == b'$' {
                                        let mut buf = [0u8; 512];
                                        buf[0] = b'$';
                                        RxState::NmeaLine { buf, len: 1 }
                                    } else if byte == 0xD3 {
                                        RxState::RtcmHeader { buf: [0xD3, 0, 0], len: 1 }
                                    } else {
                                        RxState::Idle // non-frame byte: stay idle (resync)
                                    }
                                }

                                RxState::NmeaLine { mut buf, mut len } => {
                                    if byte == b'\n' {
                                        // Strip trailing \r if present
                                        let end = if len > 0 && buf[len - 1] == b'\r' {
                                            len - 1
                                        } else {
                                            len
                                        };
                                        // Mirror to stdout (espflash monitor)
                                        let _ = std::io::stdout().write_all(&buf[..end]);
                                        let _ = std::io::stdout().write_all(b"\n");
                                        // Forward to nmea_relay
                                        if end > 1 {
                                            if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                                                let sentence_type = s[1..]
                                                    .split(',')
                                                    .next()
                                                    .unwrap_or("UNKNOWN")
                                                    .to_string();
                                                match nmea_tx.try_send((sentence_type, s.to_string())) {
                                                    Ok(_) => {}
                                                    Err(TrySendError::Full(_)) => {
                                                        log::warn!(
                                                            "NMEA: relay channel full — sentence dropped"
                                                        );
                                                    }
                                                    Err(TrySendError::Disconnected(_)) => {
                                                        log::error!(
                                                            "NMEA: relay channel disconnected"
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        RxState::Idle
                                    } else if len < buf.len() {
                                        buf[len] = byte;
                                        len += 1;
                                        RxState::NmeaLine { buf, len }
                                    } else {
                                        log::warn!("GNSS: NMEA line buffer overflow, discarding");
                                        RxState::Idle
                                    }
                                }

                                RxState::RtcmHeader { mut buf, mut len } => {
                                    buf[len] = byte;
                                    len += 1;
                                    if len == 3 {
                                        // Parse 10-bit payload length from bytes 1-2
                                        // Byte 1: bits[1:0] = length[9:8], Byte 2: bits[7:0] = length[7:0]
                                        let payload_len =
                                            (((buf[1] & 0x03) as usize) << 8) | (buf[2] as usize);
                                        if payload_len > 1023 {
                                            log::warn!(
                                                "GNSS: RTCM3 length {} > 1023, resyncing",
                                                payload_len
                                            );
                                            RxState::Idle
                                        } else {
                                            let expected = payload_len + 6; // header(3) + payload + crc(3)
                                            let mut frame_buf = Box::new([0u8; 1029]);
                                            frame_buf[0] = buf[0];
                                            frame_buf[1] = buf[1];
                                            frame_buf[2] = buf[2];
                                            RxState::RtcmBody {
                                                buf: frame_buf,
                                                len: 3,
                                                expected,
                                            }
                                        }
                                    } else {
                                        RxState::RtcmHeader { buf, len }
                                    }
                                }

                                RxState::RtcmBody { mut buf, mut len, expected } => {
                                    buf[len] = byte;
                                    len += 1;
                                    if len == expected {
                                        // Verify CRC-24Q over header + payload (all bytes except last 3)
                                        let computed = crc24q(&buf[..expected - 3]);
                                        let stored = ((buf[expected - 3] as u32) << 16)
                                            | ((buf[expected - 2] as u32) << 8)
                                            | (buf[expected - 1] as u32);
                                        if computed == stored {
                                            // Extract 12-bit message type from first two payload bytes
                                            // Payload starts at byte index 3 (after 3-byte header)
                                            let msg_type: u16 =
                                                ((buf[3] as u16) << 4) | ((buf[4] as u16) >> 4);
                                            let frame = Vec::from(&buf[..expected]);
                                            match rtcm_tx.try_send((msg_type, frame)) {
                                                Ok(_) => {}
                                                Err(TrySendError::Full(_)) => {
                                                    log::warn!(
                                                        "RTCM: relay channel full — frame dropped"
                                                    );
                                                }
                                                Err(TrySendError::Disconnected(_)) => {
                                                    log::error!(
                                                        "RTCM: relay channel disconnected"
                                                    );
                                                }
                                            }
                                        } else {
                                            log::warn!(
                                                "GNSS: RTCM3 CRC mismatch (computed={:#08x} stored={:#08x}), resyncing",
                                                computed,
                                                stored
                                            );
                                        }
                                        RxState::Idle
                                    } else {
                                        RxState::RtcmBody { buf, len, expected }
                                    }
                                }
                            };
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
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "GNSS TX", hwm_words, hwm_words * 4);
            loop {
                match cmd_rx.recv_timeout(crate::config::RELAY_RECV_TIMEOUT) {
                    Ok(line) => {
                        if let Err(e) = uart_tx.write(line.as_bytes()) {
                            let n = UART_TX_ERRORS.fetch_add(1, Ordering::Relaxed) + 1;
                            log::warn!("GNSS TX: write error #{}: {:?}", n, e);
                        }
                        if let Err(e) = uart_tx.write(b"\r\n") {
                            let n = UART_TX_ERRORS.fetch_add(1, Ordering::Relaxed) + 1;
                            log::warn!("GNSS TX: CRLF write error #{}: {:?}", n, e);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // No command within 5s — normal during idle operation. Continue.
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        log::error!("GNSS TX: cmd channel closed — TX thread exiting");
                        break;
                    }
                }
            }
            // Dead-end park (all SyncSenders dropped; thread has nothing to do).
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        })
        .expect("gnss tx spawn failed");

    Ok((cmd_tx, nmea_rx, rtcm_rx))
}
