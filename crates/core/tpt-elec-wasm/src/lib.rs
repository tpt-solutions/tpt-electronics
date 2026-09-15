// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebAssembly bindings for tpt-electronics.
//!
//! Exposes thermal simulation and impedance calculation to browser-based
//! EDA tools. The API is intentionally small and `f64`-array based:
//!
//! * [`WasmThermalSolver`] — build a solver from Gerber bytes + a small
//!   JSON stackup description, apply a power map, solve, read temperatures.
//! * [`WasmImpedanceCalculator`] — microstrip/stripline/differential-pair
//!   impedance in a single call.
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
}
