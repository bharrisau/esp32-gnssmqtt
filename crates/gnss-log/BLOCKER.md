# gnss-log — Blocker Documentation

## Background

The firmware uses a composite logger (`MqttLogger`) in `firmware/src/log_relay.rs`. It
implements `log::Log` to capture Rust log output AND installs a C vprintf hook via
`esp_log_set_vprintf()` to capture log output from C components (ESP-IDF WiFi driver,
TCP/IP stack, ROM functions). Both streams are forwarded to the MQTT log relay channel
for remote log monitoring.

This document records what is and is not blocked for a no_std implementation.

---

## Part 1 — Rust log::Log Side (NOT BLOCKED)

**Status:** Fully implementable today in no_std with no C dependency.

The `log` crate (crates.io) supports `no_std`. Implementing a custom `log::Log`:

```rust
struct MyLogger { sink: &'static dyn LogSink }

impl log::Log for MyLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool { true }
    fn log(&self, record: &log::Record) {
        // forward to sink — no alloc required with fixed-size formatting
    }
    fn flush(&self) {}
}
```

Calling `log::set_logger(&MY_LOGGER)` and `log::set_max_level(LevelFilter::Info)` is
pure Rust, no_std compatible, and requires no unsafe code beyond a static initialiser.

**Conclusion:** The Rust side of the log hook has no blocker.

---

## Part 2 — C Component Log Capture (PARTIAL BLOCKER)

**Status:** Possible with one C FFI call. Not a pure-Rust implementation.

**What needs to happen:** Capturing log output from C components (ESP-IDF WiFi, TCP/IP
stack, ROM) requires installing a vprintf-compatible callback:

```c
esp_log_set_vprintf(my_vprintf_hook);
```

Where `my_vprintf_hook` has the signature `int (*)(const char*, va_list)`.

**In Rust:** This requires:

1. A `#[no_mangle] extern "C"` function with the vprintf signature (this is the callback)
2. One `unsafe` FFI call to `esp_log_set_vprintf`

```rust
#[no_mangle]
extern "C" fn my_vprintf_hook(fmt: *const core::ffi::c_char, _args: *mut core::ffi::c_void) -> i32 {
    // In practice: use vsnprintf via FFI to render the formatted string,
    // or accept that va_list handling requires C-side glue code
    0
}

unsafe { esp_log_set_vprintf(my_vprintf_hook as *const _); }
```

**The va_list constraint:** Rust cannot portably consume a C `va_list` directly. The
firmware works around this by using a C-side `vsnprintf` call via esp-idf-sys. In a
no_std context, the same approach applies: a thin C shim or `esp-idf-sys` bindings
are needed to render the va_list to a `&str`.

**esp_log_set_vprintf availability:** This function is present in the ESP32 ROM (not
ESP-IDF-only). It is available in `esp-hal` builds via:

```rust
extern "C" {
    fn esp_log_set_vprintf(func: unsafe extern "C" fn(*const i8, ...) -> i32) -> unsafe extern "C" fn(*const i8, ...) -> i32;
}
```

**The constraint:** A pure-Rust-only implementation that captures C component logs
without ANY C FFI is not possible. The C logging subsystem requires a C-callable
function pointer. If only Rust log output is needed (no C component log capture),
there is no blocker at all.

---

## Recommended Path Forward

1. **Rust-only log capture (no blocker):** Implement `LogHook` for Rust log output in a
   `gnss-log-embassy` backend crate. Deploy this for the common case where C component
   log capture is not required.

2. **Full log capture (one FFI call):** Add an optional `c-log-capture` feature to
   `gnss-log-embassy` that enables the `esp_log_set_vprintf` FFI hook. Gate it behind
   a feature flag to make the C FFI boundary explicit and opt-in:

   ```toml
   [features]
   c-log-capture = []  # requires unsafe FFI + C va_list handling
   ```

3. **va_list handling:** Use `esp-hal`'s existing FFI bindings or a minimal C shim
   (`vsnprintf` wrapper) to render the va_list to a fixed-size stack buffer before
   forwarding to the Rust `LogSink`.
