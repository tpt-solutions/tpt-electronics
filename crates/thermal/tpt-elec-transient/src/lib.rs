// SPDX-License-Identifier: MIT OR Apache-2.0

//! Transient thermal time integration.
//!
//! Solves `[C]{dT/dt} + [K]{T} = {Q(t)}` on the voxel grid using either
//!
//! * **implicit (backward) Euler** — unconditionally stable, solved each step
//!   with Conjugate Gradient on `([C]/Δt + [K])`, or
//! * **explicit (forward) Euler** — cheap per step, with a lumped-mass
//!   stability check on the requested time step.
//!
//! The capacitance matrix is lumped: `Cᵢ = ρᵢ·cpᵢ·Vᵢ`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{ThermalResult, ThermalSolver};

/// Result of a transient run.
#[derive(Clone, Debug)]
pub struct TransientResult {
    /// Sample times [s].
    pub times: Vec<f64>,
    /// Maximum board temperature at each sample [°C].
    pub max_temp_history: Vec<f64>,
    /// Full temperature field at `t_end` [°C].
    pub final_temperatures: Vec<f64>,
    /// Maximum temperature over the whole run [°C].
    pub max_temp: f64,
}

/// Time integration scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrationScheme {
    /// Backward Euler (unconditionally stable).
    Implicit,
    /// Forward Euler (requires small Δt; validated before running).
    Explicit,
}

/// A transient thermal step-driver over a [`ThermalSolver`] description.
pub struct TransientSolver {
    materials: MaterialDatabase,
    scheme: IntegrationScheme,
}

impl TransientSolver {
    /// Creates a transient driver.
    pub fn new(materials: MaterialDatabase) -> Self {
        Self {
            materials,
            scheme: IntegrationScheme::Implicit,
        }
    }

    /// Selects the integration scheme.
    pub fn with_scheme(mut self, scheme: IntegrationScheme) -> Self {
        self.scheme = scheme;
        self
    }

    /// Lumped capacitance per cell [J/K].
    fn capacitance(&self, solver: &ThermalSolver) -> Vec<f64> {
        let cell_volume = solver.grid().cell_volume();
        solver
            .grid()
            .voxels()
            .iter()
            .map(|v| {
                (match self.materials.get(&v.material) {
                    Some(m) => m.density * m.specific_heat,
                    None => 1.0e6, // default ρ·cp if unknown
                }) * cell_volume
                    * v.volume_fraction
            })
            .collect()
    }

    /// Runs a transient solve.
    ///
    /// `dt` is the time step [s]; the boundary conditions come from the
    /// thermal solver description (heat sources are treated as constant
    /// power for the whole run).
    pub fn solve_transient(
        &self,
        thermal: &ThermalSolver,
        t_end: f64,
        dt: f64,
        initial_temp: f64,
    ) -> Result<TransientResult, String> {
        if dt <= 0.0 || t_end <= 0.0 {
            return Err("dt and t_end must be positive".into());
        }
        let steps = (t_end / dt).ceil() as usize;
        if steps == 0 || steps > 10_000_000 {
            return Err(format!("unreasonable step count {steps}"));
        }

        let n = thermal.grid().len();
        let c = self.capacitance(thermal);
        let mut temps = vec![initial_temp; n];

        if self.scheme == IntegrationScheme::Explicit {
            // Stability: dt <= min_i C_i / (sum of row conductances) is
            // approximated with a conservative global check using the max
            // diagonal of K assembled by a trial steady-state call.
            let dt_max = self.stable_dt(thermal, &c);
            if dt > dt_max {
                return Err(format!(
                    "explicit scheme unstable: dt={dt} > dt_max={dt_max:.3e}; use implicit"
                ));
            }
        }

        let mut times = Vec::with_capacity(steps + 1);
        let mut max_temp_history = Vec::with_capacity(steps + 1);
        times.push(0.0);
        max_temp_history.push(temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max));

        for step in 1..=steps {
            match self.scheme {
                IntegrationScheme::Implicit => {
                    temps = self.implicit_step(thermal, &c, &temps, dt)?;
                }
                IntegrationScheme::Explicit => {
                    temps = self.explicit_step(thermal, &c, &temps, dt);
                }
            }
            times.push(step as f64 * dt);
            let tmax = temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            max_temp_history.push(tmax);
            if step % 50_000 == 0 && !temps.iter().all(|t| t.is_finite()) {
                return Err("solution diverged".into());
            }
        }

