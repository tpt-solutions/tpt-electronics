// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decoupling capacitor selection and placement helpers, building on
//! `tpt-elec-pi-pdn`.
//!
//! * [`recommended_ladder`] — the classic per-decade capacitor ladder
//!   (bulk → MLCC) for a rail with a target impedance.
//! * [`mount_inductance`] — effective mounting inductance from pad/via
//!   geometry (loop inductance grows with escape distance).
//! * [`check_anti_resonance`] — locates parallel-resonance peaks between
//!   capacitor pairs and reports the worst one in band.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_pi_pdn::{DecouplingCap, PdnAnalysis, VrmModel};

/// Recommended capacitor ladder for a rail.
///
/// One decade of bulk capacitance per decade of frequency below the MLCC
/// corner, with `n_decades` MLCC steps of 10× down from `mlcc_value`.
/// Returns parts with `quantity = 1` — scale per rail current.
pub fn recommended_ladder(
    target_impedance: f64,
    corner_frequency: f64,
    n_decades: u32,
) -> Vec<DecouplingCap> {
    // C = 1/(2π·f·Z) at the corner, then a decade ladder below.
    let c_bulk = 1.0 / (std::f64::consts::TAU * corner_frequency * target_impedance.max(1e-9));
    let mut ladder = vec![DecouplingCap {
        value: c_bulk,
        esr: 0.02,
        esl: 2.0e-9,
        quantity: 1,
        mount_inductance: 1.0e-9,
    }];
    let mut value = 10e-6;
    for _ in 0..n_decades {
        ladder.push(DecouplingCap {
            value,
            esr: 0.01,
            esl: 0.5e-9,
            quantity: 2,
            mount_inductance: 0.5e-9,
        });
        value /= 10.0;
    }
    ladder
}

/// Effective mounting inductance [H] for a capacitor placement.
///
/// Loop model: the current path through pad, escape trace, and via barrel.
/// `escape_distance` is the distance from the pad to the via [m];
/// inductance grows roughly linearly with it.
pub fn mount_inductance(pad_length_m: f64, escape_distance_m: f64, via_height_m: f64) -> f64 {
    // ~1 nH/mm of total loop length (order-of-magnitude PCB rule).
    let loop_m = 2.0 * pad_length_m + 2.0 * escape_distance_m + 2.0 * via_height_m;
    1.0e-9 * (loop_m * 1e3)
}

/// Anti-resonance check between capacitor banks.
///
/// Returns the worst peak |Z| [Ω] in band and the frequency where the
/// smaller bank's inductive region meets the larger bank's capacitive
/// region (parallel resonance).
pub fn check_anti_resonance(caps: &[DecouplingCap], band: (f64, f64), points: usize) -> (f64, f64) {
    let pdn = PdnAnalysis {
        target_impedance: f64::INFINITY,
        frequency_range: band,
        decoupling_caps: caps.to_vec(),
        plane_capacitance: 0.0,
        vrm_model: VrmModel {
            output_resistance: f64::INFINITY,
            bandwidth_hz: 0.0,
            output_inductance: 0.0,
        },
        points_per_decade: points.max(2) as u32,
    };
    // With the VRM branch removed (infinite impedance), the profile is the
    // pure cap network. Compute via impedance_at minus the VRM contribution
    // by using a standalone sweep here.
    let mut worst = (0.0f64, band.0);
    for k in 0..=points.max(2) {
        let f = band.0 * (band.1 / band.0).powf(k as f64 / points.max(2) as f64);
        let mut y_re = 0.0;
        let mut y_im = 0.0;
        for cap in caps {
            let (zr, zi) = cap.impedance(f);
            let m2 = zr * zr + zi * zi;
            if m2 > 0.0 {
                y_re += zr / m2;
                y_im -= zi / m2;
            }
        }
        let m2 = y_re * y_re + y_im * y_im;
        let z = 1.0 / m2.sqrt();
        if z > worst.0 {
            worst = (z, f);
        }
    }
    let _ = pdn;
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_decades() {
        let ladder = recommended_ladder(0.01, 1e6, 3);
        // Bulk: C = 1/(2π·1e6·0.01) = 15.9 µF
        assert!((ladder[0].value - 15.9e-6).abs() / 15.9e-6 < 0.01);
        // MLCC decades 10 µF, 1 µF, 100 nF
        assert!((ladder[1].value - 10e-6).abs() < 1e-12);
        assert!((ladder[2].value - 1e-6).abs() < 1e-12);
        assert!((ladder[3].value - 100e-9).abs() < 1e-12);
        assert_eq!(ladder.len(), 4);
    }

    #[test]
    fn mount_inductance_grows_with_distance() {
        let near = mount_inductance(1e-3, 0.5e-3, 1.5e-3);
        let far = mount_inductance(1e-3, 5e-3, 1.5e-3);
        assert!(far > near);
        // loop = 2·(1 + 0.5 + 1.5) mm = 6 mm → 6 nH
        assert!((near - 6.0e-9).abs() < 0.1e-9);
    }

    #[test]
    fn anti_resonance_detected_between_decades() {
        // A 1 µF bank (inductive above ~5 MHz) against a 10 nF bank creates
        // a parallel resonance between their SRFs.
        let caps = vec![
            DecouplingCap {
                value: 1e-6,
                esr: 5e-3,
                esl: 1e-9,
                quantity: 1,
                mount_inductance: 0.0,
            },
            DecouplingCap {
                value: 10e-9,
                esr: 20e-3,
                esl: 0.5e-9,
                quantity: 1,
                mount_inductance: 0.0,
            },
        ];
        let (peak, f_peak) = check_anti_resonance(&caps, (1e6, 1e9), 40);
        // Individual SRFs: 50 MHz and 2.25 GHz — the peak sits above 50 MHz.
        assert!(peak > 1.0, "peak {peak}");
        assert!(f_peak > 40e6, "f {f_peak}");
    }

    #[test]
    fn single_cap_no_antiresonance() {
        let caps = vec![DecouplingCap {
            value: 10e-6,
            esr: 10e-3,
            esl: 1e-9,
            quantity: 1,
            mount_inductance: 0.0,
        }];
        let (peak, f_peak) = check_anti_resonance(&caps, (1e3, 1e9), 30);
        // A single cap has no inter-bank resonance: the band maximum is the
        // plain capacitive reactance at the low edge (1/(2π·1kHz·10µF) ≈ 15.9 Ω).
        assert!(
            (peak - 15.9).abs() < 0.5 && f_peak < 2e3,
            "peak {peak} at {f_peak}"
        );
    }
}
