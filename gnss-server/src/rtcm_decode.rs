use rtcm_rs::prelude::*;

use crate::epoch::EpochBuffer;
use crate::observation::{Constellation, EphemerisMsg, Observation, RtcmEvent};

/// Decode all RTCM3 frames in `payload`, updating the epoch buffer.
///
/// For each MSM message (GPS/GLONASS/Galileo/BeiDou MSM4 and MSM7), extracts
/// signal observations and calls `epoch_buf.push()`. If the epoch changed, the
/// flushed EpochGroup is emitted as an RtcmEvent::Epoch.
///
/// For ephemeris messages (1019/1020/1046/1042), emits RtcmEvent::Ephemeris directly.
///
/// Unknown messages are silently ignored.
pub fn decode_rtcm_payload(payload: &[u8], epoch_buf: &mut EpochBuffer) -> Vec<RtcmEvent> {
    let mut events = Vec::new();
    let mut remaining = payload;

    loop {
        match next_msg_frame(remaining) {
            (consumed, Some(frame)) => {
                remaining = &remaining[consumed..];
                let msg = frame.get_message();
                handle_message(msg, epoch_buf, &mut events);
            }
            (0, None) => break,
            (consumed, None) => {
                remaining = &remaining[consumed..];
            }
        }
    }

    events
}

fn push_and_collect(
    epoch_buf: &mut EpochBuffer,
    epoch_ms: u32,
    obs: Vec<Observation>,
    events: &mut Vec<RtcmEvent>,
) {
    if let Some(group) = epoch_buf.push(epoch_ms, obs) {
        events.push(RtcmEvent::Epoch(group));
    }
}

