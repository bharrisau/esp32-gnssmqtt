#![no_std]

/// Hook for capturing all log output and forwarding it to a remote sink.
///
/// The hook captures two log streams:
/// 1. Rust `log::Log` output — fully portable, no C required.
/// 2. C component log output (ESP-IDF components, ROM functions) — requires
///    a single C FFI call to `esp_log_set_vprintf()`. See `BLOCKER.md`.
///
/// # Implementations
///
/// - ESP-IDF: `log_relay.rs` — composite `log::Log` impl + `esp_log_set_vprintf()`
///   C FFI hook; C logs are captured via a vprintf-compatible callback.
/// - nostd Rust side: implementable today with no C — implement `log::Log` trait,
///   call `log::set_logger()`.
/// - nostd C side: requires `esp_log_set_vprintf()` C FFI call — available in
///   ESP32 ROM, not ESP-IDF-specific. See `BLOCKER.md` for details.
pub trait LogHook {
    /// Error type for log hook installation.
    type Error: core::fmt::Debug;

    /// Install this hook as the global Rust log handler.
    ///
    /// After this call, all `log::error!`, `log::warn!`, etc. output is
    /// forwarded through the associated `LogSink`.
    ///
    /// Note: C component log capture requires a separate call to install
    /// the C vprintf hook — see `BLOCKER.md`.
    fn install(self) -> Result<(), Self::Error>;
}

/// Receives log messages from an installed `LogHook`.
pub trait LogSink: Send + Sync {
    /// Called for each log record. Must be non-blocking (called from log callsite).
    ///
    /// Implementations must not allocate or lock in the hot path.
    fn on_log(&self, level: LogLevel, target: &str, message: &str);
}

/// Log severity level, mirroring `log::Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}
