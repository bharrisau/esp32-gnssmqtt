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
//!   verified RTCM frames as `RtcmFrame` tuples to an
//!   `mpsc::SyncSender<RtcmFrame>` (bounded, 32 slots).
//!
//! * **TX thread** — blocks on an `mpsc::Receiver<String>` and writes each
//!   received command line to the UART followed by `\r\n` (UM980 protocol
//!   requirement).
//!
//! `spawn_gnss` is the sole public entry point.  It creates the `UartDriver`,
//! wraps it in an `Arc` (no `Mutex` needed — `UartDriver::read` and `write`
//! both take `&self`), spawns both threads, and returns
//! `(cmd_tx, nmea_rx, rtcm_rx, free_pool_tx)` to the caller.

use esp_idf_svc::hal::delay::NON_BLOCK;
use esp_idf_svc::hal::gpio::AnyIOPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::uart::{config::Config, Uart, UartDriver};
use esp_idf_svc::hal::units::Hertz;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};

/// Type for RTCM frames sent from the GNSS RX thread to the RTCM relay.
/// Fields: (message_type_12bit, pool_buffer, valid_byte_count).
/// Buffer is pre-allocated from the RTCM buffer pool — no per-frame heap allocation.
pub type RtcmFrame = (u16, Box<[u8; 1029]>, usize);

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
    // UM980 query responses: begin with '#', end with NMEA-style checksum + \r\n.
    // Forwarded to nmea_tx as ("response", raw) → MQTT gnss/{id}/nmea/response.
    HashLine { buf: [u8; 512], len: usize },
    // Free-text output that is neither NMEA ('$'), query response ('#'), nor RTCM (0xD3).
    // Mirrored to stdout only.
    FreeLine { buf: [u8; 512], len: usize },
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
pub static UART_TX_ERRORS: AtomicU32 = AtomicU32::new(0);

/// Running count of NMEA sentences dropped due to full relay channel (TrySendError::Full).
/// Incremented atomically; read by health telemetry (Phase 13). Cumulative since boot.
pub static NMEA_DROPS: AtomicU32 = AtomicU32::new(0);

/// Running count of RTCM frames dropped due to full relay channel (TrySendError::Full).
/// Incremented atomically; read by health telemetry (Phase 13). Cumulative since boot.
/// Note: RTCM pool-exhaustion drops (in RtcmHeader arm) are separate and NOT counted here.
pub static RTCM_DROPS: AtomicU32 = AtomicU32::new(0);

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

/// Number of pre-allocated RTCM frame buffers in the pool.
/// Pool memory: RTCM_POOL_SIZE × 1029 bytes allocated once at init.
/// At 1–4 MSM7 frames/sec, 4 buffers provide ample headroom before the relay drains.
const RTCM_POOL_SIZE: usize = 4;

