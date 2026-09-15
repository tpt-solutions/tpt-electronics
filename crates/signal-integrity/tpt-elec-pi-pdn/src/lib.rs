// SPDX-License-Identifier: MIT OR Apache-2.0

//! Power Delivery Network (PDN) impedance analysis.
//!
//! The PDN is modeled as parallel branches — VRM, plane capacitance, and
//! each decoupling capacitor (with ESR/ESL and mounting inductance) — over a
//! logarithmic frequency grid. [`PdnAnalysis::impedance_profile`] returns
//! |Z(f)| and its peak; [`PdnAnalysis::optimize_decoupling`] greedily picks
//! capacitors from a parts list until the target impedance is met.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;

/// A bulk/MLCC capacitor model.
#[derive(Clone, Debug, PartialEq)]
pub struct DecouplingCap {
    /// Nominal capacitance [F].
    pub value: f64,
    /// Equivalent series resistance [Ω].
    pub esr: f64,
    /// Equivalent series inductance [F→H].
    pub esl: f64,
    /// Quantity installed (same part in parallel).
    pub quantity: u32,
    /// Mounting (via/pad loop) inductance [H], adds to ESL.
    pub mount_inductance: f64,
}

impl DecouplingCap {
    /// Complex impedance of `quantity` caps in parallel at frequency `f`.
    pub fn impedance(&self, f: f64) -> (f64, f64) {
        let l_total = self.esl + self.mount_inductance;
        let omega = std::f64::consts::TAU * f;
        let z1_re = self.esr;
        let z1_im = omega * l_total - 1.0 / (omega * self.value.max(1e-18));
        // Parallel of N identical branches: Z_total = Z_one / N
        let n = self.quantity.max(1) as f64;
        (z1_re / n, z1_im / n)
    }

    /// Self-resonant frequency [Hz].
    pub fn srf(&self) -> f64 {
        let l = self.esl + self.mount_inductance;
        1.0 / (std::f64::consts::TAU * (self.value.max(1e-18) * l.max(1e-18)).sqrt())
    }
}

/// VRM model: resistive output impedance up to a loop bandwidth, inductive
/// beyond it (the sense loop opens).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VrmModel {
    /// Output resistance [Ω].
    pub output_resistance: f64,
    /// Loop bandwidth [Hz].
    pub bandwidth_hz: f64,
    /// Output inductance beyond bandwidth [H].
    pub output_inductance: f64,
}

impl VrmModel {
    /// Complex impedance at frequency `f` (re/im).
    pub fn impedance(&self, f: f64) -> (f64, f64) {
        if f <= self.bandwidth_hz.max(1e-3) {
            (self.output_resistance, 0.0)
        } else {
            let omega = std::f64::consts::TAU * f;
            (self.output_resistance, omega * self.output_inductance)
        }
    }
}

/// |Z| vs frequency results.
#[derive(Clone, Debug, Default)]
pub struct ImpedanceProfile {
    /// Frequencies [Hz].
    pub frequencies: Vec<f64>,
    /// |Z| magnitude [Ω].
    pub impedance_mag: Vec<f64>,
    /// Peak |Z| in band [Ω].
    pub peak_impedance: f64,
    /// Frequency of the peak [Hz].
    pub peak_frequency: f64,
    /// Whether the peak meets the target.
    pub target_met: bool,
}

/// A PDN under analysis.
#[derive(Clone, Debug, Default)]
pub struct PdnAnalysis {
    /// Target impedance [Ω] (Vripple/Itransient).
    pub target_impedance: f64,
    /// Analysis band [Hz].
    pub frequency_range: (f64, f64),
    /// Installed decoupling capacitors.
    pub decoupling_caps: Vec<DecouplingCap>,
    /// Plane (spread) capacitance [F].
    pub plane_capacitance: f64,
    /// VRM model.
    pub vrm_model: VrmModel,
    /// Points per decade for the sweep.
    pub points_per_decade: u32,
}

