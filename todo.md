# tpt-electronics — Project Todo

Dual-licensed MIT OR Apache-2.0 · TPT Solutions

## Phase 0: Repository Scaffolding
- [ ] Root Cargo.toml (workspace, resolver = "2", workspace.package, workspace.dependencies)
- [ ] LICENSE-MIT
- [ ] LICENSE-APACHE
- [ ] README.md (crate table + license section per spec §12)
- [ ] CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, CHANGELOG.md
- [ ] deny.toml (license enforcement per spec §7; ban GPL: Elmer FEM, OpenFOAM, GMSH, Netgen)
- [ ] rustfmt.toml
- [ ] clippy.toml
- [ ] .github/workflows/ci.yml (fmt, clippy, test)
- [ ] .github/workflows/license.yml (cargo-deny)
- [ ] .github/workflows/benchmark.yml
- [ ] .github/workflows/docs.yml
- [ ] .github/workflows/release.yml
- [ ] .github/ISSUE_TEMPLATE/{bug_report,feature_request,rfc}.md
- [ ] .github/PULL_REQUEST_TEMPLATE.md
- [ ] crates/{core,formats,thermal,circuit,signal-integrity,rf,power,semiconductor,emc,battery,manufacturing}/ scaffolding (spec §4)
- [ ] examples/, test-data/{gerber,kicad,odbpp,ipc2581,touchstone,spice,golden}/, benches/, docs/{book,rfc,api}/, rfcs/ directory scaffolding
- [ ] Source file header template (SPDX-License-Identifier: MIT OR Apache-2.0)
- [ ] Document DCO sign-off requirement (no CLA), RFC process, 6-week release cadence (spec §9)

## Phase 1: Foundation
### tpt-elec-core
- [ ] Scaffold Cargo.toml + lib.rs (tpt_elec_core)
- [ ] ID types (BoardId, LayerId, ComponentId, NetId, TraceId, ViaId, PadId)
- [ ] Stackup/Layer/LayerType/CopperWeight types
- [ ] Component, ThermalComponentModel, ThermalPad, ViaArray types
- [ ] Unit tests
- [ ] Rustdoc + SPDX header
### tpt-elec-materials
- [ ] Scaffold crate
- [ ] MaterialDatabase, Material, ThermalConductivity (Isotropic/Anisotropic/Tensor), MaterialCategory
- [ ] Built-in material properties (copper, FR4, aluminum, SAC305, silicon, AlN)
- [ ] Unit tests
- [ ] Rustdoc + SPDX header
### tpt-elec-geometry
- [ ] Scaffold crate (depends on tpt-math, tpt-engineering)
- [ ] Trace, Pad, PadShape, DrillHole, ViaType types
- [ ] VoxelGrid, Voxel, VoxelResolution (hex/voxel meshing per spec §4 dependency strategy)
- [ ] CopperArea type
- [ ] Unit tests
- [ ] Rustdoc + SPDX header
### tpt-elec-gerber
- [ ] Scaffold crate
- [ ] GerberParser, GerberState, ParsedGerber, Primitive, Aperture types
- [ ] Parse FS, MO, AD, D01/D02/D03, G01/G02/G03, aperture macros (%AM)
- [ ] Excellon drill file parser (parse_drill)
- [ ] Golden test-data fixtures (test-data/gerber)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-kicad
- [ ] Scaffold crate
- [ ] KiCadParser, ParsedKiCadBoard, Zone, ZoneFill types
- [ ] parse_pcb (S-expression .kicad_pcb parser)
- [ ] parse_schematic (netlist, component values, power nets)
- [ ] Golden test-data fixtures (test-data/kicad)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-thermal
- [ ] Scaffold crate
- [ ] ThermalSolver, BoundaryCondition (FixedTemperature/Convection/Radiation/HeatFlux/HeatSource)
- [ ] solve_steady_state: assemble [K], apply BCs, Conjugate Gradient solve
- [ ] ThermalResult (nodal temperatures, max temp/location, heat flux)
- [ ] Anisotropic tensor handling for PCB substrates
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-convection
- [ ] Scaffold crate
- [ ] ConvectionModel, ConvectionType (Natural/Forced), Fluid
- [ ] calculate_h: Nusselt number from Rayleigh (natural) / Reynolds+Prandtl (forced)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-transient
- [ ] Scaffold crate
- [ ] solve_transient: [C]{dT/dt} + [K]{T} = {Q(t)}, explicit/implicit time integration
- [ ] TransientResult type
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-joule
- [ ] Scaffold crate
- [ ] JouleHeatingSolver, ElectricalSolver, VoltageSource
- [ ] solve_coupled: iterative electrical→Joule heating→thermal→resistivity update loop
- [ ] CoupledResult type, convergence tracking
- [ ] Unit tests + rustdoc + SPDX header
### Phase 1 Milestone
- [ ] CLI tool: takes Gerber + power map, outputs thermal CSV (spec §10 Phase 1)
- [ ] `cargo test --workspace` green; `cargo deny check licenses` passing

