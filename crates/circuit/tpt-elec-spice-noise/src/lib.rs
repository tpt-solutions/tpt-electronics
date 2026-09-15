// SPDX-License-Identifier: MIT OR Apache-2.0

//! Noise source models feeding `tpt-elec-spice-analysis` noise analysis.
//!
//! Three fundamental mechanisms, expressed as single-sided power spectral
//! densities [A²/Hz or V²/Hz]:
//!
//! * [`thermal_current_density`] / [`thermal_noise_voltage`] — Johnson–Nyquist
//! * [`shot_current_density`] — 2·q·I
//! * [`flicker_density`] — 1/f^`af` (Kf·I^af)

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_spice_models::{BOLTZMANN, ELEMENTARY_CHARGE};

/// Reference temperature used when none is supplied [K].
pub const DEFAULT_TEMPERATURE_K: f64 = 300.0;

/// Thermal (Johnson–Nyquist) current noise density of a resistor [A²/Hz]:
/// `4·k·T/R`.
pub fn thermal_current_density(resistance: f64, temperature_k: f64) -> f64 {
    4.0 * BOLTZMANN * temperature_k / resistance.max(1e-300)
}

/// Thermal voltage-noise density of a resistor [V²/Hz]: `4·k·T·R`.
pub fn thermal_noise_voltage(resistance: f64, temperature_k: f64) -> f64 {
    4.0 * BOLTZMANN * temperature_k * resistance
}

/// Shot-noise current density [A²/Hz]: `2·q·I` (forward-biased junctions).
pub fn shot_current_density(current_a: f64) -> f64 {
    2.0 * ELEMENTARY_CHARGE * current_a.max(0.0)
}

/// Flicker (1/f) noise parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlickerParams {
    /// Flicker coefficient `kf` [A²·Hz (device-specific)].
    pub kf: f64,
    /// Frequency exponent `af` (≈1).
    pub af: f64,
    /// Bias current exponent `ef` (≈2).
    pub ef: f64,
}

impl Default for FlickerParams {
    fn default() -> Self {
        Self {
            kf: 1.0e-25,
            af: 1.0,
            ef: 2.0,
        }
    }
}

/// Flicker noise current density [A²/Hz]: `kf·I^ef/f^af`.
pub fn flicker_density(params: &FlickerParams, bias_current: f64, frequency: f64) -> f64 {
    if frequency <= 0.0 {
        return 0.0;
    }
    params.kf * bias_current.max(0.0).powf(params.ef) / frequency.powf(params.af)
}

/// MOSFET channel thermal noise density [A²/Hz] `≈ (8/3)·k·T·gm`.
pub fn mosfet_channel_density(gm: f64, temperature_k: f64) -> f64 {
    (8.0 / 3.0) * BOLTZMANN * temperature_k * gm.max(0.0)
}

/// One spot-noise sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseSpot {
    /// Frequency [Hz].
    pub frequency: f64,
    /// Total density at this frequency [A²/Hz or V²/Hz].
    pub density: f64,
}

/// Contributions per mechanism for reporting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoiseBreakdown {
    /// Thermal contribution.
    pub thermal: f64,
    /// Shot contribution.
    pub shot: f64,
    /// Flicker contribution.
    pub flicker: f64,
}

impl NoiseBreakdown {
    /// Total density (contributions add in power).
    pub fn total(&self) -> f64 {
        self.thermal + self.shot + self.flicker
    }
}

/// Integrates a density (in-band samples) into an RMS value via the
/// trapezoidal rule.
pub fn integrate_band(spots: &[NoiseSpot]) -> f64 {
    if spots.len() < 2 {
        return 0.0;
    }
    let mut acc = 0.0;
    for w in spots.windows(2) {
        acc += 0.5 * (w[0].density + w[1].density) * (w[1].frequency - w[0].frequency);
    }
    acc.max(0.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_noise_of_1k() {
        // 4kTR at 300 K, R = 1 kΩ ≈ 1.656e-17 V²/Hz → ≈ 4.07 nV/√Hz
        let v2 = thermal_noise_voltage(1e3, 300.0);
        let nv_rms = v2.sqrt() * 1e9;
        assert!((nv_rms - 4.07).abs() < 0.1, "{nv_rms} nV/√Hz");
        // Norton/Thevenin pair: v² = i²·R²
        let i2 = thermal_current_density(1e3, 300.0);
        assert!((i2 * (1e3 * 1e3) - v2).abs() / v2 < 1e-12);
    }

    #[test]
    fn shot_noise_scales_with_current() {
        // 1 mA → 2·q·I ≈ 3.2e-22 A²/Hz
        let d = shot_current_density(1e-3);
        assert!((d - 2.0 * 1.602176634e-19 * 1e-3).abs() < 1e-30);
        assert_eq!(shot_current_density(-1.0), 0.0);
    }

    #[test]
    fn flicker_has_one_over_f_shape() {
        let p = FlickerParams::default();
        let at_1k = flicker_density(&p, 1e-3, 1e3);
        let at_10k = flicker_density(&p, 1e-3, 1e4);
        assert!((at_1k / at_10k - 10.0).abs() < 1e-9);
        assert_eq!(flicker_density(&p, 1e-3, 0.0), 0.0);
    }

    #[test]
    fn mosfet_channel_thermal() {
        let d = mosfet_channel_density(1e-3, 300.0);
        assert!(d > 0.0);
        assert!((d - 8.0 / 3.0 * 1.380649e-23 * 300.0 * 1e-3).abs() < 1e-30);
    }

    #[test]
    fn band_integration_white_noise() {
        // Flat 1e-20 A²/Hz from 1 kHz to 11 kHz → rms = 1e-10·√(1e4·1e-20)=…
        let spots: Vec<NoiseSpot> = (1..=10)
            .map(|k| NoiseSpot {
                frequency: k as f64 * 1e3,
                density: 1e-20,
            })
            .collect();
        let rms = integrate_band(&spots);
        // 9 trapezoid intervals of 1 kHz at 1e-20 V²/Hz... A²/Hz
        let expected = (9.0 * 1e3 * 1e-20_f64).sqrt();
        assert!((rms - expected).abs() / expected < 1e-9);
    }

    #[test]
    fn breakdown_sums_in_power() {
        let b = NoiseBreakdown {
            thermal: 1e-20,
            shot: 2e-20,
            flicker: 3e-20,
        };
        assert!((b.total() - 6e-20).abs() < 1e-32);
    }
}
