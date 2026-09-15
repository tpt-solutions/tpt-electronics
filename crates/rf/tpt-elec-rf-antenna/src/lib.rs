// SPDX-License-Identifier: MIT OR Apache-2.0

//! Antenna models, radiation patterns, and link budgets.
//!
//! [`Antenna`] carries pattern data generated for the classic wire and
//! patch families ([`AntennaType`]); [`LinkBudget::free_space_path_loss`]
//! implements the Friis FSPL in the convenient km/MHz logarithmic form.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;

/// Antenna families.
#[derive(Clone, Debug, PartialEq)]
pub enum AntennaType {
    /// Half-wave dipole.
    Dipole {
        /// Physical length [m].
        length: Length,
    },
    /// Monopole over ground.
    Monopole {
        /// Physical length [m].
        length: Length,
        /// Whether an infinite ground plane is modeled.
        ground_plane: bool,
    },
    /// Rectangular patch.
    Patch {
        /// Patch width [m].
        width: Length,
        /// Patch height [m].
        height: Length,
        /// Substrate name (informational).
        substrate: String,
    },
    /// Yagi-Uda with element list.
    Yagi {
        /// Element lengths [m] (first = reflector, then driven, directors).
        elements: Vec<Length>,
        /// Boom spacing between elements [m].
        spacing: Length,
    },
    /// Helical.
    Helical {
        /// Diameter [m].
        diameter: Length,
        /// Pitch [m].
        pitch: Length,
        /// Turn count.
        turns: u32,
    },
    /// Isotropic reference (for testing).
    Isotropic,
}

/// A radiation pattern over a theta/phi grid (gain, dBi).
#[derive(Clone, Debug)]
pub struct RadiationPattern {
    /// Theta samples [rad] (polar angle).
    pub theta: Vec<f64>,
    /// Phi samples [rad] (azimuth).
    pub phi: Vec<f64>,
    /// Gain [dBi] indexed [phi][theta].
    pub gain: Vec<Vec<f64>>,
}

impl RadiationPattern {
    /// Isotropic pattern.
    pub fn isotropic() -> Self {
        Self {
            theta: vec![0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI],
            phi: vec![0.0],
            gain: vec![vec![0.0; 3]],
        }
    }

    /// Peak gain [dBi].
    pub fn peak_gain_dbi(&self) -> f64 {
        self.gain
            .iter()
            .flat_map(|row| row.iter())
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Half-power beamwidth estimate from the elevation cut at phi = 0 [°].
    pub fn beamwidth_deg(&self) -> f64 {
        let peak = self.peak_gain_dbi();
        let cut = match self.gain.first() {
            Some(c) => c,
            None => return 360.0,
        };
        let n = cut.len();
        if n < 3 {
            return 360.0;
        }
        // −3 dB crossings around the peak
        let peak_i = (0..n)
            .max_by(|a, b| cut[*a].total_cmp(&cut[*b]))
            .unwrap_or(0);
        let thr = peak - 3.0;
        let mut lo = peak_i;
        while lo > 0 && cut[lo] > thr {
            lo -= 1;
        }
        let mut hi = peak_i;
        while hi + 1 < n && cut[hi] > thr {
            hi += 1;
        }
        let dtheta = std::f64::consts::PI / (n - 1).max(1) as f64;
        ((hi - lo) as f64 * dtheta).to_degrees()
    }
}

/// An antenna with derived numbers.
#[derive(Clone, Debug)]
pub struct Antenna {
    /// Family.
    pub antenna_type: AntennaType,
    /// Design frequency [Hz].
    pub frequency: f64,
    /// Realized gain [dBi].
    pub gain_dbi: f64,
    /// Match VSWR.
    pub vswr: f64,
    /// Pattern.
    pub radiation_pattern: RadiationPattern,
}

impl Antenna {
    /// Analytical reference antenna of the given family at `frequency`.
    pub fn design(antenna_type: AntennaType, frequency: f64) -> Self {
        let lambda = tpt_elec_core::SPEED_OF_LIGHT / frequency;
        let (gain_dbi, pattern) = match &antenna_type {
            AntennaType::Dipole { .. } => {
                // Half-wave dipole: 2.15 dBi, figure-8 elevation cut.
                let pattern = dipole_pattern();
                (2.15, pattern)
            }
            AntennaType::Monopole { ground_plane, .. } => {
                if *ground_plane {
                    (5.15, dipole_pattern_upper()) // 3 dB more than a dipole
                } else {
                    (2.15, dipole_pattern())
                }
            }
            AntennaType::Patch { .. } => {
                // ≈ 7 dBi with cos^n roll-off
                (7.0, patch_pattern())
            }
            AntennaType::Yagi { elements, .. } => {
                // ~6–15 dBi by element count: 7.5 + 2·log10(directors)… simple model
                let directors = elements.len().saturating_sub(2);
                (
                    7.5 + 2.2 * (directors as f64 + 1.0).log10(),
                    patch_pattern(),
                )
            }
            AntennaType::Helical { turns, .. } => {
                // Axial mode: G ≈ 10·log10(15·N·Cλ²) with Cλ ≈ 1
                (10.0 * (15.0 * *turns as f64).log10() - 2.0, patch_pattern())
            }
            AntennaType::Isotropic => (0.0, RadiationPattern::isotropic()),
        };
        let _ = lambda;
        Self {
            antenna_type,
            frequency,
            gain_dbi,
            vswr: 1.5,
            radiation_pattern: pattern,
        }
    }

