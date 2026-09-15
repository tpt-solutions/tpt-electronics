// SPDX-License-Identifier: MIT OR Apache-2.0

//! Steady-state thermal FEM for PCBs.
//!
//! The board is discretized as a voxel (hexahedral) grid — see
//! `tpt_elec_geometry::VoxelGrid`. Element conductances use a 7-point stencil
//! with harmonic-mean material pairing, so anisotropic substrates (FR4's
//! in-plane vs through-thickness conductivity) are handled naturally.
//!
//! The global system `[K]{T} = {Q}` is solved with Conjugate Gradient on a
//! CSR matrix (see [`tpt_elec_core::linalg`]). Dirichlet (fixed-temperature)
//! conditions use a symmetric penalty, preserving positive definiteness.
//!
//! # Boundary conditions
//!
//! * [`BoundaryCondition::FixedTemperature`] — penalty Dirichlet
//! * [`BoundaryCondition::Convection`] — h·A·(T − T∞) on surface nodes
//! * [`BoundaryCondition::Radiation`] — linearized εσ(T⁴ − T∞⁴), iterated
//! * [`BoundaryCondition::HeatFlux`] — prescribed W/m² on surface nodes
//! * [`BoundaryCondition::HeatSource`] — lumped W into node groups
//!
//! Temperatures are in degrees Celsius throughout.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::linalg::{SparseBuilder, SparseMatrix};
use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point2, Point3, Vector3};
use tpt_elec_geometry::{point_in_polygon, VoxelGrid, VoxelResolution};
use tpt_elec_gerber::{ParsedGerber, Primitive};
use tpt_elec_materials::{Axis, MaterialDatabase};

/// Stefan–Boltzmann constant [W/(m²·K⁴)].
pub const STEFAN_BOLTZMANN: f64 = 5.670374419e-8;

/// A thermal load or constraint.
#[derive(Clone, Debug)]
pub enum BoundaryCondition {
    /// Prescribed nodal temperatures (penalty enforcement).
    FixedTemperature {
        /// Cell indices (flattened).
        nodes: Vec<u32>,
        /// Temperature [°C].
        temp: f64,
    },
    /// Convective cooling with coefficient `h` [W/(m²·K)] on surface cells.
    Convection {
        /// Cell indices exposed to the coolant.
        surface: Vec<u32>,
        /// Heat transfer coefficient.
        h: f64,
        /// Coolant temperature [°C].
        t_ambient: f64,
    },
    /// Radiative cooling, linearized about the current solution and iterated.
    Radiation {
        /// Cell indices of the radiating surface.
        surface: Vec<u32>,
        /// Surface emissivity (0–1).
        emissivity: f64,
        /// Surroundings temperature [°C].
        t_surroundings: f64,
    },
    /// Prescribed heat flux [W/m²] on surface cells.
    HeatFlux {
        /// Cell indices.
        surface: Vec<u32>,
        /// Flux (positive = into the board).
        flux: f64,
    },
    /// Lumped power dissipation [W] spread over the given cells.
    HeatSource {
        /// Cell indices.
        nodes: Vec<u32>,
        /// Total power [W].
        power: f64,
    },
}

/// Result of a steady-state solve.
#[derive(Clone, Debug)]
pub struct ThermalResult {
    /// Nodal temperatures [°C] in flattened grid order.
    pub temperatures: Vec<f64>,
    /// Maximum temperature [°C].
    pub max_temp: f64,
    /// Grid cell (x, y, z) of the maximum.
    pub max_temp_location: (u32, u32, u32),
    /// Cell-centered heat flux vectors [W/m²].
    pub heat_flux: Vec<Vector3>,
    /// Achieved CG relative residual.
    pub residual: f64,
}

/// The steady-state thermal solver.
pub struct ThermalSolver {
    grid: VoxelGrid,
    materials: MaterialDatabase,
    boundary_conditions: Vec<BoundaryCondition>,
    /// Default temperature for unconstrained problems [°C].
    ambient: f64,
}

