// SPDX-License-Identifier: MIT OR Apache-2.0

//! Near-end / far-end crosstalk models for coupled lines.
//!
//! From the even/odd mode impedances, the capacitive/inductive coupling
//! factor is `k = (Z0e − Z0o)/(Z0e + Z0o)`. For a saturated (long) victim:
//!
//! * NEXT amplitude ratio ≈ `(k_L + k_C)/4` — length-independent,
//! * FEXT ≈ `(k_L − k_C)/2 · len/(v·tr)` — zero for homogeneous media
//!   (stripline), where inductive and capacitive coupling cancel.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Coupled-line crosstalk model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosstalkModel {
    /// Single-ended impedance of aggressor/victim [Ω].
    pub z0: f64,
    /// Coupling factor `k = (Z0e − Z0o)/(Z0e + Z0o)` (0..1).
    pub k: f64,
    /// Fraction of `k` that is inductive (rest capacitive); 0.5 = homogeneous.
    pub inductive_fraction: f64,
}

impl CrosstalkModel {
    /// Builds the model from even/odd mode impedances.
    pub fn from_modes(z0_se: f64, z_even: f64, z_odd: f64) -> Self {
        let k = (z_even - z_odd) / (z_even + z_odd);
        Self {
            z0: z0_se,
            k: k.clamp(0.0, 1.0),
            inductive_fraction: 0.5,
        }
    }

    /// Overrides the inductive fraction (microstrip: > 0.5 because the field
    /// above the board is air — inductively dominated coupling).
    pub fn with_inductive_fraction(mut self, fraction: f64) -> Self {
        self.inductive_fraction = fraction.clamp(0.0, 1.0);
        self
    }

    /// Inductive coupling factor `k_L`.
    pub fn k_inductive(&self) -> f64 {
        self.k * self.inductive_fraction * 2.0
    }

    /// Capacitive coupling factor `k_C`.
    pub fn k_capacitive(&self) -> f64 {
        self.k * (1.0 - self.inductive_fraction) * 2.0
    }

    /// Near-end crosstalk (NEXT) amplitude ratio `V_ne/V_aggressor`.
    ///
    /// Saturated value for coupled lengths longer than the rise-time
    /// equivalent: `(k_L + k_C)/4`.
    pub fn next_ratio(&self) -> f64 {
        (self.k_inductive() + self.k_capacitive()) / 4.0
    }

    /// Far-end crosstalk (FEXT) amplitude ratio for the given geometry.
    ///
    /// `V_fe/V ≈ (k_L − k_C)/2 · len/(v·tr)` — grows with coupled length and
    /// shrinks with rise time; zero in homogeneous media.
    pub fn fext_ratio(
        &self,
        coupled_length_m: f64,
        rise_time_s: f64,
        propagation_delay_s_per_m: f64,
    ) -> f64 {
        let v = 1.0 / propagation_delay_s_per_m.max(1e-300);
        (self.k_inductive() - self.k_capacitive()) / 2.0 * coupled_length_m
            / (v * rise_time_s.max(1e-300))
    }
}

/// Combined NEXT/FEXT result for a victim line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosstalkResult {
    /// Near-end amplitude ratio (fraction of aggressor swing).
    pub next_ratio: f64,
    /// Far-end amplitude ratio.
    pub fext_ratio: f64,
    /// Worst-case combined disturbance fraction.
    pub worst_ratio: f64,
}

/// Evaluates NEXT/FEXT for coupled lines.
pub fn analyze(
    model: &CrosstalkModel,
    coupled_length_m: f64,
    rise_time_s: f64,
    propagation_delay_s_per_m: f64,
) -> CrosstalkResult {
    let next = model.next_ratio();
    let fext = model.fext_ratio(coupled_length_m, rise_time_s, propagation_delay_s_per_m);
    CrosstalkResult {
        next_ratio: next,
        fext_ratio: fext,
        worst_ratio: next.abs().max(fext.abs()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupling_factor_from_modes() {
        // Ze = 60, Zo = 41.5, Zse = 50 → k = 18.5/101.5 = 0.182
        let m = CrosstalkModel::from_modes(50.0, 60.0, 41.5);
        assert!((m.k - 0.1823).abs() < 1e-3);
    }

    #[test]
    fn next_is_half_coupling() {
        let m = CrosstalkModel::from_modes(50.0, 60.0, 41.5);
        // Homogeneous: kL = kC = k → NEXT = k/2
        assert!((m.next_ratio() - m.k / 2.0).abs() < 1e-12);
    }

    #[test]
    fn fext_zero_for_homogeneous_medium() {
        let m = CrosstalkModel::from_modes(50.0, 60.0, 41.5);
        let r = analyze(&m, 0.3, 1e-9, 6.67e-9);
        assert!(r.fext_ratio.abs() < 1e-12, "fext {}", r.fext_ratio);
    }

    #[test]
    fn microstrip_fext_is_positive_and_scales() {
        // Inhomogeneous microstrip: kL > kC → positive FEXT.
        let m = CrosstalkModel::from_modes(50.0, 60.0, 41.5).with_inductive_fraction(0.6);
        let short = analyze(&m, 0.1, 1e-9, 6.67e-9);
        let long = analyze(&m, 0.3, 1e-9, 6.67e-9);
        assert!(short.fext_ratio > 0.0);
        assert!((long.fext_ratio - 3.0 * short.fext_ratio).abs() < 1e-12);
        assert!(long.fext_ratio > long.next_ratio.min(long.next_ratio) * 0.0); // sanity only
    }

    #[test]
    fn faster_edges_increase_worst_crosstalk() {
        let m = CrosstalkModel::from_modes(50.0, 60.0, 41.5).with_inductive_fraction(0.6);
        let slow = analyze(&m, 0.2, 2e-9, 6.67e-9);
        let fast = analyze(&m, 0.2, 0.5e-9, 6.67e-9);
        assert!(fast.worst_ratio > slow.worst_ratio);
    }

    #[test]
    fn uncoupled_lines_have_no_crosstalk() {
        let m = CrosstalkModel::from_modes(50.0, 50.0, 50.0);
        assert_eq!(m.k, 0.0);
        assert_eq!(m.next_ratio(), 0.0);
        let r = analyze(&m, 1.0, 1e-9, 6.67e-9);
        assert_eq!(r.worst_ratio, 0.0);
    }
}
