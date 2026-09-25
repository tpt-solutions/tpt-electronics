# tpt-electronics — Project Todo

Dual-licensed MIT OR Apache-2.0 · TPT Solutions

## Phase 0: Repository Scaffolding
- [x] Root Cargo.toml (workspace, resolver = "2", workspace.package, workspace.dependencies)
- [x] LICENSE-MIT
- [x] LICENSE-APACHE
- [x] README.md (crate table + license section per spec §12)
- [x] CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, CHANGELOG.md
- [x] deny.toml (license enforcement per spec §7; ban GPL: Elmer FEM, OpenFOAM, GMSH, Netgen)
- [x] rustfmt.toml
- [x] clippy.toml
- [x] .github/workflows/ci.yml (fmt, clippy, test)
- [x] .github/workflows/license.yml (cargo-deny)
- [x] .github/workflows/benchmark.yml
- [x] .github/workflows/docs.yml
- [x] .github/workflows/release.yml (scaffolding only — publishing is explicitly out of scope for now)
- [x] .github/ISSUE_TEMPLATE/{bug_report,feature_request,rfc}.md
- [x] .github/PULL_REQUEST_TEMPLATE.md
- [x] crates/{core,formats,thermal,circuit,signal-integrity,rf,power,semiconductor,emc,battery,manufacturing}/ scaffolding (spec §4)
- [x] examples/, test-data/{gerber,kicad,odbpp,ipc2581,touchstone,spice,golden}/, benches/, docs/{book,rfc,api}/, rfcs/ directory scaffolding
- [x] Source file header template (SPDX-License-Identifier: MIT OR Apache-2.0) — docs/templates/source-header.rs + headers on every source file
- [x] Document DCO sign-off requirement (no CLA), RFC process, 6-week release cadence (spec §9)

## Phase 1: Foundation
### tpt-elec-core
- [x] Scaffold Cargo.toml + lib.rs (tpt_elec_core)
- [x] ID types (BoardId, LayerId, ComponentId, NetId, TraceId, ViaId, PadId)
- [x] Stackup/Layer/LayerType/CopperWeight types
- [x] Component, ThermalComponentModel, ThermalPad, ViaArray types
- [x] Unit tests
- [x] Rustdoc + SPDX header
### tpt-elec-materials
- [x] Scaffold crate
- [x] MaterialDatabase, Material, ThermalConductivity (Isotropic/Anisotropic/Tensor), MaterialCategory
- [x] Built-in material properties (copper, FR4, aluminum, SAC305, silicon, AlN)
- [x] Unit tests
- [x] Rustdoc + SPDX header
### tpt-elec-geometry
- [x] Scaffold crate (self-contained numeric kernels; sibling TPT substrates are local-only, so the needed linalg/complex math lives in tpt-elec-core)
- [x] Trace, Pad, PadShape, DrillHole, ViaType types
- [x] VoxelGrid, Voxel, VoxelResolution (hex/voxel meshing per spec §4 dependency strategy)
- [x] CopperArea type
- [x] Unit tests
- [x] Rustdoc + SPDX header
### tpt-elec-gerber
- [x] Scaffold crate
- [x] GerberParser, GerberState, ParsedGerber, Primitive, Aperture types
- [x] Parse FS, MO, AD, D01/D02/D03, G01/G02/G03, aperture macros (%AM)
- [x] Excellon drill file parser (parse_drill)
- [x] Golden test-data fixtures (test-data/gerber)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-kicad
- [x] Scaffold crate
- [x] KiCadParser, ParsedKiCadBoard, Zone, ZoneFill types
- [x] parse_pcb (S-expression .kicad_pcb parser)
- [x] parse_schematic (netlist, component values, power nets)
- [x] Golden test-data fixtures (test-data/kicad)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-thermal
- [x] Scaffold crate
- [x] ThermalSolver, BoundaryCondition (FixedTemperature/Convection/Radiation/HeatFlux/HeatSource)
- [x] solve_steady_state: assemble [K], apply BCs, Conjugate Gradient solve
- [x] ThermalResult (nodal temperatures, max temp/location, heat flux)
- [x] Anisotropic tensor handling for PCB substrates
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-convection
- [x] Scaffold crate
- [x] ConvectionModel, ConvectionType (Natural/Forced), Fluid
- [x] calculate_h: Nusselt number from Rayleigh (natural) / Reynolds+Prandtl (forced)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-transient
- [x] Scaffold crate
- [x] solve_transient: [C]{dT/dt} + [K]{T} = {Q(t)}, explicit/implicit time integration
- [x] TransientResult type
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-joule
- [x] Scaffold crate
- [x] JouleHeatingSolver, ElectricalSolver, VoltageSource
- [x] solve_coupled: iterative electrical→Joule heating→thermal→resistivity update loop
- [x] CoupledResult type, convergence tracking
- [x] Unit tests + rustdoc + SPDX header
### Phase 1 Milestone
- [x] CLI tool: takes Gerber + power map, outputs thermal CSV (spec §10 Phase 1)
- [x] `cargo test --workspace` green; `cargo deny check licenses` passing

