// SPDX-License-Identifier: MIT OR Apache-2.0

//! Closed-form reference models for the golden fixtures.
//!
//! These deliberately do **not** call the workspace crates. A golden file is
//! only worth keeping if its expected numbers come from an authority that is
//! independent of the implementation under test; regenerating from
//! `tpt-elec-*` would just record whatever the code happens to do today and
//! would silently bless any regression.
//!
//! Every model here is an exact analytic identity, and each carries the
//! derivation in its doc comment so a reviewer can check it by hand.

use crate::json_edit::Leaf;

/// A 10-node 1-D column pinned at 100 °C (node 0) and 0 °C (node 9).
///
/// The steady state of `d²T/dx² = 0` with Dirichlet ends is exactly linear, so
/// node `k` is `T_hot + (T_cold - T_hot) * k / (N-1)`. The golden reports the
/// value at the cell the crate reads as `temperatures[5]`, i.e. node 5:
/// `100 - 100 * 5/9 = 44.444…` °C.
const SLAB_CELLS: usize = 10;
const SLAB_T_HOT: f64 = 100.0;
const SLAB_T_COLD: f64 = 0.0;
const SLAB_CENTER_INDEX: usize = 5;

/// A 1 W source on a 1 mm³ copper cell (1 mm² face) with `h = 1000` against
/// a 25 °C ambient: `T = T_inf + P/(h·A) = 25 + 1/(1000 · 1e-6) = 1025` °C.
const LUMPED_T_AMBIENT: f64 = 25.0;
const LUMPED_POWER_W: f64 = 1.0;
const LUMPED_H: f64 = 1000.0;
const LUMPED_AREA_M2: f64 = 1.0e-6;

/// Butterworth prototype `g_k = 2·sin((2k-1)π/(2n))` with `g0 = gn1 = 1`.
const BW_ORDER: usize = 5;
const BW_CUTOFF_HZ: f64 = 1.0e8;
const BW_Z0: f64 = 50.0;
/// Probe frequencies already used as keys by the golden.
const BW_PROBES_MHZ: [f64; 5] = [10.0, 50.0, 100.0, 200.0, 1000.0];

/// Analytic reference for `rf/butterworth_filter.json`.
///
/// The prototype is `g_k = 2·sin((2k-1)π/(2n))` with `g0 = gn1 = 1`. A
/// shunt-first low-pass ladder maps `g_k` to `C = g_k/(ωc·Z0)` on shunt arms
/// and `L = g_k·Z0/ωc` on series arms, which is exactly how the crate
/// denormalises. The insertion loss of a doubly terminated Butterworth ladder
/// is the identity `|H|² = 1/(1 + (f/fc)^{2n})`, i.e.
/// `IL = 10·log10(1 + (f/fc)^{2n})` dB.
pub fn butterworth_filter() -> Vec<Leaf> {
    let omega_c = std::f64::consts::TAU * BW_CUTOFF_HZ;
    let n = BW_ORDER as f64;

    let mut leaves = Vec::new();
    let mut shunt = Vec::new();
    let mut series = Vec::new();
    for k in 1..=BW_ORDER {
        let g = 2.0 * ((2.0 * k as f64 - 1.0) * std::f64::consts::PI / (2.0 * n)).sin();
        if k % 2 == 1 {
            shunt.push(g / (omega_c * BW_Z0));
        } else {
            series.push(g * BW_Z0 / omega_c);
        }
    }
    for (i, c) in shunt.iter().enumerate() {
        leaves.push(Leaf::rel(shunt_path(i), *c));
    }
    for (i, l) in series.iter().enumerate() {
        leaves.push(Leaf::rel(series_path(i), *l));
    }
    for f_mhz in BW_PROBES_MHZ {
        let ratio = f_mhz * 1.0e6 / BW_CUTOFF_HZ;
        let il = 10.0 * (1.0 + ratio.powf(2.0 * n)).log10();
        leaves.push(Leaf::new(il_path(f_mhz), -il));
    }
    leaves
}

/// Index of a shunt capacitor in `ladder.shunt_C_F`.
fn shunt_path(i: usize) -> &'static str {
    match i {
        0 => "ladder/shunt_C_F[0]",
        1 => "ladder/shunt_C_F[1]",
        2 => "ladder/shunt_C_F[2]",
        _ => panic!("a 5th-order Butterworth has 3 shunt arms, asked for {i}"),
    }
}

