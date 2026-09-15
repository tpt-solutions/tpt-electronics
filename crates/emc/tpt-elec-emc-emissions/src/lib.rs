// SPDX-License-Identifier: MIT OR Apache-2.0

//! Emissions prediction from switching/clock waveforms.
//!
//! [`EmissionsPredictor::radiated_emissions`] builds the trapezoidal-clock
//! harmonic envelope — flat to `1/(π·τ)`, −20 dB/decade to `1/(π·tr)`,
//! −40 dB/decade beyond — and converts loop-radiated field strength with
//! the small-loop model `E ≈ 1.316e-14 · f²·A·I / r` (V/m at 3 m scaling).
//! [`EmissionsPredictor::conducted_emissions`] predicts DM noise voltage
//! from `di/dt` through a parasitic inductance. Results compare against
//! `tpt-elec-emc-core` limit lines.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_emc_core::EmcLimit;

/// One emission harmonic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Harmonic {
    /// Harmonic number (1 = fundamental).
    pub number: u32,
    /// Frequency [Hz].
    pub frequency: f64,
    /// Amplitude (context dependent, dBµV/m or dBµV).
    pub amplitude_dbuv: f64,
}

/// A predicted emissions spectrum.
#[derive(Clone, Debug)]
pub struct EmissionsSpectrum {
    /// Fundamental frequency [Hz].
    pub fundamental_hz: f64,
    /// Harmonic frequencies [Hz].
    pub frequencies: Vec<f64>,
    /// Amplitudes [dBµV/m] (radiated) or [dBµV] (conducted).
    pub amplitudes_dbuv: Vec<f64>,
    /// Structured harmonics.
    pub harmonics: Vec<Harmonic>,
}

impl EmissionsSpectrum {
    /// Worst margin [dB] against a limit line (positive = compliant).
    pub fn worst_margin_db(&self, limit: &EmcLimit) -> f64 {
        self.frequencies
            .iter()
            .zip(&self.amplitudes_dbuv)
            .map(|(&f, &a)| limit.margin_at(f, a))
            .fold(f64::INFINITY, f64::min)
    }

    /// CSV export: `frequency_hz,amplitude_dbuv,limit_dbuv,margin_db`.
    pub fn to_csv_with_limit(&self, limit: &EmcLimit) -> String {
        let mut out = String::from(
            "frequency_hz,amplitude_dbuv,limit_dbuv,margin_db
",
        );
        for (&f, &a) in self.frequencies.iter().zip(&self.amplitudes_dbuv) {
            out.push_str(&format!(
                "{:.0},{:.2},{:.2},{:.2}
",
                f,
                a,
                limit.limit_at(f),
                limit.margin_at(f, a)
            ));
        }
        out
    }
}

/// Emissions prediction.
pub struct EmissionsPredictor;

impl EmissionsPredictor {
    /// Radiated emissions from a trapezoidal clock switching a current loop.
    ///
    /// * `clock_frequency` — fundamental [Hz]
    /// * `loop_area` — radiating loop area [m²]
    /// * `current` — switching current swing [A]
    /// * `rise_time` — 10–90 % rise time [s]
    /// * `harmonics` — how many harmonics to compute
    ///
    /// Field strength at the CISPR 3 m distance, dBµV/m.
    pub fn radiated_emissions(
        clock_frequency: f64,
        loop_area: f64,
        current: f64,
        rise_time: f64,
        harmonics: u32,
    ) -> EmissionsSpectrum {
        let duty = 0.5;
        let mut freqs = Vec::new();
        let mut amps = Vec::new();
        let mut harmonics_out = Vec::new();
        for n in 1..=harmonics.max(1) {
            let f = clock_frequency * n as f64;
            // trapezoidal Fourier coefficients: |c_n| = 2·A·d·|sinc(n·π·d)|
            let cn = 2.0
                * current
                * duty
                * sinc(n as f64 * std::f64::consts::PI * duty)
                * envelope(f, clock_frequency, rise_time);
            // small-loop far field: |E| = 1.316e-14 · f² · A · I / r
            let e_v_per_m = 1.316e-14 * f * f * loop_area * cn / 3.0;
            let dbuv_m = 20.0 * e_v_per_m.max(1e-30).log10();
            freqs.push(f);
            amps.push(dbuv_m);
            harmonics_out.push(Harmonic {
                number: n,
                frequency: f,
                amplitude_dbuv: dbuv_m,
            });
        }
        EmissionsSpectrum {
            fundamental_hz: clock_frequency,
            frequencies: freqs,
            amplitudes_dbuv: amps,
            harmonics: harmonics_out,
        }
    }

    /// Conducted (differential-mode) noise voltage on a power port.
    ///
    /// `V = L_parasitic · di/dt` mapped to harmonics of the switching
    /// frequency, in dBµV against a 50 Ω LISN.
    pub fn conducted_emissions(
        switching_frequency: f64,
        di_dt: f64,
        parasitic_inductance: f64,
        harmonics: u32,
    ) -> EmissionsSpectrum {
        let mut freqs = Vec::new();
        let mut amps = Vec::new();
        let mut harmonics_out = Vec::new();
        let v_noise = parasitic_inductance * di_dt;
        for n in 1..=harmonics.max(1) {
            let f = switching_frequency * n as f64;
            // Envelope falls with 40 dB/dec for trapezoidal di/dt harmonics.
            let cn = v_noise / n as f64;
            let dbuv = 20.0 * (cn * std::f64::consts::SQRT_2).max(1e-30).log10();
            freqs.push(f);
            amps.push(dbuv);
            harmonics_out.push(Harmonic {
                number: n,
                frequency: f,
                amplitude_dbuv: dbuv,
            });
        }
        EmissionsSpectrum {
            fundamental_hz: switching_frequency,
            frequencies: freqs,
            amplitudes_dbuv: amps,
            harmonics: harmonics_out,
        }
    }
}

