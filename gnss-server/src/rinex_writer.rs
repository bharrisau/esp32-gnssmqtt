// RINEX writers are wired to main in Task 2; allow dead_code until then.
#![allow(dead_code)]

use std::io::{BufWriter, Write};
use std::path::PathBuf;

use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};

use crate::observation::{Constellation, EpochGroup, EphemerisMsg, Observation};

// Physics constants
const GPS_L1_HZ: f64 = 1_575_420_000.0;
const SPEED_OF_LIGHT_M_PER_MS: f64 = 299_792.458; // m/ms

/// RINEX 2.11 observation types written per satellite.
const OBS_TYPES: &[&str] = &["C1", "L1", "S1"];

/// Convert CNR dBHz to RINEX signal strength indicator (SSI) 1–9.
///
/// Formula from RTKLIB rinex.c: ssi = round((cnr - 15) / 6 + 1.5), clamped 1..9.
/// Returns 0 when CNR is None (unknown signal strength).
fn cnr_to_ssi(cnr: Option<f64>) -> u8 {
    match cnr {
        None => 0,
        Some(v) => ((v - 15.0) / 6.0 + 1.5).round().clamp(1.0, 9.0) as u8,
    }
}

/// Write a single 16-character observation field (F14.3 + LLI + SSI).
///
/// When `value` is Some(v): writes `{:14.3}{}{}`  (14-char float, 1-char LLI, 1-char SSI).
/// When `value` is None: writes exactly 16 spaces (missing observation marker per RINEX 2.11).
fn write_obs<W: Write>(w: &mut W, value: Option<f64>, lli: u8, ssi: u8) -> std::io::Result<()> {
    match value {
        Some(v) => write!(w, "{:14.3}{}{}", v, lli, ssi),
        None => write!(w, "                "), // exactly 16 spaces
    }
}

/// Convert pseudorange from milliseconds to meters.
///
/// `pseudorange_ms` already stores the full reconstructed value (rough_int + rough_mod + fine).
fn pseudorange_m(obs: &Observation) -> Option<f64> {
    obs.pseudorange_ms
        .map(|ms| ms * SPEED_OF_LIGHT_M_PER_MS)
}

/// Convert carrier phase from milliseconds to cycles.
///
/// GPS, Galileo, BeiDou: use GPS L1 frequency (1575.42 MHz — Galileo E1 and BDS B1 share this).
/// GLONASS: FCN is required but not available in MSM signal data — returns None (written as 16
/// spaces per RINEX 2.11 spec and STATE.md decision; RTKLIB ignores missing phase gracefully).
fn carrier_phase_cycles(obs: &Observation) -> Option<f64> {
    match obs.constellation {
        Constellation::Glonass => None,
        _ => obs
            .carrier_phase_ms
            .map(|ms| ms * GPS_L1_HZ / 1000.0),
    }
}

/// Convert a constellation + SV ID to a RINEX 2.11 PRN string (e.g. "G05", "R03").
fn to_rinex_prn(c: Constellation, sv_id: u8) -> String {
    match c {
        Constellation::Gps => format!("G{:02}", sv_id),
        Constellation::Glonass => format!("R{:02}", sv_id),
        Constellation::Galileo => format!("E{:02}", sv_id),
        Constellation::BeiDou => format!("C{:02}", sv_id),
    }
}

