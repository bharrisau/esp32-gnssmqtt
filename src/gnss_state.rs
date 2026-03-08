//! Shared GNSS fix-quality state, updated by the NMEA relay thread.
//!
//! Three atomics store the most recent GGA sentence values. Sentinel values
//! (0xFF, 0xFFFF) indicate no GGA has been received yet — heartbeat emits null.
//!
//! Follows the NTRIP_STATE: AtomicU8 pattern from ntrip_client.rs.

use std::sync::atomic::{AtomicU8, AtomicU32};

/// Most recent GGA fix quality (NMEA field 6).
/// 0=No fix, 1=SPS, 2=DGPS, 4=RTK Fixed, 5=RTK Float, 6=Estimated.
/// 0xFF = sentinel: no GGA sentence received yet.
pub static GGA_FIX_TYPE: AtomicU8 = AtomicU8::new(0xFF);

/// Most recent GGA satellite count (NMEA field 7).
/// 0xFF = sentinel: no GGA sentence received yet.
pub static GGA_SATELLITES: AtomicU8 = AtomicU8::new(0xFF);

/// Most recent GGA HDOP × 10 as integer (NMEA field 8; e.g. HDOP 1.2 → stored as 12).
/// 0xFFFF = sentinel: no GGA sentence received yet.
pub static GGA_HDOP_X10: AtomicU32 = AtomicU32::new(0xFFFF);
