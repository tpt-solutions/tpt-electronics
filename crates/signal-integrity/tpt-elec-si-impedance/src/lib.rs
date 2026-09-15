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