## Phase 2: Circuit Simulation
### tpt-elec-spice-netlist
- [ ] Scaffold crate
- [ ] SpiceNetlistParser: R/C/M devices, .model, .tran, .ac, .subckt/.ends
- [ ] Golden test-data fixtures (test-data/spice)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-core
- [ ] Scaffold crate
- [ ] Circuit, Node, CircuitComponent (Resistor/Capacitor/Inductor/Diode/Mosfet/Bjt/VoltageSource/CurrentSource/OpAmp)
- [ ] MnaMatrix (Modified Nodal Analysis: G, C, B)
- [ ] Analysis enum (DcOperatingPoint/DcSweep/AcAnalysis/Transient/NoiseAnalysis)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-models
- [ ] Scaffold crate
- [ ] MosfetModel/MosfetLevel/MosfetParameters (Level1-3, BSIM3/4, EKV)
- [ ] DiodeModel + current() (Shockley diode equation)
- [ ] Unit tests (incl. test_mosfet_saturation_current from spec §8) + rustdoc + SPDX header
### tpt-elec-spice-analysis
- [ ] Scaffold crate
- [ ] SpiceAnalyzer: dc_operating_point (Newton-Raphson)
- [ ] ac_analysis (small-signal, complex MNA per frequency)
- [ ] transient (trapezoidal/Gear integration, adaptive step)
- [ ] noise_analysis (thermal/shot/flicker noise integration)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-spice-noise
- [ ] Scaffold crate
- [ ] Noise source models (thermal, shot, flicker) feeding spice-analysis noise_analysis
- [ ] Unit tests + rustdoc + SPDX header
### Phase 2 Milestone
- [ ] Simulate a buck converter in pure Rust end-to-end (netlist → MNA → transient)
- [ ] Golden test: buck_converter_transient.json (test-data/golden/spice)