/// Write the RINEX 2.11 observation file header.
///
/// Each line is exactly 80 characters: data in cols 1-60, label left-justified in cols 61-80.
/// Writes 9 mandatory header records:
///   RINEX VERSION / TYPE, PGM / RUN BY / DATE, MARKER NAME,
///   APPROX POSITION XYZ, ANTENNA: DELTA H/E/N, # / TYPES OF OBSERV,
///   WAVELENGTH FACT L1/2, TIME OF FIRST OBS, END OF HEADER
pub fn write_obs_header<W: Write>(
    w: &mut BufWriter<W>,
    station: &str,
    approx_xyz: (f64, f64, f64),
    first_obs_utc: &DateTime<Utc>,
) -> std::io::Result<()> {
    // RINEX VERSION / TYPE — F9.2,11X,A1,19X,A1,19X = 60 data + 20 label = 80 chars
    // File type 'O' at col 21, satellite system 'M' at col 41 (per RINEX 2.11 sec 5.1)
    writeln!(
        w,
        "     2.11           OBSERVATION DATA    {:<20}{:<20}",
        "M", "RINEX VERSION / TYPE"
    )?;
    // PGM / RUN BY / DATE — 3 × A20 (program, agency, date) = 60 data chars
    writeln!(
        w,
        "{:<20}{:<20}{:<20}{:<20}",
        "gnss-server",
        "gnss",
        first_obs_utc.format("%Y%m%d").to_string(),
        "PGM / RUN BY / DATE"
    )?;
    // MARKER NAME — A60 data + A20 label = 80 chars
    writeln!(w, "{:<60}{:<20}", station, "MARKER NAME")?;
    // APPROX POSITION XYZ — 3F14.4 (42 chars) + 18 spaces = 60 data + 20 label = 80
    writeln!(
        w,
        "{:14.4}{:14.4}{:14.4}                  {:<20}",
        approx_xyz.0, approx_xyz.1, approx_xyz.2,
        "APPROX POSITION XYZ"
    )?;
    // ANTENNA: DELTA H/E/N — same layout as APPROX POSITION XYZ
    writeln!(
        w,
        "{:14.4}{:14.4}{:14.4}                  {:<20}",
        0.0f64, 0.0f64, 0.0f64,
        "ANTENNA: DELTA H/E/N"
    )?;
    // # / TYPES OF OBSERV — I6,9(4X,A1,A1): count + obs-type codes (6 chars each)
    // 3 types (C1, L1, S1): data = "     3    C1    L1    S1" (24 chars) padded to 60, label 20
    writeln!(
        w,
        "{:<60}{:<20}",
        format!(
            "     {}    {}    {}    {}",
            OBS_TYPES.len(),
            OBS_TYPES[0],
            OBS_TYPES[1],
            OBS_TYPES[2]
        ),
        "# / TYPES OF OBSERV"
    )?;
    // WAVELENGTH FACT L1/2 — 2I6 for L1/L2 + remaining data + label
    // "     1     1" = 12 chars, padded to 60 + label
    writeln!(
        w,
        "     1     1{:<48}{:<20}",
        "",
        "WAVELENGTH FACT L1/2"
    )?;
    // TIME OF FIRST OBS — 5I6,F13.7,5X,A3 = 51 chars, padded to 60 + label
    let second_f =
        first_obs_utc.second() as f64 + first_obs_utc.nanosecond() as f64 / 1_000_000_000.0;
    writeln!(
        w,
        "{:6}{:6}{:6}{:6}{:6}{:13.7}     GPS{:<9}{:<20}",
        first_obs_utc.year(),
        first_obs_utc.month(),
        first_obs_utc.day(),
        first_obs_utc.hour(),
        first_obs_utc.minute(),
        second_f,
        "",
        "TIME OF FIRST OBS"
    )?;
    // END OF HEADER — 60 spaces + label
    writeln!(w, "{:<60}{:<20}", "", "END OF HEADER")?;
    Ok(())
}

