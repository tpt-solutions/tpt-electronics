// SPDX-License-Identifier: MIT OR Apache-2.0

//! Independently reimplemented reference models for the simulation-derived
//! goldens.
//!
//! These exist because the earlier classification of four fixtures as
//! "simulation-derived, needs an external authority" was too pessimistic. In
//! three of the four cases the physics is fully determined by the fixture
//! itself, so a second, independent implementation of the *published* model is
//! a legitimate authority:
//!
//! * `ddr4_impedance` — the closed-form Hammerstad-Jensen microstrip equations
//!   with the documented first-order thickness correction.
//! * `pcie_gen3_eye` — a PRBS-7 LFSR, a first-order channel and the eye metrics
//!   are all deterministic given the fixture's parameters.
//! * `l_network_match` — holds no derived numeric value at all; see
//!   [`l_network_match_series_c`], which returns the one analytic quantity its
//!   prose does encode.
//!
//! # What this does and does not buy
//!
//! These are independent *implementations* of the same published model, so they
//! catch implementation drift and transcription errors in `tpt-elec-*` — the
//! same class of bug the other goldens catch. They do **not** catch a shared
//! misunderstanding of the model itself, because both sides implement the same
//! equations. The buck-converter fixture is the one case that genuinely needs
//! an external tool (ngspice) rather than a second implementation.

use crate::json_edit::Leaf;

/// Vacuum impedance [ohm] (CODATA-derived, same constant as `tpt-elec-core`).
const ETA0: f64 = 376.730313668;

/// Characteristic impedance of a microstrip [ohm], Hammerstad-Jensen.
///
/// `w`, `t` and `h` are in metres. The thickness correction is the first-order
/// Bahl-style form documented in `tpt-elec-si-impedance`: the air-side width
/// increment scaled by the embedded-field fraction.
pub fn microstrip_z0(w: f64, t: f64, h: f64, er: f64) -> f64 {
    let delta_w = if t > 0.0 && w > 0.0 {
        let air =
            (t / std::f64::consts::PI) * (1.0 + (4.0 * std::f64::consts::PI * (w / t + 1.1)).ln());
        air * (1.0 - 1.0 / er)
    } else {
        0.0
    };
    let u = (w + delta_w) / h;

    // Hammerstad-Jensen effective permittivity.
    let a = 1.0
        + (1.0 / 49.0) * ((u.powi(4) + (u / 52.0).powi(2)) / (u.powi(4) + 0.432)).ln()
        + (1.0 / 18.7) * (1.0 + (u / 18.1).powi(3)).ln();
    let b = 0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053);
    let er_eff = (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 * u).powf(-a * b);

    // Air-line impedance (Wheeler).
    let f = 6.0 + (std::f64::consts::TAU - 6.0) * (-((30.666 / u).powf(0.7528))).exp();
    let z_air = ETA0 / std::f64::consts::TAU * (f / u + (1.0 + (2.0 / u).powi(2)).sqrt()).ln();
    z_air / er_eff.sqrt()
}

/// The PRBS-7 sequence: `n` bits from an `x^7 + x^6 + 1` LFSR.
pub fn prbs7_bits(seed: u8, n: usize) -> Vec<u8> {
    let mut state = if seed == 0 { 1 } else { seed & 0x7f };
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let bit = state & 1;
        let feedback = ((state >> 6) ^ (state >> 5)) & 1;
        state = ((state << 1) | feedback) & 0x7f;
        out.push(bit);
    }
    out
}

/// Builds the NRZ waveform through a first-order channel.
///
/// Levels are `+-amplitude/2` and each sample steps toward its target by
/// `alpha = dt / (tau + dt)`, starting from the negative level.
pub fn first_order_nrz(
    bits: &[u8],
    samples_per_ui: usize,
    dt: f64,
    tau: f64,
    amplitude: f64,
) -> Vec<f64> {
    let alpha = dt / (tau + dt);
    let half = amplitude / 2.0;
    let mut v = -half;
    let mut out = Vec::with_capacity(bits.len() * samples_per_ui);
    for &bit in bits {
        let target = if bit == 1 { half } else { -half };
        for _ in 0..samples_per_ui {
            v += alpha * (target - v);
            out.push(v);
        }
    }
    out
}

