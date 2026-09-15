// SPDX-License-Identifier: MIT OR Apache-2.0

//! EMC standards, test types, and limit lines.
//!
//! [`EmcLimit::for_standard`] returns the classic CISPR 32 / FCC Part 15
//! radiated-emissions limit tables at 3 m (values from the published
//! standard tables; informative for simulation — normative testing is
//! always done in a calibrated lab).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Regulatory standard.
#[derive(Clone, Debug, PartialEq)]
pub enum EmcStandard {
    /// CISPR 32, Class A (non-residential).
    Cispr32ClassA,
    /// CISPR 32, Class B (residential).
    Cispr32ClassB,
    /// FCC Part 15 Subpart B, Class A.
    FccPart15ClassA,
    /// FCC Part 15 Subpart B, Class B.
    FccPart15ClassB,
    /// MIL-STD-461 (requirement designation, e.g. "RE102").
    MilStd461 {
        /// Requirement designation.
        requirement: String,
    },
    /// DO-160 (avionics).
    Do160 {
        /// Section.
        section: String,
    },
    /// IEC 61000 family.
    Iec61000 {
        /// Part (e.g. "4-3").
        part: String,
    },
    /// Automotive (CISPR 25 / ISO 11452).
    Automotive {
        /// Standard name.
        standard: String,
    },
}

/// EMC test category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmcTestType {
    /// Radiated emissions.
    RadiatedEmissions,
    /// Conducted emissions (mains port).
    ConductedEmissions,
    /// Radiated immunity.
    RadiatedImmunity,
    /// Conducted immunity.
    ConductedImmunity,
    /// ESD (IEC 61000-4-2).
    Esd,
    /// Surge (IEC 61000-4-5).
    Surge,
    /// Electrical fast transient (IEC 61000-4-4).
    ElectricalFastTransient,
}

/// One point of a limit line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitPoint {
    /// Frequency [Hz].
    pub frequency: f64,
    /// Field strength [dBµV/m] (radiated) or voltage [dBµV] (conducted).
    pub limit_dbuv: f64,
    /// Detector: quasi-peak or average.
    pub detector: Detector,
}

/// Detector type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detector {
    /// Quasi-peak.
    QuasiPeak,
    /// Average.
    Average,
}

/// A limit line over a frequency range.
#[derive(Clone, Debug)]
pub struct EmcLimit {
    /// Standard the limit derives from.
    pub standard: EmcStandard,
    /// Test type.
    pub test_type: EmcTestType,
    /// Measurement distance [m] (radiated).
    pub distance_m: f64,
    /// Piecewise limit line.
    pub limit_line: Vec<LimitPoint>,
}

impl EmcLimit {
    /// CISPR 32 radiated-emissions limit line at 3 m.
    ///
    /// Class B (residential): 40 dBµV/m QP up to 230 MHz, 47 dBµV/m above.
    /// Class A: 50 / 57 dBµV/m.
    pub fn for_standard(standard: EmcStandard, test_type: EmcTestType) -> Option<Self> {
        let line = match (&standard, test_type) {
            (
                EmcStandard::Cispr32ClassB | EmcStandard::FccPart15ClassB,
                EmcTestType::RadiatedEmissions,
            ) => vec![
                LimitPoint {
                    frequency: 30e6,
                    limit_dbuv: 40.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 230e6,
                    limit_dbuv: 40.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 230.001e6,
                    limit_dbuv: 47.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 1e9,
                    limit_dbuv: 47.0,
                    detector: Detector::QuasiPeak,
                },
            ],
            (
                EmcStandard::Cispr32ClassA | EmcStandard::FccPart15ClassA,
                EmcTestType::RadiatedEmissions,
            ) => vec![
                LimitPoint {
                    frequency: 30e6,
                    limit_dbuv: 50.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 230e6,
                    limit_dbuv: 50.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 230.001e6,
                    limit_dbuv: 57.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 1e9,
                    limit_dbuv: 57.0,
                    detector: Detector::QuasiPeak,
                },
            ],
            (
                EmcStandard::Cispr32ClassB | EmcStandard::FccPart15ClassB,
                EmcTestType::ConductedEmissions,
            ) => vec![
                LimitPoint {
                    frequency: 150e3,
                    limit_dbuv: 66.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 500e3,
                    limit_dbuv: 56.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 5e6,
                    limit_dbuv: 56.0,
                    detector: Detector::QuasiPeak,
                },
                LimitPoint {
                    frequency: 30e6,
                    limit_dbuv: 60.0,
                    detector: Detector::QuasiPeak,
                },
            ],
            _ => return None,
        };
        Some(Self {
            standard,
            test_type,
            distance_m: 3.0,
            limit_line: line,
        })
    }

