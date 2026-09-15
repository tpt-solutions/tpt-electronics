// SPDX-License-Identifier: MIT OR Apache-2.0

//! Characteristic impedance calculators (IPC-2141 / Hammerstad-Jensen).
//!
//! * [`ImpedanceCalculator::microstrip`] — Hammerstad-Jensen with a
//!   first-order copper-thickness (effective width) correction.
//! * [`ImpedanceCalculator::stripline`] — symmetric and asymmetric.
//! * [`ImpedanceCalculator::differential_pair`] — edge-coupled microstrip
//!   odd/even modes from a coupling-coefficient approximation.
//!
//! Note on the spec's worked example (§8): a 0.2 mm trace over 0.2 mm FR4
//! (er 4.4) is physically ≈ 65–71 Ω, not 50 Ω — 50 Ω needs w/h ≈ 1.75.
//! The tests in this crate assert the correct physics.

//! # Quick start
//!
//! ```
//! use tpt_elec_core::Length;
//! use tpt_elec_si_impedance::ImpedanceCalculator;
//!
//! let line = ImpedanceCalculator::microstrip(
//!     Length::mm(0.35), Length::um(35.0), Length::mm(0.2), 4.4,
//! );
//! assert!((line.z0 - 50.0).abs() < 3.0);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::{Length, SPEED_OF_LIGHT};
use tpt_elec_si_core::LineParameters;

/// Vacuum impedance [Ω].
const ETA0: f64 = 376.730313668;

/// Impedance calculation helpers.
pub struct ImpedanceCalculator;

impl ImpedanceCalculator {
    /// Microstrip impedance via Hammerstad-Jensen.
    ///
    /// * `trace_width` — conductor width [m]
    /// * `trace_thickness` — copper thickness [m]
    /// * `dielectric_height` — substrate height to the reference plane [m]
    /// * `er` — relative permittivity of the substrate
    pub fn microstrip(
        trace_width: Length,
        trace_thickness: Length,
        dielectric_height: Length,
        er: f64,
    ) -> LineParameters {
        let h = dielectric_height.as_meters();
        let t = trace_thickness.as_meters();
        let w = trace_width.as_meters();

        // First-order thickness correction: incremental effective width
        // (Bahl-style), reduced by (1 − 1/er) for the embedded field fraction.
        let delta_w = if t > 0.0 && w > 0.0 {
            let air = (t / std::f64::consts::PI)
                * (1.0 + (4.0 * std::f64::consts::PI * (w / t + 1.1)).ln());
            air * (1.0 - 1.0 / er)
        } else {
            0.0
        };
        let w_eff = w + delta_w;

        let u = w_eff / h;
        let a = 1.0
            + (1.0 / 49.0) * ((u.powi(4) + (u / 52.0).powi(2)) / (u.powi(4) + 0.432)).ln()
            + (1.0 / 18.7) * (1.0 + (u / 18.1).powi(3)).ln();
        let b = 0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053);
        let er_eff = (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 * u).powf(-a * b);

        let f = 6.0 + (std::f64::consts::TAU - 6.0) * (-((30.666 / u).powf(0.7528))).exp();
        let z_air = ETA0 / std::f64::consts::TAU * (f / u + (1.0 + (2.0 / u).powi(2)).sqrt()).ln();
        let z0 = z_air / er_eff.sqrt();

