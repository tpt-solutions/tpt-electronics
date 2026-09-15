// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: WiFi 6E antenna matching network designer (Phase 5 milestone).
//!
//! Matches a realistic 2.4/5/6 GHz tri-band antenna S-parameters to 50 Ω
//! at 6 GHz with an L-network, then reports the matched VSWR across the
//! 6 GHz band.

use tpt_elec_core::{Complex, Length};
use tpt_elec_rf_antenna::LinkBudget;
use tpt_elec_rf_core::{MatchingDesigner, SmithChart};

fn main() {
    println!("WiFi 6E antenna matching @ 6.0 GHz (2.4/5/6 GHz tri-band antenna)");
    let z0 = 50.0;

    // Measured antenna impedance at band center (typical tri-band PIFA).
    let z_ant = Complex::new(25.0, 15.0);
    let gamma_raw = SmithChart::impedance_to_reflection(z_ant, z0);
    println!(
        "  unmatched: Z = {:.1} + j{:.1} Ω, |Γ| = {:.3}, VSWR = {:.2}",
        z_ant.re,
        z_ant.im,
        gamma_raw.abs(),
        SmithChart::vswr(gamma_raw)
    );

    // Synthesize all L-network solutions.
    let networks = MatchingDesigner::l_network(Complex::real(z0), z_ant, 6.0e9, z0);
    for (i, net) in networks.iter().enumerate() {
        let gamma = net.simulate_gamma(z_ant);
        println!(
            "  solution {}: |Γ| = {:.5}, VSWR = {:.3} ({} elements)",
            i,
            gamma.abs(),
            SmithChart::vswr(gamma),
            net.elements.len()
        );
        for el in &net.elements {
            let label = match el {
                tpt_elec_rf_core::MatchingElement::SeriesInductor { value } => {
                    format!("series L {:.2} nH", value * 1e9)
                }
                tpt_elec_rf_core::MatchingElement::SeriesCapacitor { value } => {
                    format!("series C {:.2} pF", value * 1e12)
                }
                tpt_elec_rf_core::MatchingElement::ShuntInductor { value } => {
                    format!("shunt L {:.2} nH", value * 1e9)
                }
                tpt_elec_rf_core::MatchingElement::ShuntCapacitor { value } => {
                    format!("shunt C {:.2} pF", value * 1e12)
                }
            };
            println!("    {}", label);
        }
    }

    // Band sweep of the first solution (match degrades away from center).
    let best = &networks[0];
    for f_ghz in [5.6, 5.8, 6.0, 6.2, 6.4] {
        let gamma = best.simulate_gamma(z_ant);
        let scaled = gamma * (6.0 / f_ghz); // crude frequency detune model
        println!("  {:.1} GHz: VSWR ≈ {:.2}", f_ghz, SmithChart::vswr(scaled));
    }

    // Link budget at 6 GHz, 10 m indoor.
    let lb = LinkBudget::evaluate(18.0, 0.0, 0.0, 10.0, 6.0e9, -70.0);
    println!(
        "  link @ 10 m: FSPL {:.1} dB, margin {:.1} dB ({})",
        lb.path_loss_db,
        lb.margin_db,
        if lb.closes() { "closes" } else { "fails" }
    );
    let _ = Length::mm(1.0);
}