    /// Interpolated limit [dBµV/m] at a frequency (clamps at band edges).
    pub fn limit_at(&self, frequency: f64) -> f64 {
        let line = &self.limit_line;
        if frequency <= line[0].frequency {
            return line[0].limit_dbuv;
        }
        for w in line.windows(2) {
            if frequency <= w[1].frequency {
                let f = (frequency - w[0].frequency) / (w[1].frequency - w[0].frequency).max(1e-9);
                return w[0].limit_dbuv + f * (w[1].limit_dbuv - w[0].limit_dbuv);
            }
        }
        line[line.len() - 1].limit_dbuv
    }

    /// Margin [dB] of a measured level against the limit (positive = pass).
    pub fn margin_at(&self, frequency: f64, measured_dbuv: f64) -> f64 {
        self.limit_at(frequency) - measured_dbuv
    }
}

/// IEC 61000-4 immunity test levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImmunityLevel {
    /// Test severity level index (1..4).
    pub level: u8,
    /// Field strength [V/m] (radiated), voltage [V] (conducted/ESD), or
    /// current [A].
    pub severity: f64,
}

impl ImmunityLevel {
    /// IEC 61000-4-3 radiated immunity severity [V/m] (level 3 = 10 V/m).
    pub fn radiated(level: u8) -> Self {
        let severity = match level {
            1 => 1.0,
            2 => 3.0,
            3 => 10.0,
            4 => 30.0,
            _ => 10.0,
        };
        Self { level, severity }
    }

    /// IEC 61000-4-2 ESD severity [kV].
    pub fn esd(level: u8) -> Self {
        let severity = match level {
            1 => 2.0,
            2 => 4.0,
            3 => 8.0,
            4 => 15.0,
            _ => 8.0,
        };
        Self { level, severity }
    }

    /// IEC 61000-4-5 surge severity [kV].
    pub fn surge(level: u8) -> Self {
        let severity = match level {
            1 => 0.5,
            2 => 1.0,
            3 => 2.0,
            4 => 4.0,
            _ => 2.0,
        };
        Self { level, severity }
    }

    /// IEC 61000-4-4 EFT burst severity [kV].
    pub fn eft(level: u8) -> Self {
        let severity = match level {
            1 => 0.5,
            2 => 1.0,
            3 => 2.0,
            4 => 4.0,
            _ => 2.0,
        };
        Self { level, severity }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cispr32_class_b_radiated_limit() {
        let limit =
            EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::RadiatedEmissions)
                .unwrap();
        assert!((limit.limit_at(88e6) - 40.0).abs() < 1e-9);
        assert!((limit.limit_at(300e6) - 47.0).abs() < 1e-9);
        let at230 = limit.limit_at(229.9e6);
        assert!((39.8..40.1).contains(&at230));
    }

    #[test]
    fn class_a_is_laxer_than_class_b() {
        let a = EmcLimit::for_standard(EmcStandard::Cispr32ClassA, EmcTestType::RadiatedEmissions)
            .unwrap();
        let b = EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::RadiatedEmissions)
            .unwrap();
        assert!(a.limit_at(100e6) > b.limit_at(100e6));
    }

    #[test]
    fn conducted_limits_exist() {
        let c = EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::ConductedEmissions)
            .unwrap();
        assert!((c.limit_at(200e3) - 64.6).abs() < 0.2); // interpolated 66→56 dBµV band
    }

    #[test]
    fn margin_arithmetic() {
        let limit =
            EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::RadiatedEmissions)
                .unwrap();
        assert!((limit.margin_at(88e6, 34.0) - 6.0).abs() < 1e-9);
        assert!(limit.margin_at(88e6, 45.0) < 0.0);
    }

    #[test]
    fn immunity_severity_tables() {
        assert!((ImmunityLevel::radiated(3).severity - 10.0).abs() < 1e-9);
        assert!((ImmunityLevel::esd(2).severity - 4.0).abs() < 1e-9);
        assert!((ImmunityLevel::surge(3).severity - 2.0).abs() < 1e-9);
        assert!((ImmunityLevel::eft(4).severity - 4.0).abs() < 1e-9);
    }

    #[test]
    fn unsupported_combinations_return_none() {
        assert!(EmcLimit::for_standard(
            EmcStandard::MilStd461 {
                requirement: "RE102".into()
            },
            EmcTestType::RadiatedEmissions
        )
        .is_none());
        assert!(EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::Esd).is_none());
    }
}
