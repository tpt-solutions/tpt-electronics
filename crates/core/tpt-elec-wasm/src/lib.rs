// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebAssembly bindings for tpt-electronics.
//!
//! Exposes thermal, impedance, SPICE, eye-diagram, and PDN analysis to
//! browser-based EDA tools. The API is intentionally small and
//! `f64`-array based:
//!
//! * [`WasmThermalSolver`] — build a solver from Gerber bytes + a small
//!   JSON stackup description, apply a power map, solve, read temperatures.
//! * [`WasmImpedanceCalculator`] — microstrip/stripline/differential-pair
//!   impedance in a single call.
//! * [`WasmSpiceAnalyzer`] — parse a SPICE netlist and run DC/AC/transient.
//! * [`WasmEyeDiagram`] — fold a waveform into an eye and check masks.
//! * [`WasmPdnAnalysis`] — PDN impedance profile and decoupling optimizer.
//!
//! The crate also builds for native targets (tests run on the host); the
//! browser build target is `wasm32-unknown-unknown` via wasm-pack.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

/// Stackup description accepted by [`WasmThermalSolver::new`].
#[derive(Deserialize)]
pub struct StackupConfig {
    /// Substrate thickness [mm]. Default 1.6.
    #[serde(default = "default_thickness")]
    pub thickness_mm: f64,
    /// Copper foil thickness [µm]. Default 35.
    #[serde(default = "default_copper")]
    pub copper_um: f64,
    /// Voxel edge length [mm]. Default 1.0.
    #[serde(default = "default_resolution")]
    pub resolution_mm: f64,
    /// Ambient temperature [°C]. Default 25.
    #[serde(default = "default_ambient")]
    pub ambient_c: f64,
    /// Convection coefficient [W/(m²·K)]. Default 10.
    #[serde(default = "default_h")]
    pub h_conv: f64,
}

fn default_thickness() -> f64 {
    1.6
}
fn default_copper() -> f64 {
    35.0
}
fn default_resolution() -> f64 {
    1.0
}
fn default_ambient() -> f64 {
    25.0
}
fn default_h() -> f64 {
    10.0
}

impl Default for StackupConfig {
    fn default() -> Self {
        Self {
            thickness_mm: default_thickness(),
            copper_um: default_copper(),
            resolution_mm: default_resolution(),
            ambient_c: default_ambient(),
            h_conv: default_h(),
        }
    }
}

/// Thermal solver over a Gerber layer.
#[wasm_bindgen]
pub struct WasmThermalSolver {
    solver: ThermalSolver,
    /// Pristine copy; each `solve` starts from it so repeated interactive
    /// calls don't stack boundary conditions or power-map sources (B5).
    pristine: ThermalSolver,
    grid_meta: (u32, u32, u32),
    config: StackupConfig,
}

#[wasm_bindgen]
impl WasmThermalSolver {
    /// Creates a solver from raw Gerber bytes and a JSON stackup config.
    ///
    /// `stackup_json` may be empty for defaults:
    /// `{"thickness_mm":1.6,"copper_um":35,"resolution_mm":1.0,"ambient_c":25,"h_conv":10}`
    #[wasm_bindgen(constructor)]
    pub fn new(gerber_data: &[u8], stackup_json: &str) -> Result<WasmThermalSolver, JsValue> {
        Self::new_inner(gerber_data, stackup_json).map_err(|e| JsValue::from_str(&e))
    }

    /// Native-callable constructor (host tests use this; `JsValue` errors
    /// can only be materialized on wasm32).
    pub fn new_inner(gerber_data: &[u8], stackup_json: &str) -> Result<WasmThermalSolver, String> {
        let text = String::from_utf8_lossy(gerber_data);
        let gerber = GerberParser::parse(&text).map_err(|e| e.to_string())?;
        let config: StackupConfig = if stackup_json.trim().is_empty() {
            StackupConfig::default()
        } else {
            serde_json::from_str(stackup_json).map_err(|e| format!("stackup json: {e}"))?
        };

        let solver = build_solver(&gerber, &config);
        let grid_meta = (solver.grid().nx(), solver.grid().ny(), solver.grid().nz());
        Ok(Self {
            pristine: solver.clone(),
            solver,
            grid_meta,
            config,
        })
    }

