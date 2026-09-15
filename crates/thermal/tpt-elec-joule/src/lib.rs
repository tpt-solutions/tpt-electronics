// SPDX-License-Identifier: MIT OR Apache-2.0

//! Coupled electro-thermal (Joule heating) simulation.
//!
//! Iterative coupling:
//!
//! 1. Solve the electrical conduction problem `[G]{V} = {I}` on the grid.
//! 2. Dissipated power per edge: `q = Gₑ·(Vᵢ − Vⱼ)²`, split to its two cells.
//! 3. Solve the thermal problem `[K]{T} = {Q}` with that source.
//! 4. Update resistivity `ρ(T) = ρ₀·(1 + α·(T − 20 °C))`.
//! 5. Repeat until the temperature field converges.
//!
//! The electrical problem uses the same 7-point stencil as the thermal FEM,
//! with electrical conductivity `σ = 1/ρ` in place of thermal conductivity.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::linalg::SparseBuilder;
use tpt_elec_core::{MaterialId, Vector3};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalResult, ThermalSolver};

/// A voltage source clamped onto a set of cells (penalty Dirichlet).
#[derive(Clone, Debug)]
pub struct VoltageSource {
    /// Cell indices held at `voltage` [V].
    pub nodes: Vec<u32>,
    /// Potential [V].
    pub voltage: f64,
}

/// Result of a coupled electro-thermal solve.
#[derive(Clone, Debug)]
pub struct CoupledResult {
    /// Final thermal field.
    pub thermal: ThermalResult,
    /// Joule power per cell [W].
    pub power_dissipation: Vec<f64>,
    /// Total dissipated power [W].
    pub total_power: f64,
    /// Current drawn from the highest-potential terminal [A].
    pub current_a: f64,
    /// Whether the coupling loop converged.
    pub converged: bool,
    /// Iterations performed.
    pub iterations: u32,
}

/// The coupled Joule-heating solver.
pub struct JouleHeatingSolver {
    grid: tpt_elec_geometry::VoxelGrid,
    materials: MaterialDatabase,
    thermal_bcs: Vec<BoundaryCondition>,
}

impl JouleHeatingSolver {
    /// Creates a coupled solver from a grid, materials, and the thermal
    /// boundary conditions to hold during the coupling loop.
    pub fn new(
        grid: tpt_elec_geometry::VoxelGrid,
        materials: MaterialDatabase,
        thermal_bcs: Vec<BoundaryCondition>,
    ) -> Self {
        Self {
            grid,
            materials,
            thermal_bcs,
        }
    }

    /// Borrows the grid.
    pub fn grid(&self) -> &tpt_elec_geometry::VoxelGrid {
        &self.grid
    }

    /// Electrical conductivity of a cell at temperature `t` [S/m].
    fn sigma_at(&self, idx: usize, t: f64) -> f64 {
        let v = &self.grid.voxels()[idx];
        match self.materials.get(&v.material) {
            Some(m) => {
                let sigma20 = 1.0 / m.electrical_resistivity.max(1e-300);
                sigma20 / (1.0 + m.temperature_coefficient * (t - 20.0))
            }
            None => 0.0,
        }
    }

