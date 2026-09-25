// SPDX-License-Identifier: MIT OR Apache-2.0

//! Thermal simulation bindings (`tpt-elec-thermal`).

use pyo3::prelude::*;
use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3};
use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

use crate::new_dict;

/// Lists the built-in materials available to the thermal solver.
#[pyfunction]
fn default_materials(py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let db = MaterialDatabase::standard();
    let d = new_dict(py);
    d.set_item("count", db.len())?;
    d.set_item("names", vec!["fr4", "copper", "aluminum"])?;
    Ok(d)
}

/// Solves a steady-state conduction problem on a uniform voxel grid.
///
/// * `size_mm` — board extents (x, y, z) in millimetres.
/// * `cell_mm` — uniform cell size in millimetres.
/// * `heat_sources` — `(x_mm, y_mm, z_mm, watts)` tuples; the power is
///   deposited into the nearest cell.
/// * `ambient_c` — temperature imposed on the outermost cells.
/// * `convection_h` — optional convection coefficient [W/(m²·K)] applied to
///   the board surface.
#[pyfunction]
#[pyo3(signature = (size_mm, cell_mm, heat_sources, ambient_c=25.0, convection_h=None, substrate_thickness_mm=None))]
fn solve_steady_state(
    py: Python<'_>,
    size_mm: (f64, f64, f64),
    cell_mm: f64,
    heat_sources: Vec<(f64, f64, f64, f64)>,
    ambient_c: f64,
    convection_h: Option<f64>,
    substrate_thickness_mm: Option<f64>,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    if cell_mm <= 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "cell_mm must be positive",
        ));
    }
    if size_mm.0 <= 0.0 || size_mm.1 <= 0.0 || size_mm.2 <= 0.0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "size_mm components must be positive",
        ));
    }

    let db = MaterialDatabase::standard();

    // A uniform FR-4 stack; `z` is the board thickness.
    let bounds = BoundingBox3::new(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(size_mm.0 * 1e-3, size_mm.1 * 1e-3, size_mm.2 * 1e-3),
    );
    let res = VoxelResolution {
        dx: Length::mm(cell_mm),
        dy: Length::mm(cell_mm),
        dz: Length::mm(cell_mm),
    };
    let grid = VoxelGrid::new(bounds, res, MaterialId::new("fr4"));
    let mut solver = ThermalSolver::new(grid, db);
    solver.set_ambient(ambient_c);
    let _ = substrate_thickness_mm;

    // Convection on the whole surface, if requested.
    if let Some(h) = convection_h {
        let n = solver.grid().len() as u32;
        let surface: Vec<u32> = (0..n).collect();
        solver.add_boundary_condition(BoundaryCondition::Convection {
            surface,
            h,
            t_ambient: ambient_c,
        });
    }

    for (x, y, _z, w) in &heat_sources {
        solver.add_heat_source_near(*x * 1e-3, *y * 1e-3, *w);
    }

    let result = solver.solve_steady_state();
    let d = new_dict(py);
    d.set_item("max_temp_c", result.max_temp)?;
    d.set_item(
        "max_temp_location",
        (
            result.max_temp_location.0,
            result.max_temp_location.1,
            result.max_temp_location.2,
        ),
    )?;
    d.set_item("residual", result.residual)?;
    d.set_item("cell_count", result.temperatures.len())?;
    d.set_item("temperatures_c", result.temperatures)?;
    Ok(d)
}

/// Registers the `thermal` submodule on `m`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "thermal")?;
    m.add_function(wrap_pyfunction!(default_materials, &m)?)?;
    m.add_function(wrap_pyfunction!(solve_steady_state, &m)?)?;
    m.add(
        "__doc__",
        "Steady-state PCB thermal solver (voxel finite volume).",
    )?;
    parent.add_submodule(&m)?;
    Ok(())
}