/// Write a single RINEX 2.11 epoch record (header line + per-satellite observations).
///
/// Epoch header handles >12 satellites with continuation lines (32-space prefix).
/// Each satellite line has 3 observations: C1 (pseudorange_m), L1 (phase_cycles), S1 (CNR).
pub fn write_epoch<W: Write>(
    w: &mut BufWriter<W>,
    epoch_utc: &DateTime<Utc>,
    group: &EpochGroup,
) -> std::io::Result<()> {
    let year2 = epoch_utc.year() % 100;
    let sats: Vec<(String, &Observation)> = group
        .observations
        .iter()
        .map(|obs| (to_rinex_prn(obs.constellation, obs.sv_id), obs))
        .collect();
    let nsat = sats.len();

    // Epoch header: " YY MM DD HH MM SS.SSSSSSS  0  N"
    let second_f =
        epoch_utc.second() as f64 + epoch_utc.nanosecond() as f64 / 1_000_000_000.0;
    write!(
        w,
        " {:02} {:2} {:2} {:2} {:2}{:11.7}  0  {:3}",
        year2,
        epoch_utc.month(),
        epoch_utc.day(),
        epoch_utc.hour(),
        epoch_utc.minute(),
        second_f,
        nsat
    )?;

    // Write satellite PRN list — max 12 per line, continuation with 32-space prefix
    for (i, (prn, _)) in sats.iter().enumerate() {
        if i > 0 && i % 12 == 0 {
            writeln!(w)?;
            write!(w, "{:32}", "")?;
        }
        write!(w, "{}", prn)?;
    }
    writeln!(w)?;

    // Per-satellite observation lines: C1, L1, S1 (3 × 16 chars = 48 chars per line)
    for (_, obs) in &sats {
        let ssi = cnr_to_ssi(obs.cnr_dbhz);
        write_obs(w, pseudorange_m(obs), 0, ssi)?; // C1
        write_obs(w, carrier_phase_cycles(obs), 0, ssi)?; // L1
        write_obs(w, obs.cnr_dbhz, 0, 0)?; // S1
        writeln!(w)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Navigation file helpers
// ---------------------------------------------------------------------------

/// Format f64 as RINEX D19.12 (Fortran double-precision notation).
///
/// Output is always exactly 19 characters: sign(1) + digit(1) + dot(1) + 12 decimal + D(1) + sign(1) + 2 digits = 19.
/// Examples: " 0.000000000000D+00", " 1.234567890123D-04", "-1.000000000000D+10"
pub fn to_d19_12(val: f64) -> String {
    if !val.is_finite() || val == 0.0 {
        return " 0.000000000000D+00".to_string();
    }
    // {:19.12E} produces exactly 19 chars but Rust uses variable-length exponents (e.g. E10 vs E-4)
    // We need D + two-digit signed exponent always. Strategy: format mantissa separately.
    // {:+.12E} gives "+1.234567890123E-4" — extract mantissa (16 chars with sign) and exponent.
    let s = format!("{:.12E}", val); // e.g. "1.234567890123E-4" or "-1.000000000000E10"
    if let Some(e_pos) = s.find('E') {
        let mantissa = &s[..e_pos]; // e.g. "1.234567890123" or "-1.000000000000"
        let exp: i32 = s[e_pos + 1..].parse().unwrap_or(0);
        // For positive mantissa: " " + mantissa (14 chars) + "D" + sign + 2-digit exp = 1+14+1+3 = 19
        // For negative mantissa: mantissa already starts with "-" (15 chars) + "D" + sign + 2-digit exp = 15+1+3 = 19
        if mantissa.starts_with('-') {
            format!("{}D{:+03}", mantissa, exp)
        } else {
            format!(" {}D{:+03}", mantissa, exp)
        }
    } else {
        " 0.000000000000D+00".to_string()
    }
}

/// Compute current GPS week from UTC system clock.
///
/// GPS epoch: 1980-01-06 00:00:00 UTC. Does not apply the 1024-week rollover.
pub fn current_gps_week() -> u32 {
    let gps_epoch = Utc.with_ymd_and_hms(1980, 1, 6, 0, 0, 0).unwrap();
    let now = Utc::now();
    let days = (now - gps_epoch).num_days();
    (days / 7) as u32
}

/// Convert GPS time-of-week (ms) + GPS week number to UTC DateTime.
///
/// GPS time is ahead of UTC by 18 leap seconds (as of 2026; next event unknown).
pub fn gps_tow_to_utc(gps_week: u32, tow_ms: u32) -> DateTime<Utc> {
    let gps_epoch = Utc.with_ymd_and_hms(1980, 1, 6, 0, 0, 0).unwrap();
    let total_ms = gps_week as i64 * 7 * 24 * 3600 * 1000 + tow_ms as i64;
    gps_epoch + Duration::milliseconds(total_ms) - Duration::seconds(18)
}

/// Write the RINEX 2.11 navigation file header.
///
/// Three lines: RINEX VERSION/TYPE, PGM/RUN BY/DATE, END OF HEADER.
pub fn write_nav_header<W: Write>(w: &mut BufWriter<W>, date: &DateTime<Utc>) -> std::io::Result<()> {
    writeln!(
        w,
        "{:<60}{:<20}",
        "     2.11           NAVIGATION DATA",
        "RINEX VERSION / TYPE"
    )?;
    writeln!(
        w,
        "{:<20}{:<20}{:<20}{:<20}",
        "gnss-server",
        "gnss",
        date.format("%Y%m%d").to_string(),
        "PGM / RUN BY / DATE"
    )?;
    writeln!(w, "{:<60}{:<20}", "", "END OF HEADER")?;
    Ok(())
}

/// Write a single GPS navigation record (8 lines) from Msg1019T.
///
/// Line 1: PRN/EPOCH/SV CLK (satellite_id, epoch, af0, af1, af2)
/// Lines 2-8: BROADCAST ORBIT 1-7 (3 spaces + 4 × D19.12)
fn write_gps_nav<W: Write>(
    w: &mut BufWriter<W>,
    epoch_utc: &DateTime<Utc>,
    msg: &rtcm_rs::msg::Msg1019T,
) -> std::io::Result<()> {
    let yy = epoch_utc.year() % 100;
    // Line 1: PRN EPOCH SV CLK
    writeln!(
        w,
        "{:2} {:02} {:2} {:2} {:2} {:2}{:5.1}{}{}{}",
        msg.gps_satellite_id,
        yy,
        epoch_utc.month(),
        epoch_utc.day(),
        epoch_utc.hour(),
        epoch_utc.minute(),
        epoch_utc.second() as f64,
        to_d19_12(msg.af0_s),
        to_d19_12(msg.af1_s_s as f64),
        to_d19_12(msg.af2_s_s2 as f64),
    )?;
    // Orbit 1: IODE, Crs, Delta n, M0
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.iode as f64),
        to_d19_12(msg.crs_m as f64),
        to_d19_12(msg.delta_n_sc_s as f64),
        to_d19_12(msg.m0_sc),
    )?;
    // Orbit 2: Cuc, e, Cus, sqrt(A)
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.cuc_rad as f64),
        to_d19_12(msg.eccentricity),
        to_d19_12(msg.cus_rad as f64),
        to_d19_12(msg.sqrt_a_sqrt_m),
    )?;
    // Orbit 3: Toe, Cic, OMEGA0, Cis
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.toe_s as f64),
        to_d19_12(msg.cic_rad as f64),
        to_d19_12(msg.omega0_sc),
        to_d19_12(msg.cis_rad as f64),
    )?;
    // Orbit 4: i0, Crc, omega, OMEGA DOT
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.i0_sc),
        to_d19_12(msg.crc_m as f64),
        to_d19_12(msg.omega_sc),
        to_d19_12(msg.omegadot_sc_s),
    )?;
    // Orbit 5: IDOT, Codes on L2, GPS week, L2 P flag
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.idot_sc_s),
        to_d19_12(msg.code_on_l2_ind as f64),
        to_d19_12(msg.gps_week_number as f64),
        to_d19_12(msg.l2_p_data_flag as f64),
    )?;
    // Orbit 6: SV accuracy, SV health, TGD, IODC
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.ura_index as f64),
        to_d19_12(msg.sv_health_ind as f64),
        to_d19_12(msg.tgd_s as f64),
        to_d19_12(msg.iodc as f64),
    )?;
    // Orbit 7: Transmission time, fit interval, 0.0, 0.0
    // toc_s used as transmission time (time of clock); 0.0 for spare fields
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.toc_s as f64),
        to_d19_12(msg.fit_interval_ind as f64),
        to_d19_12(0.0),
        to_d19_12(0.0),
    )?;
    Ok(())
}