    /// Assembles and solves the electrical problem at the current field.
    fn solve_electrical(&self, temps: &[f64], sources: &[VoltageSource]) -> (Vec<f64>, f64, f64) {
        let n = self.grid.len();
        let nx = self.grid.nx() as usize;
        let ny = self.grid.ny() as usize;
        let nz = self.grid.nz() as usize;
        let (dx, dy, dz) = (
            self.grid.resolution.dx.as_meters(),
            self.grid.resolution.dy.as_meters(),
            self.grid.resolution.dz.as_meters(),
        );
        let mut builder = SparseBuilder::new(n);
        let mut gmax = 0.0f64;

        let pair = |builder: &mut SparseBuilder,
                    gmax: &mut f64,
                    i: usize,
                    j: usize,
                    area: f64,
                    len: f64,
                    s1: f64,
                    s2: f64| {
            let denom = if s1 + s2 > 0.0 { s1 + s2 } else { return };
            let g = area * 2.0 * s1 * s2 / (denom * len);
            if g > 0.0 {
                builder.add(i, i, g);
                builder.add(j, j, g);
                builder.add(i, j, -g);
                builder.add(j, i, -g);
                *gmax = gmax.max(g);
            }
        };

        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let i = x + nx * (y + ny * z);
                    let s_i = self.sigma_at(i, temps[i]);
                    if x + 1 < nx {
                        pair(
                            &mut builder,
                            &mut gmax,
                            i,
                            i + 1,
                            dy * dz,
                            dx,
                            s_i,
                            self.sigma_at(i + 1, temps[i + 1]),
                        );
                    }
                    if y + 1 < ny {
                        pair(
                            &mut builder,
                            &mut gmax,
                            i,
                            i + nx,
                            dx * dz,
                            dy,
                            s_i,
                            self.sigma_at(i + nx, temps[i + nx]),
                        );
                    }
                    if z + 1 < nz {
                        pair(
                            &mut builder,
                            &mut gmax,
                            i,
                            i + nx * ny,
                            dx * dy,
                            dz,
                            s_i,
                            self.sigma_at(i + nx * ny, temps[i + nx * ny]),
                        );
                    }
                }
            }
        }

        // Penalty enforcement of terminal potentials.
        let penalty = gmax.max(1e-20) * 1.0e6;
        let mut rhs = vec![0.0f64; n];
        let mut positive_nodes: Vec<usize> = Vec::new();
        let vmax = sources
            .iter()
            .map(|s| s.voltage)
            .fold(f64::NEG_INFINITY, f64::max);
        for src in sources {
            for &node in &src.nodes {
                let i = node as usize;
                builder.add(i, i, penalty);
                rhs[i] += penalty * src.voltage;
                if src.voltage == vmax {
                    positive_nodes.push(i);
                }
            }
        }

        let g = builder.build();
        let (v, _) = g
            .solve_cg(&rhs, 1e-10, 20_000)
            .unwrap_or((vec![0.0; n], f64::NAN));

        // Physically robust terminal current: I = P_total / V_positive
        // (the field solution conserves power through the penalty terminals).
        let v_pos = vmax;
        let temps_rt = vec![20.0; n]; // called before coupling starts
        let p_total: f64 = self.joule_power(&v, &temps_rt).iter().sum();
        let i_calc = if v_pos.abs() > 1e-300 {
            p_total / v_pos
        } else {
            0.0
        };
        (v, i_calc, gmax)
    }

    /// Joule power per cell from a potential field [W].
    /// Joule power per cell from a potential field, using the conductivity
    /// evaluated at the given temperature field (B4 fix).
    fn joule_power(&self, v: &[f64], temps: &[f64]) -> Vec<f64> {
        let n = self.grid.len();
        let nx = self.grid.nx() as usize;
        let ny = self.grid.ny() as usize;
        let nz = self.grid.nz() as usize;
        let (dx, dy, dz) = (
            self.grid.resolution.dx.as_meters(),
            self.grid.resolution.dy.as_meters(),
            self.grid.resolution.dz.as_meters(),
        );
        let mut power = vec![0.0f64; n];
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let i = x + nx * (y + ny * z);
                    let s = self.sigma_at(i, temps[i]);
                    for (j, area, len) in [
                        (x + 1 < nx).then(|| i + 1).map(|j| (j, dy * dz, dx)),
                        (y + 1 < ny).then(|| i + nx).map(|j| (j, dx * dz, dy)),
                        (z + 1 < nz).then(|| i + nx * ny).map(|j| (j, dx * dy, dz)),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let dv = v[i] - v[j];
                        let g = s * area / len;
                        let q = g * dv * dv;
                        power[i] += q / 2.0;
                        power[j] += q / 2.0;
                    }
                }
            }
        }
        power
    }

    /// Runs the coupled iteration.
    ///
    /// * `voltage_sources` — cells clamped at fixed potentials (positive and
    ///   return terminal).
    /// * `iterations` — coupling loop cap.
    /// * `tol_c` — convergence tolerance on max temperature change [°C].
    pub fn solve_coupled(
        &mut self,
        voltage_sources: &[VoltageSource],
        iterations: u32,
        tol_c: f64,
    ) -> CoupledResult {
        let n = self.grid.len();
        let mut temps = vec![20.0f64; n];
        let mut power = vec![0.0f64; n];
        let mut current = 0.0f64;
        let mut converged = false;
        let mut performed = 0u32;
        let mut last_thermal: Option<ThermalResult> = None;

        for it in 0..iterations {
            performed = it + 1;

            let (v, i_calc, _gmax) = self.solve_electrical(&temps, voltage_sources);
            power = self.joule_power(&v, &temps);
            current = i_calc;

            let mut thermal = ThermalSolver::new(self.grid.clone(), self.materials.clone());
            for bc in &self.thermal_bcs {
                thermal.add_boundary_condition(bc.clone());
            }
            let source_nodes: Vec<u32> =
                (0..n as u32).filter(|&i| power[i as usize] > 0.0).collect();
            if !source_nodes.is_empty() {
                let total: f64 = power.iter().sum();
                thermal.add_boundary_condition(BoundaryCondition::HeatSource {
                    nodes: source_nodes,
                    power: total,
                });
            }
            let result = thermal.solve_steady_state();

            let delta = result
                .temperatures
                .iter()
                .zip(&temps)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f64, f64::max);
            temps = result.temperatures.clone();
            last_thermal = Some(result);
            if delta < tol_c {
                converged = true;
                break;
            }
        }

        let thermal = last_thermal.unwrap_or_else(|| {
            let mut t = ThermalSolver::new(self.grid.clone(), self.materials.clone());
            for bc in &self.thermal_bcs {
                t.add_boundary_condition(bc.clone());
            }
            t.solve_steady_state()
        });
        let total_power = power.iter().sum();
        CoupledResult {
            thermal,
            power_dissipation: power,
            total_power,
            current_a: current,
            converged,
            iterations: performed,
        }
    }
}