impl ThermalSolver {
    /// Creates a solver over a voxel grid.
    pub fn new(grid: VoxelGrid, materials: MaterialDatabase) -> Self {
        Self {
            grid,
            materials,
            boundary_conditions: Vec::new(),
            ambient: 25.0,
        }
    }

    /// Builds a simple two-layer board from a parsed Gerber layer.
    ///
    /// The domain spans the Gerber bounding box (plus a 2 mm margin), the
    /// substrate is 1.6 mm FR4, and copper primitives from the parsed layer
    /// are rasterized into a 35 µm top-copper sheet. This is intentionally a
    /// coarse "copper coverage" approximation suitable for early thermal
    /// exploration, not EM-accurate extraction.
    pub fn from_gerber(
        gerber: &ParsedGerber,
        materials: &MaterialDatabase,
        resolution: f64,
    ) -> Self {
        Self::from_gerber_scaled(gerber, materials, resolution, 1.6e-3, 35e-6)
    }

    /// Like [`from_gerber`](Self::from_gerber) with explicit substrate and
    /// copper thicknesses [m].
    pub fn from_gerber_scaled(
        gerber: &ParsedGerber,
        materials: &MaterialDatabase,
        resolution: f64,
        substrate_thickness_m: f64,
        copper_thickness_m: f64,
    ) -> Self {
        let (min, max) = gerber
            .bounding_box()
            .unwrap_or((Point2::new(0.0, 0.0), Point2::new(20.0e-3, 20.0e-3)));
        let margin = 2.0e-3;
        let copper_t = Length::meters(copper_thickness_m);
        let substrate_t = Length::meters(substrate_thickness_m);
        let bounds = BoundingBox3::new(
            Point3::new(min.x - margin, min.y - margin, 0.0),
            Point3::new(
                max.x + margin,
                max.y + margin,
                copper_t.as_meters() + substrate_t.as_meters(),
            ),
        );
        let grid = VoxelGrid::new(
            bounds,
            VoxelResolution {
                dx: Length::meters(resolution),
                dy: Length::meters(resolution),
                // two z-layers: copper sheet + substrate slab
                dz: Length::meters(copper_t.as_meters() + substrate_t.as_meters() / 4.0),
            },
            MaterialId::new("fr4"),
        );
        let mut solver = Self::new(grid, materials.clone());
        solver.grid.voxels_mut().iter_mut().for_each(|v| {
            v.material = MaterialId::new("fr4");
        });
        // Top copper sheet cells
        let z_top = solver.grid.nz().saturating_sub(1);
        let copper = MaterialId::new("copper");
        for z in z_top..solver.grid.nz() {
            for y in 0..solver.grid.ny() {
                for x in 0..solver.grid.nx() {
                    let c = solver.grid.center_of(x, y, z);
                    if gerber_covers(gerber, c.x, c.y) {
                        if let Some(v) = solver.grid.get_mut(x, y, z) {
                            v.material = copper.clone();
                        }
                    }
                }
            }
        }
        solver
    }

    /// Adds a boundary condition.
    pub fn add_boundary_condition(&mut self, bc: BoundaryCondition) {
        self.boundary_conditions.push(bc);
    }

    /// All boundary conditions (for inspection).
    pub fn boundary_conditions(&self) -> &[BoundaryCondition] {
        &self.boundary_conditions
    }

    /// Borrows the grid.
    pub fn grid(&self) -> &VoxelGrid {
        &self.grid
    }

    /// Sets the reference ambient temperature [°C] for unconditioned nodes.
    pub fn set_ambient(&mut self, t_c: f64) {
        self.ambient = t_c;
    }

