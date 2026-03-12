//! NMEA GSV accumulator and satellite state types.
//!
//! Parses GSV sentences from a stream of NMEA strings, accumulates satellite
//! info across multi-sentence groups, and emits a complete `SatelliteState`
//! when the final sentence of a group arrives.

use nmea::parse_nmea_sentence;
use nmea::sentences::{parse_gsv, GnssType};

/// Information about a single satellite in view.
#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
pub struct SatInfo {
    pub prn: u32,
    pub elevation: Option<f32>,
    pub azimuth: Option<f32>,
    pub snr: Option<f32>,
    pub gnss_type: String,
}

/// A complete snapshot of all satellites in view.
#[derive(Debug, serde::Serialize)]
#[allow(dead_code)]
pub struct SatelliteState {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub satellites: Vec<SatInfo>,
}

/// Accumulates multi-sentence GSV groups and emits `SatelliteState` on completion.
#[allow(dead_code)]
pub struct GsvAccumulator {
    satellites: Vec<SatInfo>,
    last_emit: std::time::Instant,
}

#[allow(dead_code)]
fn gnss_type_str(t: GnssType) -> String {
    match t {
        GnssType::Gps => "GPS".to_string(),
        GnssType::Glonass => "GLONASS".to_string(),
        GnssType::Galileo => "GALILEO".to_string(),
        GnssType::Beidou => "BEIDOU".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

#[allow(dead_code)]
impl GsvAccumulator {
    pub fn new() -> Self {
        Self {
            satellites: Vec::new(),
            last_emit: std::time::Instant::now(),
        }
    }

    /// Feed a single NMEA sentence string.
    ///
    /// Returns `Some(SatelliteState)` when the final sentence of a GSV group
    /// is received, otherwise returns `None`.
    pub fn feed(&mut self, sentence: &str) -> Option<SatelliteState> {
        if !sentence.contains("GSV") {
            return None;
        }

        let nmea_sentence = parse_nmea_sentence(sentence.trim()).ok()?;
        let gsv = parse_gsv(nmea_sentence).ok()?;

        // Reset accumulator at start of a new group
        if gsv.sentence_num == 1 {
            self.satellites.clear();
        }

        let gnss_type = gnss_type_str(gsv.gnss_type);

        for sat in (&gsv.sats_info).into_iter().flatten() {
            self.satellites.push(SatInfo {
                prn: sat.prn(),
                elevation: sat.elevation(),
                azimuth: sat.azimuth(),
                snr: sat.snr(),
                gnss_type: gnss_type.clone(),
            });
        }

        if gsv.sentence_num == gsv.number_of_sentences {
            self.last_emit = std::time::Instant::now();
            let satellites = self.satellites.clone();
            Some(SatelliteState {
                msg_type: "satellites",
                satellites,
            })
        } else {
            None
        }
    }
}

/// Wrap a raw heartbeat JSON payload as a tagged message.
///
/// Returns `Some("{\"type\":\"heartbeat\",\"data\":{...}}")` or `None` if the
/// payload is not valid UTF-8.
#[allow(dead_code)]
pub fn tag_heartbeat(payload: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(payload).ok()?;
    Some(format!(r#"{{"type":"heartbeat","data":{}}}"#, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-valid 1-sentence GSV: 1 satellite (PRN 1, el=45, az=180, snr=42)
    // GP talker (GPS) — nmea 0.7 parse_gsv requires known talker ID
    const GSV_ONE_SAT: &str = "$GPGSV,1,1,01,01,45,180,42*47\r\n";

    // 2-sentence group: first sentence has 2 sats, second has 1 sat (total 3)
    const GSV_TWO_SENT_1: &str = "$GPGSV,2,1,03,01,40,083,46,02,17,308,41*7F\r\n";
    const GSV_TWO_SENT_2: &str = "$GPGSV,2,2,03,03,07,344,39*47\r\n";

    // Single new group sentence (1-of-1, 1 sat PRN 99)
    const GSV_RESET: &str = "$GPGSV,1,1,01,99,30,090,35*44\r\n";

    // GSV with blank SNR field
    const GSV_NO_SNR: &str = "$GPGSV,1,1,01,01,45,180,*41\r\n";

    // A GGA sentence (not GSV)
    const GGA_SENTENCE: &str =
        "$GNGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\r\n";

    #[test]
    fn gsv_accumulator_basic() {
        let mut acc = GsvAccumulator::new();
        let result = acc.feed(GSV_ONE_SAT);
        assert!(result.is_some(), "expected Some(SatelliteState) for 1-of-1 GSV");
        let state = result.unwrap();
        assert_eq!(state.msg_type, "satellites");
        assert_eq!(state.satellites.len(), 1);
        let sat = &state.satellites[0];
        assert_eq!(sat.prn, 1);
        assert_eq!(sat.elevation, Some(45.0));
        assert_eq!(sat.azimuth, Some(180.0));
        assert_eq!(sat.snr, Some(42.0));
    }

    #[test]
    fn gsv_accumulator_multi_sentence() {
        let mut acc = GsvAccumulator::new();
        let r1 = acc.feed(GSV_TWO_SENT_1);
        assert!(r1.is_none(), "sentence 1-of-2 should return None");
        let r2 = acc.feed(GSV_TWO_SENT_2);
        assert!(r2.is_some(), "sentence 2-of-2 should return Some");
        let state = r2.unwrap();
        assert_eq!(state.satellites.len(), 3);
    }

    #[test]
    fn gsv_accumulator_reset_on_new_group() {
        let mut acc = GsvAccumulator::new();
        // Feed first sentence of a 2-sentence group (partial)
        let _ = acc.feed(GSV_TWO_SENT_1);
        // Now feed a new 1-of-1 group — should clear old partial data
        let result = acc.feed(GSV_RESET);
        assert!(result.is_some(), "1-of-1 after partial group should emit");
        let state = result.unwrap();
        assert_eq!(
            state.satellites.len(),
            1,
            "should only have 1 sat from new group"
        );
        assert_eq!(state.satellites[0].prn, 99);
    }

    #[test]
    fn gsv_snr_null() {
        let mut acc = GsvAccumulator::new();
        let result = acc.feed(GSV_NO_SNR);
        assert!(result.is_some(), "1-of-1 with blank SNR should emit");
        let state = result.unwrap();
        assert_eq!(state.satellites.len(), 1);
        assert!(
            state.satellites[0].snr.is_none(),
            "SNR should be None for blank field"
        );
    }

    #[test]
    fn heartbeat_tag() {
        let payload = b"{\"uptime_s\":100}";
        let result = tag_heartbeat(payload);
        assert_eq!(
            result,
            Some(r#"{"type":"heartbeat","data":{"uptime_s":100}}"#.to_string())
        );
    }

    #[test]
    fn non_gsv_ignored() {
        let mut acc = GsvAccumulator::new();
        let result = acc.feed(GGA_SENTENCE);
        assert!(result.is_none(), "GGA sentence should return None");
    }
}