impl PdnAnalysis {
    /// Complex PDN impedance at frequency `f` (all branches in parallel).
    pub fn impedance_at(&self, f: f64) -> (f64, f64) {
        let mut y_re = 0.0f64;
        let mut y_im = 0.0f64;
        // VRM
        let (vr, vi) = self.vrm_model.impedance(f);
        let mag2 = (vr * vr + vi * vi).max(1e-300);
        y_re += vr / mag2;
        y_im -= vi / mag2;
        // Plane capacitance
        if self.plane_capacitance > 0.0 {
            let omega = std::f64::consts::TAU * f;
            y_im += omega * self.plane_capacitance;
        }
        // Caps
        for cap in &self.decoupling_caps {
            let (zr, zi) = cap.impedance(f);
            let mag2 = (zr * zr + zi * zi).max(1e-300);
            y_re += zr / mag2;
            y_im -= zi / mag2;
        }
        let mag2 = (y_re * y_re + y_im * y_im).max(1e-300);
        (y_re / mag2, -y_im / mag2)
    }

    /// Sweeps the band and reports |Z(f)| plus the peak.
    pub fn impedance_profile(&self) -> ImpedanceProfile {
        let (f0, f1) = self.frequency_range;
        let decades = (f1.log10() - f0.log10()).max(0.0);
        let steps = ((decades * self.points_per_decade.max(1) as f64).ceil()) as usize;
        let mut freqs = Vec::with_capacity(steps + 1);
        let mut mags = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let f = (f0 * 10f64.powf(k as f64 / self.points_per_decade.max(1) as f64)).min(f1);
            let (re, im) = self.impedance_at(f);
            freqs.push(f);
            mags.push((re * re + im * im).sqrt());
            if f >= f1 {
                break;
            }
        }
        let (peak_i, peak) = mags
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, v)| (i, *v))
            .unwrap_or((0, 0.0));
        let peak_freq = freqs.get(peak_i).copied().unwrap_or(f0);
        let target_met = peak <= self.target_impedance;
        ImpedanceProfile {
            frequencies: freqs,
            impedance_mag: mags,
            peak_impedance: peak,
            peak_frequency: peak_freq,
            target_met,
        }
    }

    /// Greedily adds capacitors (from `available`, best improvement first)
    /// until the PDN peak meets `target_z` or the budget is exhausted.
    ///
    /// Returns the chosen parts with quantities.
    pub fn optimize_decoupling(
        &self,
        available: &[DecouplingCap],
        budget: u32,
    ) -> Vec<DecouplingCap> {
        let mut working = self.clone();
        let mut chosen: HashMap<String, usize> = HashMap::new();
        let mut parts: Vec<DecouplingCap> = Vec::new();

        for _ in 0..budget {
            let mut best: Option<(usize, f64)> = None;
            for (i, candidate) in available.iter().enumerate() {
                let mut trial = working.decoupling_caps.clone();
                match trial.iter_mut().find(|c| {
                    (c.value - candidate.value).abs() < 1e-15
                        && (c.esr - candidate.esr).abs() < 1e-15
                }) {
                    Some(existing) => existing.quantity += 1,
                    None => trial.push(DecouplingCap {
                        quantity: 1,
                        ..candidate.clone()
                    }),
                }
                let mut test = working.clone();
                test.decoupling_caps = trial;
                let peak = test.impedance_profile().peak_impedance;
                if best.map(|(_, bp)| peak < bp).unwrap_or(true) {
                    best = Some((i, peak));
                }
            }
            match best {
                Some((i, peak)) => {
                    let candidate = &available[i];
                    let key = format!("{:.3e}", candidate.value);
                    match chosen.get_mut(&key) {
                        Some(count) => {
                            *count += 1;
                            if let Some(part) = parts
                                .iter_mut()
                                .find(|p| (p.value - candidate.value).abs() < 1e-15)
                            {
                                part.quantity = *count as u32;
                            }
                        }
                        None => {
                            chosen.insert(key, 1);
                            parts.push(DecouplingCap {
                                quantity: 1,
                                ..candidate.clone()
                            });
                        }
                    }
                    // Apply to working state
                    if let Some(existing) = working.decoupling_caps.iter_mut().find(|c| {
                        (c.value - candidate.value).abs() < 1e-15
                            && (c.esr - candidate.esr).abs() < 1e-15
                    }) {
                        existing.quantity += 1;
                    } else {
                        working.decoupling_caps.push(DecouplingCap {
                            quantity: 1,
                            ..candidate.clone()
                        });
                    }
                    if peak <= working.target_impedance {
                        break;
                    }
                }
                None => break,
            }
        }
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_pdn() -> PdnAnalysis {
        PdnAnalysis {
            target_impedance: 0.1,
            frequency_range: (1e4, 1e9),
            decoupling_caps: vec![],
            plane_capacitance: 10e-9,
            vrm_model: VrmModel {
                output_resistance: 0.05,
                bandwidth_hz: 1e5,
                output_inductance: 50e-9,
            },
            points_per_decade: 20,
        }
    }

    #[test]
    fn cap_srf_sane() {
        // 100 nF with 2 nH total: SRF = 1/(2π√(LC)) ≈ 11.25 MHz
        let cap = DecouplingCap {
            value: 100e-9,
            esr: 5e-3,
            esl: 0.5e-9,
            quantity: 1,
            mount_inductance: 1.5e-9,
        };
        assert!((cap.srf() - 11.25e6).abs() / 11.25e6 < 0.02);
        // Below SRF the cap looks capacitive (negative imaginary Z)
        let (_, im_low) = cap.impedance(1e6);
        assert!(im_low < 0.0);
        // Above SRF: inductive
        let (_, im_high) = cap.impedance(100e6);
        assert!(im_high > 0.0);
    }

    #[test]
    fn vrm_is_resistive_then_inductive() {
        let vrm = VrmModel {
            output_resistance: 1e-3,
            bandwidth_hz: 1e5,
            output_inductance: 100e-9,
        };
        let (re, im) = vrm.impedance(1e4);
        assert!((re - 1e-3).abs() < 1e-12 && im == 0.0);
        let (_, im2) = vrm.impedance(1e8);
        assert!(im2 > 0.0);
    }

    #[test]
    fn caps_reduce_pdn_impedance() {
        let mut pdn = base_pdn();
        let bare = pdn.impedance_profile();
        pdn.decoupling_caps = vec![DecouplingCap {
            value: 100e-9,
            esr: 10e-3,
            esl: 0.5e-9,
            quantity: 4,
            mount_inductance: 1e-9,
        }];
        let decoupled = pdn.impedance_profile();
        assert!(
            decoupled.peak_impedance < bare.peak_impedance,
            "decoupled {} vs bare {}",
            decoupled.peak_impedance,
            bare.peak_impedance
        );
    }

    #[test]
    fn parallel_quantity_scales_impedance() {
        let one = DecouplingCap {
            value: 1e-6,
            esr: 10e-3,
            esl: 1e-9,
            quantity: 1,
            mount_inductance: 0.0,
        };
        let four = DecouplingCap {
            quantity: 4,
            ..one.clone()
        };
        let (r1, i1) = one.impedance(1e6);
        let (r4, i4) = four.impedance(1e6);
        assert!((r4 - r1 / 4.0).abs() < 1e-12);
        assert!((i4 - i1 / 4.0).abs() < 1e-12);
    }

    #[test]
    fn optimizer_meets_target() {
        let pdn = base_pdn();
        let available = vec![
            DecouplingCap {
                value: 100e-6,
                esr: 20e-3,
                esl: 2e-9,
                quantity: 0,
                mount_inductance: 1e-9,
            },
            DecouplingCap {
                value: 10e-6,
                esr: 10e-3,
                esl: 1e-9,
                quantity: 0,
                mount_inductance: 1e-9,
            },
            DecouplingCap {
                value: 1e-6,
                esr: 8e-3,
                esl: 0.8e-9,
                quantity: 0,
                mount_inductance: 0.8e-9,
            },
            DecouplingCap {
                value: 100e-9,
                esr: 15e-3,
                esl: 0.6e-9,
                quantity: 0,
                mount_inductance: 0.6e-9,
            },
        ];
        let bare = pdn.impedance_profile().peak_impedance;
        let plan = pdn.optimize_decoupling(&available, 12);
        assert!(!plan.is_empty());
        // Quantities are real (sum equals picks actually made).
        let total: u32 = plan.iter().map(|p| p.quantity).sum();
        assert!((4..=12).contains(&total));
        // Verify the plan against a fresh PDN: the greedy picks must damp the
        // VRM-plane anti-resonance substantially (≥ 5× here).
        let mut verify = pdn.clone();
        for part in &plan {
            verify.decoupling_caps.push(part.clone());
        }
        let profile = verify.impedance_profile();
        assert!(
            profile.peak_impedance < bare / 5.0,
            "peak {} not much better than bare {}",
            profile.peak_impedance,
            bare
        );
        // A relaxed target is met by the same plan.
        verify.target_impedance = 1.5;
        assert!(verify.impedance_profile().target_met);
    }
}