/// Write a single GLONASS navigation record (4 lines) from Msg1020T.
///
/// Line 1: SLOT/EPOCH/SV CLK (satellite_id, epoch, -tau_n, gamma_n, message_frame_time)
/// Lines 2-4: BROADCAST ORBIT 1-3 (3 spaces + 4 × D19.12) with positions in km
fn write_glo_nav<W: Write>(
    w: &mut BufWriter<W>,
    epoch_utc: &DateTime<Utc>,
    msg: &rtcm_rs::msg::Msg1020T,
) -> std::io::Result<()> {
    let yy = epoch_utc.year() % 100;
    // Frame time tk in seconds = hours*3600 + minutes*60 + seconds (tk_s is 0 or 30)
    let tk_s = msg.tk_h as f64 * 3600.0 + msg.tk_min as f64 * 60.0 + msg.tk_s as f64;
    // Line 1: SLOT EPOCH SV CLK
    // clock_bias = -tau_n (negated per RINEX convention)
    writeln!(
        w,
        "{:2} {:02} {:2} {:2} {:2} {:2}{:5.1}{}{}{}",
        msg.glo_satellite_id,
        yy,
        epoch_utc.month(),
        epoch_utc.day(),
        epoch_utc.hour(),
        epoch_utc.minute(),
        epoch_utc.second() as f64,
        to_d19_12(-msg.tau_n_s),
        to_d19_12(msg.gamma_n as f64),
        to_d19_12(tk_s),
    )?;
    // Orbit 1: X position (km), X velocity (km/s), X accel (km/s²), health
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.xn_km),
        to_d19_12(msg.xn_first_deriv_km_s),
        to_d19_12(msg.xn_second_deriv_km_s2 as f64),
        to_d19_12(msg.glo_eph_health_flag as f64),
    )?;
    // Orbit 2: Y position (km), Y velocity (km/s), Y accel (km/s²), frequency channel number
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.yn_km),
        to_d19_12(msg.yn_first_deriv_km_s),
        to_d19_12(msg.yn_second_deriv_km_s2 as f64),
        to_d19_12(msg.glo_satellite_freq_chan_number as f64),
    )?;
    // Orbit 3: Z position (km), Z velocity (km/s), Z accel (km/s²), age of oper info
    writeln!(
        w,
        "   {}{}{}{}",
        to_d19_12(msg.zn_km),
        to_d19_12(msg.zn_first_deriv_km_s),
        to_d19_12(msg.zn_second_deriv_km_s2 as f64),
        to_d19_12(msg.en_d as f64),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Nav writer struct
// ---------------------------------------------------------------------------

/// RINEX 2.11 navigation file writer with hourly rotation.
///
/// Opens a new file at each UTC hour boundary. File naming follows RINEX 2.11 convention:
/// `{station:4}{doy:03}{session}{yy:02}.{yy}P` where session is "0".
pub struct RinexNavWriter {
    current_hour: u32,
    writer: Option<BufWriter<std::fs::File>>,
    output_dir: PathBuf,
    station: String,
    gps_week: u32,
}

impl RinexNavWriter {
    /// Create a new nav writer. No file is opened until the first ephemeris is written.
    pub fn new(output_dir: impl Into<PathBuf>, station: String, gps_week: u32) -> Self {
        Self {
            current_hour: u32::MAX,
            writer: None,
            output_dir: output_dir.into(),
            station,
            gps_week,
        }
    }

    /// Write an ephemeris message. Rotates to a new file when the UTC hour changes.
    pub fn write_ephemeris(
        &mut self,
        epoch_utc: &DateTime<Utc>,
        eph: &EphemerisMsg,
    ) -> anyhow::Result<()> {
        let hour = epoch_utc.hour();
        if hour != self.current_hour {
            if let Some(w) = self.writer.take() {
                w.into_inner()
                    .map_err(|e| anyhow::anyhow!("nav flush error: {}", e.error()))?;
            }
            let filename = self.make_filename(epoch_utc);
            let file = std::fs::File::create(&filename)?;
            let mut bw = BufWriter::new(file);
            write_nav_header(&mut bw, epoch_utc)?;
            self.writer = Some(bw);
            self.current_hour = hour;
        }

        if let Some(w) = &mut self.writer {
            match eph {
                EphemerisMsg::Gps(msg) => write_gps_nav(w, epoch_utc, msg)?,
                EphemerisMsg::Glonass(msg) => write_glo_nav(w, epoch_utc, msg)?,
                EphemerisMsg::Galileo(_) => {
                    log::warn!("Galileo ephemeris received but nav writer not implemented — skipping");
                }
                EphemerisMsg::Beidou(_) => {
                    log::warn!("BeiDou ephemeris received but nav writer not implemented — skipping");
                }
            }
        }
        Ok(())
    }

    /// Build the RINEX 2.11 navigation filename for the given epoch.
    ///
    /// Format: `{ssss}{ddd}{f}{yy}.{yy}P`
    fn make_filename(&self, epoch_utc: &DateTime<Utc>) -> PathBuf {
        let station = format!("{:4}", self.station);
        let station = &station[..4.min(station.len())];
        let doy = epoch_utc.ordinal();
        let yy = epoch_utc.year() % 100;
        let name = format!("{}{:03}0{:02}.{:02}P", station, doy, yy, yy);
        self.output_dir.join(name)
    }
}

/// RINEX 2.11 observation file writer with hourly rotation.
///
/// Opens a new file at each UTC hour boundary. File naming follows the RINEX 2.11 convention:
/// `{station:4}{doy:03}{session}{yy:02}.{yy}O`
/// where session is "0" (full-hour session code).
pub struct RinexObsWriter {
    current_hour: u32,
    writer: Option<BufWriter<std::fs::File>>,
    output_dir: PathBuf,
    station: String,
    gps_week: u32,
}

impl RinexObsWriter {
    /// Create a new writer. No file is opened until the first epoch is written.
    pub fn new(output_dir: impl Into<PathBuf>, station: String, gps_week: u32) -> Self {
        Self {
            current_hour: u32::MAX, // sentinel — no file open yet
            writer: None,
            output_dir: output_dir.into(),
            station,
            gps_week,
        }
    }

    /// Write an epoch group. Rotates to a new file when the UTC hour changes.
    pub fn write_group(
        &mut self,
        epoch_utc: &DateTime<Utc>,
        group: &EpochGroup,
    ) -> anyhow::Result<()> {
        let hour = epoch_utc.hour();
        if hour != self.current_hour {
            // Flush and close old file (if any)
            if let Some(w) = self.writer.take() {
                w.into_inner()
                    .map_err(|e| anyhow::anyhow!("flush error: {}", e.error()))?;
            }
            // Open new file for this hour
            let filename = self.make_filename(epoch_utc);
            let file = std::fs::File::create(&filename)?;
            let mut bw = BufWriter::new(file);
            write_obs_header(&mut bw, &self.station, (0.0, 0.0, 0.0), epoch_utc)?;
            self.writer = Some(bw);
            self.current_hour = hour;
        }

        if let Some(w) = &mut self.writer {
            write_epoch(w, epoch_utc, group)?;
        }
        Ok(())
    }

    /// Build the RINEX 2.11 observation filename for the given epoch.
    ///
    /// Format: `{ssss}{ddd}{f}{yy}.{yy}O`
    fn make_filename(&self, epoch_utc: &DateTime<Utc>) -> PathBuf {
        let station = format!("{:4}", self.station);
        let station = &station[..4.min(station.len())];
        let doy = epoch_utc.ordinal(); // day of year 1..=366
        let yy = epoch_utc.year() % 100;
        let name = format!("{}{:03}0{:02}.{:02}O", station, doy, yy, yy);
        self.output_dir.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{Constellation, EpochGroup, Observation};
    use chrono::TimeZone;

    // --- to_d19_12 tests ---

    #[test]
    fn d19_12_zero_produces_exact_string() {
        let s = to_d19_12(0.0);
        assert_eq!(s.len(), 19, "to_d19_12(0.0) must be 19 chars, got {:?}", s);
        assert_eq!(s, " 0.000000000000D+00", "to_d19_12(0.0) must equal ' 0.000000000000D+00'");
    }

    #[test]
    fn d19_12_small_negative_exponent_two_digits() {
        let s = to_d19_12(1.234567890123e-4);
        assert_eq!(s.len(), 19, "to_d19_12 must always be 19 chars, got {:?}", s);
        assert!(
            s.contains("D-04"),
            "to_d19_12(1.234...e-4) must contain 'D-04' (two-digit exponent), got {:?}",
            s
        );
    }

    #[test]
    fn d19_12_negative_value_positive_exponent() {
        let s = to_d19_12(-1.0e10);
        assert_eq!(s.len(), 19, "to_d19_12 must always be 19 chars, got {:?}", s);
        assert!(
            s.contains("D+10"),
            "to_d19_12(-1.0e10) must contain 'D+10', got {:?}",
            s
        );
    }

    #[test]
    fn d19_12_always_19_chars_various() {
        // GPS/GLONASS nav data uses exponents in the range -99..+99 at most.
        // f64::MIN_POSITIVE (exponent -308) is out of scope for RINEX nav values.
        for val in &[0.0f64, 1.0, -1.0, 1e-10, -1e-10, 1e10, -1e10, 1.23456789e-4, 1.23456789e-99, 1e99] {
            let s = to_d19_12(*val);
            assert_eq!(s.len(), 19, "to_d19_12({}) must be 19 chars, got {:?}", val, s);
        }
    }

    // --- gps_tow_to_utc test ---

    #[test]
    fn gps_tow_to_utc_known_epoch() {
        // GPS epoch = 1980-01-06T00:00:00Z (week 0, tow 0)
        // GPS week 2000, tow 0: 2000 weeks after 1980-01-06 = 2058-04-07 (approx)
        // Simple test: week=0, tow=0 + 18 leap seconds offset = 1979-12-31T23:59:42Z
        // tow_ms=0 at week=0: gps_epoch + 0ms - 18s = 1980-01-05T23:59:42Z
        let utc = gps_tow_to_utc(0, 0);
        // The GPS epoch is 1980-01-06 and we subtract 18 leap seconds
        assert_eq!(utc.year(), 1980);
        assert_eq!(utc.month(), 1);
        assert_eq!(utc.day(), 5); // 1 day earlier due to 18s correction wrapping
        // Test known week: GPS week 1 = 7 days after gps epoch = 1980-01-13
        // tow=0 on week 1 = 1980-01-13T00:00:00Z - 18s = 1980-01-12T23:59:42Z
        let utc_w1 = gps_tow_to_utc(1, 0);
        assert_eq!(utc_w1.day(), 12);
    }

    // --- write_gps_nav_header test ---

    #[test]
    fn nav_header_navigation_data_label_at_col_61() {
        let mut buf = Vec::new();
        {
            let mut bw = BufWriter::new(&mut buf);
            let utc = Utc.with_ymd_and_hms(2026, 3, 12, 0, 0, 0).unwrap();
            write_nav_header(&mut bw, &utc).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        let first_line = text.lines().next().unwrap();
        assert_eq!(first_line.len(), 80, "Nav header first line must be 80 chars");
        let label_area = &first_line[60..];
        assert!(
            label_area.starts_with("RINEX VERSION / TYPE"),
            "Expected 'RINEX VERSION / TYPE' at col 61 in nav header, got: {:?}",
            label_area
        );
        // Also check "NAVIGATION DATA" is present in data area
        let data_area = &first_line[..60];
        assert!(
            data_area.contains("NAVIGATION DATA"),
            "Nav header first line data area must contain 'NAVIGATION DATA', got: {:?}",
            data_area
        );
    }

    fn make_obs(c: Constellation, sv_id: u8, pr_ms: Option<f64>, phase_ms: Option<f64>, cnr: Option<f64>, epoch_ms: u32) -> Observation {
        Observation {
            constellation: c,
            sv_id,
            signal_id: 1,
            pseudorange_ms: pr_ms,
            rough_range_ms: pr_ms.unwrap_or(0.0),
            carrier_phase_ms: phase_ms,
            cnr_dbhz: cnr,
            epoch_ms,
        }
    }

    fn make_epoch_group(observations: Vec<Observation>) -> EpochGroup {
        let gps_count = observations.iter().filter(|o| o.constellation == Constellation::Gps).count();
        let glo_count = observations.iter().filter(|o| o.constellation == Constellation::Glonass).count();
        let gal_count = observations.iter().filter(|o| o.constellation == Constellation::Galileo).count();
        let bds_count = observations.iter().filter(|o| o.constellation == Constellation::BeiDou).count();
        let epoch_ms = observations.first().map(|o| o.epoch_ms).unwrap_or(0);
        EpochGroup {
            epoch_ms,
            observations,
            gps_count,
            glo_count,
            gal_count,
            bds_count,
        }
    }

    #[test]
    fn obs_header_lines_are_80_chars() {
        let utc = Utc.with_ymd_and_hms(2026, 3, 12, 0, 0, 0).unwrap();
        let mut buf = Vec::new();
        {
            let mut bw = BufWriter::new(&mut buf);
            write_obs_header(&mut bw, "GNSS-FFFEB5", (0.0, 0.0, 0.0), &utc).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        for (i, line) in text.lines().enumerate() {
            assert_eq!(
                line.len(),
                80,
                "Header line {} has {} chars, expected 80: {:?}",
                i + 1,
                line.len(),
                line
            );
        }
    }

    #[test]
    fn obs_header_label_at_col_61() {
        let utc = Utc.with_ymd_and_hms(2026, 3, 12, 0, 0, 0).unwrap();
        let mut buf = Vec::new();
        {
            let mut bw = BufWriter::new(&mut buf);
            write_obs_header(&mut bw, "GNSS", (0.0, 0.0, 0.0), &utc).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        let first_line = text.lines().next().unwrap();
        // "RINEX VERSION / TYPE" starts at column 61 (0-indexed: chars[60..])
        let label_area = &first_line[60..];
        assert!(
            label_area.starts_with("RINEX VERSION / TYPE"),
            "Expected 'RINEX VERSION / TYPE' at col 61, got: {:?}",
            label_area
        );
    }

    #[test]
    fn write_obs_some_produces_16_chars() {
        let mut buf = Vec::new();
        write_obs(&mut buf, Some(23514789.123), 0, 7).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.len(), 16, "write_obs(Some) must produce 16 chars, got {:?}", s);
        // RINEX 2.11 obs field: F14.3 (14 chars) + LLI I1 (1 char) + SSI I1 (1 char) = 16 chars
        // With lli=0, ssi=7: "  23514789.123" + "0" + "7" = "  23514789.12307"
        assert_eq!(s, "  23514789.12307", "Expected RINEX F14.3+LLI+SSI = '  23514789.12307'");
    }

    #[test]
    fn write_obs_none_produces_16_spaces() {
        let mut buf = Vec::new();
        write_obs(&mut buf, None, 0, 0).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.len(), 16, "write_obs(None) must produce exactly 16 spaces");
        assert_eq!(s, "                ", "Expected 16 spaces");
    }

    #[test]
    fn to_rinex_prn_formats() {
        assert_eq!(to_rinex_prn(Constellation::Gps, 5), "G05");
        assert_eq!(to_rinex_prn(Constellation::Glonass, 3), "R03");
        assert_eq!(to_rinex_prn(Constellation::Galileo, 7), "E07");
        assert_eq!(to_rinex_prn(Constellation::BeiDou, 12), "C12");
    }

    #[test]
    fn cnr_to_ssi_values() {
        assert_eq!(cnr_to_ssi(None), 0, "None CNR should give SSI=0");
        // cnr=45.0: (45-15)/6 + 1.5 = 5.0 + 1.5 = 6.5 → round → 7
        // Wait, let's compute: (45.0 - 15.0) / 6.0 + 1.5 = 30.0/6.0 + 1.5 = 5.0 + 1.5 = 6.5 → round = 7
        // But the plan says "clamp((45-15)/6+1.5, 1, 9) as u8 = 6" — let me recheck
        // Actually round(6.5) in Rust f64::round = 7.0 (rounds half away from zero)
        // The plan comment may be wrong; verify the formula: (45-15)/6 + 1.5 = 6.5 → round = 7
        let ssi_45 = cnr_to_ssi(Some(45.0));
        assert_eq!(ssi_45, 7, "cnr=45 should give SSI=7 (6.5 rounds to 7)");
        // Verify clamp: cnr=5 → (5-15)/6 + 1.5 = -1.67 + 1.5 = -0.17 → round = 0 → clamp to 1
        assert_eq!(cnr_to_ssi(Some(5.0)), 1, "cnr=5 should clamp to 1");
        // cnr=80 → (80-15)/6 + 1.5 = 10.83 + 1.5 = 12.33 → round = 12 → clamp to 9
        assert_eq!(cnr_to_ssi(Some(80.0)), 9, "cnr=80 should clamp to 9");
    }

    #[test]
    fn epoch_gt12_sats_continuation_line() {
        // Build 13 GPS observations to trigger the continuation line
        let mut obs_vec = Vec::new();
        for sv in 1u8..=13 {
            obs_vec.push(make_obs(
                Constellation::Gps, sv,
                Some(78.5 + sv as f64 * 0.001),
                Some(0.001),
                Some(40.0),
                100_000,
            ));
        }
        let group = make_epoch_group(obs_vec);
        let epoch_utc = Utc.with_ymd_and_hms(2026, 3, 1, 4, 23, 11).unwrap();

        let mut buf = Vec::new();
        {
            let mut bw = BufWriter::new(&mut buf);
            write_epoch(&mut bw, &epoch_utc, &group).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();

        // Line 0: epoch header with 12 sat PRNs; line 1: continuation with sat 13
        // Continuation line starts with 32 spaces
        assert!(
            lines.len() >= 2,
            "Expected at least 2 lines for epoch header (epoch + continuation)"
        );
        let continuation = lines[1];
        assert!(
            continuation.starts_with("                                "),
            "Continuation line must start with 32 spaces, got: {:?}",
            continuation
        );
    }
}
