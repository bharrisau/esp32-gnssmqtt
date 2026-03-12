---
phase: 23-mqtt-rtcm3-gnss-nvs-crate
plan: "01"
subsystem: infra
tags: [rust, no_std, nvs, flash, sequential-storage, esp-idf, postcard, serde]

# Dependency graph
requires: []
provides:
  - "crates/gnss-nvs/ crate with NvsStore trait and two feature-gated implementations"
  - "EspNvsStore (feature=esp-idf): wraps EspNvsPartition<NvsDefault>, opens fresh EspNvs per call"
  - "SeqNvsStore<S> (feature=sequential): wraps sequential-storage 7.1.0 MapStorage for no_std flash"
  - "NsKey: Key impl combining namespace+key for MapStorage"
affects:
  - "firmware — future port from direct EspNvs usage to NvsStore trait"
  - "Phase 24 — gnss-ota crate may use similar pattern for partition abstraction"
  - "NOSTD-F02 — hardware validation of SeqNvsStore on device FFFEB5"

# Tech tracking
tech-stack:
  added:
    - "postcard 1 (no_std-compatible serde binary codec, alloc feature)"
    - "serde 1 (derive feature, no_std)"
    - "sequential-storage 7.1.0 (async flash KV map, embedded_storage_async)"
    - "embedded-storage-async 0.4 (NorFlash async trait)"
    - "embassy-futures 0.1 (block_on for async-to-sync bridge in no_std)"
    - "esp-idf-svc 0.51 (optional, esp-idf feature only)"
  patterns:
    - "Feature-gated impl modules: cfg(feature=X) at module level, re-exported from lib.rs"
    - "Open-per-call EspNvs handle: fresh EspNvs::new() per set/get call (no caching)"
    - "RefCell interior mutability: &self NvsStore trait satisfied via RefCell<MapStorage> for sequential impl"
    - "embassy_futures::block_on bridge: async MapStorage wrapped in block_on for sync NvsStore trait"
    - "NsKey postcard-compatible encoding: manual length-prefixed (namespace, key) byte layout"

key-files:
  created:
    - "crates/gnss-nvs/Cargo.toml"
    - "crates/gnss-nvs/src/lib.rs"
    - "crates/gnss-nvs/src/esp_idf.rs"
    - "crates/gnss-nvs/src/sequential.rs"
  modified:
    - "Cargo.toml (workspace: removed stray tmp_ss_check member)"
    - "Cargo.lock"

key-decisions:
  - "sequential-storage version is 7.1.0 not 0.5 as in plan — API uses embedded_storage_async (async-only); plan research was based on older version"
  - "RefCell<MapStorage> used in SeqNvsStore to satisfy NvsStore &self requirement for get/get_blob"
  - "embassy-futures::block_on chosen for async-to-sync bridge (safe in no_std embedded, not in Tokio)"
  - "cargo check --features esp-idf requires riscv32imac-esp-espidf target (run from firmware/ directory); host target unsupported by esp-idf-svc build script"
  - "NsKey uses manual length-prefixed encoding rather than postcard tuple to avoid postcard alloc dependency in no_std Key impl"

patterns-established:
  - "Pattern: NvsStore trait with namespace+key pair signature; implementors open storage per call"
  - "Pattern: Feature-gated crate modules with cfg(feature=X) at top of module file"
  - "Pattern: SeqNvsStore uses RefCell for interior mutability to satisfy &self on read operations"

requirements-completed: [NOSTD-02, NOSTD-03]

# Metrics
duration: 14min
completed: 2026-03-12
---

# Phase 23 Plan 01: gnss-nvs Crate Summary

**NvsStore trait crate with EspNvsStore (esp-idf-svc) and SeqNvsStore<S> (sequential-storage 7.1.0) feature-gated implementations, both cargo check passing**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-12T05:52:35Z
- **Completed:** 2026-03-12T06:06:15Z
- **Tasks:** 3
- **Files modified:** 4 created + 2 modified

## Accomplishments

- Created `crates/gnss-nvs/` with clean-room `NvsStore` trait — no esp-idf-svc in default dependency graph
- `EspNvsStore` wraps `EspNvsPartition<NvsDefault>` with per-call `EspNvs` handle opening, matching the established firmware pattern from `config_relay.rs`
- `SeqNvsStore<S: NorFlash>` implements `NvsStore` using sequential-storage 7.1.0 `MapStorage` with `RefCell` interior mutability and `embassy_futures::block_on` for async-to-sync bridging
- All three `cargo check` targets pass: default (no features), `--features sequential`, and `--features esp-idf` on ESP32-C6 target

## Task Commits

Each task was committed atomically:

1. **Task 1: gnss-nvs crate scaffold and NvsStore trait** - `48e569f` (feat)
2. **Task 2: ESP-IDF NvsStore implementation** - `9907a6c` (feat)
3. **Task 3: sequential-storage NvsStore implementation skeleton** - `a44c7f5` (feat)

## Files Created/Modified

- `crates/gnss-nvs/Cargo.toml` - Crate manifest with resolver=2, feature gates esp-idf and sequential, no default esp-idf deps
- `crates/gnss-nvs/src/lib.rs` - NvsStore trait definition with get/set/get_blob/set_blob and namespace+key pair signature
- `crates/gnss-nvs/src/esp_idf.rs` - EspNvsStore impl behind cfg(feature="esp-idf"); per-call EspNvs handle
- `crates/gnss-nvs/src/sequential.rs` - SeqNvsStore<S> impl behind cfg(feature="sequential"); NsKey type; hardware validation deferred to NOSTD-F02

## Decisions Made

- **sequential-storage 7.1.0 (not 0.5):** The plan cited version 0.5 but current release is 7.1.0 with a substantially different API. `MapStorage` is now a typed struct `MapStorage<K: Key, S: NorFlash, C: KeyCacheImpl<K>>` rather than free functions. Used 7.1.0 as found.
- **RefCell for interior mutability:** `NvsStore::get_blob` takes `&self` but `MapStorage::fetch_item` requires `&mut self`. Used `RefCell<MapStorage<...>>` inside `SeqNvsStore` to bridge this. Appropriate for single-threaded embedded use (panics on concurrent borrow — not a concern in the target environment).
- **NsKey manual encoding:** Used manual length-prefixed byte encoding for `NsKey` rather than postcard `to_slice` (which requires `alloc`) to keep the `Key` impl self-contained. ESP-IDF NVS limits namespaces to 15 chars and keys to 15 chars, so a 48-byte fixed buffer is sufficient.
- **esp-idf check requires ESP target:** `cargo check --features esp-idf` on host fails (esp-idf-svc build script rejects non-ESP targets). Documented in module comment; verified by running from `firmware/` directory which has `riscv32imac-esp-espidf` in `.cargo/config.toml`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] sequential-storage version mismatch — plan cited 0.5, actual is 7.1.0**
- **Found during:** Pre-task research (checking crates.io)
- **Issue:** Plan's RESEARCH.md described `MapStorage` with free-function API (`fetch_item`, `store_item` as module-level functions). The actual 7.1.0 API uses a typed `MapStorage<K, S, C>` struct with async methods. `PostcardValue` trait mentioned in research does not exist in 7.1.0.
- **Fix:** Used actual 7.1.0 API: `MapStorage<NsKey, S, NoCache>` struct, `fetch_item<&[u8]>` and `store_item` as async methods, `Value<'a>` trait for raw byte values.
- **Files modified:** crates/gnss-nvs/src/sequential.rs
- **Verification:** `cargo check -p gnss-nvs --features sequential` passes cleanly
- **Committed in:** a44c7f5

**2. [Rule 2 - Missing Critical] Removed stray tmp_ss_check workspace member**
- **Found during:** Task 1 (first cargo check)
- **Issue:** A temporary `tmp_ss_check` crate was added to `Cargo.toml` members during API research; the directory was deleted but the entry remained, causing `cargo check` to fail.
- **Fix:** Removed `"tmp_ss_check"` from workspace members in `Cargo.toml`.
- **Files modified:** Cargo.toml
- **Verification:** `cargo check -p gnss-nvs` passes after removal
- **Committed in:** 48e569f

---

**Total deviations:** 2 auto-fixed (1 bug from version mismatch, 1 blocking workspace config error)
**Impact on plan:** Both fixes necessary for correctness. Sequential-storage API deviation required adapting to actual library state — outcome matches plan intent.

## Issues Encountered

- `unused_mut` warning in `esp_idf.rs` on `get_blob` (`EspNvs::get_blob` takes `&self`, so `mut` was unnecessary). Fixed before commit.
- `unused import: Value` in `sequential.rs` initial draft. Fixed before commit.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `gnss-nvs` crate is ready for use by firmware once Phase 24 wires it in
- `EspNvsStore` can replace direct `EspNvs` usage in `config_relay.rs`, `main.rs`, and `ntrip_client.rs`
- `SeqNvsStore` skeleton compiles; hardware validation on device FFFEB5 is tracked as NOSTD-F02
- Phase 24 can proceed with gnss-ota crate (depends on Phase 23 being complete)

---
*Phase: 23-mqtt-rtcm3-gnss-nvs-crate*
*Completed: 2026-03-12*
