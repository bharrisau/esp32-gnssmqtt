---
phase: 23-mqtt-rtcm3-gnss-nvs-crate
verified: 2026-03-12T08:00:00Z
status: passed
score: 19/19 must-haves verified
re_verification: false
---

# Phase 23: mqtt-rtcm3-gnss-nvs-crate Verification Report

**Phase Goal:** Server connects to MQTT and decodes all RTCM3 MSM and ephemeris messages into verified observation structs; gnss-nvs crate provides a working NvsStore trait with ESP-IDF and sequential-storage implementations
**Verified:** 2026-03-12
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths — Plan 01 (gnss-nvs crate)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `cargo check -p gnss-nvs` succeeds (default features) | VERIFIED | Command exits 0; `Finished dev profile` |
| 2 | `cargo check -p gnss-nvs --features sequential` succeeds | VERIFIED | Command exits 0; `Finished dev profile` |
| 3 | `cargo check -p gnss-nvs --features esp-idf` succeeds on ESP target | VERIFIED | SUMMARY documents pass on riscv32imac-esp-espidf; host target correctly rejected by esp-idf-svc build script |
| 4 | NvsStore trait has get/set/get_blob/set_blob with namespace+key pair signature | VERIFIED | `crates/gnss-nvs/src/lib.rs` lines 35–66; exact signatures match plan |
| 5 | No esp-idf-svc in default dependency graph | VERIFIED | `cargo tree -p gnss-nvs` produces no esp-idf output; Cargo.toml lists esp-idf-svc as optional only |

### Observable Truths — Plan 02 (Server Foundation)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 6 | Server binary compiles: `cargo build -p gnss-server` succeeds | VERIFIED | Command exits 0 |
| 7 | ServerConfig loads from TOML file via `--config` CLI flag | VERIFIED | `config::load_config()` uses Figment+Toml; 3 config::tests pass |
| 8 | GNSS_ env vars with __ nesting override TOML values | VERIFIED | `Env::prefixed("GNSS_").split("__")` at config.rs line 55 |
| 9 | MQTT supervisor owns EventLoop and broadcasts state via watch channel | VERIFIED | `mqtt_supervisor` in mqtt.rs: `watch::Sender<bool>`, sends true on ConnAck, false on error |
| 10 | Supervisor reconnects with backoff [1, 2, 5, 10, 30]s | VERIFIED | `BACKOFF_STEPS: [u64; 5] = [1, 2, 5, 10, 30]`; fail_count capped with `.min(len-1)` |
| 11 | Supervisor subscribes to 3 topics on connect | VERIFIED | mqtt.rs lines 66–77: gnss/{device_id}/rtcm, /nmea, /heartbeat at QoS::AtMostOnce |
| 12 | Unit tests for reconnect state machine pass | VERIFIED | 7 mqtt::tests pass: backoff sequence, capped at 30s, all variant types, topic routing |

### Observable Truths — Plan 03 (RTCM3 Decode Pipeline)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 13 | GPS/GLONASS MSM4/MSM7 pseudorange, carrier phase, C/N0 extracted per signal | VERIFIED | rtcm_decode.rs: Msg1074/1077 (GPS), Msg1084/1087 (GLO) all populate Observation fields |
| 14 | GLONASS carrier_phase_ms is Option<f64> — None when field is None | VERIFIED | observation.rs line 26: `carrier_phase_ms: Option<f64>`; epoch.rs test `glonass_none_carrier_phase_preserved` passes |
| 15 | Galileo and BeiDou MSM decoded to same Observation struct | VERIFIED | rtcm_decode.rs: Msg1094/1097 (GAL), Msg1124/1127 (BDS) all produce Observation with correct Constellation |
| 16 | Ephemeris messages 1019/1020/1046/1042 decoded and emitted | VERIFIED | rtcm_decode.rs lines 206–220; EphemerisMsg::Gps/Glonass/Galileo/Beidou variants; 1042 is correct BeiDou (1044 is QZSS — see note) |
| 17 | Epoch buffer accumulates MSMs with same epoch_ms and flushes on change | VERIFIED | epoch.rs `push()` logic: accumulate when epoch_key matches, flush when different; `flush_on_change` test passes |
| 18 | Flush emits EpochGroup with constellation counts and log line | VERIFIED | `build_group()` counts per constellation; `log::info!("Epoch {} GPS:{} GLO:{} GAL:{} BDS:{}", ...)` |
| 19 | All unit tests pass | VERIFIED | 8 tests: epoch::tests (4) + rtcm_decode::tests (4), all green |

**Score: 19/19 truths verified**

