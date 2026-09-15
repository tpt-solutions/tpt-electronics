// SPDX-License-Identifier: MIT OR Apache-2.0

//! Control loop analysis and compensator synthesis for power converters.
//!
//! * [`TransferFunction`] — rational s-domain plant/compensator model with
//!   complex evaluation via Horner's method.
//! * [`ControlLoop::bode_plot`] / [`ControlLoop::stability_margins`] —
//!   gain crossover, phase margin, gain margin.
//! * [`CompensatorDesigner`] — k-factor Type II/III design placing zeros
//!   and poles around a target crossover for a phase-boost target.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Complex;

/// Rational transfer function `N(s)/D(s)`; coefficients are in *descending*
/// powers of s (`num[0]·s^m + … + num[m]`).
#[derive(Clone, Debug, PartialEq)]
pub struct TransferFunction {
    /// Numerator coefficients (descending powers).
    pub numerator: Vec<f64>,
    /// Denominator coefficients (descending powers).
    pub denominator: Vec<f64>,
}

impl TransferFunction {
    /// Constant gain.
    pub fn gain(k: f64) -> Self {
        Self {
            numerator: vec![k],
            denominator: vec![1.0],
        }
    }

    /// Evaluates `H(s)` at a complex point.
    pub fn eval(&self, s: Complex) -> Complex {
        let poly = |c: &[f64]| -> Complex {
            let mut acc = Complex::ZERO;
            for &coef in c {
                acc = acc * s + Complex::real(coef);
            }
            acc
        };
        poly(&self.numerator) / poly(&self.denominator)
    }

    /// Magnitude in dB at frequency `f` [Hz].
    pub fn magnitude_db_at(&self, f: f64) -> f64 {
        20.0 * self
            .eval(Complex::new(0.0, std::f64::consts::TAU * f))
            .abs()
            .log10()
    }

    /// Phase in degrees at frequency `f` [Hz].
    pub fn phase_at(&self, f: f64) -> f64 {
        self.eval(Complex::new(0.0, std::f64::consts::TAU * f))
            .arg()
            .to_degrees()
    }

    /// Series combination (product).
    pub fn series(&self, other: &TransferFunction) -> TransferFunction {
        TransferFunction {
            numerator: poly_mul(&self.numerator, &other.numerator),
            denominator: poly_mul(&self.denominator, &other.denominator),
        }
    }
}

fn poly_mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    out
}

/// Magnitude/phase sweep.
#[derive(Clone, Debug, Default)]
pub struct BodePlot {
    /// Frequencies [Hz].
    pub frequencies: Vec<f64>,
    /// |H| in dB.
    pub magnitude_db: Vec<f64>,
    /// Phase in degrees.
    pub phase_deg: Vec<f64>,
}

/// An open loop: plant × compensator.
#[derive(Clone, Debug)]
pub struct ControlLoop {
    /// Power-stage plant model.
    pub plant: TransferFunction,
    /// Controller.
    pub compensator: TransferFunction,
}

impl ControlLoop {
    /// Builds a loop from plant and compensator.
    pub fn new(plant: TransferFunction, compensator: TransferFunction) -> Self {
        Self { plant, compensator }
    }

    /// Open-loop transfer function.
    pub fn open_loop(&self) -> TransferFunction {
        self.plant.series(&self.compensator)
    }

    /// Sweeps a log-frequency Bode plot of the open loop.
    pub fn bode_plot(&self, freq_range: (f64, f64), points: usize) -> BodePlot {
        let ol = self.open_loop();
        let points = points.max(2);
        let decades = (freq_range.1.log10() - freq_range.0.log10()).max(0.0);
        let mut plot = BodePlot::default();
        for k in 0..points {
            let f = freq_range.0 * 10f64.powf(decades * k as f64 / (points - 1) as f64);
            plot.frequencies.push(f);
            plot.magnitude_db.push(ol.magnitude_db_at(f));
            plot.phase_deg.push(ol.phase_at(f));
        }
        plot
    }