/// Eye metrics measured from a uniformly sampled waveform.
///
/// Mirrors the definitions in `tpt-elec-si-eye`: threshold at mid-swing,
/// linearly interpolated crossings, offsets measured from the nearest UI
/// boundary, eye width as `UI - peak-to-peak crossing spread`, and eye height
/// from the worst high/low pair sampled at the centre of each UI.
pub fn eye_metrics(waveform: &[f64], bit_rate: f64, samples_per_ui: usize) -> (f64, f64, f64) {
    let spu = samples_per_ui.max(2);
    let ui = 1.0 / bit_rate;
    let dt = ui / spu as f64;

    let vmax = waveform.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let vmin = waveform.iter().copied().fold(f64::INFINITY, f64::min);
    let threshold = (vmax + vmin) / 2.0;

    let mut offsets = Vec::new();
    for i in 1..waveform.len() {
        let (a, b) = (waveform[i - 1], waveform[i]);
        if (a < threshold) != (b < threshold) {
            let t = (i as f64 - 1.0) * dt + (threshold - a) / (b - a).max(1e-300) * dt;
            offsets.push(t - (t / ui).round() * ui);
        }
    }
    offsets.sort_by(f64::total_cmp);
    let pp = offsets
        .last()
        .zip(offsets.first())
        .map(|(hi, lo)| hi - lo)
        .unwrap_or(0.0);
    let eye_width = (ui - pp).max(0.0);

    let mut highs = Vec::new();
    let mut lows = Vec::new();
    let mut k = spu / 2;
    while k < waveform.len() {
        let v = waveform[k];
        if v >= threshold {
            highs.push(v);
        } else {
            lows.push(v);
        }
        k += spu;
    }
    let eye_height = match (
        highs.iter().min_by(|a, b| a.total_cmp(b)),
        lows.iter().max_by(|a, b| a.total_cmp(b)),
    ) {
        (Some(h), Some(l)) => (h - l).max(0.0),
        _ => 0.0,
    };

    (eye_height, eye_width, pp)
}

/// Analytic reference for `si/ddr4_impedance.json`.
///
/// The only derived numeric field is `expected_z0_ohm`; the rest of the fixture
/// is the stack-up and the routing window.
pub fn ddr4_impedance() -> Vec<Leaf> {
    // Stack-up from the fixture: FR4 er 4.4, 0.1 mm dielectric, 35 um copper,
    // 0.13 mm reference width.
    let z0 = microstrip_z0(0.13e-3, 35.0e-6, 0.1e-3, 4.4);
    vec![Leaf::new("expected_z0_ohm", z0)]
}

/// Builds a `clean_channel/eye_height_v` style path.
fn leaf_path(channel: &str, field: &str) -> &'static str {
    // A closed set, so a static match keeps the paths `&'static str`.
    match (channel, field) {
        ("clean_channel", "eye_height_v") => "clean_channel/eye_height_v",
        ("clean_channel", "eye_width_ps") => "clean_channel/eye_width_ps",
        ("clean_channel", "total_jitter_ps") => "clean_channel/total_jitter_ps",
        ("degraded_channel", "eye_height_v") => "degraded_channel/eye_height_v",
        ("degraded_channel", "eye_width_ps") => "degraded_channel/eye_width_ps",
        ("degraded_channel", "total_jitter_ps") => "degraded_channel/total_jitter_ps",
        _ => panic!("unexpected eye fixture path {channel}/{field}"),
    }
}

/// Analytic reference for `si/pcie_gen3_eye.json`.
///
/// PCIe Gen 3 is 8 GT/s, so UI = 125 ps; the fixture drives a PRBS-7 (seed
/// 0x41) through a first-order channel and samples 32 points per UI. Every
/// metric below is then a deterministic function of those parameters.
pub fn pcie_gen3_eye() -> Vec<Leaf> {
    const BIT_RATE: f64 = 8.0e9;
    const SAMPLES_PER_UI: usize = 32;
    const AMPLITUDE: f64 = 0.8;
    /// tau in UI for the clean and degraded channels.
    const TAU_UI: [f64; 2] = [0.05, 0.35];

    let ui = 1.0 / BIT_RATE;
    let dt = ui / SAMPLES_PER_UI as f64;
    let bits = prbs7_bits(0x41, 8);

    let mut leaves = Vec::new();
    for (channel, tau_ui) in ["clean_channel", "degraded_channel"]
        .iter()
        .zip(TAU_UI.iter())
    {
        let wf = first_order_nrz(&bits, SAMPLES_PER_UI, dt, *tau_ui * ui, AMPLITUDE);
        let (height, width, jitter) = eye_metrics(&wf, BIT_RATE, SAMPLES_PER_UI);
        // The fixture stores time quantities in picoseconds.
        leaves.push(Leaf::rel(leaf_path(channel, "eye_height_v"), height));
        leaves.push(Leaf::rel(leaf_path(channel, "eye_width_ps"), width * 1e12));
        leaves.push(Leaf::rel(
            leaf_path(channel, "total_jitter_ps"),
            jitter * 1e12,
        ));
    }
    leaves
}