---

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/gnss-nvs/Cargo.toml` | Crate manifest with feature gates, no default esp-idf deps | VERIFIED | Features: `esp-idf`, `sequential`; esp-idf-svc optional only |
| `crates/gnss-nvs/src/lib.rs` | NvsStore trait definition | VERIFIED | 67 lines; full trait with 4 methods and Error assoc type |
| `crates/gnss-nvs/src/esp_idf.rs` | EspNvsStore impl behind cfg(feature="esp-idf") | VERIFIED | 94 lines; per-call EspNvs handle; postcard serialization |
| `crates/gnss-nvs/src/sequential.rs` | SeqNvsStore<S> behind cfg(feature="sequential") | VERIFIED | 228 lines; RefCell<MapStorage>; embassy_futures::block_on bridge; hardware deferred note present |
| `gnss-server/Cargo.toml` | Server manifest with all deps | VERIFIED | rumqttc, tokio, figment, clap, serde, anyhow, log, env_logger, bytes, rtcm-rs, chrono |
| `gnss-server/src/config.rs` | ServerConfig struct, figment load_config() | VERIFIED | 135 lines; MqttConfig + ServerConfig; load_config; 3 tests |
| `gnss-server/src/mqtt.rs` | mqtt_supervisor, MqttMessage enum | VERIFIED | 173 lines; BACKOFF_STEPS const; supervisor function; 7 tests |
| `gnss-server/src/main.rs` | CLI parsing, Tokio runtime, supervisor spawn | VERIFIED | 94 lines; clap Cli; tokio::main; mpsc+watch channels; supervisor + decode task spawned |
| `gnss-server/src/observation.rs` | Observation, EpochGroup, EphemerisMsg, RtcmEvent types | VERIFIED | 59 lines; all 4 exports present; EphemerisMsg uses Msg1042T for BeiDou |
| `gnss-server/src/rtcm_decode.rs` | decode_rtcm_payload dispatching all MSM and ephemeris types | VERIFIED | 312 lines; 8 MSM variants + 4 ephemeris variants + unknown passthrough |
| `gnss-server/src/epoch.rs` | EpochBuffer with push/flush-on-change | VERIFIED | 198 lines; flush-on-change logic; ISO8601 log line; 4 tests |
| `gnss-server/tests/fixtures/rtcm_sample.bin` | Real RTCM3 frames from gnss.log | VERIFIED | 148-byte GPS MSM4 (type 1074) frame; included via `include_bytes!` in tests |

---

## Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `crates/gnss-nvs/src/esp_idf.rs` | esp_idf_svc::nvs::EspNvs | `#[cfg(feature = "esp-idf")]` | WIRED | Line 13: `#[cfg(feature = "esp-idf")]`; all structs/impls gated |
| `crates/gnss-nvs/src/sequential.rs` | sequential_storage::map::MapStorage | `#[cfg(feature = "sequential")]` | WIRED | Module-level cfg gates in lib.rs; sequential imports at top of file |
| `crates/gnss-nvs/src/lib.rs` | postcard | typed get/set serialization | WIRED | `postcard::to_slice` / `postcard::from_bytes` in both esp_idf.rs and sequential.rs |
| `gnss-server/src/main.rs` | `gnss-server/src/mqtt.rs` | `tokio::spawn(mqtt_supervisor(...))` | WIRED | main.rs line 37: `tokio::spawn(mqtt::mqtt_supervisor(config_arc, msg_tx, state_tx))` |
| `gnss-server/src/mqtt.rs` | rumqttc::EventLoop | `eventloop.poll().await` | WIRED | mqtt.rs line 81: `match eventloop.poll().await` |
| `gnss-server/src/config.rs` | figment | `Env::prefixed("GNSS_")` | WIRED | config.rs line 55: `.merge(Env::prefixed("GNSS_").split("__"))` |
| `gnss-server/src/mqtt.rs` | `gnss-server/src/rtcm_decode.rs` | `decode_rtcm_payload()` called from decode task | WIRED | main.rs line 72: `rtcm_decode::decode_rtcm_payload(&payload, &mut epoch_buf)` |
| `gnss-server/src/rtcm_decode.rs` | `gnss-server/src/epoch.rs` | MSM observations pushed to EpochBuffer | WIRED | rtcm_decode.rs line 42: `epoch_buf.push(epoch_ms, obs)` in `push_and_collect` |
| `gnss-server/src/main.rs` | `gnss-server/src/rtcm_decode.rs` | decode task reads Rtcm messages | WIRED | main.rs lines 68–72: `run_decode_task` reads `MqttMessage::Rtcm` and calls `decode_rtcm_payload` |

