// SPDX-License-Identifier: MIT OR Apache-2.0

//! Yield prediction for board/semiconductor manufacturing.
//!
//! * [`YieldPredictor::poisson`] — `Y = e^(−D·A)` (defect density × area).
//! * [`YieldPredictor::murphy`] — the integral of a triangular defect
//!   density distribution (more pessimistic than Poisson at large D·A).
//! * [`YieldPredictor::seeds`] — the Seeds exponential model.
//! * Composite board yield: bare PCB × assembly × per-component yields.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Yield models.
pub struct YieldPredictor;

impl YieldPredictor {
    /// Poisson model: `Y = e^(−D·A)`.
    ///
    /// * `defect_density` — defects per area [1/m² or 1/cm², keep units
    ///   consistent with `area`]
    /// * `area` — die/board area in matching units
    pub fn poisson(defect_density: f64, area: f64) -> f64 {
        (-(defect_density * area).max(0.0)).exp()
    }

    /// Murphy model: `Y = [(1 − e^(−D·A))/(D·A)]²` — triangular defect
    /// density distribution; more optimistic than Poisson for all D·A > 0.
    pub fn murphy(defect_density: f64, area: f64) -> f64 {
        let da = (defect_density * area).max(1e-12);
        let y = (1.0 - (-da).exp()) / da;
        y * y
    }

    /// Seeds model: `Y = e^(−√(D·A))` — crosses the Poisson curve at
    /// D·A = 1 (pessimistic below, optimistic above).
    pub fn seeds(defect_density: f64, area: f64) -> f64 {
        (-(defect_density * area).max(0.0).sqrt()).exp()
    }

    /// Composite board yield: bare board × solder paste × reflow ×
    /// per-component attach yields raised to component counts.
    pub fn board_yield(
        pcb_yield: f64,
        assembly_yield: f64,
        component_yields: &[(f64, u32)],
    ) -> f64 {
        let mut y = pcb_yield.clamp(0.0, 1.0) * assembly_yield.clamp(0.0, 1.0);
        for &(component_yield, count) in component_yields {
            y *= component_yield.clamp(0.0, 1.0).powi(count as i32);
        }
        y.clamp(0.0, 1.0)
    }

    /// Defect density implied by a measured Poisson yield [1/area].
    pub fn defect_density_from_yield(yield_: f64, area: f64) -> f64 {
        -yield_.clamp(1e-9, 1.0).ln() / area.max(1e-12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisson_reference_points() {
        // D·A = 0.1 → 90.5 %
        assert!((YieldPredictor::poisson(1.0, 0.1) - 0.9048).abs() < 1e-3);
        // D·A = 1 → 36.8 %
        assert!((YieldPredictor::poisson(2.0, 0.5) - 0.3679).abs() < 1e-3);
        // Zero defects → 100 %
        assert_eq!(YieldPredictor::poisson(0.0, 10.0), 1.0);
    }

    #[test]
    fn murphy_is_more_optimistic_than_poisson() {
        for da in [0.5, 1.0, 2.0, 4.0] {
            let p = YieldPredictor::poisson(da, 1.0);
            let m = YieldPredictor::murphy(da, 1.0);
            assert!(m > p, "murphy {m} vs poisson {p} at DA={da}");
        }
    }

    #[test]
    fn seeds_crosses_poisson_at_da_one() {
        // Below D·A = 1 Seeds is more pessimistic; above, more optimistic.
        assert!(YieldPredictor::seeds(0.25, 1.0) < YieldPredictor::poisson(0.25, 1.0));
        assert!((YieldPredictor::seeds(1.0, 1.0) - YieldPredictor::poisson(1.0, 1.0)).abs() < 1e-9);
        assert!(YieldPredictor::seeds(4.0, 1.0) > YieldPredictor::poisson(4.0, 1.0));
    }

    #[test]
    fn board_yield_multiplies() {
        // 100 parts at 99.9 % each: ~90.5 %
        let y = YieldPredictor::board_yield(0.99, 0.995, &[(0.999, 100)]);
        assert!((0.80..0.92).contains(&y), "y {y}");
        // One bad component type tanks it
        let bad = YieldPredictor::board_yield(1.0, 1.0, &[(0.5, 10)]);
        assert!(bad < 0.001);
    }

    #[test]
    fn density_round_trip() {
        let d = 0.7;
        let y = YieldPredictor::poisson(d, 2.0);
        let d_back = YieldPredictor::defect_density_from_yield(y, 2.0);
        assert!((d_back - d).abs() / d < 1e-6);
    }
}