    /// Stability margins from a fine frequency sweep.
    ///
    /// * Phase margin: `180° + ∠L` at the gain crossover (|L| = 1).
    /// * Gain margin: `−20·log₁₀|L|` at the phase crossover (∠L = −180°).
    pub fn stability_margins(&self) -> StabilityMargins {
        let ol = self.open_loop();
        let f0 = 1e-1f64;
        let f1 = 1e9f64;
        let n = 4000;
        let decades = f1.log10() - f0.log10();
        let mut prev_mag_diff: Option<f64> = None;
        let mut prev_phase_diff: Option<f64> = None;
        let mut margins = StabilityMargins::default();

        for k in 0..=n {
            let f = f0 * 10f64.powf(decades * k as f64 / n as f64);
            let omega = Complex::new(0.0, std::f64::consts::TAU * f);
            let l = ol.eval(omega);
            let mag_db = 20.0 * l.abs().log10();
            let phase = l.arg().to_degrees();

            let mag_diff = mag_db; // crossing at 0 dB
            let phase_diff = phase + 180.0; // crossing at −180°

            if let Some(pm0) = prev_mag_diff {
                if (pm0 > 0.0) != (mag_diff > 0.0) {
                    // linear interpolation on log-frequency
                    let frac = pm0 / (pm0 - mag_diff);
                    let fc = f * 10f64.powf(-decades / n as f64 * frac);
                    let phase_at = ol.phase_at(fc);
                    if margins.gain_crossover_hz == 0.0 {
                        margins.gain_crossover_hz = fc;
                        margins.phase_margin = phase_at + 180.0;
                    }
                }
            }
            prev_mag_diff = Some(mag_diff);

            if let Some(pd0) = prev_phase_diff {
                if (pd0 > 0.0) != (phase_diff > 0.0) && phase_diff.abs() < 400.0 {
                    let frac = pd0 / (pd0 - phase_diff);
                    let fp = f * 10f64.powf(-decades / n as f64 * frac);
                    let mag_at = ol.magnitude_db_at(fp);
                    if margins.phase_crossover_hz == 0.0 {
                        margins.phase_crossover_hz = fp;
                        margins.gain_margin = -mag_at;
                    }
                }
            }
            prev_phase_diff = Some(phase_diff);
        }
        margins
    }
}

/// Gain/phase margin results.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StabilityMargins {
    /// Gain crossover frequency [Hz] (0 if none in band).
    pub gain_crossover_hz: f64,
    /// Phase margin [°] (∞-safe: reported as-is, 180 = very stable).
    pub phase_margin: f64,
    /// Phase crossover frequency [Hz] (0 if none).
    pub phase_crossover_hz: f64,
    /// Gain margin [dB].
    pub gain_margin: f64,
}

/// Compensator type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompensatorType {
    /// Pure integrator.
    TypeI,
    /// Integrator + one zero/one pole.
    TypeII,
    /// Integrator + two zeros/two poles.
    TypeIII,
}

/// k-factor compensator synthesis.
pub struct CompensatorDesigner;