/// Index of a series inductor in `ladder.series_L_H`.
fn series_path(i: usize) -> &'static str {
    match i {
        0 => "ladder/series_L_H[0]",
        1 => "ladder/series_L_H[1]",
        _ => panic!("a 5th-order Butterworth has 2 series arms, asked for {i}"),
    }
}

/// Key of an insertion-loss sample in `insertion_loss_db`.
///
/// Written as `if`/`else` rather than a `match` on the frequency: float
/// literals are not valid patterns before recent Rust, and this crate has to
/// build on the workspace MSRV.
fn il_path(f_mhz: f64) -> &'static str {
    if f_mhz == 10.0 {
        "insertion_loss_db/10.0_MHz"
    } else if f_mhz == 50.0 {
        "insertion_loss_db/50.0_MHz"
    } else if f_mhz == 100.0 {
        "insertion_loss_db/100.0_MHz"
    } else if f_mhz == 200.0 {
        "insertion_loss_db/200.0_MHz"
    } else if f_mhz == 1000.0 {
        "insertion_loss_db/1000.0_MHz"
    } else {
        panic!("unexpected probe frequency {f_mhz}")
    }
}

pub fn simple_resistor_board() -> Vec<Leaf> {
    let last = (SLAB_CELLS - 1) as f64;
    let center = SLAB_T_HOT + (SLAB_T_COLD - SLAB_T_HOT) * SLAB_CENTER_INDEX as f64 / last;
    let lumped = LUMPED_T_AMBIENT + LUMPED_POWER_W / (LUMPED_H * LUMPED_AREA_M2);
    vec![
        Leaf::new("values/slab_max_temp_c", SLAB_T_HOT),
        Leaf::new("values/slab_center_temp_c", center),
        Leaf::new("values/lumped_convection_temp_c", lumped),
    ]
}

/// One registered golden case.
pub struct Case {
    /// Stable case identifier, matching the `case` field in the file.
    pub name: &'static str,
    /// Path relative to the workspace root.
    pub path: &'static str,
    /// Where the expected numbers come from.
    pub basis: &'static str,
    /// Recomputed numeric leaves.
    pub leaves: Vec<Leaf>,
}

/// Every golden this task can regenerate from an independent authority.
pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "simple_resistor_board",
            path: "test-data/golden/thermal/simple_resistor_board.json",
            basis: "analytic 1-D conduction and lumped convection T = Tinf + P/(hA)",
            leaves: simple_resistor_board(),
        },
        Case {
            name: "butterworth_filter",
            path: "test-data/golden/rf/butterworth_filter.json",
            basis: "analytic Butterworth prototype g_k and |H|^2 = 1/(1+(f/fc)^(2n))",
            leaves: butterworth_filter(),
        },
        Case {
            name: "rc_lowpass_ac",
            path: "test-data/golden/spice/rc_lowpass_ac.json",
            basis: "parameters-only fixture; the test derives the analytic reference itself",
            leaves: Vec::new(),
        },
        Case {
            name: "ddr4_impedance",
            path: "test-data/golden/si/ddr4_impedance.json",
            basis: "independent Hammerstad-Jensen implementation in `refmodel`",
            leaves: crate::refmodel::ddr4_impedance(),
        },
        Case {
            name: "pcie_gen3_eye",
            path: "test-data/golden/si/pcie_gen3_eye.json",
            basis: "independent PRBS-7 + first-order channel + eye metrics in `refmodel`",
            leaves: crate::refmodel::pcie_gen3_eye(),
        },
    ]
}

/// Goldens that hold **no derived numeric value**, so there is nothing to
/// regenerate. They are inputs plus pass criteria, verified in-crate.
pub fn specification_only() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![(
        "l_network_match",
        "test-data/golden/rf/l_network_match.json",
        "max_gamma_magnitude is a pass criterion, not a measurement; |Gamma| ~ 0 at f0 \
         is an exact property of a lossless L-section",
    )]
}

/// Goldens whose reference numbers were produced by an **external** tool rather
/// than by a model in this workspace.
///
/// These are not regenerable here — `xtask` has no Zolotarev implementation, and
/// writing one would only be a second implementation of our own maths. But they
/// are not blocked either: the values are frozen, and the in-crate test compares
/// the design against them case for case, so drift is still caught.
pub fn externally_referenced() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![(
        "elliptic_pole_zero",
        "test-data/golden/rf/elliptic_pole_zero.json",
        "pole/zero sets from scipy.signal.ellipap 1.16.2, an implementation \
         independent of this workspace; checked in-crate to 1e-9 and separately \
         against the equiripple definition",
    )]
}