    /// Applies a power map: flat `[x_mm, y_mm, watts, …]` triples.
    pub fn apply_power_map(&mut self, points: &[f64]) {
        for chunk in points.chunks(3) {
            if chunk.len() == 3 {
                self.solver
                    .add_heat_source_near(chunk[0] * 1e-3, chunk[1] * 1e-3, chunk[2]);
            }
        }
    }

    /// Runs the steady-state solve and returns the max temperature [°C].
    pub fn solve(&mut self, power_map: &[f64]) -> Result<f64, JsValue> {
        self.solve_inner(power_map)
            .map_err(|e| JsValue::from_str(&e))
    }

    fn solve_inner(&mut self, power_map: &[f64]) -> Result<f64, String> {
        // Reset to the pristine model so repeated calls are idempotent.
        self.solver = self.pristine.clone();
        if !power_map.is_empty() {
            self.apply_power_map(power_map);
        }
        // Convection over the top surface
        let top_nodes: Vec<u32> = {
            let g = self.solver.grid();
            let z = g.nz() - 1;
            (0..g.nx())
                .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
                .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
                .collect()
        };
        self.solver
            .add_boundary_condition(BoundaryCondition::Convection {
                surface: top_nodes,
                h: self.config.h_conv,
                t_ambient: self.config.ambient_c,
            });
        let result = self.solver.solve_steady_state();
        if result.residual.is_finite() {
            Ok(result.max_temp)
        } else {
            Err("solver did not converge".into())
        }
    }

    /// Temperature map in flattened grid order [°C].
    pub fn get_temperature_map(&mut self) -> Vec<f32> {
        let result = self.solver.solve_steady_state();
        result.temperatures.iter().map(|t| *t as f32).collect()
    }

    /// Grid dimensions [nx, ny, nz].
    pub fn grid_dimensions(&self) -> Vec<u32> {
        vec![self.grid_meta.0, self.grid_meta.1, self.grid_meta.2]
    }
}

fn build_solver(gerber: &tpt_elec_gerber::ParsedGerber, config: &StackupConfig) -> ThermalSolver {
    // Scale resolution proportionally to substrate thickness so the API
    // stays usable across board sizes.
    let resolution = config.resolution_mm * 1e-3;
    let materials = MaterialDatabase::standard();

    ThermalSolver::from_gerber_scaled(
        gerber,
        &materials,
        resolution,
        config.thickness_mm * 1e-3,
        config.copper_um * 1e-6,
    )
}

/// Impedance calculations.
#[wasm_bindgen]
pub struct WasmImpedanceCalculator;

#[wasm_bindgen]
impl WasmImpedanceCalculator {
    /// Microstrip characteristic impedance [Ω].
    pub fn microstrip(
        trace_width_mm: f64,
        trace_thickness_mm: f64,
        dielectric_height_mm: f64,
        er: f64,
    ) -> f64 {
        use tpt_elec_core::Length;
        use tpt_elec_si_impedance::ImpedanceCalculator;
        ImpedanceCalculator::microstrip(
            Length::mm(trace_width_mm),
            Length::mm(trace_thickness_mm),
            Length::mm(dielectric_height_mm),
            er,
        )
        .z0
    }

