use chrono::{DateTime, Utc};

use crate::observation::{Constellation, EpochGroup, Observation};

/// Accumulates RTCM3 MSM signal observations and flushes when the epoch changes.
///
/// Epoch boundary is detected by comparing the epoch_ms timestamp. When a new
/// epoch arrives (different epoch_ms), the accumulated observations are flushed
/// as an EpochGroup and a log line is emitted.
pub struct EpochBuffer {
    epoch_key: u32,
    observations: Vec<Observation>,
}

impl EpochBuffer {
    /// Create a new empty buffer. epoch_key = 0 means "no epoch yet".
    pub fn new() -> Self {
        EpochBuffer {
            epoch_key: 0,
            observations: Vec::new(),
        }
    }

    /// Push observations for the given epoch_ms.
    ///
    /// Returns `Some(EpochGroup)` when the epoch changes (flushing the previous
    /// accumulated observations). Returns `None` when the observations are
    /// accumulated into the current epoch.
    pub fn push(
        &mut self,
        epoch_ms: u32,
        new_obs: Vec<Observation>,
    ) -> Option<EpochGroup> {
        if self.epoch_key != 0 && epoch_ms != self.epoch_key {
            // Epoch boundary — flush accumulated observations
            let group = self.build_group();
            self.epoch_key = epoch_ms;
            self.observations = new_obs;
            Some(group)
        } else {
            self.epoch_key = epoch_ms;
            self.observations.extend(new_obs);
            None
        }
    }

    fn build_group(&mut self) -> EpochGroup {
        let observations = std::mem::take(&mut self.observations);

        let gps_count = observations
            .iter()
            .filter(|o| o.constellation == Constellation::Gps)
            .count();
        let glo_count = observations
            .iter()
            .filter(|o| o.constellation == Constellation::Glonass)
            .count();
        let gal_count = observations
            .iter()
            .filter(|o| o.constellation == Constellation::Galileo)
            .count();
        let bds_count = observations
            .iter()
            .filter(|o| o.constellation == Constellation::BeiDou)
            .count();

        let epoch_ms = self.epoch_key;

        // Log the epoch boundary
        let now: DateTime<Utc> = Utc::now();
        log::info!(
            "Epoch {} GPS:{} GLO:{} GAL:{} BDS:{}",
            now.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            gps_count,
            glo_count,
            gal_count,
            bds_count,
        );

        EpochGroup {
            epoch_ms,
            observations,
            gps_count,
            glo_count,
            gal_count,
            bds_count,
        }
    }
}

impl Default for EpochBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::Observation;

    fn gps_obs(epoch_ms: u32) -> Vec<Observation> {
        vec![Observation {
            constellation: Constellation::Gps,
            sv_id: 1,
            signal_id: 1,
            pseudorange_ms: Some(0.0),
            carrier_phase_ms: Some(0.0),
            cnr_dbhz: Some(40.0),
            epoch_ms,
        }]
    }

    fn glo_obs(epoch_ms: u32) -> Vec<Observation> {
        vec![Observation {
            constellation: Constellation::Glonass,
            sv_id: 2,
            signal_id: 1,
            pseudorange_ms: Some(0.0),
            carrier_phase_ms: None, // FCN not available
            cnr_dbhz: Some(35.0),
            epoch_ms,
        }]
    }

    #[test]
    fn flush_on_change() {
        let mut buf = EpochBuffer::new();

        // Push GPS obs at epoch 1000
        let result1 = buf.push(1000, gps_obs(1000));
        assert!(result1.is_none(), "First push should not flush");

        // Push GPS obs at epoch 2000 — should flush epoch 1000
        let result2 = buf.push(2000, gps_obs(2000));
        let group = result2.expect("Second push with different epoch_ms must flush");

        assert_eq!(group.epoch_ms, 1000);
        assert_eq!(group.gps_count, 1);
        assert_eq!(group.glo_count, 0);
        assert_eq!(group.observations.len(), 1);
    }

    #[test]
    fn accumulate_same_epoch() {
        let mut buf = EpochBuffer::new();

        // Push GPS obs at epoch 1000
        let result1 = buf.push(1000, gps_obs(1000));
        assert!(result1.is_none(), "GPS push should not flush");

        // Push GLO obs at same epoch 1000 — should accumulate
        let result2 = buf.push(1000, glo_obs(1000));
        assert!(result2.is_none(), "GLO push at same epoch should not flush");

        // Verify accumulation by flushing with a different epoch
        let result3 = buf.push(2000, gps_obs(2000));
        let group = result3.expect("New epoch should flush");
        assert_eq!(group.gps_count, 1);
        assert_eq!(group.glo_count, 1);
        assert_eq!(group.observations.len(), 2);
    }

    #[test]
    fn epoch_key_resets() {
        let mut buf = EpochBuffer::new();

        buf.push(1000, gps_obs(1000));
        buf.push(2000, gps_obs(2000));

        // Now flush epoch 2000 by pushing epoch 3000
        let group = buf.push(3000, gps_obs(3000)).expect("must flush epoch 2000");
        assert_eq!(group.epoch_ms, 2000, "flushed group must have epoch_ms 2000");
    }

    #[test]
    fn glonass_none_carrier_phase_preserved() {
        let mut buf = EpochBuffer::new();

        let glo = vec![Observation {
            constellation: Constellation::Glonass,
            sv_id: 5,
            signal_id: 1,
            pseudorange_ms: Some(1.0),
            carrier_phase_ms: None, // None is valid — FCN required for conversion
            cnr_dbhz: Some(30.0),
            epoch_ms: 1000,
        }];

        buf.push(1000, glo);
        let group = buf.push(2000, gps_obs(2000)).expect("flush");
        let obs = &group.observations[0];
        assert!(
            obs.carrier_phase_ms.is_none(),
            "GLONASS None carrier_phase_ms must not be replaced with 0.0"
        );
    }
}
