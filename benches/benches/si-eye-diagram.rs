// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eye diagram analysis benchmark: PRBS-7 through a channel, 32 samples/UI.

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_elec_si_eye::{EyeDiagram, Prbs};

fn build_waveform(bits: usize, spu: u32, tau_ui: f64) -> Vec<f64> {
    let ui = 125e-12;
    let dt = ui / spu as f64;
    let alpha = dt / (tau_ui * ui + dt);
    let mut prbs = Prbs::prbs7(0x41);
    let mut v = -0.4;
    let mut wf = Vec::with_capacity(bits * spu as usize);
    for _ in 0..bits {
        let bit = prbs.next_bit();
        let target = if bit == 1 { 0.4 } else { -0.4 };
        for _ in 0..spu {
            v += alpha * (target - v);
            wf.push(v);
        }
    }
    wf
}

fn bench_eye(c: &mut Criterion) {
    let wf = build_waveform(254, 32, 0.2);
    c.bench_function("si_eye_254bits_32spu", |b| {
        b.iter(|| EyeDiagram::from_waveform(&wf, 8e9, 32))
    });
}

criterion_group!(benches, bench_eye);
criterion_main!(benches);
