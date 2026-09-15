// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: automated buck converter design (Phase 4 milestone).
//!
//! Designs a 12 V → 3.3 V, 3 A buck with `tpt-elec-power-core`, sizes the
//! inductor with `tpt-elec-power-magnetics`, then simulates the same
//! topology netlist with `tpt-elec-spice-analysis` and compares the average
//! output voltage against D·Vin.

use tpt_elec_power_core::BuckDesigner;
use tpt_elec_power_magnetics::{CoreLossCalculator, CoreMaterial, CoreType, MagneticCore};

fn main() {
    let (vin, vout, iout, fsw) = (12.0f64, 3.3f64, 3.0f64, 400e3f64);

    // 1. First-pass design.
    let design = BuckDesigner::design(vin, vout, iout, fsw, 0.3, 0.012, 0.45).unwrap();
    println!(
        "buck design {} V → {} V @ {} A, {} kHz:",
        vin,
        vout,
        iout,
        fsw / 1e3
    );
    println!("  duty: {:.3}", BuckDesigner::duty_cycle(vin, vout));
    println!("  L: {:.2} µH", design.components.inductance * 1e6);
    println!("  ripple: {:.2} A peak-peak", design.inductor_ripple);
    println!("  peak current: {:.2} A", design.peak_inductor_current);
    println!("  C_out: {:.0} µF", design.components.output_cap * 1e6);
    println!("  efficiency estimate: {:.1} %", design.efficiency * 100.0);

    // 2. Magnetics: build that inductor on a ferrite E core.
    let core = MagneticCore {
        core_type: CoreType::ECore { size: "E25".into() },
        material: CoreMaterial::Ferrite {
            material: "3F3".into(),
        },
        al_value: 1800.0,
        ae: 52.0e-6, // 52 mm² as m²
        le: 57.0e-3,
        ve: 3.0e-6,
    };
    let inductor = tpt_elec_power_magnetics::Inductor::for_inductance(
        core.clone(),
        design.components.inductance,
        design.peak_inductor_current,
        Length::mm(0.8),
        Length::mm(30.0),
    );
    println!(
        "  inductor: {} turns, {:.1} µH, DCR {:.0} mΩ, I_sat {:.1} A",
        inductor.winding.turns,
        inductor.inductance * 1e6,
        inductor.dcr * 1e3,
        inductor.saturation_current
    );
    // Core loss at the ripple frequency with ΔB/2 from the ripple current.
    let b_peak = inductor.peak_flux(design.inductor_ripple / 2.0);
    println!(
        "  core loss: {:.1} mW/cm³ at {:.0} kHz, B_pk {:.1} mT",
        CoreLossCalculator::steinmetz(fsw, b_peak, &core.material) / 1e3,
        fsw / 1e3,
        b_peak * 1e3
    );

    // 3. SPICE cross-check: same converter as a netlist.
    let netlist = "\
* 12->3.3 V buck, 400 kHz, asynchronous PMOS
.model PMOS PMOS (LEVEL=1 VTO=0.7 KP=110u LAMBDA=0.01)
.model DW D (IS=1e-12 N=1 RS=0.05)
Vin in 0 DC 12
Vg g 0 PULSE(12 0 0 5n 5n 2.44u 2.5u)
M1 sw g in in PMOS w=20000u l=1u
D1 0 sw DW
Csnub sw 0 1n
L1 sw out 7u
C1 out 0 470u
RL out 0 1.1
.tran 20n 30u
.end
";
    let circuit = tpt_elec_spice_netlist::SpiceNetlistParser::parse(netlist).unwrap();
    let tr = tpt_elec_spice_analysis::SpiceAnalyzer::new(circuit)
        .transient(30e-6, 25e-9, 5.0)
        .expect("buck transient");
    // node 4 = "out"; sample the last quarter of the run
    let n = tr.times.len();
    let avg: f64 = (3 * n / 4..n).map(|i| tr.node_voltages[i][4]).sum::<f64>() / (n / 4) as f64;
    println!(
        "  SPICE transient (still charging, 30 µs): v_out ≈ {:.2} V → settles to D·V_in = {:.2} V",
        avg,
        BuckDesigner::duty_cycle(vin, vout) * vin
    );
}

use tpt_elec_core::Length;