/// Spawn the GNSS UART hub.
///
/// Initialises `UartDriver` at 115 200 baud with a 4 KiB receive ring buffer,
/// then spawns an RX thread and a TX thread.  Returns `(cmd_tx, nmea_rx, rtcm_rx, free_pool_tx, uart_arc)`:
///
/// * `cmd_tx: SyncSender<String>` — send an ASCII command string here and the TX
///   thread will write it to the UM980 with a trailing `\r\n`.
///   Bounded to 16: a config batch is typically ≤ 16 commands (with 100ms delay
///   between sends). Capacity 16 prevents config_relay from blocking UART TX drain
///   on large batches.
/// * `nmea_rx: Receiver<(String, String)>` — receive `(sentence_type, raw)`
///   tuples; `sentence_type` is the field between `$` and the first `,` (e.g.
///   `"GNGGA"`, `"GNRMC"`).
/// * `rtcm_rx: Receiver<RtcmFrame>` — receive `(message_type, pool_buffer, frame_len)`
///   tuples; `message_type` is the 12-bit RTCM3 message type; `pool_buffer` is the
///   pre-allocated pool buffer holding the complete raw frame (preamble + header +
///   payload + CRC bytes); `frame_len` is the number of valid bytes.
/// * `free_pool_tx: SyncSender<Box<[u8; 1029]>>` — pass to `rtcm_relay::spawn_relay`
///   so it can return buffers to the pool after publishing each frame.
/// * `uart_arc: Arc<UartDriver<'static>>` — shared reference to the UART driver;
///   passed to `ntrip_client::spawn_ntrip_client` so it can write RTCM correction
///   bytes directly to the UM980 without going through the String-typed `cmd_tx`.
#[allow(clippy::type_complexity)]
pub fn spawn_gnss(
    uart: impl Peripheral<P = impl Uart> + 'static,
    tx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl esp_idf_svc::hal::gpio::InputPin> + 'static,
    reboot_tx: std::sync::mpsc::SyncSender<()>,
) -> anyhow::Result<(SyncSender<String>, Receiver<(String, String)>, Receiver<RtcmFrame>, SyncSender<Box<[u8; 1029]>>, Arc<UartDriver<'static>>)> {
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
            .rx_fifo_size(crate::config::UART_RX_BUF_SIZE),
    )?;

    // Arc allows both threads to share the driver without a Mutex.
    // UartDriver::read / write take &self, so &-access from two threads is sound.
    let uart = Arc::new(uart);

    // Channel: NMEA sentences from RX thread to caller (Phase 5 consumer).
    // Bounded to 64 so the RX thread can drop sentences without blocking UART reads.
    let (nmea_tx, nmea_rx) = mpsc::sync_channel::<(String, String)>(64);

    // Channel: RTCM frames from RX thread to rtcm_relay (Phase 7, pool-backed since Phase 10).
    // Bounded to 32; at 1-4 frames/sec, full channel means relay is stalled.
    let (rtcm_tx, rtcm_rx) = mpsc::sync_channel::<RtcmFrame>(32);

    // Free pool: pre-allocated Box<[u8; 1029]> buffers circulate between GNSS RX and RTCM relay.
    // free_pool_rx goes to the RX closure (take a buffer before each frame).
    // free_pool_tx goes OUT via spawn_gnss return value to rtcm_relay (return buffer after publish).
    let (free_pool_tx, free_pool_rx) =
        mpsc::sync_channel::<Box<[u8; 1029]>>(RTCM_POOL_SIZE);
    for _ in 0..RTCM_POOL_SIZE {
        free_pool_tx
            .send(Box::new([0u8; 1029]))
            .expect("RTCM pool init: send failed — channel full at init?");
    }

    // Channel: command strings from caller to TX thread.
    // Bounded to 16: a config batch is typically ≤ 16 commands (with 100ms delay between sends).
    // Capacity 16 prevents config_relay from blocking UART TX drain on large batches.
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<String>(16);

    // -------------------------------------------------------------------------
    // RX thread — NON_BLOCK polling + state machine byte processing
    // -------------------------------------------------------------------------
    let uart_rx = Arc::clone(&uart);
    // Clone free_pool_tx for use inside the RX closure (TrySendError::Full return path).
    let free_pool_tx_clone = free_pool_tx.clone();
    // reboot_tx: signals main.rs to re-apply UM980 config if UM980 resets
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
                crate::watchdog::GNSS_RX_HEARTBEAT.fetch_add(1, Ordering::Relaxed);
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
                                    } else if byte == b'#' {
                                        // UM980 query response line
                                        let mut buf = [0u8; 512];
                                        buf[0] = b'#';
                                        RxState::HashLine { buf, len: 1 }
                                    } else if byte == b'\r' || byte == b'\n' {
                                        RxState::Idle // bare line endings between responses: skip
                                    } else {
                                        // Other free-text (version banners, etc.) — stdout only
                                        let mut buf = [0u8; 512];
                                        buf[0] = byte;
                                        RxState::FreeLine { buf, len: 1 }
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
                                                match nmea_tx.try_send((sentence_type.clone(), s.to_string())) {
                                                    Ok(_) => {}
                                                    Err(TrySendError::Full(_)) => {
                                                        NMEA_DROPS.fetch_add(1, Ordering::Relaxed);
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
                                                // UM980 reboot detection: '$devicename,COM1*67' is the first line
                                                // the UM980 emits after a reset (power glitch, internal watchdog, etc.).
                                                // sentence_type is derived from between '$' and first ',' → "devicename".
                                                // On detection, signal main.rs to re-apply startup configuration.
                                                if sentence_type == "devicename" {
                                                    log::warn!("GNSS: UM980 reboot detected ('$devicename' banner) — signalling config re-apply");
                                                    match reboot_tx.try_send(()) {
                                                        Ok(_) => {}
                                                        Err(std::sync::mpsc::TrySendError::Full(_)) => {
                                                            log::warn!("GNSS: UM980 reboot signal channel full — re-apply already queued");
                                                        }
                                                        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                                            log::error!("GNSS: UM980 reboot signal channel closed");
                                                        }
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

                                RxState::HashLine { mut buf, mut len } => {
                                    if byte == b'\n' {
                                        let end = if len > 0 && buf[len - 1] == b'\r' { len - 1 } else { len };
                                        if end > 0 {
                                            let _ = std::io::stdout().write_all(&buf[..end]);
                                            let _ = std::io::stdout().write_all(b"\n");
                                            if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                                                match nmea_tx.try_send(("response".to_string(), s.to_string())) {
                                                    Ok(_) => {}
                                                    Err(TrySendError::Full(_)) => {
                                                        NMEA_DROPS.fetch_add(1, Ordering::Relaxed);
                                                        log::warn!("GNSS: query response channel full — dropped");
                                                    }
                                                    Err(TrySendError::Disconnected(_)) => {
                                                        log::error!("GNSS: query response channel disconnected");
                                                    }
                                                }
                                            }
                                        }
                                        RxState::Idle
                                    } else if len < buf.len() {
                                        buf[len] = byte;
                                        len += 1;
                                        RxState::HashLine { buf, len }
                                    } else {
                                        log::warn!("GNSS: query response buffer overflow, discarding");
                                        RxState::Idle
                                    }
                                }

                                RxState::FreeLine { mut buf, mut len } => {
                                    if byte == b'\n' {
                                        // Strip trailing \r if present
                                        let end = if len > 0 && buf[len - 1] == b'\r' {
                                            len - 1
                                        } else {
                                            len
                                        };
                                        if end > 0 {
                                            let _ = std::io::stdout().write_all(&buf[..end]);
                                            let _ = std::io::stdout().write_all(b"\n");
                                        }
                                        RxState::Idle
                                    } else if len < buf.len() {
                                        buf[len] = byte;
                                        len += 1;
                                        RxState::FreeLine { buf, len }
                                    } else {
                                        log::warn!("GNSS: free-text line buffer overflow, discarding");
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
                                            // Take a buffer from the pre-allocated pool (no heap allocation here).
                                            match free_pool_rx.try_recv() {
                                                Ok(mut frame_buf) => {
                                                    frame_buf[0] = buf[0];
                                                    frame_buf[1] = buf[1];
                                                    frame_buf[2] = buf[2];
                                                    RxState::RtcmBody {
                                                        buf: frame_buf,
                                                        len: 3,
                                                        expected,
                                                    }
                                                }
                                                Err(_) => {
                                                    // All pool buffers in flight — relay is behind. Drop this frame.
                                                    log::warn!(
                                                        "RTCM: buffer pool exhausted ({} slots) — frame dropped",
                                                        RTCM_POOL_SIZE
                                                    );
                                                    RxState::Idle
                                                }
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
                                            // Send the pool buffer directly — no Vec allocation.
                                            // On channel full/disconnect: return buf to pool to prevent starvation.
                                            match rtcm_tx.try_send((msg_type, buf, expected)) {
                                                Ok(_) => {}
                                                Err(TrySendError::Full((_, returned_buf, _))) => {
                                                    RTCM_DROPS.fetch_add(1, Ordering::Relaxed);
                                                    log::warn!(
                                                        "RTCM: relay channel full — frame dropped"
                                                    );
                                                    // MUST return buffer to pool to prevent pool starvation.
                                                    let _ = free_pool_tx_clone.try_send(returned_buf);
                                                }
                                                Err(TrySendError::Disconnected((_, returned_buf, _))) => {
                                                    log::error!(
                                                        "RTCM: relay channel disconnected"
                                                    );
                                                    // Return buffer to pool even on disconnect.
                                                    let _ = free_pool_tx_clone.try_send(returned_buf);
                                                }
                                            }
                                        } else {
                                            log::warn!(
                                                "GNSS: RTCM3 CRC mismatch (computed={:#08x} stored={:#08x}), resyncing",
                                                computed,
                                                stored
                                            );
                                            // CRC failed — return buffer to pool to prevent starvation.
                                            let _ = free_pool_tx_clone.try_send(buf);
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

    // Clone for ntrip_client before moving uart into the TX thread.
    // All three callers (uart_rx, uart_tx, uart_for_ntrip) share the same
    // UartDriver via Arc — no Mutex needed since read/write take &self.
    let uart_for_ntrip = Arc::clone(&uart);

    // -------------------------------------------------------------------------
    // TX thread — blocking mpsc drain → UART write
    // -------------------------------------------------------------------------
    let uart_tx = uart; // moves the original Arc; RX thread holds uart_rx clone
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
                        log::info!("Send: {}", line);
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

    Ok((cmd_tx, nmea_rx, rtcm_rx, free_pool_tx, uart_for_ntrip))
}