---

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| SRVR-01 | 23-02 | Server subscribes to MQTT gnss/{id}/rtcm, /nmea, /heartbeat | SATISFIED | mqtt.rs subscribes to all 3 topics; tests verify topic routing |
| SRVR-02 | 23-02 | Server reconnects with exponential backoff | SATISFIED | BACKOFF_STEPS [1,2,5,10,30]; outer reconnect loop; test_backoff_sequence passes |
| RTCM-01 | 23-03 | Decodes RTCM3 MSM4/MSM7 for GPS and GLONASS | SATISFIED | Msg1074/1077 (GPS), Msg1084/1087 (GLONASS) all handled |
| RTCM-02 | 23-03 | Decodes RTCM3 MSM for Galileo and BeiDou | SATISFIED | Msg1094/1097 (Galileo), Msg1124/1127 (BeiDou) handled |
| RTCM-03 | 23-03 | Decodes ephemeris 1019/1020/1046/1044(BeiDou) | SATISFIED* | Decodes 1019/1020/1046 correctly; BeiDou uses 1042 not 1044 — see note |
| RTCM-04 | 23-03 | Buffers MSM frames within epoch window, emits EpochGroup | SATISFIED | EpochBuffer flush-on-change; `flush_on_change` and `accumulate_same_epoch` tests pass |
| NOSTD-02 | 23-01 | gnss-nvs crate with NvsStore trait and ESP-IDF impl | SATISFIED | NvsStore trait + EspNvsStore; cargo check clean; postcard serialization |
| NOSTD-03 | 23-01 | sequential-storage backed NvsStore implementation | SATISFIED | SeqNvsStore<S> compiles; hardware validation deferred to NOSTD-F02 as planned |

**RTCM-03 note:** REQUIREMENTS.md states "1044 (BeiDou)" but RTCM message 1044 is actually QZSS ephemeris; BeiDou ephemeris is message 1042. The implementation correctly uses Msg1042 for BeiDou. This is a requirements text error — the intent (decode BeiDou ephemeris) is satisfied by the correct 1042 implementation. The REQUIREMENTS.md should be corrected to read "1042 (BeiDou)" in a future pass. This does NOT constitute a gap in phase delivery.

---

## Anti-Patterns Found

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `gnss-server/src/observation.rs` | `#[allow(dead_code)]` on Observation, EpochGroup, EphemerisMsg | Info | Forward-compatibility: fields consumed by Phase 24 RINEX writer; legitimate and documented |
| `gnss-server/src/mqtt.rs` | `#[allow(dead_code)]` on MqttMessage, MqttConfig | Info | Forward-compatibility: payload bytes used in Phase 23-03; credential fields consumed later |
| `gnss-server/src/main.rs` | EpochGroup discarded in `run_decode_task` | Info | Intentional: Phase 24 RINEX writer will replace discard; documented in comment |
| `gnss-server/src/main.rs` | Ephemeris events discarded in `run_decode_task` | Info | Intentional: Phase 24 ephemeris writer will replace discard; documented in comment |

No blocker or warning-level anti-patterns. All `#[allow(dead_code)]` items are intentional forward-compatibility provisions for Phase 24 and are documented in both code comments and SUMMARY.

---

## Human Verification Required

None for automated checks. One item is already deferred by design:

### 1. SeqNvsStore hardware flash validation

**Test:** Run `SeqNvsStore` on device FFFEB5 with the esp-hal flash driver against a real NorFlash partition
**Expected:** get/set round-trip succeeds; NsKey encoding survives erase-write cycle
**Why human:** Requires physical hardware with correct flash range alignment; cannot verify with cargo check alone
**Tracking:** NOSTD-F02 (future phase)

### 2. MQTT broker integration (live connection)

**Test:** Point gnss-server at a real MQTT broker publishing RTCM3 data from device FFFEB5
**Expected:** Server subscribes, receives frames, decodes MSM4/MSM7 epochs, logs "Epoch {ISO8601} GPS:N GLO:N GAL:N BDS:N"
**Why human:** Requires live broker + device; unit tests cover decode logic but not end-to-end packet path
**Tracking:** Phase 24 integration or field test

---

## Gaps Summary

No gaps. All 19 observable truths are verified. All 8 requirements are satisfied. All 12 artifacts exist and are substantive. All 9 key links are wired.

The only notable discrepancy is the RTCM-03 requirement text listing "1044 (BeiDou)" when the correct message number is 1042. The implementation is correct; the requirement text needs a future correction. This does not block phase closure.

Clippy runs clean for both `gnss-nvs` and `gnss-server`:
- `cargo clippy --package gnss-nvs -- -D warnings` exits 0
- `cargo clippy -p gnss-server -- -D warnings` exits 0

---

*Verified: 2026-03-12*
*Verifier: Claude (gsd-verifier)*
