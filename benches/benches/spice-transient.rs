// SPDX-License-Identifier: MIT OR Apache-2.0

//! SPICE transient benchmark: the buck converter netlist over 50 µs.

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_elec_spice_analysis::SpiceAnalyzer;
use tpt_elec_spice_netlist::SpiceNetlistParser;

const BUCK: &str = "\
.model PMOS PMOS (LEVEL=1 VTO=0.7 KP=110u LAMBDA=0.01)
.model DW D (IS=1e-12 N=1 RS=0.05)
Vin in 0 DC 12
Vg g 0 PULSE(12 0 0 2n 2n 4.98u 10u)
M1 sw g in in PMOS w=10000u l=1u
D1 0 sw DW
Csnub sw 0 1n
L1 sw out 10u
C1 out 0 100u
RL out 0 6
";

fn bench_spice(c: &mut Criterion) {
    let source = format!("{BUCK}\n.tran 20n 50u\n.end\n");
    c.bench_function("spice_transient_buck_50us", |b| {
        b.iter(|| {
            let circuit = SpiceNetlistParser::parse(&source).unwrap();
            SpiceAnalyzer::new(circuit)
                .transient(50e-6, 20e-9, 5.0)
                .unwrap()
        })
    });
}

criterion_group!(benches, bench_spice);
criterion_main!(benches);