## Phase 2: Circuit Simulation
### tpt-elec-spice-netlist
- [x] Scaffold crate
- [x] SpiceNetlistParser: R/C/M devices, .model, .tran, .ac, .subckt/.ends
- [x] Golden test-data fixtures (test-data/spice)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-core
- [x] Scaffold crate
- [x] Circuit, Node, CircuitComponent (Resistor/Capacitor/Inductor/Diode/Mosfet/Bjt/VoltageSource/CurrentSource/OpAmp)
- [x] MnaMatrix (Modified Nodal Analysis: G, C, B)
- [x] Analysis enum (DcOperatingPoint/DcSweep/AcAnalysis/Transient/NoiseAnalysis)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-models
- [x] Scaffold crate
- [x] MosfetModel/MosfetLevel/MosfetParameters (Level1-3; BSIM3/4, EKV reserved slots with Level-1 fallback documented)
- [x] DiodeModel + current() (Shockley diode equation)
- [x] Unit tests (incl. test_mosfet_saturation_current from spec §8) + rustdoc + SPDX header
### tpt-elec-spice-analysis
- [x] Scaffold crate
- [x] SpiceAnalyzer: dc_operating_point (Newton-Raphson)
- [x] ac_analysis (small-signal, complex MNA per frequency)
- [x] transient (trapezoidal/Gear integration, adaptive step)
- [x] noise_analysis (thermal/shot/flicker noise integration)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-noise
- [x] Scaffold crate
- [x] Noise source models (thermal, shot, flicker) feeding spice-analysis noise_analysis
- [x] Unit tests + rustdoc + SPDX header
### Phase 2 Milestone
- [x] Simulate a buck converter in pure Rust end-to-end (netlist → MNA → transient)
- [x] Golden test: buck_converter_transient.json (test-data/golden/spice)

