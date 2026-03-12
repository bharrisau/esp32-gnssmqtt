//! sequential-storage flash-backed [`NvsStore`] implementation.
//!
//! This module is only compiled when the `sequential` feature is enabled.
//! It targets no_std embedded environments where sequential-storage provides
//! wear-levelled flash KV storage via the `MapStorage` API.
//!
//! # Safety note
//! The async→sync bridge uses `embassy_futures::block_on`, which is safe only
//! in no_std embedded contexts (never from within a Tokio runtime). See
//! RESEARCH.md Pitfall 3 for details.
//!
//! # Hardware validation deferred to NOSTD-F02
//! This implementation compiles and satisfies the type constraints, but has
//! not been validated on physical flash hardware (device FFFEB5). Real flash
//! operations require correct flash range alignment and a functioning NorFlash
//! driver — see NOSTD-F02 tracking item.

use core::cell::RefCell;

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::{
    cache::NoCache,
    map::{Key, MapConfig, MapStorage, SerializationError},
    Error as SsError,
};

use crate::NvsStore;

/// Maximum combined length of namespace and key strings (bytes, postcard-encoded).
/// NVS namespace ≤ 15 chars, key ≤ 15 chars; 2 length prefixes + separator = ~34 bytes max.
const NS_KEY_MAX_LEN: usize = 48;

/// Combined namespace+key used as the sequential-storage map key.
///
/// Serialized as a postcard-encoded `(namespace, key)` tuple stored in a fixed-size
/// byte array. The `len` field tracks the used portion.
#[derive(Clone, Eq, PartialEq)]
pub struct NsKey {
    buf: [u8; NS_KEY_MAX_LEN],
    len: usize,
}

impl NsKey {
    /// Construct an `NsKey` from a namespace and key string pair.
    pub fn new(namespace: &str, key: &str) -> Option<Self> {
        let mut buf = [0u8; NS_KEY_MAX_LEN];
        // Encode as length-prefixed (postcard varint) namespace then key strings.
        // We use a simple manual encoding: [ns_len: u8, ns_bytes..., key_len: u8, key_bytes...]
        // All NVS namespace and key strings are ≤ 15 bytes per ESP-IDF limits.
        let ns = namespace.as_bytes();
        let k = key.as_bytes();
        if ns.len() > 127 || k.len() > 127 {
            return None;
        }
        let total = 1 + ns.len() + 1 + k.len();
        if total > NS_KEY_MAX_LEN {
            return None;
        }
        buf[0] = ns.len() as u8;
        buf[1..1 + ns.len()].copy_from_slice(ns);
        buf[1 + ns.len()] = k.len() as u8;
        buf[1 + ns.len() + 1..total].copy_from_slice(k);
        Some(Self { buf, len: total })
    }
}

impl Key for NsKey {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        if buffer.len() < self.len {
            return Err(SerializationError::BufferTooSmall);
        }
        buffer[..self.len].copy_from_slice(&self.buf[..self.len]);
        Ok(self.len)
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        if buffer.is_empty() {
            return Err(SerializationError::InvalidFormat);
        }
        let ns_len = buffer[0] as usize;
        if buffer.len() < 1 + ns_len + 1 {
            return Err(SerializationError::InvalidFormat);
        }
        let key_len = buffer[1 + ns_len] as usize;
        let total = 1 + ns_len + 1 + key_len;
        if buffer.len() < total {
            return Err(SerializationError::InvalidFormat);
        }
        if total > NS_KEY_MAX_LEN {
            return Err(SerializationError::InvalidData);
        }
        let mut buf = [0u8; NS_KEY_MAX_LEN];
        buf[..total].copy_from_slice(&buffer[..total]);
        Ok((Self { buf, len: total }, total))
    }

    fn get_len(buffer: &[u8]) -> Result<usize, SerializationError> {
        if buffer.is_empty() {
            return Err(SerializationError::InvalidFormat);
        }
        let ns_len = buffer[0] as usize;
        if buffer.len() < 1 + ns_len + 1 {
            return Err(SerializationError::InvalidFormat);
        }
        let key_len = buffer[1 + ns_len] as usize;
        Ok(1 + ns_len + 1 + key_len)
    }
}

