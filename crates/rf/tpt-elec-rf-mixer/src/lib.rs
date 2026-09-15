// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mixer models: conversion gain, image rejection, and third-order
//! intercept arithmetic (single stage and cascaded per Friis).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A frequency-converting mixer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mixer {
    /// Conversion gain [dB] (negative = conversion loss, typical passive).
    pub conversion_gain_db: f64,
    /// Noise figure [dB].
    pub noise_figure_db: f64,
    /// Input third-order intercept [dBm].
    pub iip3_dbm: f64,
    /// LO drive level [dBm].
    pub lo_power_dbm: f64,
}

/// Image rejection result for an I/Q (image-reject) mixer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRejection {
    /// Rejection [dBc].
    pub rejection_dbc: f64,
    /// Whether it meets a typical 30 dB spec.
    pub meets_30db: bool,
}

impl Mixer {
    /// A typical double-balanced passive mixer.
    pub fn typical_passive(lo_power_dbm: f64) -> Self {
        Self {
            conversion_gain_db: -7.0,
            noise_figure_db: 7.5,
            iip3_dbm: 22.0,
            lo_power_dbm,
        }
    }

    /// Output power [dBm] for an RF input at `p_in_dbm` (linear conversion).
    pub fn output_power_dbm(&self, p_in_dbm: f64) -> f64 {
        p_in_dbm + self.conversion_gain_db
    }

    /// Output third-order intercept [dBm]: OIP3 = IIP3 + G.
    pub fn oip3_dbm(&self) -> f64 {
        self.iip3_dbm + self.conversion_gain_db
    }

    /// Third-order intermod power [dBm] for two equal tones at `p_tone_dbm`.
    ///
    /// IM3 (per tone) = 3·P_tone − 2·IIP3.
    pub fn im3_power_dbm(&self, p_tone_dbm: f64) -> f64 {
        3.0 * p_tone_dbm - 2.0 * self.iip3_dbm
    }

    /// Image rejection [dBc] for an I/Q mixer with gain imbalance
    /// `gain_error_db` and phase imbalance `phase_error_deg`.
    ///
    /// IRR = 10·log10[(1 + ε² + 2ε·cosφ)/(1 + ε² − 2ε·cosφ)] with
    /// ε = amplitude ratio (from the dB error).
    pub fn image_rejection(gain_error_db: f64, phase_error_deg: f64) -> ImageRejection {
        let eps = 10f64.powf(gain_error_db / 20.0);
        let phi = phase_error_deg.to_radians();
        let e2 = eps * eps;
        let num = 1.0 + e2 + 2.0 * eps * phi.cos();
        let den = (1.0 + e2 - 2.0 * eps * phi.cos()).max(1e-12);
        let rejection = 10.0 * (num / den).log10();
        ImageRejection {
            rejection_dbc: rejection,
            meets_30db: rejection >= 30.0,
        }
    }
}

/// Friis cascade of a multi-stage chain.
pub struct CascadeCalculator;

impl CascadeCalculator {
    /// Cascaded gain [dB]: sum of stage gains.
    pub fn cascade_gain_db(gains_db: &[f64]) -> f64 {
        gains_db.iter().sum()
    }

    /// Cascaded noise figure [dB] (Friis): F = F1 + (F2−1)/G1 + …
    /// Gains are linear ratios.
    pub fn cascade_nf_db(noise_figures_db: &[f64], gains_db: &[f64]) -> f64 {
        assert!(!noise_figures_db.is_empty());
        let linear = |db: f64| 10f64.powf(db / 10.0);
        let mut f_total = linear(noise_figures_db[0]);
        let mut g_prod = 1.0;
        for i in 1..noise_figures_db.len() {
            g_prod *= linear(gains_db[i - 1]);
            f_total += (linear(noise_figures_db[i]) - 1.0) / g_prod;
        }
        10.0 * f_total.log10()
    }

    /// Cascaded IIP3 [dBm]: 1/IIP3_total = 1/IIP3_1 + G1/IIP3_2 + …
    pub fn cascade_iip3_dbm(iip3_dbm: &[f64], gains_db: &[f64]) -> f64 {
        assert!(!iip3_dbm.is_empty());
        let linear = |db: f64| 10f64.powf(db / 10.0);
        let mut inv = 1.0 / linear(iip3_dbm[0]);
        let mut g_prod = 1.0;
        for i in 1..iip3_dbm.len() {
            g_prod *= linear(gains_db[i - 1]);
            inv += g_prod / linear(iip3_dbm[i]);
        }
        10.0 * (1.0 / inv).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_and_intercept_arithmetic() {
        let m = Mixer::typical_passive(7.0);
        assert!((m.output_power_dbm(0.0) + 7.0).abs() < 1e-9);
        // OIP3 = 22 − 7 = 15 dBm
        assert!((m.oip3_dbm() - 15.0).abs() < 1e-9);
        // Two tones at −5 dBm: IM3 = 3·(−5) − 2·22 = −59 dBm
        assert!((m.im3_power_dbm(-5.0) + 59.0).abs() < 1e-9);
    }

    #[test]
    fn perfect_iq_rejects_image_infinite() {
        let ir = Mixer::image_rejection(0.0, 0.0);
        assert!(ir.rejection_dbc > 100.0);
        assert!(ir.meets_30db);
    }

    #[test]
    fn phase_error_limits_image_rejection() {
        // Small-angle limit: IRR ≈ 20·log10(2/φ_rad) → 0.5° ≈ 47 dB
        let ir = Mixer::image_rejection(0.0, 0.5);
        assert!(
            (ir.rejection_dbc - 47.2).abs() < 1.5,
            "ir {}",
            ir.rejection_dbc
        );
        // 1° phase error → ~41 dB
        let ir1 = Mixer::image_rejection(0.0, 1.0);
        assert!((ir1.rejection_dbc - 41.2).abs() < 1.5);
        // Combined 0.5 dB + 2° → ~26 dB
        let ir2 = Mixer::image_rejection(0.5, 2.0);
        assert!(
            (20.0..32.0).contains(&ir2.rejection_dbc),
            "ir {}",
            ir2.rejection_dbc
        );
        assert!(!ir2.meets_30db);
    }

    #[test]
    fn friis_first_stage_dominates_nf() {
        // LNA 1 dB NF, 20 dB gain, followed by a noisy 10 dB NF mixer.
        let nf = CascadeCalculator::cascade_nf_db(&[1.0, 10.0], &[20.0]);
        // F = 1.2589 + (10 − 1)/100 = 1.3489 → 1.30 dB
        assert!((nf - 1.30).abs() < 0.03, "nf {nf}");
        // Without the LNA gain, the mixer NF would dominate
        let nf2 = CascadeCalculator::cascade_nf_db(&[1.0, 10.0], &[2.0]);
        assert!(nf2 > nf);
    }

    #[test]
    fn cascade_iip3_last_stage_dominates() {
        // High-gain LNA before the mixer: mixer IIP3 dominates.
        // 1/IIP3 = 1/1000 mW + 100/10 mW → IIP3 = −10.0 dBm.
        let iip3 = CascadeCalculator::cascade_iip3_dbm(&[30.0, 10.0], &[20.0]);
        assert!((iip3 + 10.0).abs() < 0.1, "iip3 {iip3}");
        assert!(iip3 < 0.0);
    }

    #[test]
    fn cascade_gain_sums() {
        assert!((CascadeCalculator::cascade_gain_db(&[-1.0, 20.0, -7.0]) - 12.0).abs() < 1e-12);
    }
}
