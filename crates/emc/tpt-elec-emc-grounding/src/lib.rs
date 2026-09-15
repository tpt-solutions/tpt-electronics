// SPDX-License-Identifier: MIT OR Apache-2.0

//! Grounding and return-path modeling.
//!
//! * [`ground_plane_impedance`] — R + jωL of a ground path vs frequency
//!   (the inductive term dominates above the self-resonant corner).
//! * [`split_plane_gap_inductance`] — return-path inductance across a slot.
//! * [`ground_loop_coupling`] — transferred voltage between loops through
//!   mutual inductance.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Complex impedance of a ground connection (conductor/plane path) [Ω].
///
/// `Z(f) = R + jωL`; above the corner `R/L` the path is inductive and the
/// "ground" is no longer an equipotential.
pub fn ground_path_impedance(resistance_ohm: f64, inductance_h: f64, frequency: f64) -> (f64, f64) {
    let omega = std::f64::consts::TAU * frequency;
    (resistance_ohm, omega * inductance_h)
}

/// Corner frequency [Hz] above which inductance dominates: `R/(2πL)`.
pub fn inductive_corner_hz(resistance_ohm: f64, inductance_h: f64) -> f64 {
    resistance_ohm / (std::f64::consts::TAU * inductance_h.max(1e-15))
}

/// Estimated return-path inductance [H] when a signal crosses a slot/gap
/// in the reference plane.
///
/// Model: the return current detours around a slot of length `slot_length`
/// and width `slot_width`; inductance scales with the detour perimeter
/// (≈ 1 nH/mm of loop).
pub fn split_plane_gap_inductance(slot_length_m: f64, slot_width_m: f64) -> f64 {
    let detour_m = 2.0 * slot_length_m + slot_width_m;
    1.0e-9 * (detour_m * 1e3)
}

/// Voltage coupled into a victim loop through mutual inductance [V].
///
/// `V = M · di/dt` — the classic ground-loop / crosstalk mechanism.
pub fn ground_loop_coupling(mutual_inductance_h: f64, di_dt: f64) -> f64 {
    mutual_inductance_h * di_dt
}

/// Mutual inductance between two coplanar loops sharing a common ground run
/// (order-of-magnitude model: M grows with the shared run length).
pub fn mutual_inductance(shared_run_m: f64, separation_m: f64) -> f64 {
    // M ≈ µ0/(4π) · ln(1 + shared/separation) scaled per meter of shared run
    const MU0: f64 = 4.0e-7 * std::f64::consts::PI;
    let ratio = 1.0 + shared_run_m / separation_m.max(1e-6);
    MU0 * shared_run_m * ratio.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_path_is_resistive_at_dc() {
        let (r, im) = ground_path_impedance(0.01, 10e-9, 1e3);
        assert!((r - 0.01).abs() < 1e-12);
        assert!((im - 6.28e-5).abs() < 1e-7); // negligible at 1 kHz
    }

    #[test]
    fn inductive_beyond_corner() {
        let corner = inductive_corner_hz(0.01, 10e-9);
        // 0.01/(2π·10n) ≈ 159 kHz
        assert!((corner - 159e3).abs() / 159e3 < 0.01);
        // At 1000× the corner: ωL = 1000·R = 10 Ω of reactance vs 1 mΩ of R
        let (_, im) = ground_path_impedance(0.01, 10e-9, corner * 1000.0);
        assert!(im > 1.0);
    }

    #[test]
    fn slot_gap_inductance_scales_with_detour() {
        let small = split_plane_gap_inductance(2e-3, 0.2e-3);
        let large = split_plane_gap_inductance(20e-3, 0.2e-3);
        assert!(large > small * 5.0);
        // 4.2 mm detour → 4.2 nH
        assert!((small - 4.2e-9).abs() < 0.05e-9);
    }

    #[test]
    fn ground_loop_voltage() {
        // M = 10 nH, 1 A/ns edge → 10 V of coupled noise!
        let v = ground_loop_coupling(10e-9, 1e9);
        assert!((v - 10.0).abs() < 1e-9);
    }

    #[test]
    fn mutual_inductance_falls_with_separation() {
        let close = mutual_inductance(0.05, 1e-3);
        let far = mutual_inductance(0.05, 10e-3);
        assert!(close > far);
        assert!(close > 0.0 && far > 0.0);
    }
}
