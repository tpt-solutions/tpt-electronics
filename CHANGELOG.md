# Changelog

All notable changes to `tpt-electronics` are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project adheres to [Semantic Versioning](https://semver.org/).
Minor releases occur on a 6-week cadence.

## [Unreleased]

### Added

#### Phase 0 — Repository scaffolding
- Cargo workspace (`resolver = "2"`) with shared `[workspace.package]`,
  `[workspace.dependencies]`, and workspace lints (`unsafe_code = "forbid"`).
- Dual licensing: `LICENSE-MIT` + `LICENSE-APACHE` (MIT OR Apache-2.0).
- Community files: README, CONTRIBUTING (DCO, no CLA), SECURITY,
  CODE_OF_CONDUCT, CHANGELOG.
- License enforcement via `cargo-deny` (`deny.toml`).
- CI: fmt / clippy / test, cargo-deny licenses, criterion benchmarks,
  mdBook docs deployment, release automation.
- Issue templates (bug report, feature request, RFC) and PR template.

#### Phase 1 — Foundation
- `tpt-elec-core`: ID types, board stackup and component models, SI units,
  dense + sparse (CSR) linear algebra with Conjugate Gradient, complex numbers.
- `tpt-elec-materials`: material database with built-in copper, FR4
  (anisotropic), aluminum, SAC305, silicon, AlN, and more.
- `tpt-elec-geometry`: traces, pads, drills, copper areas, voxel grids.
- `tpt-elec-gerber`: RS-274X Gerber parser + Excellon drill parser.
- `tpt-elec-kicad`: `.kicad_pcb` S-expression parser and netlist parser.
- `tpt-elec-thermal`: steady-state thermal FEM (anisotropic tensors,
  Conjugate Gradient) with fixed-temperature / convection / radiation /
  heat-flux / heat-source boundary conditions.
- `tpt-elec-convection`: natural / forced convection correlations.
- `tpt-elec-transient`: implicit + explicit time integration.
- `tpt-elec-joule`: coupled electro-thermal solver.
- `tpt-elec-cli`: Gerber + power map → thermal CSV.

#### Phase 2 — Circuit simulation
- `tpt-elec-spice-core`: circuit graph, MNA formulation, analysis kinds.
- `tpt-elec-spice-models`: MOSFET (Level 1–3), diode, BJT, op-amp models.
- `tpt-elec-spice-analysis`: DC operating point (Newton-Raphson), AC sweep,
  transient (trapezoidal with adaptive stepping), noise analysis.
- `tpt-elec-spice-netlist`: SPICE netlist parser (R/L/C/M/D/V/I, .model,
  .tran, .ac, .subckt).
- `tpt-elec-spice-noise`: thermal / shot / flicker noise source models.
- Milestone: buck converter simulated end-to-end in pure Rust with a golden
  transient reference.

#### Phase 3 — Signal & power integrity
- `tpt-elec-touchstone`: Touchstone S-parameter file parser.
- `tpt-elec-si-core`: transmission lines and S-parameter data model.
- `tpt-elec-si-impedance`: Hammerstad-Jensen microstrip, symmetric/asymmetric
  stripline, differential pair (odd/even mode).
- `tpt-elec-si-crosstalk`: NEXT/FEXT coupled-line models.
- `tpt-elec-si-eye`: eye diagram analysis with jitter separation and standard
  masks (PCIe Gen1-6, DDR4/5, USB3/4, 10GbE).
- `tpt-elec-pi-pdn` / `tpt-elec-pi-decoupling`: PDN impedance profiling and
  decoupling optimization.
- Milestone: PCIe eye-diagram compliance checking; golden SI references.

#### Phase 4 — Power electronics
- `tpt-elec-power-core`: converter topologies and automated design.
- `tpt-elec-power-magnetics`: core loss (Steinmetz), inductor design.
- `tpt-elec-power-switches`: switching/conduction loss models.
- `tpt-elec-power-control`: Bode analysis, stability margins, Type II/III
  compensator synthesis (k-factor).
- `tpt-elec-power-battery`: battery-aware power delivery helpers.

#### Phase 5 — RF / microwave
- `tpt-elec-rf-core`: Smith chart, L/pi matching network synthesis.
- `tpt-elec-rf-filters`: Butterworth/Chebyshev/Bessel ladder synthesis.
- `tpt-elec-rf-antenna`: dipole/patch/monopole patterns, link budgets, FSPL.
- `tpt-elec-rf-mixer`: conversion gain, image rejection, IIP3.
- `tpt-elec-rf-links`: end-to-end RF chain composition.

#### Phase 6 — EMC / battery
- `tpt-elec-emc-core`: CISPR 32 / FCC Part 15 / MIL-STD-461 limit lines.
- `tpt-elec-emc-emissions`: radiated/conducted emissions prediction.
- `tpt-elec-emc-immunity`: IEC 61000-4 immunity test modeling.
- `tpt-elec-emc-shielding`: plane-wave shielding effectiveness (R+A+B).
- `tpt-elec-emc-grounding`: return-path and ground-loop modeling.
- `tpt-elec-battery-core`: cell models + equivalent circuit model.
- `tpt-elec-battery-thermal`: thermal runaway propagation.
- `tpt-elec-battery-bms`: SOC estimation (coulomb counting, Kalman/EKF/UKF).
- `tpt-elec-battery-pack`: series/parallel pack composition.

#### Phase 7 — Semiconductor / manufacturing
- `tpt-elec-odbpp`: ODB++ (ZIP) reader.
- `tpt-elec-ipc2581`: IPC-2581-C XML parser.
- `tpt-elec-semi-core/mosfet/diode/bjt/process`: device physics and compact
  models, PDK abstractions.
- `tpt-elec-mfg-dfm`: DRC engine (trace width/spacing, vias, annular ring,
  placement) + IPC-2152/IPC-2221 trace current capacity.
- `tpt-elec-mfg-test`: IEEE 1149.1 boundary-scan pattern generation.
- `tpt-elec-mfg-yield`: Poisson/Murphy yield prediction.

#### Phase 8 — WASM & ecosystem
- `tpt-elec-wasm`: wasm-bindgen bindings for thermal + impedance, plus
  `WasmSpiceAnalyzer` (DC/AC/transient), `WasmEyeDiagram` (mask compliance),
  and `WasmPdnAnalysis` (impedance profile + decoupling optimizer).
- KiCad Action Plugin scaffold, VS Code extension scaffold, browser thermal
  viewer demo.

[0.1.0]: https://github.com/tpt-solutions/tpt-electronics/releases/tag/v0.1.0
