//! Log relay — captures all log output (Rust + C components) and publishes to MQTT.
//!
//! Architecture:
//!   Rust `log::` calls bypass `esp_log_vprintf_func` entirely — EspLogger writes directly
//!   to the newlib stdout FILE* via fwrite. Two complementary capture paths are used:
//!
//!   1. `MqttLogger` (Rust path): composite `log::Log` implementation wrapping EspLogger.
//!      Intercepts at the trait level so every `log::info!()` etc. is forwarded to LOG_TX.
//!      Installed via `log::set_boxed_logger` in place of `EspLogger::initialize_default()`.
//!
//!   2. vprintf hook (C path): `install_mqtt_log_hook()` (log_shim.c) replaces
//!      `esp_log_vprintf_func`. C component logs (wifi, tcp/ip, etc.) that go through
//!      `esp_log_write` reach `rust_log_try_send` via FFI and into LOG_TX.
//!
//!   3. The relay thread drains LOG_TX and publishes each message to
//!      `gnss/{device_id}/log` at QoS 0 (AtMostOnce).
//!
//! Re-entrancy guard:
//!   LOG_REENTERING is set to true while the relay thread is inside the MQTT publish path.
//!   Both MqttLogger::log() and log_shim.c check this before forwarding — preventing a
//!   feedback loop if the MQTT stack or relay thread itself emits log output.
//!   CRITICAL: the relay thread's publish path must NEVER call any log:: macro.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{sync_channel, SyncSender};
use embedded_svc::mqtt::client::QoS;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::mqtt::client::EspMqttClient;

/// Re-entrancy guard: set to true while the relay thread is inside the MQTT publish path.
/// Checked by log_shim.c via `rust_log_is_reentering()` before forwarding log output to MQTT.
static LOG_REENTERING: AtomicBool = AtomicBool::new(false);

/// Global sender end of the log channel. Stored here so `rust_log_try_send` can reach it
/// without any allocation or locking on the hot path.
static LOG_TX: std::sync::OnceLock<SyncSender<String>> = std::sync::OnceLock::new();

/// Composite logger: wraps EspLogger for UART output and also forwards to LOG_TX for MQTT.
///
/// Rust's `log::` calls bypass `esp_log_vprintf_func` entirely (EspLogger writes directly
/// to the newlib stdout FILE*). Intercepting here at the `log::Log` trait level is the
/// only reliable way to capture Rust module logs for MQTT relay.
pub struct MqttLogger {
    inner: EspLogger,
}

impl MqttLogger {
    /// Install as the global Rust logger. Call once, before any `log::` use.
    /// Equivalent to `EspLogger::initialize_default()` but also enables MQTT forwarding.
    pub fn initialize() {
        let logger = Box::new(MqttLogger { inner: EspLogger::new() });
        let max_level = logger.inner.get_max_level();
        log::set_boxed_logger(logger).expect("logger already set");
        log::set_max_level(max_level);
    }
}

impl log::Log for MqttLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        // UART output via EspLogger (preserves existing behavior, handles level filtering)
        self.inner.log(record);

        // MQTT forwarding: skip if relay thread is publishing (re-entrancy guard)
        if LOG_REENTERING.load(Ordering::Relaxed) {
            return;
        }
        if let Some(tx) = LOG_TX.get() {
            let msg = format!(
                "{} ({}) {}: {}",
                match record.level() {
                    log::Level::Error => "E",
                    log::Level::Warn  => "W",
                    log::Level::Info  => "I",
                    log::Level::Debug => "D",
                    log::Level::Trace => "V",
                },
                unsafe { esp_idf_svc::sys::esp_log_timestamp() },
                record.target(),
                record.args(),
            );
            let _ = tx.try_send(msg);
        }
    }

    fn flush(&self) {}
}

/// FFI: Called from log_shim.c to check if we are currently publishing a log message.
/// Returns 1 if re-entering (skip MQTT path), 0 otherwise.
/// Ordering::Relaxed is sufficient — a missed early message is acceptable; correctness
/// (no deadlock, no feedback loop) is preserved by the structural guard in the relay thread.
#[no_mangle]
pub extern "C" fn rust_log_is_reentering() -> i32 {
    if LOG_REENTERING.load(Ordering::Relaxed) { 1 } else { 0 }
}