    /// Effective aperture [m²]: A = λ²·G/(4π).
    pub fn effective_aperture(&self) -> f64 {
        let lambda = tpt_elec_core::SPEED_OF_LIGHT / self.frequency;
        lambda * lambda * 10f64.powf(self.gain_dbi / 10.0) / (4.0 * std::f64::consts::PI)
    }
}

fn dipole_pattern() -> RadiationPattern {
    let n = 91;
    let theta: Vec<f64> = (0..n)
        .map(|i| std::f64::consts::PI * i as f64 / (n - 1) as f64)
        .collect();
    // Half-wave dipole field pattern
    let gain: Vec<f64> = theta
        .iter()
        .map(|&t| {
            let f = (std::f64::consts::FRAC_PI_2 * t.cos()).cos() / t.sin().max(1e-9);
            10.0 * (f * f).log10() + 2.15
        })
        .collect();
    RadiationPattern {
        theta,
        phi: vec![0.0],
        gain: vec![gain],
    }
}

fn dipole_pattern_upper() -> RadiationPattern {
    let mut p = dipole_pattern();
    for row in &mut p.gain {
        for g in row.iter_mut() {
            *g += 3.0;
        }
    }
    p
}

fn patch_pattern() -> RadiationPattern {
    let n = 91;
    let theta: Vec<f64> = (0..n)
        .map(|i| std::f64::consts::PI * i as f64 / (n - 1) as f64)
        .collect();
    // cos^8 roll-off (broadside patch approximation)
    let gain: Vec<f64> = theta
        .iter()
        .map(|&t| 7.0 + 20.0 * (t.cos().powf(16.0)).log10())
        .collect();
    RadiationPattern {
        theta,
        phi: vec![0.0],
        gain: vec![gain],
    }
}

/// RF link budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkBudget {
    /// Transmit power [dBm].
    pub tx_power_dbm: f64,
    /// Transmit antenna gain [dBi].
    pub tx_antenna_gain_dbi: f64,
    /// Path loss [dB].
    pub path_loss_db: f64,
    /// Receive antenna gain [dBi].
    pub rx_antenna_gain_dbi: f64,
    /// Receiver sensitivity [dBm].
    pub rx_sensitivity_dbm: f64,
    /// Link margin [dB] (EIRP − loss + Grx − sensitivity).
    pub margin_db: f64,
}

impl LinkBudget {
    /// Free-space path loss [dB]: `20·log10(d_km) + 20·log10(f_MHz) + 32.44`.
    pub fn free_space_path_loss(distance_m: f64, frequency_hz: f64) -> f64 {
        let d_km = distance_m / 1e3;
        let f_mhz = frequency_hz / 1e6;
        20.0 * d_km.max(1e-6).log10() + 20.0 * f_mhz.max(1e-6).log10() + 32.44
    }