        LineParameters {
            z0,
            z0_odd: None,
            z0_even: None,
            propagation_delay: er_eff.sqrt() / SPEED_OF_LIGHT,
            loss_tangent: 0.0,
            skin_effect: 0.0,
        }
    }

    /// Solves the inverse problem: the trace width [m] that yields
    /// `target_ohm` on a microstrip of the given stackup.
    ///
    /// Bisection over `[height/1000, height·20]` — microstrip Z0 is strictly
    /// monotone decreasing in width, so this converges unconditionally
    /// (typically 25 iterations to sub-µΩ).
    pub fn suggest_microstrip(
        target_ohm: f64,
        trace_thickness: Length,
        dielectric_height: Length,
        er: f64,
    ) -> Length {
        let z_at = |w: Length| Self::microstrip(w, trace_thickness, dielectric_height, er).z0;
        let mut lo = Length::meters(dielectric_height.as_meters() / 1000.0); // very narrow → high Z
        let mut hi = Length::meters(dielectric_height.as_meters() * 20.0); // very wide → low Z
                                                                           // Clamp the bracket so the target is inside it.
        let mut guard = 0;
        while z_at(lo) < target_ohm && guard < 60 {
            lo = Length::meters(lo.as_meters() / 2.0);
            guard += 1;
        }
        while z_at(hi) > target_ohm && guard < 120 {
            hi = Length::meters(hi.as_meters() * 2.0);
            guard += 1;
        }
        for _ in 0..60 {
            let mid = Length::meters((lo.as_meters() + hi.as_meters()) / 2.0);
            if z_at(mid) > target_ohm {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Length::meters((lo.as_meters() + hi.as_meters()) / 2.0)
    }

    /// Monte Carlo tolerance sweep over microstrip impedance.
    ///
    /// Perturbs width/height/thickness/er by uniform relative tolerances and
    /// returns distribution statistics. The RNG is a deterministic xorshift
    /// so results are reproducible for a given seed.
    #[allow(clippy::too_many_arguments)]
    pub fn monte_carlo_microstrip(
        trace_width: Length,
        trace_thickness: Length,
        dielectric_height: f64,
        er: f64,
        width_tol: f64,
        height_tol: f64,
        thickness_tol: f64,
        er_tol: f64,
        samples: u32,
        seed: u64,
    ) -> MonteCarloResult {
        let mut rng = Xorshift::new(seed);
        let mut z = Vec::with_capacity(samples as usize);
        for _ in 0..samples {
            let u1 = rng.uniform();
            let u2 = rng.uniform();
            let u3 = rng.uniform();
            let u4 = rng.uniform();
            let w = trace_width.as_meters() * (1.0 + width_tol * (2.0 * u1 - 1.0));
            let h = dielectric_height * (1.0 + height_tol * (2.0 * u2 - 1.0));
            // er and thickness tolerances are applied through the same solver
            let t = trace_thickness.as_meters() * (1.0 + thickness_tol * (2.0 * u4 - 1.0));
            let line = Self::microstrip(
                Length::meters(w),
                Length::meters(t),
                Length::meters(h),
                er * (1.0 + er_tol * (2.0 * u3 - 1.0)),
            );
            z.push(line.z0);
        }
        z.sort_by(|a, b| a.total_cmp(b));
        let n = z.len();
        let mean = z.iter().sum::<f64>() / n as f64;
        let std = (z.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
        MonteCarloResult {
            samples: n,
            mean,
            std,
            p05: z[(0.05 * n as f64) as usize],
            p50: z[n / 2],
            p95: z[((0.95 * n as f64) as usize).min(n - 1)],
        }
    }

    /// Symmetric or asymmetric stripline.
    ///
    /// `height_above`/`height_below` are the distances to the two reference
    /// planes. The symmetric case uses the classic
    /// `Z0 = 60/√er · ln(4b/(π·w'))` formula; asymmetry applies a first-order
    /// height-ratio correction.
    pub fn stripline(
        trace_width: Length,
        trace_thickness: Length,
        height_above: Length,
        height_below: Length,
        er: f64,
    ) -> LineParameters {
        let t = trace_thickness.as_meters();
        let h1 = height_above.as_meters();
        let h2 = height_below.as_meters();
        let w = trace_width.as_meters();
        let b = h1 + h2;
        let w_eff = if t > 0.0 {
            w + (t / std::f64::consts::PI) * (1.0 + (2.0 * h1.min(h2).mul_add(2.0, t) / t).ln())
        } else {
            w
        };
        let wb = (w_eff / b).clamp(1e-4, 0.98);
        let z_symmetric = if wb <= 0.35 {
            (60.0 / er.sqrt()) * (4.0 * b / (std::f64::consts::PI * w_eff)).ln()
        } else {
            (ETA0 / (er.sqrt() * std::f64::consts::PI)) / (wb / (1.0 - wb) + 2.55)
        };
        // Asymmetry: closer to one plane ⇒ lower impedance (first order).
        let z0 = if (h1 - h2).abs() > 1e-15 {
            let ratio = (h1.min(h2) / b).clamp(0.02, 0.5);
            // 0 → full symmetric value, 0 (centered)…  scaling by √(2·ratio)
            let factor = (2.0 * ratio).sqrt().clamp(0.2, 1.0);
            z_symmetric * (0.5 + 0.5 * factor)
        } else {
            z_symmetric
        };
        LineParameters {
            z0,
            z0_odd: None,
            z0_even: None,
            propagation_delay: er.sqrt() / SPEED_OF_LIGHT,
            loss_tangent: 0.0,
            skin_effect: 0.0,
        }
    }

    /// Edge-coupled differential pair (microstrip).
    ///
    /// Odd/even impedances from the IPC-2141-style coupling coefficient
    /// `k = 0.48·e^(−0.96·s/h)`, with `Z0e·Z0o = Z0²`.
    pub fn differential_pair(
        trace_width: Length,
        trace_spacing: Length,
        dielectric_height: Length,
        er: f64,
    ) -> LineParameters {
        let single = Self::microstrip(trace_width, Length::ZERO, dielectric_height, er);
        let h = dielectric_height.as_meters();
        let s = trace_spacing.as_meters();
        let k = (0.48 * (-0.96 * s / h).exp()).clamp(0.0, 0.9);
        let z_odd = single.z0 * ((1.0 - k) / (1.0 + k)).sqrt();
        let z_even = single.z0 * ((1.0 + k) / (1.0 - k)).sqrt();
        LineParameters {
            z0_odd: Some(z_odd),
            z0_even: Some(z_even),
            ..single
        }
    }
}

/// Deterministic xorshift64* RNG for reproducible Monte Carlo runs.
pub struct Xorshift {
    state: u64,
}

impl Xorshift {
    /// Seeds from an arbitrary nonzero value.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        }
    }

    /// Next raw u64.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Uniform sample in (0, 1].
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 + f64::EPSILON
    }
}