/// FFI: Called from log_shim.c with each formatted log line.
/// Converts the C string to an owned String and sends via try_send.
/// Any TrySendError (channel full or not yet initialised) is silently discarded — LOG-03.
///
/// # Safety
/// `msg` must be a valid, non-null pointer to a null-terminated C string for at least
/// `_len` bytes. The pointed-to memory must remain valid for the duration of this call.
/// These invariants are guaranteed by log_shim.c's stack buffer usage.
#[no_mangle]
pub unsafe extern "C" fn rust_log_try_send(msg: *const core::ffi::c_char, _len: usize) {
    if msg.is_null() {
        return;
    }
    if let Some(tx) = LOG_TX.get() {
        // SAFETY: log_shim.c guarantees msg is null-terminated within a stack buffer.
        let s = unsafe { std::ffi::CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned();
        // try_send: never blocks; silently drops if channel is full (LOG-03).
        let _ = tx.try_send(s);
    }
}

/// Spawn the log relay thread.
///
/// Creates a bounded sync_channel of capacity 32 (enough to absorb bursts without
/// excessive memory use). Stores the sender in LOG_TX so `rust_log_try_send` can reach it.
/// Spawns a dedicated relay thread that reads from the channel and publishes to MQTT.
///
/// Returns `Ok(())` immediately after spawning. Does NOT install the vprintf hook —
/// that is done in main.rs via `install_mqtt_log_hook()` (Plan 02).
///
/// # Errors
/// Returns `Err` if the relay thread cannot be spawned (out of task slots / stack).
pub fn spawn_log_relay(
    client: Arc<Mutex<EspMqttClient<'static>>>,
    device_id: String,
) -> anyhow::Result<()> {
    let (tx, log_rx) = sync_channel::<String>(32);

    // Store sender globally so FFI rust_log_try_send can reach it.
    // OnceLock::set fails silently if already set — spawn_log_relay must only be called once.
    let _ = LOG_TX.set(tx);

    std::thread::Builder::new()
        .stack_size(4096)
        .spawn(move || {
            // HWM at thread entry: confirms configured stack size is adequate.
            // Value × 4 = bytes free. Safe to call log:: here — before the main loop.
            let hwm_words = unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
            };
            log::info!(
                "[HWM] {}: {} words ({} bytes) stack remaining at entry",
                "log relay",
                hwm_words,
                hwm_words * 4
            );

            let topic = format!("gnss/{}/log", device_id);

            loop {
                match log_rx.recv_timeout(crate::config::SLOW_RECV_TIMEOUT) {
                    Ok(msg) => {
                        // Set re-entrancy guard BEFORE any MQTT work.
                        // CRITICAL: do NOT call log::, log::info!, etc. inside this block.
                        // Any log call here would re-enter mqtt_log_vprintf, check the guard,
                        // and skip forwarding — but the log:: call is still made, wasting cycles.
                        // More importantly: never log here even accidentally — the guard prevents
                        // a feedback loop but does not make it safe or efficient to log.
                        LOG_REENTERING.store(true, Ordering::Relaxed);

                        // Acquire mutex per message; do NOT hold across loop iterations.
                        if let Ok(mut c) = client.lock() {
                            // QoS::AtMostOnce (0): fire-and-forget; no re-delivery on disconnect.
                            // retain = false: log messages are transient.
                            let _ = c.enqueue(&topic, QoS::AtMostOnce, false, msg.as_bytes());
                        }

                        // Clear re-entrancy guard AFTER all MQTT work is done.
                        LOG_REENTERING.store(false, Ordering::Relaxed);
                    }
                    Err(_) => {
                        // Timeout or channel closed — continue silently.
                        // Do not log here (guard is clear, but we want no log noise from relay).
                    }
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("log relay spawn failed: {}", e))?;

    Ok(())
}