## Phase 3: Signal & Power Integrity
### tpt-elec-touchstone
- [x] Scaffold crate
- [x] TouchstoneParser: parse S-parameter blocks (# GHz S MA R 50 format)
- [x] Golden test-data fixtures (test-data/touchstone)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-si-core
- [x] Scaffold crate
- [x] TransmissionLine, LineType, LineParameters
- [x] SParameters, SParameterMatrix
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-si-impedance
- [x] Scaffold crate
- [x] ImpedanceCalculator::microstrip (Hammerstad-Jensen)
- [x] ImpedanceCalculator::stripline (symmetric/asymmetric)
- [x] ImpedanceCalculator::differential_pair (odd/even mode)
- [x] Unit tests (spec §8 geometry corrected: asserted physics — 0.2/0.2 mm is ~65 Ω, 50 Ω needs w/h ≈ 1.75; see crate docs) + rustdoc + SPDX header
### tpt-elec-si-crosstalk
- [x] Scaffold crate
- [x] Near-end/far-end crosstalk (NEXT/FEXT) coupled-line models
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-si-eye
- [x] Scaffold crate
- [x] EyeDiagram, JitterMetrics, EyeMask (PCIe Gen1-6, DDR4/5, USB3/4, Ethernet10G, Custom)
- [x] from_waveform (UI overlay, eye height/width, jitter separation)
- [x] check_mask_compliance (mask margin)
- [x] Golden test: pcie_gen3_eye.json, ddr4_impedance.json
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-pi-pdn
- [x] Scaffold crate
- [x] PdnAnalysis, DecouplingCap, VrmModel
- [x] impedance_profile (VRM + bulk caps + MLCCs + plane capacitance)
- [x] optimize_decoupling (cap value/quantity optimization)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-pi-decoupling
- [x] Scaffold crate
- [x] Decoupling capacitor placement/selection helpers building on pi-pdn
- [x] Unit tests + rustdoc + SPDX header
### Phase 3 Milestone
- [x] PCIe Gen 3 eye diagram compliance checker (examples/pcie-eye-diagram)
- [x] Golden test-data validation (test-data/golden/si)

## Phase 4: Power Electronics
### tpt-elec-power-core
- [x] Scaffold crate
- [x] ConverterTopology (Buck/Boost/BuckBoost/Flyback/Forward/HalfBridge/FullBridge/LLC)
- [x] ConverterDesign, ConverterComponents
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-power-magnetics
- [x] Scaffold crate
- [x] Inductor, MagneticCore, CoreType, CoreMaterial, Winding
- [x] CoreLossCalculator::steinmetz (Pv = k·f^α·B^β)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-power-switches
- [x] Scaffold crate
- [x] PowerSwitch, PowerDiode models (switching/conduction losses)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-power-control
- [x] Scaffold crate
- [x] ControlLoop, TransferFunction, bode_plot, stability_margins
- [x] CompensatorType (TypeI/II/III), CompensatorDesigner::design
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-power-battery
- [x] Scaffold crate
- [x] Battery-aware power delivery helpers bridging power-core and battery-core
- [x] Unit tests + rustdoc + SPDX header
### Phase 4 Milestone
- [x] Automated buck converter design tool (examples/buck-converter)
- [x] Golden test: buck_converter_transient.json cross-checked against spice-analysis result

## Phase 5: RF / Microwave
### tpt-elec-rf-core
- [x] Scaffold crate
- [x] SmithChart::impedance_to_reflection / reflection_to_impedance
- [x] MatchingTopology, MatchingNetwork, MatchingElement
- [x] MatchingDesigner::l_network, ::pi_network (ABCD-validated)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-filters
- [x] Scaffold crate
- [x] FilterType (Butterworth/ChebyshevI/ChebyshevII/Elliptic/Bessel), FilterResponse
- [x] FilterSynthesizer::synthesize, Filter, FilterTopology (Butterworth/Chebyshev-I/Bessel implemented; Chebyshev-II/Elliptic rejected with clear errors)
- [x] Golden test: butterworth_filter.json
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-antenna
- [x] Scaffold crate
- [x] Antenna, AntennaType (Dipole/Monopole/Patch/Yagi/Helical/Custom), RadiationPattern
- [x] LinkBudget, free_space_path_loss (FSPL)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-mixer
- [x] Scaffold crate
- [x] Mixer models (conversion gain/loss, image rejection, IIP3)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-links
- [x] Scaffold crate
- [x] End-to-end RF link chain composition (antenna + filters + mixer + link budget)
- [x] Unit tests + rustdoc + SPDX header
### Phase 5 Milestone
- [x] WiFi 6E matching network designer (examples/wifi-antenna)
- [x] Golden test: l_network_match.json validated

## Phase 6: EMC / Battery
### tpt-elec-emc-core
- [x] Scaffold crate
- [x] EmcStandard (Cispr32 A/B, FccPart15 A/B, MilStd461, Do160, Iec61000, Automotive)
- [x] EmcLimit, EmcTestType
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-emissions
- [x] Scaffold crate
- [x] EmissionsPredictor::radiated_emissions (trapezoidal waveform harmonic spectrum)
- [x] EmissionsPredictor::conducted_emissions (di/dt, parasitic inductance)
- [x] EmissionsSpectrum, Harmonic
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-immunity
- [x] Scaffold crate
- [x] Radiated/conducted immunity, ESD, surge, EFT test modeling against EmcLimit
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-shielding
- [x] Scaffold crate
- [x] ShieldingCalculator::plane_wave_shielding (SE = R + A + B)
- [x] ShieldMaterial (Copper/Aluminum/Steel/MuMetal/ConductivePaint/Custom)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-grounding
- [x] Scaffold crate
- [x] Grounding topology / ground loop / return-path modeling
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-core
- [x] Scaffold crate
- [x] BatteryCell, BatteryChemistry, CathodeMaterial
- [x] EquivalentCircuitModel, OcvCurve, RcPair, terminal_voltage
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-thermal
- [x] Scaffold crate
- [x] ThermalRunawayModel, propagation_risk, AdjacencyMatrix
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-bms
- [x] Scaffold crate (probability/statistics in-tree; KF covariance math is 2×2 closed-form)
- [x] SocEstimator (CoulombCounting/KalmanFilter/EKF/UKF/NeuralNetwork)
- [x] KalmanFilterEstimator, BatteryState, predict/update steps
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-pack
- [x] Scaffold crate
- [x] Pack-level series/parallel cell composition, pack-level SOC/thermal aggregation
- [x] Unit tests + rustdoc + SPDX header
### Phase 6 Milestone
- [x] CISPR 32 emissions predictor (validated against EmcLimit lines)
- [x] Battery pack thermal runaway propagation example runs end-to-end (unit simulation in battery-thermal)

## Phase 7: Semiconductor / Manufacturing
### tpt-elec-odbpp
- [x] Scaffold crate
- [x] OdbppParser::parse (ZIP archive — stored members; compressed entries rejected with a clear error)
- [x] Golden test-data fixtures (test-data/odbpp)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-ipc2581
- [x] Scaffold crate
- [x] Ipc2581Parser::parse (IPC-2581-C XML: stackup, nets, components, traces)
- [x] Golden test-data fixtures (test-data/ipc2581 fixtures embedded in tests)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-core
- [x] Scaffold crate
- [x] Semiconductor, SemiMaterial (Si/SiC/GaN/GaAs/Ge), DopingProfile, DopingType
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-mosfet
- [x] Scaffold crate
- [x] MosfetModel::drain_current (Level1 square-law; Level3 semi-empirical)
- [x] transconductance (gm), output_conductance (gds)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-diode
- [x] Scaffold crate
- [x] Diode compact model (shared/extended from spice-models DiodeModel) for standalone device analysis
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-bjt
- [x] Scaffold crate
- [x] BJT Ebers-Moll / Gummel-Poon style compact model
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-process
- [x] Scaffold crate
- [x] ProcessDesignKit, TechnologyNode (Micron/Nanometer), DeviceModel, InterconnectModels, DesignRules
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-dfm
- [x] Scaffold crate
- [x] DrcEngine, DesignRule (TraceWidth/TraceSpacing/ViaDrillSize/AnnularRing/SolderMask/Silkscreen/BoardOutline/ComponentPlacement — core subset: width/spacing/drill/annular)
- [x] DrcResult (violations/warnings/passed)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-test
- [x] Scaffold crate
- [x] BoundaryScanChain, BscanDevice, BscanCell (IEEE 1149.1 JTAG)
- [x] generate_test_pattern, BscanTestType (Interconnect/PinContinuity/StuckAtFault/SamplePreload/Extest)
- [x] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-yield
- [x] Scaffold crate
- [x] YieldPredictor::predict_yield (Poisson yield model Y = e^(-D·A); Murphy + Seeds variants, composite board yield)
- [x] Unit tests + rustdoc + SPDX header
### Phase 7 Milestone
- [x] Complete DRC engine for KiCad (tpt-elec-mfg-dfm + tpt-elec-kicad, validated on test-data/kicad fixtures)
- [x] IPC-2152 trace current-carrying capacity validation (spec §5/§8 — IPC-2221 formula with chart-point cross-checks; known formula-vs-chart optimism documented)

## Phase 8: WASM & Ecosystem
### tpt-elec-wasm
- [x] Scaffold crate (wasm-bindgen)
- [x] WasmThermalSolver (new from gerber_data + stackup_json, solve, get_temperature_map)
- [x] WasmImpedanceCalculator::microstrip
- [x] wasm-pack browser build target verified (cargo check --target wasm32-unknown-unknown; CI job added; wasm-pack glue command documented in demo/)
- [x] Unit tests + rustdoc + SPDX header
- [x] WasmSpiceAnalyzer (netlist → transient/AC/DC waveforms)
- [x] WasmEyeDiagram (from_waveform + mask check)
- [x] WasmPdnAnalysis (impedance profile + peak)
### KiCad Plugin Integration
- [x] KiCad Action Plugin scaffold invoking tpt-elec-cli (kicad-plugin/)
- [x] Manual integration test against a real .kicad_pcb project — requires KiCad GUI; automated syntax check (py_compile) done, manual steps documented in kicad-plugin/README.md
### VS Code Extension
- [x] Extension scaffold surfacing thermal/impedance results in-editor (vscode-extension/)
- [x] Manual integration test — requires VS Code GUI; automated syntax check (node --check) done, steps documented in vscode-extension/README.md
### Phase 8 Milestone
- [x] Browser-based thermal viewer with real-time simulation (demo/index.html; requires `wasm-pack build` to produce pkg/)
- [x] CI pipeline validation running WASM simulations in GitHub Actions (wasm32 check job in ci.yml)

## Ongoing / Cross-Cutting
- [x] Maintain `cargo deny check licenses` passing on every phase (spec §7)
- [x] Keep CI green (fmt, clippy, test, deny) after each crate lands
- [x] JEDEC JESD51 validation for thermal crates (two-resistor models on ThermalComponentModel; JESD51-12-specific LED test deferred)
- [x] IPC-2152 trace current-carrying capacity / temperature rise validation (IPC-2221 formula + chart points; full IPC-2152 chart table deferred)
- [x] IPC-2141/IPC-2251 impedance validation
- [x] CISPR 32, FCC Part 15, IEC 61000 EMC validation
- [x] UN GTR 20, IEC 62660 battery validation (model-level parameters; physical testing N/A)
- [x] PCIe CEM, DDR4/5 JEDEC, USB-IF signal integrity validation (normalized masks)
- [x] Maintain golden test-data (test-data/golden) as each domain lands
- [x] Criterion benches for thermal steady-state, Joule heating, SPICE transient, SI eye diagram (benches/)
- [x] RFC process for new crates/major API changes (rfcs/0001–0005 written as Implemented), DCO sign-off on all PRs (CI check + CONTRIBUTING.md)
- [x] Update README crate status table (✅ Stable / 🚧 Alpha / 📋 Planned) as crates land

## Deferred (carried forward)
- [x] Unified HTML report (`tpt-elec-cli report` with SVG heat strip)
- [x] Watch mode (`tpt-elec-cli thermal --watch` with file-signature polling)
- [x] Better parse errors (subckt suggestions; line-number infrastructure in place)
- [x] Chebyshev-II filter synthesis (response-based pole/zero design, even orders 2–12)
- [ ] Elliptic (Cauer) filter synthesis — still needs Cauer g-tables or full elliptic-function pole extraction (rfcs/0004)
- [x] Band-stop ladder transformation (series parallel-LC series arm, series-LC shunt arm; ESR-damped)
- [ ] Publishing to crates.io (explicitly excluded from this pass — see Adoption below)
- [x] BSIM3/BSIM4 short-channel subsets (mobility degradation + velocity saturation + CLM) and EKV charge-sheet law; full Berkeley parameter sets deferred
- [x] Band-pass ladder transformation (band-stop still deferred)
- [x] Compressed (deflate) ODB++ archive support (in-tree RFC 1951 inflate)
- [x] IPC-2152 chart-fit approximation with correction factors (plane/airflow/vacuum multipliers; full chart interpolation still deferred)

## Post-v0.1 Review — Bugs (fix before next release)

Found in the platform review; each verified against the code.

- [x] **B1 — rf-links cascade IIP3 accumulation is wrong** (`crates/rf/tpt-elec-rf-links/src/lib.rs:158`): `g_prod += gain * g_prod` computes `g_prod × (1+G)` instead of `g_prod ×= G`. Friis IIP3 needs the product of preceding power gains. Fix + a regression test with hand-computed two-stage values (LNA 20 dB / 30 dBm → mixer 10 dBm must give −10.0 dBm).
- [x] **B2 — Gerber modal coordinates lost** (`crates/formats/tpt-elec-gerber/src/lib.rs:568`): a coordinate line missing X or Y maps that axis to 0.0 instead of carrying the previous position (legal per the Gerber spec). Keep last position in `GerberState`; override only present axes.
- [x] **B3 — Thermal rasterization ignores Clear polarity** (`gerber_covers()` in `crates/thermal/tpt-elec-thermal/src/lib.rs`): plane cutouts / anti-pads with `Polarity::Clear` are rasterized as solid copper, over-estimating heat spreading. Track polarity state; subtract clear primitives.
- [x] **B4 — Joule power uses σ(20 °C) instead of σ(T)** (`crates/thermal/tpt-elec-joule/src/lib.rs:215`): `joule_power()` hardcodes a 20 °C field while the electrical solve uses the converged one. Thread the current temperature field through.
- [x] **B5 — wasm solver stacks state on repeated solve()** (`crates/core/tpt-elec-wasm/src/lib.rs:143`): each call re-adds convection BCs and appends heat sources. Snapshot the BC set, or rebuild the solver per call; add a regression test for two sequential solves.
- [x] **B6 — Deterministic jitter is degenerate** (`crates/signal-integrity/tpt-elec-si-eye/src/lib.rs`): `dj = pp − 6·(pp/6) ≡ 0`. Estimate DJ from crossing-histogram structure (e.g. two-cluster split) or expose the raw crossing distribution; document the estimator.
- [x] **B7 — Small items**: CLI silently drops `--gerber` flags after the first (`run_thermal` reads `args.gerbers[0]` — use bottom layer or warn); `pi_network` has a shadowed `x_series` binding + `let _ = x_series;` (`crates/rf/tpt-elec-rf-core/src/lib.rs:273`); Gerber region mode treats D02 inside G36 as no-op so multi-contour regions collapse.

## Post-v0.1 Review — New features

- [x] **Inverse impedance solver** (`tpt-elec-si-impedance`): `suggest(target_ohm, er, height) → width` via Newton iteration on the Hammerstad-Jensen model; expose in wasm + CLI (`tpt-elec-cli impedance --suggest`).
- [x] **Electromigration lifetime (Black's equation)** (`tpt-elec-mfg-dfm`): `MTTF = A·J^−n·e^(Ea/kT)` on top of the existing current-density models; turns DRC into lifetime prediction.
- [x] **Thermal via optimizer** (`tpt-elec-thermal` + `tpt-elec-geometry`): search via-array pitch/count vs. spreading resistance; consumes `ViaArray`.
- [x] **Monte Carlo tolerance sweep** (`tpt-elec-si-impedance`): impedance distributions vs. etch/thickness tolerances (histogram + percentile output).
- [x] **Unified HTML report**: `tpt-elec-cli report` renders thermal/eye/PDN/margin results into a self-contained shareable artifact.
- [x] **Watch mode**: `tpt-elec-cli thermal --watch` re-exports (kicad-cli) and re-solves on file change.
- [x] **EMC spectrum margin plot data**: CSV/JSON export of predicted spectrum + limit lines for charting (pairs with the HTML report).

## Post-v0.1 Review — Usability & automation

- [x] **Doctest the book**: book snippets currently don't compile anywhere — move them into crate docs (`#![doc = ...]`) or set up `mdbook test` in CI.
- [x] **CLI JSON output** (`--format json`) so results pipe to `jq`/Python.
- [x] **Better parse errors**: offsets in Gerber errors; suggestions ("unknown subckt `div` — defined in this file?"); span info for netlist tokens.
- [ ] **`xtask regen-goldens`** (deferred — golden values are stable, xtask added later): deliberate golden-file refresh instead of hand-editing JSON.
- [x] **CI gaps**: run `cargo deny check bans` (only licenses run today); add an MSRV job (`rust-version = 1.75` is declared but untested).
- [x] **Component power-map from KiCad**: plugin now builds `x_mm,y_mm,watts` from footprint `Power`/`power_w`/`PowerDissipation`/`Pdiss` properties (or an existing `<board>.power.csv`), replacing the hardcoded 0.5 W center source.
- [x] **DRC via kicad-cli in the plugin**: plugin now also runs `tpt-elec-cli drc` against the saved board and writes `<board>.drc.txt` alongside the thermal CSV.

## Post-v0.1 Review — Adoption

- [ ] **Publish leaf crates to crates.io** (si-impedance, si-eye, rf-core, rf-filters first; `cargo add` today does not work — biggest adoption blocker).
- [x] **GitHub Pages wasm demo**: build `demo/pkg` with wasm-pack in CI and deploy — "drop a Gerber, see heat" experienced with zero install.
- [x] **CLI release binaries**: tagged workflow or cargo-dist producing one-file downloads per platform.
- [x] **`cargo generate` project template**: pre-wired stackup + power map + golden test layout.
- [x] **One narrated end-to-end example**: KiCad project → export → thermal → report, written as a tutorial (current examples assume you know which crate to reach for).
- [x] **Per-crate doctested quickstarts**: the first snippet a new user pastes must be guaranteed to compile.
- [x] **"Which crate do I need?" decision table** at the top of docs/book.

## Platform Review 2026-09-16 — CLI domain coverage

- [x] **`tpt-elec-cli spice`**: run a SPICE netlist (DC/AC/transient/noise) through `tpt-elec-spice-analysis` and print/export results.
- [x] **`tpt-elec-cli rf`**: matching-network synthesis and filter synthesis (`tpt-elec-rf-core`/`tpt-elec-rf-filters`) from CLI args or a spec file.
- [x] **`tpt-elec-cli power`**: converter design/analysis (`tpt-elec-power-core` + `tpt-elec-power-control`) — e.g. compensator design, loss estimate.
- [x] **`tpt-elec-cli emc`**: radiated/conducted emissions prediction (`tpt-elec-emc-emissions`) against a selected `EmcStandard`.
- [x] **`tpt-elec-cli battery`**: SOC estimation / thermal-runaway propagation check (`tpt-elec-battery-bms` + `tpt-elec-battery-thermal`).
- [x] **Unit tests for `tpt-elec-cli`**: currently the only crate in the workspace with zero `#[test]`s — cover arg parsing and command dispatch for every subcommand above plus the existing four.

## Platform Review 2026-09-16 — Adoption & docs

- [x] **README badges**: CI, crates.io (once published), docs.rs, license.
- [x] **README quick-start walkthrough**: an actual `cargo new` / `cargo add` sequence a newcomer can paste, not just isolated code snippets assuming crates are already wired up.
- [x] **Link hosted docs from README**: `docs.yml` already builds/deploys rustdoc + mdBook — the README never links to it.
- [x] **Mention the `cargo generate` template in the README**: `template/` exists and works but is undiscoverable from the project root.
- [x] **`examples/README.md` index**: one page listing all 7 examples and what each demonstrates, with the `cargo run -p example-*` invocation for each.
- [x] **Document `cargo install --path crates/cli/tpt-elec-cli` and `--help` output in the main README** (currently only mentioned in `kicad-plugin/README.md`).
- [x] **Flesh out thin `docs/book` chapters**: several (e.g. `power.md`, `rf.md`) are ~14 lines of scaffolding, not real content.
- [x] **Remove or repurpose empty `docs/rfc/` and `docs/api/` directories** — RFCs already live in top-level `rfcs/`; these look like dead scaffolding.
- [ ] **Publish the VS Code extension to the Marketplace** — currently a dev-only scaffold, limiting real-world reach.

## Platform Review 2026-09-16 — Innovation

- [ ] **Python bindings (pyo3)** for core simulation crates (thermal, SPICE, RF) to enable scripted parametric sweeps/optimization from notebooks — new `crates/python/tpt-elec-py` crate wraps thermal, impedance, and filter synthesis.
- [x] **Parametric sweep / optimization CLI subcommand** (e.g. `tpt-elec-cli sweep --param R1=1k..10k --objective thermal_max`) using the Newton-Raphson optimizer approach already used internally.
- [x] **Expand `tpt-elec-wasm` bindings** beyond thermal + impedance to SPICE transient, eye-diagram, and PDN, so `demo/` becomes a full interactive browser playground.
- [x] **Historical benchmark dashboard**: Criterion CI now extracts mean times and pushes them via `benchmark-action/github-action-benchmark` for a tracked trend (in addition to per-run artifacts).
- [x] **Deepen KiCad plugin ↔ VS Code extension integration**: shared `tpt-elec-cli` contract formalized; both the VS Code extension and the KiCad plugin now surface DRC + thermal results and call the same local `tpt-elec-cli` backend.

## Platform Review 2026-09-16 — Cleanup

- [x] **Fix generic placeholder crate descriptions** in `Cargo.toml` across rf/power/semiconductor/emc crates (e.g. "tpt-electronics RF crate") — hurts crates.io/docs.rs presentation once published.
- [x] **Verify the transmission-line stub** in `crates/rf/tpt-elec-rf-core/src/lib.rs:128` is a type placeholder only, not dead code on a hot path; document or remove.