    /// Adds a lumped heat source at the cells nearest (x, y) in the top layer.
    pub fn add_heat_source_near(&mut self, x: f64, y: f64, power: f64) {
        let nx = self.grid.nx();
        let ny = self.grid.ny();
        let z = self.grid.nz().saturating_sub(1);
        let gx = ((x - self.grid.bounds.min.x) / self.grid.resolution.dx.as_meters())
            .floor()
            .clamp(0.0, nx as f64 - 1.0) as u32;
        let gy = ((y - self.grid.bounds.min.y) / self.grid.resolution.dy.as_meters())
            .floor()
            .clamp(0.0, ny as f64 - 1.0) as u32;
        if let Some(idx) = self.grid.index(gx, gy, z) {
            self.boundary_conditions
                .push(BoundaryCondition::HeatSource {
                    nodes: vec![idx as u32],
                    power,
                });
        }
    }

    /// Assembles the transient system `(K + C/Δt)·T = Q + (C/Δt)·T_prev`.
    ///
    /// Exposed for time-integration crates ([`tpt_elec_transient`]); the
    /// capacitance matrix is the caller's lumped `C` [J/K] per cell.
    pub fn assemble_transient_step(
        &self,
        capacitance: &[f64],
        t_prev: &[f64],
        dt: f64,
    ) -> (SparseMatrix, Vec<f64>, f64) {
        self.assemble(&[], Some((capacitance, t_prev, dt)))
    }

    /// Assembles the steady-state system `(K, Q)` without mass terms.
    /// Exposed for time-integration crates that implement their own
    /// integration schemes (e.g. explicit Euler).
    pub fn assemble_system(&self) -> (SparseMatrix, Vec<f64>, f64) {
        self.assemble(&[], None)
    }

