use rtcm_rs::msg::{Msg1019T, Msg1020T, Msg1042T, Msg1046T};

/// GNSS constellation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constellation {
    Gps,
    Glonass,
    Galileo,
    BeiDou,
}

/// A single signal observation from one satellite.
///
/// pseudorange_ms stores the full reconstructed pseudorange (rough_int + rough_mod + fine) in ms.
/// GLONASS carrier phase does not include FCN; conversion to cycles is deferred to Phase 24.
/// Fields are consumed by Phase 24 RINEX writer.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Observation {
    pub constellation: Constellation,
    pub sv_id: u8,
    pub signal_id: u8,
    /// Full reconstructed pseudorange in ms: rough_int + rough_mod + fine_ms.
    /// Option::None when satellite rough range integer is invalid (0xFF).
    pub pseudorange_ms: Option<f64>,
    /// Rough range component (rough_int + rough_mod) in ms.
    /// Stored for documentation/debugging; RINEX writer uses pseudorange_ms directly.
    #[allow(dead_code)]
    pub rough_range_ms: f64,
    /// FCN not in MSM signal data; carrier phase requires FCN for cycle conversion —
    /// emit raw ms value, conversion deferred to Phase 24.
    pub carrier_phase_ms: Option<f64>,
    pub cnr_dbhz: Option<f64>,
    pub epoch_ms: u32,
}

/// A group of observations sharing the same epoch, accumulated across constellations.
/// Fields are consumed by Phase 24 RINEX writer.
#[derive(Debug)]
#[allow(dead_code)]
pub struct EpochGroup {
    pub epoch_ms: u32,
    pub observations: Vec<Observation>,
    pub gps_count: usize,
    pub glo_count: usize,
    pub gal_count: usize,
    pub bds_count: usize,
}

/// Ephemeris message variants wrapping the rtcm-rs decoded types.
///
/// Note: BeiDou ephemeris is RTCM message 1042 (Msg1042T), not 1044 (which is QZSS).
#[allow(dead_code)]
pub enum EphemerisMsg {
    Gps(Msg1019T),
    Glonass(Msg1020T),
    Galileo(Msg1046T),
    Beidou(Msg1042T),
}

/// Events emitted by the RTCM3 decode pipeline.
pub enum RtcmEvent {
    Epoch(EpochGroup),
    Ephemeris(EphemerisMsg),
}