    /// Differential impedance (2× odd mode) [Ω].
    pub fn differential(
        trace_width_mm: f64,
        spacing_mm: f64,
        dielectric_height_mm: f64,
        er: f64,
    ) -> f64 {
        use tpt_elec_core::Length;
        use tpt_elec_si_impedance::ImpedanceCalculator;
        ImpedanceCalculator::differential_pair(
            Length::mm(trace_width_mm),
            Length::mm(spacing_mm),
            Length::mm(dielectric_height_mm),
            er,
        )
        .differential_impedance()
        .unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------
// SPICE
// ---------------------------------------------------------------------------

/// SPICE analyzer over a parsed netlist.
#[wasm_bindgen]
pub struct WasmSpiceAnalyzer {
    analyzer: tpt_elec_spice_analysis::SpiceAnalyzer,
}

#[wasm_bindgen]
impl WasmSpiceAnalyzer {
    /// Parses a SPICE netlist and prepares analyses.
    pub fn new(netlist: &str) -> Result<WasmSpiceAnalyzer, JsValue> {
        Self::new_inner(netlist).map_err(|e| JsValue::from_str(&e))
    }

    /// Native-callable constructor (host tests use this).
    pub fn new_inner(netlist: &str) -> Result<WasmSpiceAnalyzer, String> {
        let circuit = tpt_elec_spice_netlist::SpiceNetlistParser::parse(netlist)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            analyzer: tpt_elec_spice_analysis::SpiceAnalyzer::new(circuit),
        })
    }

    /// Non-ground node names in declaration order.
    pub fn node_names(&self) -> Vec<String> {
        self.analyzer
            .circuit()
            .nodes()
            .iter()
            .skip(1)
            .map(|n| n.name.clone())
            .collect()
    }

    /// DC operating point: node voltages `[v0, v1, …]` (index 0 = ground).
    pub fn dc_operating_point(&self) -> Result<Vec<f64>, JsValue> {
        self.dc_inner().map_err(|e| JsValue::from_str(&e))
    }

    fn dc_inner(&self) -> Result<Vec<f64>, String> {
        let op = self
            .analyzer
            .dc_operating_point()
            .map_err(|e| e.to_string())?;
        Ok(op.node_voltages)
    }

    /// AC sweep of one node: interleaved `[f0, mag0, f1, mag1, …]`.
    ///
    /// `node` is a netlist node name; empty string uses the first non-ground node.
    pub fn ac_analysis(
        &self,
        start_hz: f64,
        stop_hz: f64,
        points_per_decade: u32,
        node: &str,
    ) -> Result<Vec<f64>, JsValue> {
        self.ac_inner(start_hz, stop_hz, points_per_decade, node)
            .map_err(|e| JsValue::from_str(&e))
    }

    fn ac_inner(
        &self,
        start_hz: f64,
        stop_hz: f64,
        points_per_decade: u32,
        node: &str,
    ) -> Result<Vec<f64>, String> {
        let id = self.resolve_node(node)?;
        let ac = self
            .analyzer
            .ac_analysis(start_hz, stop_hz, points_per_decade)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(ac.frequencies.len() * 2);
        for (i, &f) in ac.frequencies.iter().enumerate() {
            let mag = ac
                .node_voltages
                .get(i)
                .and_then(|row| row.get(id))
                .map(|z| z.abs())
                .unwrap_or(0.0);
            out.push(f);
            out.push(mag);
        }
        Ok(out)
    }

    /// Transient of one node: interleaved `[t0, v0, t1, v1, …]`.
    ///
    /// If `t_stop`/`t_step` are ≤ 0, values from the netlist `.tran` are used.
    /// `node` empty string uses the first non-ground node.
    pub fn transient(&self, node: &str, t_stop: f64, t_step: f64) -> Result<Vec<f64>, JsValue> {
        self.transient_inner(node, t_stop, t_step)
            .map_err(|e| JsValue::from_str(&e))
    }

    fn transient_inner(&self, node: &str, t_stop: f64, t_step: f64) -> Result<Vec<f64>, String> {
        let id = self.resolve_node(node)?;
        let (stop, step) = self.tran_params(t_stop, t_step)?;
        let tr = self
            .analyzer
            .transient(stop, step, 4.0)
            .map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(tr.times.len() * 2);
        for (i, &t) in tr.times.iter().enumerate() {
            let v = tr
                .node_voltages
                .get(i)
                .and_then(|row| row.get(id))
                .copied()
                .unwrap_or(0.0);
            out.push(t);
            out.push(v);
        }
        Ok(out)
    }