    /// Builds a complete budget and computes the margin.
    pub fn evaluate(
        tx_power_dbm: f64,
        tx_gain_dbi: f64,
        rx_gain_dbi: f64,
        distance_m: f64,
        frequency_hz: f64,
        rx_sensitivity_dbm: f64,
    ) -> Self {
        let path_loss = Self::free_space_path_loss(distance_m, frequency_hz);
        let margin = tx_power_dbm + tx_gain_dbi - path_loss + rx_gain_dbi - rx_sensitivity_dbm;
        Self {
            tx_power_dbm,
            tx_antenna_gain_dbi: tx_gain_dbi,
            path_loss_db: path_loss,
            rx_antenna_gain_dbi: rx_gain_dbi,
            rx_sensitivity_dbm,
            margin_db: margin,
        }
    }

    /// Whether the link closes (margin ≥ 0).
    pub fn closes(&self) -> bool {
        self.margin_db >= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fspl_known_values() {
        // 2.4 GHz at 1 km: 100.05 dB
        let fspl = LinkBudget::free_space_path_loss(1000.0, 2.4e9);
        assert!((fspl - 100.05).abs() < 0.05, "fspl {fspl}");
        // 5.8 GHz at 10 m (Wi-Fi room): 67.7 dB
        let room = LinkBudget::free_space_path_loss(10.0, 5.8e9);
        assert!((room - 67.66).abs() < 0.1, "room {room}");
        // Doubled distance: +6 dB
        let a = LinkBudget::free_space_path_loss(100.0, 900e6);
        let b = LinkBudget::free_space_path_loss(200.0, 900e6);
        assert!((b - a - 6.0206).abs() < 1e-3);
    }

    #[test]
    fn wifi_link_budget() {
        // 20 dBm AP + 2 dBi antennas, 30 m at 2.4 GHz, −70 dBm sensitivity
        let lb = LinkBudget::evaluate(20.0, 2.0, 2.0, 30.0, 2.4e9, -70.0);
        assert!(lb.closes());
        assert!(
            (20.0..30.0).contains(&lb.margin_db),
            "margin {}",
            lb.margin_db
        );
    }

    #[test]
    fn dipole_antenna_reference() {
        let a = Antenna::design(
            AntennaType::Dipole {
                length: Length::mm(62.5), // ~λ/2 at 2.4 GHz
            },
            2.4e9,
        );
        assert!((a.gain_dbi - 2.15).abs() < 1e-9);
        // Broadside is the peak; end-fire is deeply notched
        assert!(a.radiation_pattern.peak_gain_dbi() >= 2.15 - 1e-9);
        assert!(a.radiation_pattern.beamwidth_deg() > 40.0);
        // Effective aperture sane: λ²G/4π ≈ (0.125²)·1.64/4π ≈ 2.03e-3 m²
        assert!((a.effective_aperture() - 2.0e-3).abs() / 2.0e-3 < 0.1);
    }

    #[test]
    fn patch_has_more_gain_than_dipole() {
        let patch = Antenna::design(
            AntennaType::Patch {
                width: Length::mm(30.0),
                height: Length::mm(30.0),
                substrate: "fr4".into(),
            },
            2.4e9,
        );
        assert!(patch.gain_dbi > 6.0);
    }

    #[test]
    fn yagi_gain_grows_with_directors() {
        let short = Antenna::design(
            AntennaType::Yagi {
                elements: vec![Length::mm(600.0), Length::mm(590.0), Length::mm(550.0)],
                spacing: Length::mm(80.0),
            },
            433e6,
        );
        let long = Antenna::design(
            AntennaType::Yagi {
                elements: vec![
                    Length::mm(600.0),
                    Length::mm(590.0),
                    Length::mm(550.0),
                    Length::mm(550.0),
                    Length::mm(550.0),
                    Length::mm(540.0),
                ],
                spacing: Length::mm(80.0),
            },
            433e6,
        );
        assert!(long.gain_dbi > short.gain_dbi);
    }
}
