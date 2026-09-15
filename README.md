# tpt-electronics

A fully open-source, MIT-licensed multiphysics simulation engine for electronic
systems, written in pure Rust.

`tpt-electronics` solves the problems that kill hardware projects — thermal
runaway, electromigration, signal-integrity failures, power-delivery droop, EMC
certification failures, and battery-pack safety — using standard EDA inputs
(Gerber, ODB++, IPC-2581, KiCad, Touchstone, SPICE) and zero proprietary or
GPL-contaminated dependencies.

**No proprietary cores. No open-core bait. No GPL traps. Just open source.**

## Highlights

- **100% Rust, zero runtime dependencies** — the numeric stack (dense/sparse
  linear algebra, Conjugate Gradient, complex arithmetic) lives in
  `tpt-elec-core`; nothing else is pulled in.
- **Standards-driven** — validated against JEDEC JESD51 (thermal), IPC-2141 /
  IPC-2152 / IPC-2221 (boards), CISPR 32 / FCC Part 15 / IEC 61000 (EMC),
  IEEE 1149.1 (test), UN GTR 20 / IEC 62660 (battery).
- **WASM-ready** — compile the whole engine to WebAssembly for browser EDA
  tools, VS Code extensions, or CI pipelines. No server round-trips.
- **License-clean** — every crate is `MIT OR Apache-2.0`; `cargo-deny` enforces
  a permissive-only dependency graph.

## Crates

| Crate | Description | Status |
|---|---|---|
| `tpt-elec-core` | Core domain types, units, linear algebra | ✅ Stable |
| `tpt-elec-materials` | Material property database | ✅ Stable |
| `tpt-elec-geometry` | Traces, pads, voxels, copper areas | ✅ Stable |
| `tpt-elec-gerber` | Gerber RS-274X + Excellon parser | ✅ Stable |
| `tpt-elec-kicad` | KiCad `.kicad_pcb` / netlist parser | ✅ Stable |
| `tpt-elec-odbpp` | ODB++ reader | 🚧 Alpha |
| `tpt-elec-ipc2581` | IPC-2581-C XML parser | 🚧 Alpha |
| `tpt-elec-touchstone` | Touchstone S-parameter parser | ✅ Stable |
| `tpt-elec-spice-netlist` | SPICE netlist parser | ✅ Stable |
| `tpt-elec-thermal` | Steady-state thermal FEM | ✅ Stable |
| `tpt-elec-convection` | Natural/forced convection | ✅ Stable |
| `tpt-elec-transient` | Transient thermal integration | ✅ Stable |
| `tpt-elec-joule` | Electro-thermal (Joule heating) | ✅ Stable |
| `tpt-elec-spice-core` | Circuit graph + MNA | ✅ Stable |
| `tpt-elec-spice-models` | MOSFET/diode/BJT models | ✅ Stable |
| `tpt-elec-spice-analysis` | DC/AC/transient/noise | ✅ Stable |
| `tpt-elec-spice-noise` | Noise source models | 🚧 Alpha |
| `tpt-elec-si-core` | Transmission lines, S-parameters | ✅ Stable |
| `tpt-elec-si-impedance` | Microstrip/stripline/diff-pair | ✅ Stable |
| `tpt-elec-si-crosstalk` | NEXT/FEXT models | 🚧 Alpha |
| `tpt-elec-si-eye` | Eye diagrams + mask compliance | ✅ Stable |
| `tpt-elec-pi-pdn` | PDN impedance analysis | ✅ Stable |
| `tpt-elec-pi-decoupling` | Decap selection/placement | 🚧 Alpha |
| `tpt-elec-rf-core` | Smith chart, L/pi matching | ✅ Stable |
| `tpt-elec-rf-filters` | Filter synthesis | ✅ Stable |
| `tpt-elec-rf-antenna` | Antennas, link budgets | ✅ Stable |
| `tpt-elec-rf-mixer` | Mixer models | 🚧 Alpha |
| `tpt-elec-rf-links` | RF chain composition | 🚧 Alpha |
| `tpt-elec-power-core` | Converter topologies/design | ✅ Stable |
| `tpt-elec-power-magnetics` | Cores, windings, Steinmetz | ✅ Stable |
| `tpt-elec-power-switches` | Switch/diode losses | ✅ Stable |
| `tpt-elec-power-control` | Loops, Bode, compensation | ✅ Stable |
| `tpt-elec-power-battery` | Battery-aware power delivery | 🚧 Alpha |
| `tpt-elec-semi-core` | Semiconductor fundamentals | 🚧 Alpha |
| `tpt-elec-semi-mosfet` | MOSFET compact models | 🚧 Alpha |
| `tpt-elec-semi-diode` | Diode compact models | 🚧 Alpha |
| `tpt-elec-semi-bjt` | BJT Ebers-Moll model | 🚧 Alpha |
| `tpt-elec-semi-process` | PDK abstractions | 🚧 Alpha |
| `tpt-elec-emc-core` | EMC standards + limit lines | ✅ Stable |
| `tpt-elec-emc-emissions` | Emissions prediction | ✅ Stable |
| `tpt-elec-emc-immunity` | Immunity test modeling | 🚧 Alpha |
| `tpt-elec-emc-shielding` | Shielding effectiveness | ✅ Stable |
| `tpt-elec-emc-grounding` | Grounding / return paths | 🚧 Alpha |
| `tpt-elec-battery-core` | Cell + ECM models | ✅ Stable |
| `tpt-elec-battery-thermal` | Thermal runaway propagation | 🚧 Alpha |
| `tpt-elec-battery-bms` | SOC estimation (KF/EKF/UKF) | ✅ Stable |
| `tpt-elec-battery-pack` | Pack composition | ✅ Stable |
| `tpt-elec-mfg-dfm` | DRC engine + IPC-2152 | ✅ Stable |
| `tpt-elec-mfg-test` | IEEE 1149.1 boundary scan | 🚧 Alpha |
| `tpt-elec-mfg-yield` | Yield prediction | ✅ Stable |
| `tpt-elec-wasm` | WebAssembly bindings | 🚧 Alpha |
| `tpt-elec-cli` | Gerber → thermal CSV CLI | ✅ Stable |