    /// Assembles the conductance matrix and load vector.
    fn assemble(
        &self,
        h_rad: &[f64],
        mass: Option<(&[f64], &[f64], f64)>,
    ) -> (SparseMatrix, Vec<f64>, f64) {
        let total = self.grid.len();
        let mut builder = SparseBuilder::new(total);
        let mut q = vec![0.0; total];
        let nx = self.grid.nx() as usize;
        let ny = self.grid.ny() as usize;
        let nz = self.grid.nz() as usize;
        let r = &self.grid.resolution;
        let (dx, dy, dz) = (r.dx.as_meters(), r.dy.as_meters(), r.dz.as_meters());
        let mut gmax: f64 = 0.0;

        let k_at = |idx: usize, axis: Axis| -> f64 {
            let v = &self.grid.voxels()[idx];
            match self.materials.get(&v.material) {
                Some(m) => m.thermal_conductivity.along(axis),
                None => 0.3, // conservative default for unknown materials
            }
        };

        let pair = |builder: &mut SparseBuilder,
                    gmax: &mut f64,
                    i: usize,
                    j: usize,
                    area: f64,
                    len: f64,
                    k1: f64,
                    k2: f64| {
            // Series (harmonic mean) conductance across the interface.
            let denom = if k1 + k2 > 0.0 { k1 + k2 } else { 1e-300 };
            let g = area * 2.0 * k1 * k2 / (denom * len);
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
                    if x + 1 < nx {
                        pair(
                            &mut builder,
                            &mut gmax,
                            i,
                            i + 1,
                            dy * dz,
                            dx,
                            k_at(i, Axis::X),
                            k_at(i + 1, Axis::X),
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
                            k_at(i, Axis::Y),
                            k_at(i + nx, Axis::Y),
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
                            k_at(i, Axis::Z),
                            k_at(i + nx * ny, Axis::Z),
                        );
                    }
                }
            }
        }

        let top_area = dx * dy;
        let penalty_scale = gmax.max(1e-12) * 1.0e6;
        for bc in &self.boundary_conditions {
            match bc {
                BoundaryCondition::FixedTemperature { nodes, temp } => {
                    for &node in nodes {
                        let n = node as usize;
                        if n < total {
                            builder.add(n, n, penalty_scale);
                            q[n] += penalty_scale * temp;
                        }
                    }
                }
                BoundaryCondition::Convection {
                    surface,
                    h,
                    t_ambient,
                } => {
                    for &node in surface {
                        let n = node as usize;
                        if n < total {
                            builder.add(n, n, h * top_area);
                            q[n] += h * top_area * t_ambient;
                        }
                    }
                }
                BoundaryCondition::Radiation {
                    surface,
                    emissivity,
                    t_surroundings,
                } => {
                    for &node in surface {
                        let n = node as usize;
                        if n >= total {
                            continue;
                        }
                        let hr = h_rad.get(n).copied().unwrap_or(0.0);
                        builder.add(n, n, hr * top_area);
                        q[n] += hr * top_area * *t_surroundings;
                        let _ = emissivity; // applied during linearization loop
                    }
                }
                BoundaryCondition::HeatFlux { surface, flux } => {
                    for &node in surface {
                        let n = node as usize;
                        if n < total {
                            q[n] += flux * top_area;
                        }
                    }
                }
                BoundaryCondition::HeatSource { nodes, power } => {
                    if !nodes.is_empty() {
                        let per = power / nodes.len() as f64;
                        for &node in nodes {
                            let n = node as usize;
                            if n < total {
                                q[n] += per;
                            }
                        }
                    }
                }
            }
        }
        // Lumped mass terms for transient integration.
        if let Some((c, t_prev, dt)) = mass {
            for i in 0..total {
                let m = c[i] / dt;
                builder.add(i, i, m);
                q[i] += m * t_prev[i];
            }
        }
        (builder.build(), q, penalty_scale)
    }

    /// Solves `[K]{T} = {Q}` for the steady state.
    pub fn solve_steady_state(&self) -> ThermalResult {
        let n = self.grid.len();
        let has_constraint = self.boundary_conditions.iter().any(|bc| {
            matches!(
                bc,
                BoundaryCondition::FixedTemperature { .. }
                    | BoundaryCondition::Convection { .. }
                    | BoundaryCondition::Radiation { .. }
            )
        });
        if !has_constraint {
            // Unconstrained: uniform ambient.
            let temps = vec![self.ambient; n];
            return self.make_result(temps, 0.0);
        }

        let mut h_rad = vec![0.0f64; n];
        let mut temps: Option<Vec<f64>> = None;
        let mut residual = f64::NAN;
        // Iterate radiation linearization (harmless when no Radiation BCs).
        for _ in 0..4 {
            let (k, q, _penalty) = self.assemble(&h_rad, None);
            let (t, rel) = k
                .solve_cg(&q, 1e-10, 20_000)
                .unwrap_or((q.iter().map(|_| self.ambient).collect(), f64::NAN));
            residual = rel;
            // Update linearized radiation coefficients from current temps.
            let mut updated = false;
            if self
                .boundary_conditions
                .iter()
                .any(|bc| matches!(bc, BoundaryCondition::Radiation { .. }))
            {
                for bc in self.boundary_conditions.iter() {
                    if let BoundaryCondition::Radiation {
                        surface,
                        emissivity,
                        t_surroundings,
                    } = bc
                    {
                        for &node in surface {
                            let node = node as usize;
                            let ts = t.get(node).copied().unwrap_or(*t_surroundings);
                            let ts_k = ts + 273.15;
                            let tu_k = t_surroundings + 273.15;
                            let hr = emissivity
                                * STEFAN_BOLTZMANN
                                * (ts_k + tu_k)
                                * (ts_k * ts_k + tu_k * tu_k);
                            if (hr - h_rad[node]).abs() > 1e-9 {
                                updated = true;
                            }
                            h_rad[node] = hr;
                        }
                    }
                }
            }
            temps = Some(t);
            if !updated {
                break;
            }
        }
        self.make_result(temps.unwrap_or_default(), residual)
    }

    fn make_result(&self, temperatures: Vec<f64>, residual: f64) -> ThermalResult {
        let mut max_temp = f64::NEG_INFINITY;
        let mut max_idx = 0usize;
        for (i, &t) in temperatures.iter().enumerate() {
            if t > max_temp {
                max_temp = t;
                max_idx = i;
            }
        }
        let nx = self.grid.nx() as usize;
        let ny = self.grid.ny() as usize;
        let mz = max_idx / (nx * ny);
        let my = (max_idx % (nx * ny)) / nx;
        let mx = max_idx % nx;

        // Cell-centered heat flux by central differences.
        let k_axis = |idx: usize, axis: Axis| -> f64 {
            let v = &self.grid.voxels()[idx];
            self.materials
                .get(&v.material)
                .map(|m| m.thermal_conductivity.along(axis))
                .unwrap_or(0.3)
        };
        let nz = self.grid.nz() as usize;
        let (dx, dy, dz) = (
            self.grid.resolution.dx.as_meters(),
            self.grid.resolution.dy.as_meters(),
            self.grid.resolution.dz.as_meters(),
        );
        let t_at = |x: i64, y: i64, z: i64| -> Option<f64> {
            if x < 0 || y < 0 || z < 0 || x >= nx as i64 || y >= ny as i64 || z >= nz as i64 {
                None
            } else {
                temperatures
                    .get((x as usize) + nx * (y as usize) + nx * ny * (z as usize))
                    .copied()
            }
        };
        let mut heat_flux = Vec::with_capacity(temperatures.len());
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let i = x + nx * (y + ny * z);
                    let (xl, xr) = (
                        t_at(x as i64 - 1, y as i64, z as i64).unwrap_or(temperatures[i]),
                        t_at(x as i64 + 1, y as i64, z as i64).unwrap_or(temperatures[i]),
                    );
                    let (yl, yr) = (
                        t_at(x as i64, y as i64 - 1, z as i64).unwrap_or(temperatures[i]),
                        t_at(x as i64, y as i64 + 1, z as i64).unwrap_or(temperatures[i]),
                    );
                    let (zl, zr) = (
                        t_at(x as i64, y as i64, z as i64 - 1).unwrap_or(temperatures[i]),
                        t_at(x as i64, y as i64, z as i64 + 1).unwrap_or(temperatures[i]),
                    );
                    heat_flux.push(Vector3::new(
                        -k_axis(i, Axis::X) * (xr - xl) / (2.0 * dx),
                        -k_axis(i, Axis::Y) * (yr - yl) / (2.0 * dy),
                        -k_axis(i, Axis::Z) * (zr - zl) / (2.0 * dz),
                    ));
                }
            }
        }

        ThermalResult {
            temperatures,
            max_temp,
            max_temp_location: (mx as u32, my as u32, mz as u32),
            heat_flux,
            residual,
        }
    }
}