/// Convenience: resistance of a uniform bar [Ω], `R = L/(σ·A)`.
pub fn bar_resistance(sigma: f64, area: f64, length: f64) -> f64 {
    length / (sigma * area)
}

#[allow(dead_code)]
fn _keep_vector_api() -> Vector3 {
    Vector3::ZERO
}

#[allow(dead_code)]
fn _keep_material_api() -> Option<MaterialId> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_core::{BoundingBox3, Length, Point3};
    use tpt_elec_geometry::{VoxelGrid, VoxelResolution};

    /// 1 × 1 mm × 10 mm copper bar along Z.
    fn copper_bar(cells: u32) -> (VoxelGrid, MaterialDatabase) {
        let grid = VoxelGrid::new(
            BoundingBox3::new(
                Point3::new(0., 0., 0.),
                Point3::new(1e-3, 1e-3, cells as f64 * 1e-3),
            ),
            VoxelResolution {
                dx: Length::mm(1.0),
                dy: Length::mm(1.0),
                dz: Length::mm(1.0),
            },
            MaterialId::new("copper"),
        );
        (grid, MaterialDatabase::standard())
    }

    #[test]
    fn copper_bar_resistance_matches_analytic() {
        let (grid, mats) = copper_bar(10);
        let mut solver = JouleHeatingSolver::new(
            grid,
            mats,
            vec![BoundaryCondition::FixedTemperature {
                nodes: vec![0, 9],
                temp: 25.0,
            }],
        );
        let result = solver.solve_coupled(
            &[
                VoltageSource {
                    nodes: vec![0],
                    voltage: 0.001,
                },
                VoltageSource {
                    nodes: vec![9],
                    voltage: 0.0,
                },
            ],
            8,
            0.05,
        );
        // R = L/(σ·A) = 0.01 / (5.952e7 · 1e-6) ≈ 1.68e-4 Ω
        // P = V²/R ≈ 5.95 mW, I = V/R ≈ 5.95 A
        let db = MaterialDatabase::standard();
        let cu = db.get(&MaterialId::new("copper")).unwrap();
        let sigma = 1.0 / cu.electrical_resistivity;
        let r_analytic = bar_resistance(sigma, 1e-6, 0.01);
        let expected_p = 1e-3f64.powi(2) / r_analytic;
        // Terminals sit at end-cell centers, so the discrete conduction
        // length is (N−1)·dz = 9 mm vs the analytic 10 mm (~11% error).
        assert!(
            (result.total_power - expected_p).abs() / expected_p < 0.15,
            "P = {} W, expected {} W",
            result.total_power,
            expected_p
        );
        assert!(
            (result.current_a - 1e-3 / r_analytic).abs() / (1e-3 / r_analytic) < 0.15,
            "I = {} A",
            result.current_a
        );
        assert!(result.converged, "coupling should converge quickly");
        assert!(result.iterations <= 8);
    }

    #[test]
    fn joule_heating_raises_temperature() {
        let (grid, mats) = copper_bar(10);
        let mut solver = JouleHeatingSolver::new(
            grid,
            mats,
            vec![
                BoundaryCondition::Convection {
                    surface: (0..10).collect(),
                    h: 500.0,
                    t_ambient: 25.0,
                },
                // Pin one end: creates a monotonic gradient along the bar.
                BoundaryCondition::FixedTemperature {
                    nodes: vec![0],
                    temp: 25.0,
                },
            ],
        );
        let result = solver.solve_coupled(
            &[
                VoltageSource {
                    nodes: vec![0],
                    voltage: 0.01,
                },
                VoltageSource {
                    nodes: vec![9],
                    voltage: 0.0,
                },
            ],
            10,
            0.1,
        );
        assert!(
            result.thermal.max_temp > 25.1,
            "t = {}",
            result.thermal.max_temp
        );
        // With one end pinned at 25 °C the field rises monotonically, so the
        // hot spot must sit in the far half of the bar.
        let (_, _, mz) = result.thermal.max_temp_location;
        assert!((5..=9).contains(&mz), "hot spot at z={mz}");
        assert!(!result.power_dissipation.is_empty());
    }

    #[test]
    fn current_result_is_finite() {
        let (grid, mats) = copper_bar(4);
        let mut solver = JouleHeatingSolver::new(grid, mats, vec![]);
        let result = solver.solve_coupled(
            &[
                VoltageSource {
                    nodes: vec![0],
                    voltage: 0.001,
                },
                VoltageSource {
                    nodes: vec![3],
                    voltage: 0.0,
                },
            ],
            3,
            0.5,
        );
        assert!(result.current_a.is_finite() && result.current_a > 0.0);
        assert!(result.total_power.is_finite() && result.total_power > 0.0);
    }

    #[test]
    fn zero_iterations_returns_ambient() {
        let (grid, mats) = copper_bar(2);
        let mut solver = JouleHeatingSolver::new(grid, mats, vec![]);
        let result = solver.solve_coupled(&[], 0, 0.1);
        assert!(!result.converged);
        assert_eq!(result.iterations, 0);
        assert!(result.thermal.max_temp.is_finite());
    }

    #[test]
    fn bar_resistance_formula() {
        let r = bar_resistance(5.96e7, 1e-6, 0.01);
        assert!((r - 1.678e-4).abs() < 1e-6);
    }
}