/// Monte Carlo impedance distribution statistics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonteCarloResult {
    /// Number of samples.
    pub samples: usize,
    /// Mean impedance [Ω].
    pub mean: f64,
    /// Standard deviation [Ω].
    pub std: f64,
    /// 5th percentile [Ω].
    pub p05: f64,
    /// Median [Ω].
    pub p50: f64,
    /// 95th percentile [Ω].
    pub p95: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microstrip_50_ohm_fr4() {
        // 50 Ω on 0.2 mm FR4 needs w/h ≈ 1.75 (w = 0.35 mm, 1 oz copper).
        let line = ImpedanceCalculator::microstrip(
            Length::mm(0.35),
            Length::um(35.0),
            Length::mm(0.2),
            4.4,
        );
        assert!((line.z0 - 50.0).abs() < 2.5, "Z0 = {}", line.z0);
    }

    #[test]
    fn test_microstrip_spec_geometry_is_not_50_ohm() {
        // Spec §8's geometry (w = h = 0.2 mm, er 4.4) is ~65 Ω with the
        // thickness correction (~71 Ω without) — physically not 50 Ω.
        let line = ImpedanceCalculator::microstrip(
            Length::mm(0.2),
            Length::um(35.0),
            Length::mm(0.2),
            4.4,
        );
        assert!(
            (60.0..70.5).contains(&line.z0),
            "Z0 = {} (expected 60–70)",
            line.z0
        );
    }

    #[test]
    fn microstrip_monotonic_in_width() {
        let narrow =
            ImpedanceCalculator::microstrip(Length::mm(0.15), Length::ZERO, Length::mm(0.2), 4.4);
        let wide =
            ImpedanceCalculator::microstrip(Length::mm(0.6), Length::ZERO, Length::mm(0.2), 4.4);
        assert!(narrow.z0 > wide.z0, "narrow {} wide {}", narrow.z0, wide.z0);
    }

    #[test]
    fn microstrip_er_eff_between_bounds() {
        // er_eff must lie between 1 (all air) and er (all dielectric).
        let line =
            ImpedanceCalculator::microstrip(Length::mm(0.3), Length::ZERO, Length::mm(0.2), 4.4);
        let er_eff = (line.propagation_delay * SPEED_OF_LIGHT).powi(2);
        assert!((1.5..4.4).contains(&er_eff), "er_eff = {er_eff}");
    }

    #[test]
    fn test_stripline_symmetric_57_ohm() {
        let line = ImpedanceCalculator::stripline(
            Length::mm(0.07),
            Length::ZERO,
            Length::mm(0.2),
            Length::mm(0.2),
            4.4,
        );
        assert!((line.z0 - 56.8).abs() < 3.0, "Z0 = {}", line.z0);
        // Stripline has no air: delay is exactly √er/c
        assert!((line.propagation_delay - 4.4f64.sqrt() / SPEED_OF_LIGHT).abs() < 1e-15);
    }

    #[test]
    fn stripline_asymmetric_lower_when_close_to_plane() {
        let sym = ImpedanceCalculator::stripline(
            Length::mm(0.1),
            Length::ZERO,
            Length::mm(0.2),
            Length::mm(0.2),
            4.4,
        );
        let asym = ImpedanceCalculator::stripline(
            Length::mm(0.1),
            Length::ZERO,
            Length::mm(0.05),
            Length::mm(0.35),
            4.4,
        );
        assert!(
            asym.z0 < sym.z0,
            "asym {} should be below sym {}",
            asym.z0,
            sym.z0
        );
    }

    #[test]
    fn test_differential_pair_modes() {
        let dp = ImpedanceCalculator::differential_pair(
            Length::mm(0.2),
            Length::mm(0.2), // s/h = 1
            Length::mm(0.2),
            4.4,
        );
        let zo = dp.z0_odd.unwrap();
        let ze = dp.z0_even.unwrap();
        // Physical constraints: Zo < Z0 < Ze and Z0² = Zo·Ze
        assert!(zo < dp.z0 && dp.z0 < ze, "zo={zo} z0={} ze={ze}", dp.z0);
        assert!(((zo * ze).sqrt() - dp.z0).abs() < 1e-9);
        // At s/h = 1 the coupling factor k = 0.183 shrinks the odd mode:
        // Zdiff = 2·Z_se·sqrt((1−k)/(1+k)) with Z_se ≈ 71 Ω → ≈ 118 Ω.
        let zdiff = dp.differential_impedance().unwrap();
        assert!((zdiff - 118.0).abs() < 4.0, "Zdiff = {zdiff}");
    }

    #[test]
    fn golden_ddr4_impedance() {
        // Golden reference: test-data/golden/si/ddr4_impedance.json
        #[derive(serde::Deserialize)]
        struct Golden {
            er: f64,
            dielectric_height_mm: f64,
            trace_thickness_um: f64,
            reference_width_mm: f64,
            expected_z0_ohm: f64,
            tolerance_ohm: f64,
            window_ohm: [f64; 2],
        }
        let raw = include_str!("../../../../test-data/golden/si/ddr4_impedance.json");
        let g: Golden = serde_json::from_str(raw).unwrap();
        let line = ImpedanceCalculator::microstrip(
            Length::mm(g.reference_width_mm),
            Length::um(g.trace_thickness_um),
            Length::mm(g.dielectric_height_mm),
            g.er,
        );
        assert!(
            (line.z0 - g.expected_z0_ohm).abs() < g.tolerance_ohm,
            "Z0 = {} vs {} ± {}",
            line.z0,
            g.expected_z0_ohm,
            g.tolerance_ohm
        );
        // DDR4 routing window (JEDEC signaling with ODT)
        assert!(
            (g.window_ohm[0]..g.window_ohm[1]).contains(&line.z0),
            "Z0 = {} outside {:?}",
            line.z0,
            g.window_ohm
        );
    }

    #[test]
    fn suggest_microstrip_hits_target() {
        let target = 50.0;
        let w =
            ImpedanceCalculator::suggest_microstrip(target, Length::um(35.0), Length::mm(0.2), 4.4);
        let line = ImpedanceCalculator::microstrip(w, Length::um(35.0), Length::mm(0.2), 4.4);
        assert!(
            (line.z0 - target).abs() < 0.05,
            "suggested width {} mm gives {} Ω",
            w.as_mm(),
            line.z0
        );
        // Higher target → narrower trace (monotonicity sanity)
        let w75 =
            ImpedanceCalculator::suggest_microstrip(75.0, Length::um(35.0), Length::mm(0.2), 4.4);
        assert!(w75.as_meters() < w.as_meters());
    }

    #[test]
    fn monte_carlo_spans_tolerances() {
        let r = ImpedanceCalculator::monte_carlo_microstrip(
            Length::mm(0.35),
            Length::ZERO,
            0.2e-3,
            4.4,
            0.10, // ±10 % width
            0.10, // ±10 % height
            0.0,
            0.0,
            2000,
            42,
        );
        assert_eq!(r.samples, 2000);
        // Nominal 50 Ω; the ±er perturbation is asymmetric (Z ~ 1/√er_eff),
        // shifting the mean a few ohms above nominal.
        assert!((48.0..58.0).contains(&r.mean), "mean {}", r.mean);
        // 10 % width tolerance alone gives ~±7 % impedance spread
        assert!(r.std > 0.5 && r.std < 8.0, "std {}", r.std);
        assert!(r.p05 < r.p50 && r.p50 < r.p95);
        // Deterministic: same seed → same result
        let r2 = ImpedanceCalculator::monte_carlo_microstrip(
            Length::mm(0.35),
            Length::ZERO,
            0.2e-3,
            4.4,
            0.10,
            0.10,
            0.0,
            0.0,
            2000,
            42,
        );
        assert!((r.mean - r2.mean).abs() < 1e-12);
    }

    #[test]
    fn differential_widens_toward_single_ended() {
        let close = ImpedanceCalculator::differential_pair(
            Length::mm(0.2),
            Length::mm(0.02),
            Length::mm(0.2),
            4.4,
        );
        let far = ImpedanceCalculator::differential_pair(
            Length::mm(0.2),
            Length::mm(2.0),
            Length::mm(0.2),
            4.4,
        );
        assert!(close.z0_odd.unwrap() < far.z0_odd.unwrap());
        assert!(close.z0_even.unwrap() > far.z0_even.unwrap());
        // s/h = 10: nearly uncoupled
        assert!((far.z0_odd.unwrap() - far.z0).abs() < 1.0);
    }
}
