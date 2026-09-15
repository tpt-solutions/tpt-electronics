// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    let line = tpt_elec_si_impedance::ImpedanceCalculator::microstrip(
        tpt_elec_core::Length::mm(0.35),
        tpt_elec_core::Length::um(35.0),
        tpt_elec_core::Length::mm(0.2),
        4.4,
    );
    println!("Z0 = {:.1} Ω", line.z0);

    let w = tpt_elec_si_impedance::ImpedanceCalculator::suggest_microstrip(
        50.0,
        tpt_elec_core::Length::um(35.0),
        tpt_elec_core::Length::mm(0.2),
        4.4,
    );
    println!("50 Ω microstrip width: {:.1} µm", w.as_um());
}
