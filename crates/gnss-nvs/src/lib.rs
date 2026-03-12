//! NVS storage abstraction for GNSS firmware.
//!
//! Provides the [`NvsStore`] trait with two feature-gated backing implementations:
//! - `esp-idf` feature: [`EspNvsStore`] wrapping `EspNvs` from `esp-idf-svc`
//! - `sequential` feature: [`SeqNvsStore`] using `sequential-storage` for no_std embedded targets
//!
//! The trait itself has no ESP-IDF dependency, enabling clean-room design and portability.

extern crate alloc;

#[cfg(feature = "esp-idf")]
pub mod esp_idf;

#[cfg(feature = "sequential")]
pub mod sequential;

#[cfg(feature = "esp-idf")]
pub use esp_idf::EspNvsStore;

#[cfg(feature = "sequential")]
pub use sequential::SeqNvsStore;

/// Trait for key-value NVS storage abstraction.
///
/// Organises values by `namespace` and `key` pairs, mirroring the ESP-IDF NVS API.
/// Typed access is provided via `get<T>` / `set<T>` using postcard serialization.
/// Raw byte access is provided via `get_blob` / `set_blob`.
pub trait NvsStore {
    /// The error type returned by all storage operations.
    type Error: core::fmt::Debug;

    /// Retrieve and deserialize a typed value from storage.
    ///
    /// Returns `Ok(None)` if no value exists for the given namespace+key pair.
    fn get<T: serde::de::DeserializeOwned>(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<T>, Self::Error>;

    /// Serialize and store a typed value.
    fn set<T: serde::Serialize>(
        &mut self,
        namespace: &str,
        key: &str,
        value: &T,
    ) -> Result<(), Self::Error>;

    /// Read raw bytes for a namespace+key into the provided buffer.
    ///
    /// Returns `Ok(None)` if no value exists. On success, returns a subslice of `buf`
    /// containing the stored bytes.
    fn get_blob<'a>(
        &self,
        namespace: &str,
        key: &str,
        buf: &'a mut [u8],
    ) -> Result<Option<&'a [u8]>, Self::Error>;

    /// Store raw bytes for a namespace+key.
    fn set_blob(
        &mut self,
        namespace: &str,
        key: &str,
        data: &[u8],
    ) -> Result<(), Self::Error>;
}