## Phase 3: Signal & Power Integrity
### tpt-elec-touchstone
- [ ] Scaffold crate
- [ ] TouchstoneParser: parse S-parameter blocks (# GHz S MA R 50 format)
- [ ] Golden test-data fixtures (test-data/touchstone)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-si-core
- [ ] Scaffold crate
- [ ] TransmissionLine, LineType, LineParameters
- [ ] SParameters, SParameterMatrix
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-si-impedance
- [ ] Scaffold crate
- [ ] ImpedanceCalculator::microstrip (Hammerstad-Jensen)
- [ ] ImpedanceCalculator::stripline (symmetric/asymmetric)
- [ ] ImpedanceCalculator::differential_pair (odd/even mode)
- [ ] Unit tests (incl. test_microstrip_impedance_50_ohm from spec §8) + rustdoc + SPDX header
### tpt-elec-si-crosstalk
- [ ] Scaffold crate
- [ ] Near-end/far-end crosstalk (NEXT/FEXT) coupled-line models
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-si-eye
- [ ] Scaffold crate
- [ ] EyeDiagram, JitterMetrics, EyeMask (PCIe Gen1-6, DDR4/5, USB3/4, Ethernet10G, Custom)
- [ ] from_waveform (UI overlay, eye height/width, jitter separation)
- [ ] check_mask_compliance (mask margin)
- [ ] Golden test: pcie_gen3_eye.json, ddr4_impedance.json
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-pi-pdn
- [ ] Scaffold crate
- [ ] PdnAnalysis, DecouplingCap, VrmModel
- [ ] impedance_profile (VRM + bulk caps + MLCCs + plane capacitance)
- [ ] optimize_decoupling (cap value/quantity optimization)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-pi-decoupling
- [ ] Scaffold crate
- [ ] Decoupling capacitor placement/selection helpers building on pi-pdn
- [ ] Unit tests + rustdoc + SPDX header
### Phase 3 Milestone
- [ ] PCIe Gen 4 eye diagram compliance checker (examples/pcie-eye-diagram)
- [ ] Golden test-data validation (test-data/golden/si)

## Phase 4: Power Electronics
### tpt-elec-power-core
- [ ] Scaffold crate
- [ ] ConverterTopology (Buck/Boost/BuckBoost/Flyback/Forward/HalfBridge/FullBridge/LLC)
- [ ] ConverterDesign, ConverterComponents
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-power-magnetics
- [ ] Scaffold crate
- [ ] Inductor, MagneticCore, CoreType, CoreMaterial, Winding
- [ ] CoreLossCalculator::steinmetz (Pv = k·f^α·B^β)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-power-switches
- [ ] Scaffold crate
- [ ] PowerSwitch, PowerDiode models (switching/conduction losses)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-power-control
- [ ] Scaffold crate
- [ ] ControlLoop, TransferFunction, bode_plot, stability_margins
- [ ] CompensatorType (TypeI/II/III), CompensatorDesigner::design
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-power-battery
- [ ] Scaffold crate
- [ ] Battery-aware power delivery helpers bridging power-core and battery-core
- [ ] Unit tests + rustdoc + SPDX header
### Phase 4 Milestone
- [ ] Automated buck converter design tool (examples/buck-converter)
- [ ] Golden test: buck_converter_transient.json cross-checked against spice-analysis result

## Phase 5: RF / Microwave
### tpt-elec-rf-core
- [ ] Scaffold crate
- [ ] SmithChart::impedance_to_reflection / reflection_to_impedance
- [ ] MatchingTopology, MatchingNetwork, MatchingElement
- [ ] MatchingDesigner::l_network, ::pi_network
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-filters
- [ ] Scaffold crate
- [ ] FilterType (Butterworth/ChebyshevI/ChebyshevII/Elliptic/Bessel), FilterResponse
- [ ] FilterSynthesizer::synthesize, Filter, FilterTopology
- [ ] Golden test: butterworth_filter.json
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-antenna
- [ ] Scaffold crate
- [ ] Antenna, AntennaType (Dipole/Monopole/Patch/Yagi/Helical/Custom), RadiationPattern
- [ ] LinkBudget, free_space_path_loss (FSPL)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-mixer
- [ ] Scaffold crate
- [ ] Mixer models (conversion gain/loss, image rejection, IIP3)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-rf-links
- [ ] Scaffold crate
- [ ] End-to-end RF link chain composition (antenna + filters + mixer + link budget)
- [ ] Unit tests + rustdoc + SPDX header
### Phase 5 Milestone
- [ ] WiFi 6E matching network designer (examples/wifi-antenna)
- [ ] Golden test: l_network_match.json validated

## Phase 6: EMC / Battery
### tpt-elec-emc-core
- [ ] Scaffold crate
- [ ] EmcStandard (Cispr32 A/B, FccPart15 A/B, MilStd461, Do160, Iec61000, Automotive)
- [ ] EmcLimit, EmcTestType
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-emissions
- [ ] Scaffold crate
- [ ] EmissionsPredictor::radiated_emissions (trapezoidal waveform harmonic spectrum)
- [ ] EmissionsPredictor::conducted_emissions (di/dt, parasitic inductance)
- [ ] EmissionsSpectrum, Harmonic
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-immunity
- [ ] Scaffold crate
- [ ] Radiated/conducted immunity, ESD, surge, EFT test modeling against EmcLimit
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-shielding
- [ ] Scaffold crate
- [ ] ShieldingCalculator::plane_wave_shielding (SE = R + A + B)
- [ ] ShieldMaterial (Copper/Aluminum/Steel/MuMetal/ConductivePaint/Custom)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-emc-grounding
- [ ] Scaffold crate
- [ ] Grounding topology / ground loop / return-path modeling
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-core
- [ ] Scaffold crate
- [ ] BatteryCell, BatteryChemistry, CathodeMaterial
- [ ] EquivalentCircuitModel, OcvCurve, RcPair, terminal_voltage
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-thermal
- [ ] Scaffold crate
- [ ] ThermalRunawayModel, propagation_risk, AdjacencyMatrix
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-bms
- [ ] Scaffold crate (depends on tpt-math-prob-dist)
- [ ] SocEstimator (CoulombCounting/KalmanFilter/EKF/UKF/NeuralNetwork)
- [ ] KalmanFilterEstimator, BatteryState, predict/update steps
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-battery-pack
- [ ] Scaffold crate
- [ ] Pack-level series/parallel cell composition, pack-level SOC/thermal aggregation
- [ ] Unit tests + rustdoc + SPDX header
### Phase 6 Milestone
- [ ] CISPR 32 emissions predictor (validated against EmcLimit lines)
- [ ] Battery pack thermal runaway propagation example runs end-to-end

## Phase 7: Semiconductor / Manufacturing
### tpt-elec-odbpp
- [ ] Scaffold crate
- [ ] OdbppParser::parse (ZIP archive, XML/text layers/features/attributes)
- [ ] Golden test-data fixtures (test-data/odbpp)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-ipc2581
- [ ] Scaffold crate
- [ ] Ipc2581Parser::parse (IPC-2581-C XML: stackup, nets, components, traces)
- [ ] Golden test-data fixtures (test-data/ipc2581)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-core
- [ ] Scaffold crate
- [ ] Semiconductor, SemiMaterial (Si/SiC/GaN/GaAs/Ge), DopingProfile, DopingType
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-mosfet
- [ ] Scaffold crate
- [ ] MosfetModel::drain_current (Level1 square-law + higher levels)
- [ ] transconductance (gm), output_conductance (gds)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-diode
- [ ] Scaffold crate
- [ ] Diode compact model (shared/extended from spice-models DiodeModel) for standalone device analysis
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-bjt
- [ ] Scaffold crate
- [ ] BJT Ebers-Moll / Gummel-Poon style compact model
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-semi-process
- [ ] Scaffold crate
- [ ] ProcessDesignKit, TechnologyNode (Micron/Nanometer), DeviceModel, InterconnectModels, DesignRules
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-dfm
- [ ] Scaffold crate
- [ ] DrcEngine, DesignRule (TraceWidth/TraceSpacing/ViaDrillSize/AnnularRing/SolderMask/Silkscreen/BoardOutline/ComponentPlacement)
- [ ] DrcResult (violations/warnings/passed)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-test
- [ ] Scaffold crate
- [ ] BoundaryScanChain, BscanDevice, BscanCell (IEEE 1149.1 JTAG)
- [ ] generate_test_pattern, BscanTestType (Interconnect/PinContinuity/StuckAtFault/SamplePreload/Extest)
- [ ] Unit tests + rustdoc + SPDX header
### tpt-elec-mfg-yield
- [ ] Scaffold crate
- [ ] YieldPredictor::predict_yield (Poisson yield model Y = e^(-D·A))
- [ ] Unit tests + rustdoc + SPDX header
### Phase 7 Milestone
- [ ] Complete DRC engine for KiCad (examples using tpt-elec-mfg-dfm + tpt-elec-kicad)
- [ ] IPC-2152 trace current-carrying capacity validation (spec §5/§8)

## Phase 8: WASM & Ecosystem
### tpt-elec-wasm
- [ ] Scaffold crate (wasm-bindgen)
- [ ] WasmThermalSolver (new from gerber_data + stackup_json, solve, get_temperature_map)
- [ ] WasmImpedanceCalculator::microstrip
- [ ] wasm-pack browser build target verified
- [ ] Unit tests + rustdoc + SPDX header
### KiCad Plugin Integration
- [ ] KiCad Action Plugin scaffold invoking tpt-elec-wasm/tpt-elec-mfg-dfm
- [ ] Manual integration test against a real .kicad_pcb project
### VS Code Extension
- [ ] Extension scaffold surfacing thermal/impedance results in-editor
- [ ] Manual integration test
### Phase 8 Milestone
- [ ] Browser-based thermal viewer with real-time simulation (upload Gerber, see heat map)
- [ ] CI pipeline validation running WASM simulations in GitHub Actions

## Ongoing / Cross-Cutting
- [ ] Maintain `cargo deny check licenses` passing on every phase (spec §7)
- [ ] Keep CI green (fmt, clippy, test, deny) after each crate lands
- [ ] JEDEC JESD51 validation for thermal crates (incl. JESD51-12 for LED packages)
- [ ] IPC-2152 trace current-carrying capacity / temperature rise validation
- [ ] IPC-2141/IPC-2251 impedance validation
- [ ] CISPR 32, FCC Part 15, IEC 61000 EMC validation
- [ ] UN GTR 20, IEC 62660 battery validation
- [ ] PCIe CEM, DDR4/5 JEDEC, USB-IF signal integrity validation
- [ ] Maintain golden test-data (test-data/golden) as each domain lands
- [ ] Criterion benches for thermal steady-state, Joule heating, SPICE transient, SI eye diagram (benches/)
- [ ] RFC process for new crates/major API changes (rfcs/0001+), DCO sign-off on all PRs
- [ ] Update README crate status table (✅ Stable / 🚧 Alpha / 📋 Planned) as crates land