/// Whether a parsed Gerber image has copper covering point (x, y).
fn gerber_covers(g: &ParsedGerber, x: f64, y: f64) -> bool {
    let p = Point2::new(x, y);
    for prim in &g.primitives {
        let hit = match prim {
            Primitive::Line { start, end, width } => {
                point_segment_distance(p, *start, *end) <= width.as_meters() / 2.0
            }
            Primitive::Arc {
                start, end, center, ..
            } => {
                // Conservative: treat as straight chord.
                point_segment_distance(p, *start, *end) <= 2.0 * center.distance_to(start)
            }
            Primitive::Flash { position, aperture } => {
                let r = aperture.approx_half_extent();
                position.distance_to(&p) <= r
            }
            Primitive::Region { boundary, .. } => point_in_polygon(p, boundary),
        };
        if hit {
            return true;
        }
    }
    false
}

fn point_segment_distance(p: Point2, a: Point2, b: Point2) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len2 = abx * abx + aby * aby;
    if len2 == 0.0 {
        return p.distance_to(&a);
    }
    let t = (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0);
    let proj = Point2::new(a.x + t * abx, a.y + t * aby);
    p.distance_to(&proj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_geometry::VoxelResolution;

    fn one_cell_grid() -> VoxelGrid {
        VoxelGrid::new(
            BoundingBox3::new(Point3::new(0., 0., 0.), Point3::new(1e-3, 1e-3, 1e-3)),
            VoxelResolution {
                dx: Length::mm(1.0),
                dy: Length::mm(1.0),
                dz: Length::mm(1.0),
            },
            MaterialId::new("copper"),
        )
    }

    fn column_grid(cells: u32) -> VoxelGrid {
        // 1D column: 1 mm × 1 mm cross-section, `cells` mm tall, copper.
        VoxelGrid::new(
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
        )
    }

    #[test]
    fn lumped_convection_single_cell() {
        // One copper cell, 1 W source, convection h: T = T∞ + P/(h·A)
        let grid = one_cell_grid();
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let area = 1e-3 * 1e-3; // dx*dy
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 1000.0,
            t_ambient: 25.0,
        });
        solver.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![0],
            power: 1.0,
        });
        let r = solver.solve_steady_state();
        let expected = 25.0 + 1.0 / (1000.0 * area);
        assert!(
            (r.max_temp - expected).abs() < expected * 1e-6,
            "got {}, expected {}",
            r.max_temp,
            expected
        );
        assert_eq!(r.max_temp_location, (0, 0, 0));
    }

    #[test]
    fn one_d_slab_conduction_is_linear() {
        // 10 mm copper column, ends pinned at 100 °C and 0 °C.
        let grid = column_grid(10);
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let n = solver.grid().len();
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![0],
            temp: 100.0,
        });
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![(n - 1) as u32],
            temp: 0.0,
        });
        let r = solver.solve_steady_state();
        assert!(r.residual < 1e-8);
        assert!((r.max_temp - 100.0).abs() < 0.1, "max {}", r.max_temp);
        for (i, &t) in r.temperatures.iter().enumerate() {
            let expected = 100.0 * (1.0 - i as f64 / 9.0);
            assert!(
                (t - expected).abs() < 0.2,
                "T[{i}] = {t}, expected {expected}"
            );
        }
    }

    #[test]
    fn slab_flux_matches_fourier() {
        // Heat flow through the 1 mm² column: Q = k·A·ΔT/L
        let grid = column_grid(10);
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let n = solver.grid().len();
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![0],
            temp: 100.0,
        });
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![(n - 1) as u32],
            temp: 0.0,
        });
        let r = solver.solve_steady_state();
        // Cell centers of the 10 cells span 9·dx = 9 mm; gradient is exact for
        // the linear profile: q/A = k·ΔT/((N-1)·dx) = 385·100/0.009
        let mid = r.heat_flux[5];
        let expected_flux = 385.0 * 100.0 / (9.0 * 1e-3);
        assert!(
            (mid.z.abs() - expected_flux).abs() < expected_flux * 0.001,
            "flux z = {:?}, expected {}",
            mid,
            expected_flux
        );
    }

    #[test]
    fn radiation_cools_below_convection_only() {
        let grid = one_cell_grid();
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let area = 1e-6;
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 10.0,
            t_ambient: 25.0,
        });
        solver.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![0],
            power: 0.5,
        });
        let t_conv = solver.solve_steady_state().max_temp;

        let grid = one_cell_grid();
        let mut solver2 = ThermalSolver::new(grid, MaterialDatabase::standard());
        solver2.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 10.0,
            t_ambient: 25.0,
        });
        solver2.add_boundary_condition(BoundaryCondition::Radiation {
            surface: vec![0],
            emissivity: 0.9,
            t_surroundings: 25.0,
        });
        solver2.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![0],
            power: 0.5,
        });
        let t_rad = solver2.solve_steady_state().max_temp;
        assert!(t_rad < t_conv - 1.0, "rad {t_rad} vs conv {t_conv}");
        // h·A·ΔT + εσA(T⁴-T∞⁴) = P → several °C cooler here
        let _ = area;
    }

    #[test]
    fn heat_flux_bc_heats_board() {
        let grid = one_cell_grid();
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 1000.0,
            t_ambient: 25.0,
        });
        solver.add_boundary_condition(BoundaryCondition::HeatFlux {
            surface: vec![0],
            flux: 1000.0, // W/m² on the 1 mm² face → 1 mW
        });
        let r = solver.solve_steady_state();
        let expected = 25.0 + 1000.0 * 1e-6 / (1000.0 * 1e-6);
        assert!((r.max_temp - expected).abs() < 1e-6);
    }

    #[test]
    fn anisotropic_fr4_uses_z_conductivity() {
        // Column of FR4: through-thickness path must use kz = 0.3.
        let cells = 5;
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
            MaterialId::new("fr4"),
        );
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![0],
            temp: 100.0,
        });
        let n = solver.grid().len();
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![(n - 1) as u32],
            temp: 0.0,
        });
        let r = solver.solve_steady_state();
        // Cell centers span 4·dx = 4 mm; q/A = kz·ΔT/((N-1)·dx)
        // = 0.3·100/0.004 = 7500 W/m² — kz (0.3), NOT kx (0.6).
        let flux = r.heat_flux[2].z;
        assert!((flux - 7500.0).abs() < 50.0, "flux {flux}, expected ~7500");
    }

    #[test]
    fn unconstrained_grid_stays_at_ambient() {
        let grid = one_cell_grid();
        let solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let r = solver.solve_steady_state();
        assert!((r.max_temp - 25.0).abs() < 1e-12);
    }

    #[test]
    fn heat_source_near_targets_top_layer() {
        let grid = one_cell_grid();
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        solver.add_heat_source_near(0.0005, 0.0005, 1.0);
        assert_eq!(solver.boundary_conditions().len(), 1);
    }

    #[test]
    fn golden_simple_resistor_board() {
        // Golden reference: test-data/golden/thermal/simple_resistor_board.json
        #[derive(serde::Deserialize)]
        struct Golden {
            values: std::collections::HashMap<String, f64>,
            tolerance_c: f64,
        }
        let raw = include_str!("../../../../test-data/golden/thermal/simple_resistor_board.json");
        let golden: Golden = serde_json::from_str(raw).expect("golden file parses");
        let tol = golden.tolerance_c;

        // Case 1: 1D copper slab with pinned ends.
        let grid = column_grid(10);
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        let n = solver.grid().len();
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![0],
            temp: 100.0,
        });
        solver.add_boundary_condition(BoundaryCondition::FixedTemperature {
            nodes: vec![(n - 1) as u32],
            temp: 0.0,
        });
        let r = solver.solve_steady_state();
        assert!(
            (r.max_temp - golden.values["slab_max_temp_c"]).abs() <= tol,
            "slab max {} vs golden {}",
            r.max_temp,
            golden.values["slab_max_temp_c"]
        );
        let center = r.temperatures[5];
        assert!(
            (center - golden.values["slab_center_temp_c"]).abs() <= tol,
            "slab center {} vs golden {}",
            center,
            golden.values["slab_center_temp_c"]
        );

        // Case 2: lumped convection on a single 1 W copper cell.
        let grid = one_cell_grid();
        let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 1000.0,
            t_ambient: 25.0,
        });
        solver.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![0],
            power: 1.0,
        });
        let r = solver.solve_steady_state();
        assert!(
            (r.max_temp - golden.values["lumped_convection_temp_c"]).abs() <= tol,
            "lumped {} vs golden {}",
            r.max_temp,
            golden.values["lumped_convection_temp_c"]
        );
    }
}