    fn resolve_node(&self, name: &str) -> Result<usize, String> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(1);
        }
        self.analyzer
            .circuit()
            .node_by_name(name)
            .ok_or_else(|| format!("unknown node {name:?}"))
    }

    fn tran_params(&self, t_stop: f64, t_step: f64) -> Result<(f64, f64), String> {
        use tpt_elec_spice_core::Analysis;
        let mut stop = t_stop;
        let mut step = t_step;
        if stop <= 0.0 || step <= 0.0 {
            for a in &self.analyzer.circuit().analyses {
                if let Analysis::Transient {
                    t_stop: ns,
                    t_step: nst,
                    ..
                } = a
                {
                    if stop <= 0.0 {
                        stop = *ns;
                    }
                    if step <= 0.0 {
                        step = *nst;
                    }
                    break;
                }
            }
        }
        if stop <= 0.0 {
            stop = 1e-3;
        }
        if step <= 0.0 {
            step = stop / 200.0;
        }
        Ok((stop, step))
    }
}

// ---------------------------------------------------------------------------
// Eye diagram
// ---------------------------------------------------------------------------

/// Eye-diagram analyzer over a sampled waveform.
#[wasm_bindgen]
pub struct WasmEyeDiagram {
    eye: tpt_elec_si_eye::EyeDiagram,
}

#[wasm_bindgen]
impl WasmEyeDiagram {
    /// Folds `waveform` into unit intervals at `bit_rate`.
    ///
    /// `samples_per_ui` is samples per unit interval (≥ 2).
    pub fn new(waveform: &[f64], bit_rate: f64, samples_per_ui: u32) -> WasmEyeDiagram {
        Self {
            eye: tpt_elec_si_eye::EyeDiagram::from_waveform(waveform, bit_rate, samples_per_ui),
        }
    }

    /// Vertical eye opening [V].
    pub fn eye_height(&self) -> f64 {
        self.eye.eye_height
    }

    /// Horizontal eye opening [s].
    pub fn eye_width(&self) -> f64 {
        self.eye.eye_width
    }

    /// Unit interval [s].
    pub fn ui(&self) -> f64 {
        self.eye.ui
    }

    /// Total peak-to-peak jitter [s].
    pub fn jitter_total(&self) -> f64 {
        self.eye.jitter.total_jitter
    }

    /// Random jitter σ [s].
    pub fn jitter_random(&self) -> f64 {
        self.eye.jitter.random_jitter
    }

    /// Deterministic jitter [s].
    pub fn jitter_deterministic(&self) -> f64 {
        self.eye.jitter.deterministic_jitter
    }

