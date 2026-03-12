//! USB-Serial-JTAG stdin → UM980 command bridge.
//!
//! Reads lines from stdin using the same line-editing logic as before, but
//! sends completed lines to the GNSS TX channel (`Sender<String>`) instead of
//! writing to UART directly.  Development use only.

use std::io::{Read, Write};
use std::sync::mpsc::SyncSender;

/// Spawn the stdin line-editor bridge (Thread B only).
///
/// Reads characters from USB-Serial-JTAG stdin, assembles them into a line
/// with local echo and backspace support, and sends each completed line via
/// `cmd_tx` to the GNSS TX thread which writes it to the UM980 with `\r\n`.
///
/// Returns `Ok(())` immediately after spawning; the thread runs indefinitely.
pub fn spawn_bridge(cmd_tx: SyncSender<String>) -> anyhow::Result<()> {
    // Thread B — USB → UM980: line-editing with local echo and backspace support.
    //
    // espflash monitor (host) is line-buffered — it only renders received bytes when it
    // sees \n. To force immediate display, every redraw ends with \n then ANSI cursor-up
    // (\x1b[A) to keep the prompt on a single line visually.
    std::thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "UART bridge", hwm_words, hwm_words * 4);
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
                                    // Move to new line and send buffered command via GNSS TX channel.
                                    let _ = std::io::stdout().write_all(b"\r\n");
                                    let _ = std::io::stdout().flush();
                                    if line_len > 0 {
                                        let s = String::from_utf8_lossy(&line[..line_len]).into_owned();
                                        match cmd_tx.try_send(s) {
                                            Ok(_) => {}
                                            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                                                log::warn!("UART bridge: GNSS cmd channel full — command dropped");
                                            }
                                            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                                log::warn!("UART bridge: GNSS cmd channel closed");
                                            }
                                        }
                                        line_len = 0;
                                    }
                                }
                                0x7F | 0x08 if line_len > 0 => {
                                    // DEL or backspace — erase last character and redraw.
                                    line_len -= 1;
                                    redraw(&line, line_len);
                                }
                                0x7F | 0x08 => {}
                                0x20..=0x7E if line_len < line.len() => {
                                    // Printable ASCII — buffer and redraw.
                                    line[line_len] = byte;
                                    line_len += 1;
                                    redraw(&line, line_len);
                                }
                                0x20..=0x7E => {}
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