Status legend: ✅ Stable · 🚧 Alpha · 📋 Planned

## Quick Start

Thermal analysis from a Gerber file:

```rust
use tpt_elec_gerber::GerberParser;
use tpt_elec_thermal::ThermalSolver;
use tpt_elec_materials::MaterialDatabase;

let gerber = GerberParser::parse(include_str!("board.gtl")).unwrap();
let materials = MaterialDatabase::standard();
let solver = ThermalSolver::from_gerber(&gerber, &materials, 1e-3);
let result = solver.solve_steady_state();
println!("Max temperature: {:.1} °C", result.max_temp);
```

50 Ω microstrip on 1.6 mm FR4:

```rust
use tpt_elec_si_impedance::ImpedanceCalculator;

let line = ImpedanceCalculator::microstrip(3.0e-4, 3.5e-5, 1.6e-3, 4.4);
println!("Z0 = {:.1} Ω", line.z0);
```

SPICE in pure Rust:

```rust
use tpt_elec_spice_netlist::SpiceNetlistParser;
use tpt_elec_spice_analysis::SpiceAnalyzer;

let circuit = SpiceNetlistParser::parse("R1 in out 1k\nC1 out 0 1n\n...").unwrap();
let analyzer = SpiceAnalyzer::new(circuit);
let ac = analyzer.ac_analysis(1.0, 1.0e9, 10).unwrap();
```

## CLI

```console
$ tpt-elec-cli thermal --gerber top.gtl --stackup stackup.json \
    --power-map power.csv --out thermal.csv
```

## Governance

- **License:** MIT OR Apache-2.0 (dual)
- **Contributions:** MIT OR Apache-2.0, CLA-free via DCO (`git commit -s`)
- **Governance:** Benevolent Dictator + [RFC process](rfcs/)
- **Releases:** SemVer, 6-week minor cadence
- **Security:** [SECURITY.md](SECURITY.md), private disclosure

## Development

```console
$ cargo build --workspace
$ cargo test --workspace
$ cargo fmt --all -- --check
$ cargo clippy --workspace
$ cargo deny check licenses
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