impl CompensatorDesigner {
    /// Designs a compensator so the loop crosses over at `crossover_hz` with
    /// the target phase margin.
    ///
    /// The required phase boost is `target_pm − (plant phase at fc) − 90°`
    /// (the integrator's share). Type II is chosen for boost ≤ 90°, Type III
    /// beyond; `compensator_type` may force a kind.
    pub fn design(
        plant: &TransferFunction,
        compensator_type: CompensatorType,
        crossover_hz: f64,
        target_phase_margin: f64,
    ) -> TransferFunction {
        let plant_phase = plant.phase_at(crossover_hz);
        let boost = target_phase_margin - plant_phase - 90.0;
        let fc = crossover_hz;
        let _k = |num: Vec<f64>, den: Vec<f64>| TransferFunction {
            numerator: num,
            denominator: den,
        };

        // Build final form: K_i/s × zeros × poles, gain set so |L(fc)| = 1.
        let (zeros, poles) = match compensator_type {
            CompensatorType::TypeI => (vec![], vec![]),
            CompensatorType::TypeII => {
                let kk = (45.0f64 + boost / 2.0).to_radians().tan().max(1.0);
                (
                    vec![std::f64::consts::TAU * fc / kk],
                    vec![std::f64::consts::TAU * fc * kk],
                )
            }
            CompensatorType::TypeIII => {
                let kk = ((45.0f64 + boost / 4.0).to_radians().tan().powi(2)).max(1.0);
                let sq = kk.sqrt();
                (
                    vec![
                        std::f64::consts::TAU * fc / sq,
                        std::f64::consts::TAU * fc / sq,
                    ],
                    vec![
                        std::f64::consts::TAU * fc * sq,
                        std::f64::consts::TAU * fc * sq,
                    ],
                )
            }
        };
        // Numerator: Ki·∏(1+s/wz) = Ki·(∏wz⁻¹)·∏(s+wz) → build in s-form
        let mut num = vec![1.0];
        for &wz in &zeros {
            num = poly_mul(&num, &[1.0 / wz, 1.0]);
        }
        let mut den = vec![1.0, 0.0]; // integrator s
        for &wp in &poles {
            den = poly_mul(&den, &[1.0 / wp, 1.0]);
        }
        // Gain Ki so |H(fc)| = 1/|plant(fc)|
        let s_fc = Complex::new(0.0, std::f64::consts::TAU * fc);
        let shape = TransferFunction {
            numerator: num.clone(),
            denominator: den.clone(),
        };
        let shape_mag = shape.eval(s_fc).abs().max(1e-300);
        let plant_mag = plant.eval(s_fc).abs().max(1e-300);
        let ki = 1.0 / (shape_mag * plant_mag);
        TransferFunction {
            numerator: num.iter().map(|c| c * ki).collect(),
            denominator: den,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Buck power stage (voltage mode): Gvc = (1 + s/ωz_esr)/(1 + s/ω0·Q·s…)
    /// simplified to a dominant pole + ESR zero.
    fn buck_plant() -> TransferFunction {
        // DC gain 10 (20 dB), dominant pole at 1 kHz, ESR zero at 20 kHz.
        TransferFunction {
            numerator: vec![1.0 / (std::f64::consts::TAU * 20e3), 1.0],
            denominator: vec![1.0 / (std::f64::consts::TAU * 1e3), 1.0],
        }
        .series(&TransferFunction::gain(10.0))
    }

    #[test]
    fn transfer_function_evaluates() {
        let tf = TransferFunction {
            numerator: vec![1.0, 2.0],        // s + 2
            denominator: vec![1.0, 0.0, 4.0], // s² + 4
        };
        // at s = j0: 2/4 = 0.5
        assert!((tf.eval(Complex::ZERO).re - 0.5).abs() < 1e-12);
        // gain block: 20 dB at any frequency
        assert!((TransferFunction::gain(10.0).magnitude_db_at(1e3) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn integrator_phase_is_minus_90() {
        let integ = TransferFunction {
            numerator: vec![100.0],
            denominator: vec![1.0, 0.0],
        };
        assert!((integ.phase_at(1e3) + 90.0).abs() < 1e-9);
    }

    #[test]
    fn bode_plot_shape() {
        let loop_ = ControlLoop::new(buck_plant(), TransferFunction::gain(1.0));
        let bode = loop_.bode_plot((10.0, 1e6), 100);
        assert_eq!(bode.frequencies.len(), 100);
        // DC-region magnitude ≈ 20 dB
        assert!((bode.magnitude_db[0] - 20.0).abs() < 0.1);
        // Above both corners the ESR zero flattens: gain ≈ 20 dB + 20log(1k/20k)= 6 dB
        assert!((bode.magnitude_db[99] - (20.0 + 20.0 * (1e3f64 / 2e4).log10())).abs() < 0.5);
    }

    #[test]
    fn type_ii_hits_target_phase_margin() {
        let plant = buck_plant();
        let fc = 20e3;
        let comp = CompensatorDesigner::design(&plant, CompensatorType::TypeII, fc, 60.0);
        let loop_ = ControlLoop::new(plant, comp);
        let m = loop_.stability_margins();
        assert!(m.gain_crossover_hz > 0.0, "no crossover found");
        assert!(
            (m.gain_crossover_hz - fc).abs() / fc < 0.15,
            "fc = {} vs target {}",
            m.gain_crossover_hz,
            fc
        );
        assert!(
            (m.phase_margin - 60.0).abs() < 8.0,
            "PM = {}°",
            m.phase_margin
        );
    }

    #[test]
    fn type_iii_gives_higher_boost() {
        let plant = buck_plant();
        let fc = 30e3;
        let comp = CompensatorDesigner::design(&plant, CompensatorType::TypeIII, fc, 75.0);
        let loop_ = ControlLoop::new(plant, comp);
        let m = loop_.stability_margins();
        assert!(m.gain_crossover_hz > 0.0);
        assert!(
            (m.phase_margin - 75.0).abs() < 10.0,
            "PM = {}°",
            m.phase_margin
        );
    }

    #[test]
    fn more_boost_more_phase_margin() {
        let plant = buck_plant();
        let c60 = CompensatorDesigner::design(&plant, CompensatorType::TypeIII, 20e3, 55.0);
        let c80 = CompensatorDesigner::design(&plant, CompensatorType::TypeIII, 20e3, 80.0);
        let m60 = ControlLoop::new(buck_plant(), c60).stability_margins();
        let m80 = ControlLoop::new(buck_plant(), c80).stability_margins();
        assert!(m80.phase_margin > m60.phase_margin);
    }
}
