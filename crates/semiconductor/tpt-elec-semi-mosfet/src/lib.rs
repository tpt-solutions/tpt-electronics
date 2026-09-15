// SPDX-License-Identifier: MIT OR Apache-2.0

//! MOSFET compact models for standalone device analysis.
//!
//! Wraps the Level 1–3 models from `tpt-elec-spice-models` and adds
//! device-physics helpers: threshold voltage with body effect,
//! transconductance `gm`, output conductance `gds`, and a constant-mobility
//! long-channel transconductance estimate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use tpt_elec_core::Length;
pub use tpt_elec_spice_models::{
    MosfetLevel, MosfetModel, MosfetOperatingPoint, MosfetParameters, MosfetPolarity,
};

/// A MOSFET device instance (model + geometry).
#[derive(Clone, Debug)]
pub struct MosfetDevice {
    /// Compact model.
    pub model: MosfetModel,
}

impl MosfetDevice {
    /// A Level-1 NMOS.
    pub fn level1(parameters: MosfetParameters) -> Self {
        Self {
            model: MosfetModel::level1(parameters),
        }
    }

    /// Drain current [A].
    pub fn drain_current(&self, vgs: f64, vds: f64, vbs: f64) -> f64 {
        self.model.drain_current(vgs, vds, vbs)
    }

    /// Threshold voltage including body effect [V].
    pub fn threshold_voltage(&self, vbs: f64) -> f64 {
        self.model.threshold_voltage(vbs)
    }

    /// Transconductance ∂Id/∂Vgs [S] at the operating point.
    pub fn transconductance(&self, vgs: f64, vds: f64, vbs: f64) -> f64 {
        self.model.operating_point(vgs, vds, vbs).gm
    }

    /// Output conductance ∂Id/∂Vds [S].
    pub fn output_conductance(&self, vgs: f64, vds: f64, vbs: f64) -> f64 {
        self.model.operating_point(vgs, vds, vbs).gds
    }

    /// Full operating point (current + small-signal terms).
    pub fn operating_point(&self, vgs: f64, vds: f64, vbs: f64) -> MosfetOperatingPoint {
        self.model.operating_point(vgs, vds, vbs)
    }

    /// Long-channel square-law estimate of gm at saturation:
    /// `gm = √(2·β·ID)` with β = kp·W/L.
    pub fn gm_saturation_estimate(&self, drain_current: f64) -> f64 {
        let p = &self.model.parameters;
        let beta = p.kp * (p.w.as_meters() / p.l.as_meters());
        (2.0 * beta * drain_current).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> MosfetDevice {
        MosfetDevice::level1(MosfetParameters {
            vth0: 0.7,
            kp: 110e-6,
            w: Length::um(10.0),
            l: Length::um(1.0),
            ..Default::default()
        })
    }

    #[test]
    fn saturation_current_matches_square_law() {
        let d = device();
        let id = d.drain_current(2.0, 5.0, 0.0);
        let expected = 0.5 * 110e-6 * 10.0 * (2.0 - 0.7f64).powi(2) * (1.0 + 0.01 * 5.0);
        assert!((id - expected).abs() < 1e-12);
        assert!(d.operating_point(2.0, 5.0, 0.0).saturated);
    }

    #[test]
    fn gm_and_gds_consistent_with_numerics() {
        let d = device();
        let (vgs, vds, vbs) = (2.0, 3.0, 0.0);
        let eps = 1e-6;
        let gm_num = (d.drain_current(vgs + eps, vds, vbs) - d.drain_current(vgs - eps, vds, vbs))
            / (2.0 * eps);
        let gds_num = (d.drain_current(vgs, vds + eps, vbs) - d.drain_current(vgs, vds - eps, vbs))
            / (2.0 * eps);
        assert!((d.transconductance(vgs, vds, vbs) - gm_num).abs() / gm_num < 1e-4);
        assert!((d.output_conductance(vgs, vds, vbs) - gds_num).abs() / gds_num < 1e-4);
    }

    #[test]
    fn body_effect_raises_threshold() {
        let d = device();
        assert!(d.threshold_voltage(-1.0) > d.threshold_voltage(0.0));
    }

    #[test]
    fn square_law_gm_estimate() {
        let d = device();
        let id = d.drain_current(2.0, 5.0, 0.0);
        let gm_est = d.gm_saturation_estimate(id);
        let gm = d.transconductance(2.0, 5.0, 0.0);
        assert!((gm_est - gm).abs() / gm < 0.2, "est {gm_est} vs {gm}");
    }

    #[test]
    fn cutoff_is_zero() {
        let d = device();
        assert_eq!(d.drain_current(0.3, 5.0, 0.0), 0.0);
        assert_eq!(d.transconductance(0.3, 5.0, 0.0), 0.0);
    }
}