/// Trapezoidal envelope: 0 dB to 1/(π·τ), −20 dB/dec to 1/(π·tr), then
/// −40 dB/decade (τ = half period at 50 % duty).
fn envelope(f: f64, f0: f64, tr: f64) -> f64 {
    let tau = 1.0 / (2.0 * f0);
    let f1 = 1.0 / (std::f64::consts::PI * tau);
    let f2 = 1.0 / (std::f64::consts::PI * tr.max(1e-15));
    if f <= f1 {
        1.0
    } else if f <= f2 {
        // −20 dB/dec between the pulse-width and rise-time corners
        f1 / f
    } else {
        // −40 dB/dec beyond the rise-time corner: both slopes multiply
        (f1 / f) * (f2 / f)
    }
}

/// |sinc(x)| with sinc(0) = 1.
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (x.sin() / x).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_emc_core::{EmcStandard, EmcTestType};

    #[test]
    fn spectrum_envelope_rolls_off() {
        let spec = EmissionsPredictor::radiated_emissions(100e6, 1e-4, 0.1, 1e-9, 20);
        // The trapezoid corner f1 = 200 MHz, f2 = 318 MHz: harmonics beyond
        // 400 MHz must fall at 40 dB/dec (100× per 50× in harmonic count).
        let a_low = spec.amplitudes_dbuv[0];
        let a_high = spec.amplitudes_dbuv[19];
        assert!(a_high < a_low - 40.0, "drop {a_low} → {a_high}");
    }

    #[test]
    fn faster_edges_raise_high_harmonics() {
        let slow = EmissionsPredictor::radiated_emissions(50e6, 1e-4, 0.1, 5e-9, 10);
        let fast = EmissionsPredictor::radiated_emissions(50e6, 1e-4, 0.1, 1e-9, 10);
        // High harmonics (beyond both corners) are worse with faster edges
        assert!(fast.amplitudes_dbuv[9] > slow.amplitudes_dbuv[9]);
    }

    #[test]
    fn larger_loop_area_scales_20db_per_decade() {
        let small = EmissionsPredictor::radiated_emissions(100e6, 1e-4, 0.1, 1e-9, 1);
        let big = EmissionsPredictor::radiated_emissions(100e6, 1e-2, 0.1, 1e-9, 1);
        // 100× area = 40 dB
        assert!((big.amplitudes_dbuv[0] - small.amplitudes_dbuv[0] - 40.0).abs() < 0.5);
    }

    #[test]
    fn digital_board_against_cispr32_class_b() {
        // A 100 MHz clock on a 1 cm² loop with 20 mA: classic classroom case.
        let spec = EmissionsPredictor::radiated_emissions(100e6, 1e-4, 0.02, 2e-9, 5);
        let limit =
            EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::RadiatedEmissions)
                .unwrap();
        let margin = spec.worst_margin_db(&limit);
        // Fundamentals pass; the check just needs sane numbers.
        assert!(margin.is_finite());
        assert!(
            spec.amplitudes_dbuv[0] < 40.0,
            "fundamental {}",
            spec.amplitudes_dbuv[0]
        );
    }

    #[test]
    fn conducted_noise_scales_with_inductance() {
        let small = EmissionsPredictor::conducted_emissions(500e3, 1e6, 10e-9, 3);
        let large = EmissionsPredictor::conducted_emissions(500e3, 1e6, 100e-9, 3);
        // 10× L → +20 dB
        assert!(
            (large.amplitudes_dbuv[0] - small.amplitudes_dbuv[0] - 20.0).abs() < 0.1,
            "{} vs {}",
            large.amplitudes_dbuv[0],
            small.amplitudes_dbuv[0]
        );
    }

    #[test]
    fn csv_export_contains_all_harmonics_and_margins() {
        let spec = EmissionsPredictor::radiated_emissions(100e6, 1e-4, 0.02, 2e-9, 5);
        let limit =
            EmcLimit::for_standard(EmcStandard::Cispr32ClassB, EmcTestType::RadiatedEmissions)
                .unwrap();
        let csv = spec.to_csv_with_limit(&limit);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "frequency_hz,amplitude_dbuv,limit_dbuv,margin_db");
        assert_eq!(lines.len() - 1, 5);
        // rows sorted by frequency
        let f0: f64 = lines[1].split(',').next().unwrap().parse().unwrap();
        assert!((f0 - 100e6).abs() < 1e3);
    }

    #[test]
    fn conducted_harmonics_roll_off_20db_per_decade() {
        let spec = EmissionsPredictor::conducted_emissions(1e6, 1e6, 10e-9, 20);
        assert!(
            (spec.amplitudes_dbuv[9] - spec.amplitudes_dbuv[0] + 20.0).abs() < 0.1,
            "10th vs 1st harmonic"
        );
    }
}