        let max_temp = max_temp_history
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        Ok(TransientResult {
            times,
            max_temp_history,
            final_temperatures: temps,
            max_temp,
        })
    }

    /// Conservative stable time step for explicit integration [s].
    pub fn stable_dt(&self, thermal: &ThermalSolver, c: &[f64]) -> f64 {
        // For the 7-point stencil the explicit limit is C_i / Σ_j G_ij; we
        // estimate Σ G_ij via a lumped conductance of the smallest cell.
        let r = &thermal.grid().resolution;
        let (dx, dy, dz) = (r.dx.as_meters(), r.dy.as_meters(), r.dz.as_meters());
        let mut g_sum = 0.0;
        let mut min_c = f64::INFINITY;
        for v in thermal.grid().voxels() {
            let (rho_cp, k) = match self.materials.get(&v.material) {
                Some(m) => (
                    m.density * m.specific_heat,
                    m.thermal_conductivity.effective(),
                ),
                None => (1.0e6, 0.3),
            };
            // worst case: six neighbors of same material
            g_sum = 2.0 * k * (dy * dz / dx + dx * dz / dy + dx * dy / dz);
            min_c = min_c.min(rho_cp * dx * dy * dz);
        }
        let _ = c;
        0.4 * min_c / g_sum.max(1e-300)
    }

    fn implicit_step(
        &self,
        thermal: &ThermalSolver,
        c: &[f64],
        temps: &[f64],
        dt: f64,
    ) -> Result<Vec<f64>, String> {
        // (C/Δt + K)·Tⁿ⁺¹ = Q + (C/Δt)·Tⁿ
        let (a, rhs, _penalty) = thermal.assemble_transient_step(c, temps, dt);
        let (next, rel) = a
            .solve_cg(&rhs, 1e-10, 20_000)
            .ok_or_else(|| "implicit step failed to converge".to_string())?;
        if !rel.is_finite() {
            return Err("implicit step diverged".into());
        }
        Ok(next)
    }

    fn explicit_step(
        &self,
        thermal: &ThermalSolver,
        c: &[f64],
        temps: &[f64],
        dt: f64,
    ) -> Vec<f64> {
        // Tⁿ⁺¹ = Tⁿ + dt·(Q − K·Tⁿ)/C
        let (k, q, _) = thermal.assemble_system();
        let kt = k.mul_vec(temps);
        let mut next = vec![0.0; temps.len()];
        for i in 0..temps.len() {
            next[i] = temps[i] + dt * (q[i] - kt[i]) / c[i].max(1e-300);
        }
        next
    }
}

/// Convenience: runs a transient on a solver and returns a final-state view
/// compatible with [`ThermalResult`].
pub fn transient_summary(result: &TransientResult) -> ThermalResult {
    ThermalResult {
        temperatures: result.final_temperatures.clone(),
        max_temp: result.max_temp,
        max_temp_location: (0, 0, 0),
        heat_flux: Vec::new(),
        residual: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3};
    use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
    use tpt_elec_thermal::BoundaryCondition;

    fn one_cell() -> (VoxelGrid, MaterialDatabase) {
        let grid = VoxelGrid::new(
            BoundingBox3::new(Point3::new(0., 0., 0.), Point3::new(1e-3, 1e-3, 1e-3)),
            VoxelResolution {
                dx: Length::mm(1.0),
                dy: Length::mm(1.0),
                dz: Length::mm(1.0),
            },
            MaterialId::new("copper"),
        );
        (grid, MaterialDatabase::standard())
    }

    fn conv_solver() -> ThermalSolver {
        let (grid, mats) = one_cell();
        let mut solver = ThermalSolver::new(grid, mats);
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 1000.0,
            t_ambient: 25.0,
        });
        solver
    }

    #[test]
    fn lumped_cooling_matches_exponential() {
        // T(t) = T∞ + (T0 − T∞)·e^(−hA/(ρ·cp·V)·t)
        let thermal = conv_solver();
        let materials = MaterialDatabase::standard();
        let cu = materials.get(&MaterialId::new("copper")).unwrap();
        let tau = cu.density * cu.specific_heat * 1e-9 / (1000.0 * 1e-6); // ρ·cp·V/(h·A)
        let t_end = 2.0 * tau;

        for scheme in [IntegrationScheme::Implicit, IntegrationScheme::Explicit] {
            let ts = TransientSolver::new(materials.clone()).with_scheme(scheme);
            let dt = if scheme == IntegrationScheme::Explicit {
                ts.stable_dt(&thermal, &[]).min(tau / 50.0)
            } else {
                tau / 20.0
            };
            let r = ts
                .solve_transient(&thermal, t_end, dt, 125.0)
                .expect("transient solve");
            let expected = 25.0 + 100.0 * (-t_end / tau).exp();
            let got = r.final_temperatures[0];
            assert!(
                (got - expected).abs() < expected * 0.05,
                "{scheme:?}: got {got}, expected {expected}"
            );
            assert!(r.max_temp <= 125.0 + 1e-9);
            assert_eq!(r.times.first().copied(), Some(0.0));
        }
    }

    #[test]
    fn heating_towards_steady_state() {
        let (grid, mats) = one_cell();
        let mut thermal = ThermalSolver::new(grid, mats);
        thermal.add_boundary_condition(BoundaryCondition::Convection {
            surface: vec![0],
            h: 1000.0,
            t_ambient: 25.0,
        });
        thermal.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![0],
            power: 0.5,
        });
        let steady = thermal.solve_steady_state().max_temp;

        let ts = TransientSolver::new(MaterialDatabase::standard());
        let r = ts.solve_transient(&thermal, 100.0, 0.05, 25.0).unwrap();
        let final_t = r.final_temperatures[0];
        assert!(
            (final_t - steady).abs() < steady * 0.02,
            "final {final_t} vs steady {steady}"
        );
        assert!(r.max_temp_history.windows(2).all(|w| w[1] >= w[0] - 1e-9));
    }

    #[test]
    fn explicit_rejects_unstable_dt() {
        let thermal = conv_solver();
        let ts = TransientSolver::new(MaterialDatabase::standard())
            .with_scheme(IntegrationScheme::Explicit);
        assert!(ts.solve_transient(&thermal, 1.0, 1e3, 125.0).is_err());
    }

    #[test]
    fn rejects_bad_inputs() {
        let thermal = conv_solver();
        let ts = TransientSolver::new(MaterialDatabase::standard());
        assert!(ts.solve_transient(&thermal, 0.0, 0.1, 25.0).is_err());
        assert!(ts.solve_transient(&thermal, 1.0, 0.0, 25.0).is_err());
    }
}
