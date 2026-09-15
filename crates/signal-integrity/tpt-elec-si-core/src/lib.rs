// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transmission-line models and S-parameter data structures.
//!
//! This crate owns the shared vocabulary of the signal-integrity domain:
//! [`LineParameters`] for single/coupled lines, and [`SParameters`] for
//! frequency-domain two-port data (populated by `tpt-elec-touchstone`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::{Complex, Length};

/// Transmission-line geometry family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineType {
    /// Trace over a reference plane.
    Microstrip,
    /// Trace between two reference planes.
    Stripline,
    /// Trace with coplanar reference on the same layer.
    CoplanarWaveguide,
    /// Two coupled traces driven differentially.
    DifferentialPair,
}

/// Characteristic parameters of a (possibly coupled) line.
#[derive(Clone, Debug, PartialEq)]
pub struct LineParameters {
    /// Single-ended characteristic impedance [Ω].
    pub z0: f64,
    /// Odd-mode impedance (differential half) [Ω], when coupled.
    pub z0_odd: Option<f64>,
    /// Even-mode impedance [Ω], when coupled.
    pub z0_even: Option<f64>,
    /// Propagation delay per unit length [s/m].
    pub propagation_delay: f64,
    /// Dielectric loss tangent.
    pub loss_tangent: f64,
    /// Skin-effect resistance coefficient: R(f) = skin_effect·√f [Ω/(m·√Hz)].
    pub skin_effect: f64,
}

impl LineParameters {
    /// Single-ended line.
    pub fn single(z0: f64, er_eff: f64) -> Self {
        Self {
            z0,
            z0_odd: None,
            z0_even: None,
            propagation_delay: er_eff.sqrt() / tpt_elec_core::SPEED_OF_LIGHT,
            loss_tangent: 0.0,
            skin_effect: 0.0,
        }
    }

    /// Coupled line with even/odd modes.
    pub fn coupled(z0: f64, z_odd: f64, z_even: f64, er_eff: f64) -> Self {
        Self {
            z0,
            z0_odd: Some(z_odd),
            z0_even: Some(z_even),
            propagation_delay: er_eff.sqrt() / tpt_elec_core::SPEED_OF_LIGHT,
            loss_tangent: 0.0,
            skin_effect: 0.0,
        }
    }

    /// Differential impedance (2×odd mode) [Ω], if coupled.
    pub fn differential_impedance(&self) -> Option<f64> {
        self.z0_odd.map(|z| 2.0 * z)
    }

    /// Propagation delay over a physical length [s].
    pub fn delay_over(&self, length: Length) -> f64 {
        self.propagation_delay * length.as_meters()
    }

    /// Velocity of propagation [m/s].
    pub fn velocity(&self) -> f64 {
        1.0 / self.propagation_delay.max(1e-300)
    }
}

/// One 2×2 S-parameter matrix sample (magnitude/phase as complex values).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SParameterMatrix {
    /// Input reflection.
    pub s11: Complex,
    /// Forward transmission.
    pub s21: Complex,
    /// Reverse transmission.
    pub s12: Complex,
    /// Output reflection.
    pub s22: Complex,
}

impl SParameterMatrix {
    /// |S11| in dB (return loss, negative in dB).
    pub fn return_loss_db(&self) -> f64 {
        20.0 * self.s11.abs().log10()
    }

    /// |S21| in dB (insertion loss, negative in dB).
    pub fn insertion_loss_db(&self) -> f64 {
        20.0 * self.s21.abs().log10()
    }

    /// VSWR from |S11|.
    pub fn vswr(&self) -> f64 {
        let m = self.s11.abs().min(0.999_999);
        (1.0 + m) / (1.0 - m)
    }
}

/// Frequency-domain S-parameter table.
#[derive(Clone, Debug, Default)]
pub struct SParameters {
    /// Frequencies [Hz].
    pub frequencies: Vec<f64>,
    /// S-matrix per frequency.
    pub data: Vec<SParameterMatrix>,
    /// Reference resistance [Ω] the parameters are normalized to.
    pub reference_impedance: f64,
}

impl SParameters {
    /// Empty table for a given reference impedance.
    pub fn new(reference_impedance: f64) -> Self {
        Self {
            frequencies: Vec::new(),
            data: Vec::new(),
            reference_impedance,
        }
    }

    /// Number of samples.
    pub fn len(&self) -> usize {
        self.frequencies.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.frequencies.is_empty()
    }

    /// Pushes one sample.
    pub fn push(&mut self, freq: f64, matrix: SParameterMatrix) {
        self.frequencies.push(freq);
        self.data.push(matrix);
    }

    /// Inserts the table into a Z₀ system: converts S11 to an impedance [Ω].
    pub fn input_impedance(&self, index: usize) -> Option<Complex> {
        let s11 = self.data.get(index)?.s11;
        let z0 = self.reference_impedance;
        Some(Complex::real(z0) * (Complex::ONE + s11) / (Complex::ONE - s11))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_parameters_single() {
        // er_eff = 4 → v = c/2, delay = 6.667 ns/m
        let lp = LineParameters::single(50.0, 4.0);
        assert!((lp.propagation_delay - 6.667e-9).abs() < 1e-9);
        assert!((lp.velocity() - 1.49896229e8).abs() < 1e3);
        let t = lp.delay_over(Length::mm(100.0));
        assert!((t - 0.6667e-9).abs() < 1e-12);
        assert!(lp.differential_impedance().is_none());
    }

    #[test]
    fn coupled_line_differential() {
        let lp = LineParameters::coupled(50.0, 40.0, 62.5, 4.0);
        assert!((lp.differential_impedance().unwrap() - 80.0).abs() < 1e-12);
        assert!(lp.z0_even.unwrap() > lp.z0);
        assert!(lp.z0_odd.unwrap() < lp.z0);
    }

    #[test]
    fn sparameter_metrics() {
        let m = SParameterMatrix {
            s11: Complex::from_polar(0.1, 0.0),
            s21: Complex::from_polar(0.995, -0.5),
            s12: Complex::ZERO,
            s22: Complex::ZERO,
        };
        // |S11| = 0.1 → −20 dB
        assert!((m.return_loss_db() + 20.0).abs() < 1e-9);
        // VSWR = 1.1/0.9 = 1.222
        assert!((m.vswr() - 1.2222).abs() < 1e-3);
        // |S21| ≈ −0.043 dB
        assert!(m.insertion_loss_db().abs() < 0.1);
    }

    #[test]
    fn sparameters_input_impedance() {
        let mut sp = SParameters::new(50.0);
        // S11 = +0.3333 → Zin = 50·(1.3333/0.6667) = 100 Ω
        sp.push(
            1e9,
            SParameterMatrix {
                s11: Complex::real(1.0 / 3.0),
                ..Default::default()
            },
        );
        let z = sp.input_impedance(0).unwrap();
        assert!((z.re - 100.0).abs() < 0.1);
        assert!(z.im.abs() < 1e-9);
    }
}