    /// Checks a standard mask by name (`"pcie-gen3"`, `"ddr4"`, …).
    ///
    /// Returns `[passed, margin_ui, margin_amplitude]`.
    pub fn check_mask(&self, mask: &str, amplitude_v: f64) -> Result<Vec<f64>, JsValue> {
        self.check_mask_inner(mask, amplitude_v)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Native-callable mask check (host tests use this; `JsValue` errors
    /// can only be materialized on wasm32).
    pub fn check_mask_inner(&self, mask: &str, amplitude_v: f64) -> Result<Vec<f64>, String> {
        let m = parse_mask(mask)?;
        let r = self.eye.check_mask_compliance(&m, amplitude_v);
        Ok(vec![
            if r.passed { 1.0 } else { 0.0 },
            r.margin_ui,
            r.margin_amplitude,
        ])
    }

    /// Checks a custom rectangular mask: forbidden `(half_ui, half_amplitude)`
    /// normalized to the UI and swing. Returns `[passed, margin_ui, margin_amplitude]`.
    pub fn check_mask_custom(
        &self,
        half_ui: f64,
        half_amplitude: f64,
        amplitude_v: f64,
    ) -> Vec<f64> {
        let m = tpt_elec_si_eye::EyeMask::Custom {
            half_ui,
            half_amplitude,
        };
        let r = self.eye.check_mask_compliance(&m, amplitude_v);
        vec![
            if r.passed { 1.0 } else { 0.0 },
            r.margin_ui,
            r.margin_amplitude,
        ]
    }
}

fn parse_mask(name: &str) -> Result<tpt_elec_si_eye::EyeMask, String> {
    use tpt_elec_si_eye::EyeMask;
    let n = name.trim().to_ascii_lowercase().replace(['_', '-'], "");
    Ok(match n.as_str() {
        "pciegen1" | "pcie1" => EyeMask::PcieGen1,
        "pciegen2" | "pcie2" => EyeMask::PcieGen2,
        "pciegen3" | "pcie3" => EyeMask::PcieGen3,
        "pciegen4" | "pcie4" => EyeMask::PcieGen4,
        "pciegen5" | "pcie5" => EyeMask::PcieGen5,
        "pciegen6" | "pcie6" => EyeMask::PcieGen6,
        "ddr4" => EyeMask::Ddr4,
        "ddr5" => EyeMask::Ddr5,
        "usb3" => EyeMask::Usb3,
        "usb4" => EyeMask::Usb4,
        "ethernet10g" | "10gbe" => EyeMask::Ethernet10G,
        "custom" => {
            return Err(
                "custom mask requires half_ui/half_amplitude — use check_mask_custom".into(),
            )
        }
        other => return Err(format!("unknown mask {other:?}")),
    })
}

// ---------------------------------------------------------------------------
// PDN
// ---------------------------------------------------------------------------

#[derive(Deserialize, serde::Serialize)]
struct PdnConfig {
    #[serde(default = "default_target_z")]
    target_impedance: f64,
    #[serde(default = "default_f0")]
    f_start: f64,
    #[serde(default = "default_f1")]
    f_end: f64,
    #[serde(default)]
    plane_capacitance: f64,
    #[serde(default)]
    vrm: VrmConfig,
    #[serde(default = "default_ppd")]
    points_per_decade: u32,
    #[serde(default)]
    caps: Vec<CapConfig>,
}

#[derive(Deserialize, Default, serde::Serialize)]
struct VrmConfig {
    #[serde(default = "default_vrm_r")]
    output_resistance: f64,
    #[serde(default = "default_vrm_bw")]
    bandwidth_hz: f64,
    #[serde(default = "default_vrm_l")]
    output_inductance: f64,
}

#[derive(Deserialize, serde::Serialize)]
struct CapConfig {
    value: f64,
    #[serde(default)]
    esr: f64,
    #[serde(default)]
    esl: f64,
    #[serde(default = "default_qty")]
    quantity: u32,
    #[serde(default)]
    mount_inductance: f64,
}

fn default_target_z() -> f64 {
    0.1
}
fn default_f0() -> f64 {
    1e4
}
fn default_f1() -> f64 {
    1e9
}
fn default_ppd() -> u32 {
    20
}
fn default_qty() -> u32 {
    1
}
fn default_vrm_r() -> f64 {
    0.05
}
fn default_vrm_bw() -> f64 {
    1e5
}
fn default_vrm_l() -> f64 {
    50e-9
}

impl From<CapConfig> for tpt_elec_pi_pdn::DecouplingCap {
    fn from(c: CapConfig) -> Self {
        Self {
            value: c.value,
            esr: c.esr,
            esl: c.esl,
            quantity: c.quantity.max(1),
            mount_inductance: c.mount_inductance,
        }
    }
}

/// PDN impedance analysis.
#[wasm_bindgen]
pub struct WasmPdnAnalysis {
    analysis: tpt_elec_pi_pdn::PdnAnalysis,
}

#[wasm_bindgen]
impl WasmPdnAnalysis {
    /// Builds a PDN from a JSON config:
    /// `{ "target_impedance": 0.1, "f_start": 1e4, "f_end": 1e9,
    ///    "plane_capacitance": 1e-8,
    ///    "vrm": { "output_resistance": 0.05, "bandwidth_hz": 1e5,
    ///             "output_inductance": 50e-9 },
    ///    "points_per_decade": 20,
    ///    "caps": [{ "value": 100e-9, "esr": 0.01, "esl": 0.5e-9,
    ///               "quantity": 4, "mount_inductance": 1e-9 }] }`
    pub fn new(config_json: &str) -> Result<WasmPdnAnalysis, JsValue> {
        Self::new_inner(config_json).map_err(|e| JsValue::from_str(&e))
    }

    /// Native-callable constructor (host tests use this).
    pub fn new_inner(config_json: &str) -> Result<WasmPdnAnalysis, String> {
        let cfg: PdnConfig = if config_json.trim().is_empty() {
            PdnConfig {
                target_impedance: default_target_z(),
                f_start: default_f0(),
                f_end: default_f1(),
                plane_capacitance: 10e-9,
                vrm: VrmConfig::default(),
                points_per_decade: default_ppd(),
                caps: Vec::new(),
            }
        } else {
            serde_json::from_str(config_json).map_err(|e| format!("pdn config json: {e}"))?
        };
        Ok(Self {
            analysis: tpt_elec_pi_pdn::PdnAnalysis {
                target_impedance: cfg.target_impedance,
                frequency_range: (cfg.f_start, cfg.f_end),
                decoupling_caps: cfg.caps.into_iter().map(Into::into).collect(),
                plane_capacitance: cfg.plane_capacitance,
                vrm_model: tpt_elec_pi_pdn::VrmModel {
                    output_resistance: cfg.vrm.output_resistance,
                    bandwidth_hz: cfg.vrm.bandwidth_hz,
                    output_inductance: cfg.vrm.output_inductance,
                },
                points_per_decade: cfg.points_per_decade,
            },
        })
    }

    /// Impedance profile: interleaved `[f0, |Z|0, f1, |Z|1, …]`.
    pub fn impedance_profile(&self) -> Vec<f64> {
        let p = self.analysis.impedance_profile();
        let mut out = Vec::with_capacity(p.frequencies.len() * 2);
        for (f, z) in p.frequencies.iter().zip(&p.impedance_mag) {
            out.push(*f);
            out.push(*z);
        }
        out
    }

    /// Peak |Z| in band [Ω].
    pub fn peak_impedance(&self) -> f64 {
        self.analysis.impedance_profile().peak_impedance
    }

    /// Frequency of the peak [Hz].
    pub fn peak_frequency(&self) -> f64 {
        self.analysis.impedance_profile().peak_frequency
    }

    /// Whether the peak meets `target_impedance`.
    pub fn target_met(&self) -> bool {
        self.analysis.impedance_profile().target_met
    }

    /// Greedy decoupling optimizer. `available` is a JSON array of cap
    /// objects (`value`, `esr`, `esl`, …); `budget` is max part-count picks.
    ///
    /// Returns a JSON array of chosen parts with quantities.
    pub fn optimize_decoupling(&self, available: &str, budget: u32) -> Result<String, JsValue> {
        self.optimize_inner(available, budget)
            .map_err(|e| JsValue::from_str(&e))
    }

    fn optimize_inner(&self, available: &str, budget: u32) -> Result<String, String> {
        let parts: Vec<CapConfig> =
            serde_json::from_str(available).map_err(|e| format!("available json: {e}"))?;
        let caps: Vec<tpt_elec_pi_pdn::DecouplingCap> = parts.into_iter().map(Into::into).collect();
        let chosen = self.analysis.optimize_decoupling(&caps, budget);
        let json: Vec<serde_json::Value> = chosen
            .iter()
            .map(|c| {
                serde_json::json!({
                    "value": c.value,
                    "esr": c.esr,
                    "esl": c.esl,
                    "quantity": c.quantity,
                    "mount_inductance": c.mount_inductance,
                })
            })
            .collect();
        serde_json::to_string(&json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GERBER: &str = "\
G04 wasm demo*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.250*%
G01*
D10*
X5000000Y5000000D02*
X30000000Y5000000D01*
M02*
";

    #[test]
    fn wasm_solver_round_trip() {
        let mut solver = WasmThermalSolver::new_inner(GERBER.as_bytes(), "").unwrap();
        assert_eq!(solver.grid_dimensions().len(), 3);
        let max_temp = solver.solve_inner(&[10.0, 10.0, 0.3]).unwrap();
        assert!(max_temp > 25.1, "max {max_temp}");
        let map = solver.get_temperature_map();
        assert!(!map.is_empty());
    }

    #[test]
    fn repeated_solves_are_idempotent() {
        // Regression (B5): a second solve must not stack BCs / sources.
        let mut solver = WasmThermalSolver::new_inner(GERBER.as_bytes(), "").unwrap();
        let first = solver.solve_inner(&[10.0, 10.0, 0.3]).unwrap();
        let second = solver.solve_inner(&[10.0, 10.0, 0.3]).unwrap();
        assert!((first - second).abs() < 1e-9, "{first} vs {second}");
    }

    #[test]
    fn wasm_impedance() {
        let z = WasmImpedanceCalculator::microstrip(0.35, 0.035, 0.2, 4.4);
        assert!((z - 50.0).abs() < 3.0, "{z}");
        let diff = WasmImpedanceCalculator::differential(0.2, 0.1, 0.2, 4.4);
        assert!(diff > 60.0 && diff < 160.0, "{diff}");
    }

    #[test]
    fn coordinate_without_fs_is_an_error() {
        // The parser tolerates junk text (no commands → empty image) but a
        // coordinate before any %FS format spec is a hard error.
        assert!(WasmThermalSolver::new_inner(b"garbage".as_slice(), "").is_ok());
        assert!(WasmThermalSolver::new_inner(b"X0Y0D02*".as_slice(), "").is_err());
    }

    #[test]
    fn stackup_json_parses() {
        let solver = WasmThermalSolver::new_inner(
            GERBER.as_bytes(),
            r#"{"thickness_mm": 1.0, "resolution_mm": 2.0}"#,
        )
        .unwrap();
        // 1 mm substrate with 0.285 mm cells: a handful of z layers
        assert!((2..=8).contains(&solver.grid_dimensions()[2]));
    }

    // -- SPICE ---------------------------------------------------------------

    const RC_NETLIST: &str = "\
RC low-pass demo
V1 in 0 DC 5 AC 1
R1 in out 1k
C1 out 0 1n
.tran 1n 10u
.ac dec 10 1k 10meg
.end
";

    #[test]
    fn spice_dc_and_node_names() {
        let a = WasmSpiceAnalyzer::new_inner(RC_NETLIST).unwrap();
        let names = a.node_names();
        assert!(names.contains(&"OUT".to_string()), "{names:?}");
        let v = a.dc_inner().unwrap();
        // DC: V1 drives in=5 V through R into C → out settles to 5 V
        assert!((v[1] - 5.0).abs() < 1e-6, "in {}", v[1]);
        assert!((v[2] - 5.0).abs() < 1e-3, "out {}", v[2]);
    }

    #[test]
    fn spice_ac_interleaved() {
        let a = WasmSpiceAnalyzer::new_inner(RC_NETLIST).unwrap();
        let ac = a.ac_inner(1e3, 1e6, 10, "out").unwrap();
        assert!(ac.len() >= 4 && ac.len() % 2 == 0);
        // Low frequency: |Vout| ≈ |Vin| = 1
        assert!((ac[1] - 1.0).abs() < 0.1, "mag {}", ac[1]);
        // High frequency corner rolls off
        let last_mag = *ac.last().unwrap();
        assert!(last_mag < ac[1], "roll-off {last_mag} vs {}", ac[1]);
    }

    #[test]
    fn spice_transient_from_netlist_tran() {
        let a = WasmSpiceAnalyzer::new_inner(RC_NETLIST).unwrap();
        let tr = a.transient_inner("out", 0.0, 0.0).unwrap();
        assert!(tr.len() >= 4 && tr.len() % 2 == 0);
        // Final sample approaches the 5 V DC drive
        let last_v = *tr.last().unwrap();
        assert!(last_v > 3.0, "final {}", last_v);
    }

    #[test]
    fn spice_unknown_node_errors() {
        let a = WasmSpiceAnalyzer::new_inner(RC_NETLIST).unwrap();
        assert!(a.ac_inner(1e3, 1e4, 5, "nope").is_err());
    }

    // -- Eye -----------------------------------------------------------------

    #[test]
    fn eye_mask_check() {
        use tpt_elec_si_eye::Prbs;
        let bit_rate = 80e6;
        let spu = 32u32;
        let ui = 1.0 / bit_rate;
        let dt = ui / spu as f64;
        let tau = 0.5e-9;
        let alpha = dt / (tau + dt);
        let mut prbs = Prbs::prbs7(0x41);
        let mut v = -0.5;
        let mut wf = Vec::new();
        for _ in 0..8 {
            let target = if prbs.next_bit() == 1 { 0.5 } else { -0.5 };
            for _ in 0..spu {
                v += alpha * (target - v);
                wf.push(v);
            }
        }
        let eye = WasmEyeDiagram::new(&wf, bit_rate, spu);
        assert!(eye.eye_height() > 0.5, "h {}", eye.eye_height());
        assert!(eye.ui() > 0.0);
        let r = eye.check_mask_inner("pcie-gen3", 1.0).unwrap();
        assert_eq!(r.len(), 3);
        // Clean eye vs a huge custom mask must fail
        let fail = eye.check_mask_custom(0.6, 0.6, 1.0);
        assert_eq!(fail[0], 0.0);
        assert!(eye.check_mask_inner("bogus", 1.0).is_err());
    }

    // -- PDN -----------------------------------------------------------------

    #[test]
    fn pdn_profile_and_optimize() {
        let cfg = r#"{
            "target_impedance": 0.1,
            "f_start": 1e4,
            "f_end": 1e9,
            "plane_capacitance": 10e-9,
            "vrm": { "output_resistance": 0.05, "bandwidth_hz": 1e5,
                     "output_inductance": 50e-9 },
            "points_per_decade": 10
        }"#;
        let pdn = WasmPdnAnalysis::new_inner(cfg).unwrap();
        let profile = pdn.impedance_profile();
        assert!(profile.len() >= 4 && profile.len() % 2 == 0);
        let bare_peak = pdn.peak_impedance();
        assert!(bare_peak > 0.0);

        let available = r#"[
            { "value": 100e-6, "esr": 0.02, "esl": 2e-9 },
            { "value": 1e-6,  "esr": 0.008, "esl": 0.8e-9 },
            { "value": 100e-9,"esr": 0.015,"esl": 0.6e-9 }
        ]"#;
        let plan = pdn.optimize_inner(available, 8).unwrap();
        assert!(plan.contains("value"));
        // Rebuild with the plan and confirm the peak drops
        let mut chosen: Vec<CapConfig> = serde_json::from_str(&plan).unwrap();
        for c in &mut chosen {
            if c.quantity == 0 {
                c.quantity = 1;
            }
        }
        let mut with = serde_json::from_str::<PdnConfig>(cfg).unwrap();
        with.caps = chosen;
        let with_json = serde_json::to_string(&with).unwrap();
        let better = WasmPdnAnalysis::new_inner(&with_json).unwrap();
        assert!(
            better.peak_impedance() < bare_peak,
            "{} vs {}",
            better.peak_impedance(),
            bare_peak
        );
    }

    #[test]
    fn pdn_empty_config_defaults() {
        let pdn = WasmPdnAnalysis::new_inner("").unwrap();
        assert!(pdn.peak_impedance() > 0.0);
        assert!(!pdn.target_met());
    }
}
