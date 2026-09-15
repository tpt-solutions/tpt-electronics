// SPDX-License-Identifier: MIT OR Apache-2.0

//! BJT compact model (Ebers-Moll transport form) for standalone device
//! analysis, reusing the `tpt-elec-spice-models` formulation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use tpt_elec_spice_models::{thermal_voltage, BjtModel, BjtOperatingPoint, BjtPolarity};

/// A BJT device instance.
#[derive(Clone, Debug)]
pub struct BjtDevice {
    /// Compact model.
    pub model: BjtModel,
    /// Operating temperature [K].
    pub temperature_k: f64,
}

impl BjtDevice {
    /// A generic small-signal NPN.
    pub fn npn() -> Self {
        Self {
            model: BjtModel {
                polarity: BjtPolarity::Npn,
                is: 1e-15,
                bf: 150.0,
                br: 2.0,
                va: 80.0,
            },
            temperature_k: 300.0,
        }
    }

    /// Operating point for junction voltages (positive = forward for NPN).
    pub fn operating_point(&self, vbe: f64, vbc: f64) -> BjtOperatingPoint {
        self.model.operating_point(vbe, vbc, self.temperature_k)
    }

    /// Common-emitter current gain at the operating point.
    pub fn beta(&self, vbe: f64, vbc: f64) -> f64 {
        let op = self.operating_point(vbe, vbc);
        if op.ib.abs() > 1e-18 {
            op.ic / op.ib
        } else {
            0.0
        }
    }

    /// Transconductance gm [S] in the active region.
    pub fn transconductance(&self, vbe: f64, vbc: f64) -> f64 {
        self.operating_point(vbe, vbc).gm.abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_region_gain() {
        let q = BjtDevice::npn();
        let beta = q.beta(0.68, -1.0);
        assert!((100.0..151.0).contains(&beta), "beta {beta}");
    }

    #[test]
    fn collector_current_scales_with_exp_vbe() {
        let q = BjtDevice::npn();
        let ic1 = q.operating_point(0.65, -2.0).ic;
        let vt = thermal_voltage(300.0);
        let ic_e = q.operating_point(0.65 + vt, -2.0).ic;
        // one thermal voltage → e-folding of the current
        assert!((ic_e / ic1 - std::f64::consts::E).abs() / std::f64::consts::E < 0.01);
    }

    #[test]
    fn saturation_collapses_beta() {
        let q = BjtDevice::npn();
        let active = q.beta(0.70, -1.0);
        let sat = q.beta(0.72, 0.65); // both junctions strongly forward
        assert!(sat < active / 4.0, "sat {sat} vs active {active}");
    }

    #[test]
    fn transconductance_is_ic_over_vt() {
        let q = BjtDevice::npn();
        let vbe = 0.68;
        let op = q.operating_point(vbe, -2.0);
        let expected = op.ic / thermal_voltage(300.0);
        assert!((q.transconductance(vbe, -2.0) - expected).abs() / expected < 0.02);
    }

    #[test]
    fn pnp_mirrors_current_sign() {
        let mut q = BjtDevice::npn();
        q.model.polarity = BjtPolarity::Pnp;
        assert!(q.operating_point(-0.68, 1.0).ic < 0.0);
    }
}
