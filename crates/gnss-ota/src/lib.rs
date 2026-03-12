#![no_std]

/// Handle to one of the two OTA application partition slots (ota_0 or ota_1).
///
/// A dual-slot OTA implementation writes the new firmware image to the inactive
/// slot, verifies it, then sets it as the next boot target before resetting.
///
/// # Implementations
///
/// - ESP-IDF: `esp-idf-svc` `EspOta` — std only, not available in this crate
/// - nostd: blocked — see `BLOCKER.md` for specific reasons
pub trait OtaSlot {
    /// Error type for OTA operations.
    type Error: core::fmt::Debug;

    /// Total writable capacity of this partition in bytes.
    fn capacity(&self) -> usize;

    /// Erase this partition. Must be called before `write_chunk`.
    fn erase(&mut self) -> Result<(), Self::Error>;

    /// Write `data` at `offset` bytes from the start of this partition.
    ///
    /// Caller must call `erase()` first. Writes must be sequential (flash constraint).
    fn write_chunk(&mut self, offset: usize, data: &[u8]) -> Result<(), Self::Error>;

    /// Verify the written image using CRC32.
    ///
    /// Returns `Ok(true)` if the CRC matches `expected`.
    fn verify_crc32(&self, expected: u32) -> Result<bool, Self::Error>;

    /// Mark this slot as the next boot target.
    ///
    /// Does not reset the device. Call a platform reset after this returns `Ok`.
    fn set_as_boot_target(&mut self) -> Result<(), Self::Error>;
}

/// Manages the two OTA partition slots and tracks the currently booted slot.
///
/// The implementing type is responsible for determining which slot (0 or 1) is
/// currently booted and providing access to the inactive slot for writing.
pub trait OtaManager {
    /// The slot type returned by this manager.
    type Slot: OtaSlot;
    /// Error type for slot management operations.
    type Error: core::fmt::Debug;

    /// Returns the index (0 or 1) of the currently booted slot.
    fn booted_slot_index(&self) -> Result<usize, Self::Error>;

    /// Returns the slot that is NOT currently booted, ready for writing.
    ///
    /// The caller should call `erase()` on the returned slot before writing.
    fn inactive_slot(&mut self) -> Result<Self::Slot, Self::Error>;
}