/// Error type for `SeqNvsStore`.
#[derive(Debug)]
pub enum SeqError<E: core::fmt::Debug> {
    /// Error from the underlying flash / sequential-storage layer.
    Storage(SsError<E>),
    /// Serialization or key construction failed.
    Serialization,
    /// The provided buffer is too small for the stored value.
    BufferTooSmall,
    /// The namespace or key string exceeds the maximum encoded length.
    KeyTooLong,
}

/// sequential-storage flash-backed implementation of [`NvsStore`].
///
/// Generic over `S: NorFlash` from `embedded_storage_async`. Suitable for
/// use with `esp-hal` flash drivers on ESP32-C6 and other embedded targets.
///
/// # Hardware validation deferred to NOSTD-F02
/// This struct compiles and implements `NvsStore`, but has not been exercised
/// against real flash hardware. Validation requires an ESP32-C6 with the
/// `esp-hal` flash driver wired to the correct flash range.
pub struct SeqNvsStore<S: NorFlash> {
    storage: RefCell<MapStorage<NsKey, S, NoCache>>,
    data_buf: RefCell<[u8; 512]>,
}

impl<S: NorFlash> SeqNvsStore<S> {
    /// Create a new `SeqNvsStore` over the given flash storage and range.
    pub fn new(flash: S, config: MapConfig<S>) -> Self {
        Self {
            storage: RefCell::new(MapStorage::new(flash, config, NoCache::new())),
            data_buf: RefCell::new([0u8; 512]),
        }
    }
}

impl<S: NorFlash> NvsStore for SeqNvsStore<S>
where
    S::Error: core::fmt::Debug,
{
    type Error = SeqError<S::Error>;

    fn get<T: serde::de::DeserializeOwned>(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<T>, Self::Error> {
        let mut buf = [0u8; 512];
        match self.get_blob(namespace, key, &mut buf)? {
            Some(slice) => {
                let value: T = postcard::from_bytes(slice).map_err(|_| SeqError::Serialization)?;
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
        let serialized =
            postcard::to_slice(value, &mut buf).map_err(|_| SeqError::Serialization)?;
        self.set_blob(namespace, key, serialized)
    }

    fn get_blob<'a>(
        &self,
        namespace: &str,
        key: &str,
        buf: &'a mut [u8],
    ) -> Result<Option<&'a [u8]>, Self::Error> {
        let ns_key = NsKey::new(namespace, key).ok_or(SeqError::KeyTooLong)?;

        let mut storage = self.storage.borrow_mut();
        let mut data_buf = self.data_buf.borrow_mut();

        // Safety: embassy_futures::block_on is safe in no_std embedded contexts.
        // Never call this from a Tokio async context (see RESEARCH.md Pitfall 3).
        let result: Option<&[u8]> =
            embassy_futures::block_on(storage.fetch_item(&mut *data_buf, &ns_key))
                .map_err(SeqError::Storage)?;

        match result {
            None => Ok(None),
            Some(slice) => {
                if buf.len() < slice.len() {
                    return Err(SeqError::BufferTooSmall);
                }
                let len = slice.len();
                buf[..len].copy_from_slice(slice);
                Ok(Some(&buf[..len]))
            }
        }
    }

    fn set_blob(
        &mut self,
        namespace: &str,
        key: &str,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let ns_key = NsKey::new(namespace, key).ok_or(SeqError::KeyTooLong)?;

        // Safety: embassy_futures::block_on is safe in no_std embedded contexts.
        // Never call this from a Tokio async context (see RESEARCH.md Pitfall 3).
        let mut data_buf = [0u8; 512];
        embassy_futures::block_on(
            self.storage
                .borrow_mut()
                .store_item(&mut data_buf, &ns_key, &data),
        )
        .map_err(SeqError::Storage)
    }
}
