//! LED state module — drives GPIO15 (active-low) with blink patterns for WiFi+MQTT state.
//!
//! LedState variants:
//! - Connecting (0): 200ms on / 200ms off repeating
//! - Connected  (1): steady on
//! - Error      (2): 3× rapid pulse (100ms on / 100ms off) then 700ms off; 1300ms cycle

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use esp_idf_hal::gpio::{Gpio15, Output, PinDriver};

/// Three LED states reflecting WiFi + MQTT connectivity.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub enum LedState {
    Connecting = 0,
    Connected  = 1,
    Error      = 2,
}

impl LedState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => LedState::Connected,
            2 => LedState::Error,
            _ => LedState::Connecting,
        }
    }
}

/// LED task — drives GPIO15 (active-low) based on shared `state` Arc.
///
/// Polls state every 50 ms and drives blink timing via an elapsed-time counter
/// so state changes take effect within one poll interval, not at the end of a
/// full blink period.
///
/// Intended to run in a dedicated thread spawned in main.rs (Plan 03-03).
pub fn led_task(mut pin: PinDriver<'static, Gpio15, Output>, state: Arc<AtomicU8>) -> ! {
    // HWM at thread entry: confirms configured stack size is adequate. Value × 4 = bytes free.
    let hwm_words = unsafe {
        esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
    };
    log::info!("[HWM] {}: {} words ({} bytes) stack remaining at entry",
        "LED task", hwm_words, hwm_words * 4);
    let mut elapsed_ms: u64 = 0;
    let mut prev_state = LedState::Connecting;
    let mut connected_on = false; // track whether LED is already driven on for Connected state

    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));

        let current = LedState::from_u8(state.load(Ordering::Relaxed));

        // When transitioning away from Connected, reset counter so new blink starts cleanly.
        if prev_state == LedState::Connected && current != LedState::Connected {
            elapsed_ms = 0;
            connected_on = false;
        }

        match current {
            LedState::Connecting => {
                // 200ms on / 200ms off
                let pos = elapsed_ms % 400;
                if pos < 200 {
                    pin.set_low().ok();
                } else {
                    pin.set_high().ok();
                }
            }
            LedState::Connected => {
                // Steady on — only set once to avoid bus churn
                if !connected_on {
                    pin.set_low().ok();
                    connected_on = true;
                }
            }
            LedState::Error => {
                // 3× rapid pulse (100ms on / 100ms off) then 700ms off
                // Total cycle = 1300ms
                // ON when: position < 600 AND (position % 200) < 100
                let pos = elapsed_ms % 1300;
                if pos < 600 && (pos % 200) < 100 {
                    pin.set_low().ok();
                } else {
                    pin.set_high().ok();
                }
            }
        }

        prev_state = current;
        elapsed_ms = elapsed_ms.wrapping_add(50);
    }
}
