// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: PCIe Gen 3 eye diagram compliance checker (Phase 3 milestone).
//!
//! Drives a PRBS-7 through a first-order channel model, analyzes the eye,
//! and checks it against the PCIe Gen 3 mask at 8 GT/s.

use tpt_elec_si_eye::{EyeDiagram, EyeMask, Prbs};

fn main() {
    let bit_rate = 8.0e9; // Gen 3
    let ui = 1.0 / bit_rate;
    let spu = 32u32;

    println!("PCIe Gen 3 eye check (8 GT/s, UI = {:.1} ps)", ui * 1e12);

    for (label, tau_ui, launch_amplitude) in [
        ("short channel (τ = 0.05 UI)", 0.05, 0.8),
        ("long channel (τ = 0.20 UI)", 0.20, 0.8),
        ("marginal channel (τ = 0.35 UI)", 0.35, 0.8),
    ] {
        let eye = simulate(tau_ui, launch_amplitude, bit_rate, spu);
        let mask = eye.check_mask_compliance(&EyeMask::PcieGen3, launch_amplitude);
        println!(
            "{:<32} height {:5.0} mV | width {:6.1} ps | TJ {:5.1} ps | {}",
            label,
            eye.eye_height * 1e3,
            eye.eye_width * 1e12,
            eye.jitter.total_jitter * 1e12,
            if mask.passed {
                format!("PASS (margin {:.2} UI)", mask.margin_ui)
            } else {
                format!(
                    "FAIL ({:.2} UI, {:.0} mV short)",
                    mask.margin_ui,
                    -mask.margin_amplitude * 1e3
                )
            }
        );
    }
}

fn simulate(tau_ui: f64, amplitude: f64, bit_rate: f64, spu: u32) -> EyeDiagram {
    let ui = 1.0 / bit_rate;
    let dt = ui / spu as f64;
    let tau = tau_ui * ui;
    let alpha = dt / (tau + dt);

    let mut prbs = Prbs::prbs7(0x41);
    let mut v = -amplitude / 2.0;
    let mut waveform = Vec::new();
    // 16 repetitions of the 127-bit PRBS for a stable eye.
    for _ in 0..16 {
        let bit = prbs.next_bit();
        let target = if bit == 1 {
            amplitude / 2.0
        } else {
            -amplitude / 2.0
        };
        for _ in 0..spu {
            v += alpha * (target - v);
            waveform.push(v);
        }
    }
    EyeDiagram::from_waveform(&waveform, bit_rate, spu)
}