fn handle_message(msg: Message, epoch_buf: &mut EpochBuffer, events: &mut Vec<RtcmEvent>) {
    match msg {
        // GPS MSM4
        Message::Msg1074(m) => {
            let epoch_ms = m.gps_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Gps,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ms,
                    cnr_dbhz: s.gnss_signal_cnr_dbhz.map(|v| v as f64),
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // GPS MSM7
        Message::Msg1077(m) => {
            let epoch_ms = m.gps_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Gps,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ext_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ext_ms,
                    cnr_dbhz: s.gnss_signal_cnr_ext_dbhz,
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // GLONASS MSM4
        Message::Msg1084(m) => {
            let epoch_ms = m.glo_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Glonass,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ms,
                    // FCN not in MSM signal data; carrier phase requires FCN for cycle conversion —
                    // emit raw ms value, conversion deferred to Phase 24.
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ms,
                    cnr_dbhz: s.gnss_signal_cnr_dbhz.map(|v| v as f64),
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // GLONASS MSM7
        Message::Msg1087(m) => {
            let epoch_ms = m.glo_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Glonass,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ext_ms,
                    // FCN not in MSM signal data; carrier phase requires FCN for cycle conversion —
                    // emit raw ms value, conversion deferred to Phase 24.
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ext_ms,
                    cnr_dbhz: s.gnss_signal_cnr_ext_dbhz,
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // Galileo MSM4
        Message::Msg1094(m) => {
            let epoch_ms = m.gal_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Galileo,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ms,
                    cnr_dbhz: s.gnss_signal_cnr_dbhz.map(|v| v as f64),
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // Galileo MSM7
        Message::Msg1097(m) => {
            let epoch_ms = m.gal_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::Galileo,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ext_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ext_ms,
                    cnr_dbhz: s.gnss_signal_cnr_ext_dbhz,
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // BeiDou MSM4
        Message::Msg1124(m) => {
            let epoch_ms = m.bds_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::BeiDou,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ms,
                    cnr_dbhz: s.gnss_signal_cnr_dbhz.map(|v| v as f64),
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // BeiDou MSM7
        Message::Msg1127(m) => {
            let epoch_ms = m.bds_epoch_time_ms;
            let obs: Vec<Observation> = m
                .data_segment
                .signal_data
                .iter()
                .map(|s| Observation {
                    constellation: Constellation::BeiDou,
                    sv_id: s.satellite_id,
                    signal_id: s.signal_id.band(),
                    pseudorange_ms: s.gnss_signal_fine_pseudorange_ext_ms,
                    carrier_phase_ms: s.gnss_signal_fine_phaserange_ext_ms,
                    cnr_dbhz: s.gnss_signal_cnr_ext_dbhz,
                    epoch_ms,
                })
                .collect();
            push_and_collect(epoch_buf, epoch_ms, obs, events);
        }
        // GPS Ephemeris
        Message::Msg1019(m) => {
            events.push(RtcmEvent::Ephemeris(EphemerisMsg::Gps(m)));
        }
        // GLONASS Ephemeris
        Message::Msg1020(m) => {
            events.push(RtcmEvent::Ephemeris(EphemerisMsg::Glonass(m)));
        }
        // Galileo Ephemeris
        Message::Msg1046(m) => {
            events.push(RtcmEvent::Ephemeris(EphemerisMsg::Galileo(m)));
        }
        // BeiDou Ephemeris (RTCM msg 1042)
        Message::Msg1042(m) => {
            events.push(RtcmEvent::Ephemeris(EphemerisMsg::Beidou(m)));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::Constellation;

    /// Real GPS MSM4 (type 1074) frame extracted from gnss.log
    const GPS_MSM4_FRAME: &[u8] = include_bytes!("../tests/fixtures/rtcm_sample.bin");

    #[test]
    fn gps_msm4_decode() {
        let mut epoch_buf = EpochBuffer::new();
        let events = decode_rtcm_payload(GPS_MSM4_FRAME, &mut epoch_buf);

        // No flush on first frame (no previous epoch)
        assert!(
            events.is_empty(),
            "First MSM4 frame should not flush (no prior epoch)"
        );

        // Push a dummy observation with a different epoch to force flush
        use crate::observation::Observation;
        let dummy_obs = vec![Observation {
            constellation: Constellation::Gps,
            sv_id: 1,
            signal_id: 1,
            pseudorange_ms: None,
            carrier_phase_ms: None,
            cnr_dbhz: None,
            epoch_ms: 99999999,
        }];
        let group = epoch_buf
            .push(99999999, dummy_obs)
            .expect("flush should return group from MSM4 frame data");

        // Verify we got GPS observations
        assert!(
            group.gps_count > 0,
            "Decoded GPS MSM4 should have at least one GPS observation"
        );
        assert_eq!(
            group.glo_count, 0,
            "GPS MSM4 should have no GLONASS observations"
        );

        // Verify Observation struct fields
        let first_obs = &group.observations[0];
        assert_eq!(first_obs.constellation, Constellation::Gps);
        assert!(first_obs.sv_id > 0, "sv_id should be non-zero");
    }

    #[test]
    fn glo_phase_is_option() {
        // GLONASS carrier_phase_ms must be Option<f64> — None is valid
        use crate::observation::Observation;
        let glo_obs = Observation {
            constellation: Constellation::Glonass,
            sv_id: 3,
            signal_id: 1,
            pseudorange_ms: Some(1.0),
            carrier_phase_ms: None, // None is valid — FCN required for cycle conversion
            cnr_dbhz: Some(30.0),
            epoch_ms: 500,
        };
        // carrier_phase_ms is Option<f64> — this test verifies the type compiles and None works
        assert!(glo_obs.carrier_phase_ms.is_none());
    }

    #[test]
    fn ephemeris_1019_event_type() {
        // Verify EphemerisMsg::Gps variant exists and wraps Msg1019T
        use rtcm_rs::msg::Msg1019T;
        let eph = EphemerisMsg::Gps(Msg1019T::default());
        assert!(matches!(eph, EphemerisMsg::Gps(_)));
    }

    #[test]
    fn decode_unknown_message_silent() {
        // A valid RTCM3 frame with message 1006 (station coord) should be silently ignored.
        // Bytes: d3 00 15 3e e0 00 03 ba 95 27 f2 be 8b 43 af f5 66 38 02 21 4a 82 00 00 02 e6 d2
        let msg1006: &[u8] = &[
            0xd3, 0x00, 0x15, 0x3e, 0xe0, 0x00, 0x03, 0xba, 0x95, 0x27, 0xf2, 0xbe, 0x8b, 0x43,
            0xaf, 0xf5, 0x66, 0x38, 0x02, 0x21, 0x4a, 0x82, 0x00, 0x00, 0x02, 0xe6, 0xd2,
        ];
        let mut epoch_buf = EpochBuffer::new();
        let events = decode_rtcm_payload(msg1006, &mut epoch_buf);
        assert!(events.is_empty(), "Message 1006 should produce no events");
    }
}
