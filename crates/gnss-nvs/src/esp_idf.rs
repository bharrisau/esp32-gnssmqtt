//! ESP-IDF NVS implementation of [`NvsStore`].
//!
//! This module is only compiled when the `esp-idf` feature is enabled.
//! It wraps `EspNvs` from `esp-idf-svc` using the established firmware pattern:
//! open a fresh `EspNvs` handle per call (see Pitfall 4 in RESEARCH.md).
//!
//! # Note
//! This implementation requires the ESP-IDF build environment. Checking with
//! `cargo check -p gnss-nvs --features esp-idf` requires targeting the ESP32
//! (e.g., `--target riscv32imac-esp-espidf`). Host target checks will fail
//! because esp-idf-svc requires the ESP-IDF CMake toolchain.

#[cfg(feature = "esp-idf")]
use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};

#[cfg(feature = "esp-idf")]
use crate::NvsStore;

/// ESP-IDF NVS storage implementation.
///
/// Holds an `EspNvsPartition` handle and opens a fresh `EspNvs` per call,
/// matching the established firmware pattern from `firmware/src/config_relay.rs`.
#[cfg(feature = "esp-idf")]
pub struct EspNvsStore {
    partition: EspNvsPartition<NvsDefault>,
}

#[cfg(feature = "esp-idf")]
impl EspNvsStore {
    /// Create a new `EspNvsStore` backed by the given NVS partition.
    pub fn new(partition: EspNvsPartition<NvsDefault>) -> Self {
        Self { partition }
    }
}

#[cfg(feature = "esp-idf")]
impl NvsStore for EspNvsStore {
    type Error = esp_idf_svc::sys::EspError;

    fn get<T: serde::de::DeserializeOwned>(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<T>, Self::Error> {
        let mut buf = [0u8; 512];
        match self.get_blob(namespace, key, &mut buf)? {
            Some(slice) => {
                let value: T =
                    postcard::from_bytes(slice).map_err(|_| esp_idf_svc::sys::EspError::from_non_zero(
                        core::num::NonZeroI32::new(esp_idf_svc::sys::ESP_ERR_INVALID_SIZE).unwrap(),
                    ))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn set<T: serde::Serialize>(
        &mut self,
        namespace: &str,
        key: &str,
        value: &T,
    ) -> Result<(), Self::Error> {
        let mut buf = [0u8; 512];
        let serialized = postcard::to_slice(value, &mut buf).map_err(|_| {
            esp_idf_svc::sys::EspError::from_non_zero(
                core::num::NonZeroI32::new(esp_idf_svc::sys::ESP_ERR_INVALID_SIZE).unwrap(),
            )
        })?;
        self.set_blob(namespace, key, serialized)
    }

    fn get_blob<'a>(
        &self,
        namespace: &str,
        key: &str,
        buf: &'a mut [u8],
    ) -> Result<Option<&'a [u8]>, Self::Error> {
        // Open a fresh handle per call — matches firmware pattern; avoids multi-handle corruption.
        let nvs = EspNvs::new(self.partition.clone(), namespace, true)?;
        nvs.get_blob(key, buf)
    }

    fn set_blob(
        &mut self,
        namespace: &str,
        key: &str,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // Open a fresh handle per call — matches firmware pattern; avoids multi-handle corruption.
        let nvs = EspNvs::new(self.partition.clone(), namespace, true)?;
        nvs.set_blob(key, data)
    }
}
