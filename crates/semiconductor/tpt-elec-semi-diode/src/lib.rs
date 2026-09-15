// SPDX-License-Identifier: MIT OR Apache-2.0

//! Diode compact model for standalone device analysis (shared formulation
//! with `tpt-elec-spice-models`): Shockley conduction, series resistance,
//! junction capacitance, and reverse breakdown parameters.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use tpt_elec_spice_models::{thermal_voltage, DiodeModel};

/// A diode device instance.
#[derive(Clone, Debug)]
pub struct DiodeDevice {
    /// Compact model.
    pub model: DiodeModel,
    /// Operating temperature [K].
    pub temperature_k: f64,
}

impl DiodeDevice {
    /// A 1N4148-style small-signal diode.
    pub fn small_signal() -> Self {
        Self {
            model: DiodeModel {
                is: 2.52e-9,
                n: 1.752,
                rs: 0.5664,
                cjo: 3.5e-12,
                vj: 0.55,
                m: 0.32,
                bv: 100.0,
                ibv: 1e-4,
            },
            temperature_k: 300.0,
        }
    }

    /// Forward current [A] at applied voltage.
    pub fn current(&self, v: f64) -> f64 {
        self.model.current(v, self.temperature_k)
    }

    /// Dynamic conductance [S].
    pub fn conductance(&self, v: f64) -> f64 {
        self.model.conductance(v, self.temperature_k)
    }

    /// Junction capacitance [F].
    pub fn capacitance(&self, v: f64) -> f64 {
        self.model.junction_capacitance(v)
    }

    /// Small-signal AC resistance at the DC bias [Ω].
    pub fn ac_resistance(&self, v: f64) -> f64 {
        1.0 / self.conductance(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_v_for_current(d: &DiodeDevice, target: f64) -> f64 {
        let (mut lo, mut hi) = (0.0f64, 1.5f64);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            if d.current(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (lo + hi) / 2.0
    }

    #[test]
    fn forward_drop_near_700mv() {
        let d = DiodeDevice::small_signal();
        let v_1ma = find_v_for_current(&d, 1e-3);
        let v_10ma = find_v_for_current(&d, 1e-2);
        assert!((0.5..0.75).contains(&v_1ma), "v(1mA) = {v_1ma}");
        assert!(v_10ma > v_1ma);
    }

    #[test]
    fn ideality_splits_thermal_voltage() {
        // Use an rs-free model: series resistance flattens the local slope.
        let d = DiodeDevice {
            model: DiodeModel {
                is: 1e-14,
                n: 1.752,
                rs: 0.0,
                ..Default::default()
            },
            temperature_k: 300.0,
        };
        let v = 0.7;
        let slope = (d.current(v + 0.01).log10() - d.current(v).log10()) / 0.01;
        let expected = 1.0 / (d.model.n * thermal_voltage(300.0) * 10.0f64.ln());
        assert!((slope - expected).abs() / expected < 0.01);
    }

    #[test]
    fn reverse_blocking() {
        let d = DiodeDevice::small_signal();
        assert!(d.current(-1.0).abs() < d.model.is * 2.0);
        assert!(d.model.bv > 0.0);
    }

    #[test]
    fn capacitance_varies_with_bias() {
        let d = DiodeDevice::small_signal();
        let c_rev = d.capacitance(-1.0);
        let c_zero = d.capacitance(0.0);
        assert!(c_rev < c_zero);
        assert!(d.ac_resistance(0.7).is_finite());
    }
}