/// Goldens that are genuinely blocked and **never** rewritten.
pub fn simulation_derived() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![(
        "buck_converter_transient",
        "test-data/golden/spice/buck_converter_transient.json",
        "startup waveform is a nonlinear SPICE transient; needs a cross-check against an \
         external solver (ngspice/LTspice), which a second implementation of our own \
         model cannot provide",
    )]
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
    fn butterworth_ladder_matches_hand_computed_values() {
        // g1 = 2 sin(pi/10) -> C = g1/(w_c Z0); g2 = 2 sin(3pi/10) -> L = g2 Z0/w_c.
        let w_c = std::f64::consts::TAU * 1.0e8;
        let g1 = 2.0 * (std::f64::consts::PI / 10.0).sin();
        let g2 = 2.0 * (3.0 * std::f64::consts::PI / 10.0).sin();
        let leaves = butterworth_filter();
        assert!((leaf(&leaves, "ladder/shunt_C_F[0]") - g1 / (w_c * 50.0)).abs() < 1e-18);
        assert!((leaf(&leaves, "ladder/series_L_H[0]") - g2 * 50.0 / w_c).abs() < 1e-18);
        // g3 = 2 sin(pi/2) = 2 is the middle shunt arm.
        assert!((leaf(&leaves, "ladder/shunt_C_F[1]") - 2.0 / (w_c * 50.0)).abs() < 1e-18);
    }

    #[test]
    fn butterworth_is_three_db_at_the_cutoff() {
        let leaves = butterworth_filter();
        let il = leaf(&leaves, "insertion_loss_db/100.0_MHz");
        assert!((il + 3.010_299_956_64).abs() < 1e-9, "{il}");
        assert!(leaf(&leaves, "insertion_loss_db/10.0_MHz").abs() < 1e-3);
        assert!(leaf(&leaves, "insertion_loss_db/1000.0_MHz") < -99.0);
    }

    #[test]
    fn lumped_convection_is_exact() {
        let leaves = simple_resistor_board();
        let lumped = leaf(&leaves, "values/lumped_convection_temp_c");
        // 25 + 1/(1000 * 1e-6); allow one ulp of float slop.
        assert!((lumped - 1025.0).abs() < 1e-9, "{lumped}");
        let center = leaf(&leaves, "values/slab_center_temp_c");
        assert!((center - 44.444_444_444_4).abs() < 1e-9, "{center}");
    }

    #[test]
    fn case_and_leaf_names_are_unique() {
        let mut names: Vec<&str> = cases().iter().map(|c| c.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate case names");
        for c in cases() {
            let mut paths: Vec<&str> = c.leaves.iter().map(|l| l.path).collect();
            paths.sort_unstable();
            let n = paths.len();
            paths.dedup();
            assert_eq!(paths.len(), n, "duplicate leaf path in {}", c.name);
        }
    }

    #[test]
    fn every_golden_is_classified() {
        // Guards against a new golden appearing without deciding how to treat it.
        let known: Vec<&str> = cases()
            .iter()
            .map(|c| c.name)
            .chain(specification_only().iter().map(|(n, _, _)| *n))
            .chain(externally_referenced().iter().map(|(n, _, _)| *n))
            .chain(simulation_derived().iter().map(|(n, _, _)| *n))
            .collect();
        assert_eq!(known.len(), 8, "expected 8 goldens, classified: {known:?}");

        // And every classified name must actually have a file on disk, so a
        // renamed or moved fixture cannot be silently dropped from the check.
        for (name, path, _) in specification_only()
            .into_iter()
            .chain(externally_referenced())
            .chain(simulation_derived())
        {
            let full = crate::workspace_root().expect("root").join(path);
            assert!(full.is_file(), "{name} classified but {path} is missing");
        }
    }

    #[test]
    fn l_network_match_series_c_is_analytic() {
        // C = 1/(2*pi*f*X) for the fixture's 25 + j15 ohm load at 6 GHz.
        let c = crate::refmodel::l_network_match_series_c();
        // The fixture's prose records "series C 1.768e-12 F".
        assert!((c - 1.768e-12).abs() < 1e-15, "{c}");
    }
}