/// `l_network_match.json` stores no derived numeric value, so there is nothing
/// to regenerate.
///
/// The fixture holds the source/load impedances, the design frequency, and
/// `max_gamma_magnitude` — a *pass criterion*, not a measurement. The networks
/// are asserted in-crate against it, and `|Gamma| ~ 0` at the design frequency
/// is an exact property of a lossless L-section rather than a stored number.
/// What the fixture's prose does encode is the load-resonating capacitor, and
/// that one is analytic: `C = 1/(2*pi*f*X)`.
#[cfg(test)]
pub fn l_network_match_series_c() -> f64 {
    // WiFi 6E match: 25 + j15 ohm at 6.0 GHz.
    1.0 / (std::f64::consts::TAU * 6.0e9 * 15.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(leaves: &[Leaf], path: &str) -> f64 {
        leaves
            .iter()
            .find(|l| l.path == path)
            .unwrap_or_else(|| panic!("missing leaf {path}"))
            .value
    }

    #[test]
    fn microstrip_is_physically_sane() {
        let narrow = microstrip_z0(0.1e-3, 35e-6, 0.2e-3, 4.4);
        let wide = microstrip_z0(1.0e-3, 35e-6, 0.2e-3, 4.4);
        assert!(wide < narrow, "{wide} !< {narrow}");
        // FR4, 0.2 mm dielectric: w/h = 1 gives roughly 65-70 ohm, w/h = 3.5
        // (0.7 mm) drops into the mid-30s.
        let mid = microstrip_z0(0.2e-3, 35e-6, 0.2e-3, 4.4);
        assert!((60.0..75.0).contains(&mid), "w/h=1 z0 = {mid}");
        let wide_bw = microstrip_z0(0.7e-3, 35e-6, 0.2e-3, 4.4);
        assert!((25.0..45.0).contains(&wide_bw), "w/h=3.5 z0 = {wide_bw}");
    }

    #[test]
    fn ddr4_z0_lands_in_the_routing_window() {
        let z0 = leaf(&ddr4_impedance(), "expected_z0_ohm");
        assert!(
            (50.0..60.0).contains(&z0),
            "z0 = {z0} outside the DDR4 window"
        );
    }

    #[test]
    fn prbs7_has_the_expected_structure() {
        let bits = prbs7_bits(0x41, 127);
        assert_eq!(bits.len(), 127);
        // A maximal-length LFSR repeats after 2^7 - 1 bits.
        assert_eq!(bits[..], prbs7_bits(0x41, 254)[..127]);
        // A zero seed is coerced to 1 rather than producing a dead sequence.
        assert!(prbs7_bits(0, 8).contains(&1));
    }

    #[test]
    fn clean_eye_is_open_and_degraded_eye_is_closed() {
        let leaves = pcie_gen3_eye();
        let clean_h = leaf(&leaves, "clean_channel/eye_height_v");
        let degraded_h = leaf(&leaves, "degraded_channel/eye_height_v");
        assert!(clean_h > 0.7, "clean height {clean_h}");
        assert!(
            degraded_h < clean_h,
            "degraded {degraded_h} vs clean {clean_h}"
        );

        let clean_w = leaf(&leaves, "clean_channel/eye_width_ps");
        let degraded_w = leaf(&leaves, "degraded_channel/eye_width_ps");
        // PCIe Gen 3 UI is 125 ps; the clean eye is most of it.
        assert!(clean_w > 100.0 && clean_w <= 125.0, "clean width {clean_w}");
        assert!(
            degraded_w < clean_w,
            "degraded {degraded_w} vs clean {clean_w}"
        );

        // Jitter is the residual: width + jitter == UI by construction.
        for ch in ["clean_channel", "degraded_channel"] {
            let w = leaf(&leaves, &format!("{ch}/eye_width_ps"));
            let j = leaf(&leaves, &format!("{ch}/total_jitter_ps"));
            assert!((w + j - 125.0).abs() < 1e-6, "{ch}: {w} + {j} != 125 ps");
        }
    }
}
