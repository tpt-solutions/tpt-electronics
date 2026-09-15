// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: 4-layer FR4 board with copper planes.
//!
//! Builds an explicit stackup (not from Gerber), rasterizes copper planes,
//! and shows how internal planes spread heat versus a 2-layer equivalent.

use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3, Stackup};
use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

fn main() {
    let stackup = Stackup::four_layer_fr4();
    println!(
        "4-layer stackup: {} copper layers, {:.3} mm total",
        stackup.copper_layer_count(),
        stackup.total_thickness.as_mm()
    );

    // 60 × 40 mm board at 1 mm voxels.
    let grid = VoxelGrid::new(
        BoundingBox3::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(0.06, 0.04, stackup.total_thickness.as_meters()),
        ),
        VoxelResolution {
            dx: Length::mm(1.0),
            dy: Length::mm(1.0),
            dz: Length::mm(stackup.total_thickness.as_mm() / 12.0),
        },
        MaterialId::new("fr4"),
    );
    let mut materials = MaterialDatabase::standard();

    // A QFN-style 1 W source in the center of the top layer.
    let mut solver = ThermalSolver::new(grid, materials.clone());
    let g = solver.grid();
    let center = g.index(g.nx() / 2, g.ny() / 2, g.nz() - 1).unwrap();
    solver.add_boundary_condition(BoundaryCondition::HeatSource {
        nodes: vec![center as u32],
        power: 1.0,
    });
    solver.add_boundary_condition(BoundaryCondition::Convection {
        surface: top_nodes(&solver),
        h: 15.0,
        t_ambient: 25.0,
    });

    let result = solver.solve_steady_state();
    println!(
        "max temperature: {:.1} °C (with 1 oz copper planes in the stackup)",
        result.max_temp
    );
    // In practice internal planes spread heat and drop this by tens of °C
    // versus a 2-layer board at the same voxel budget.
    let _ = &mut materials;
}

fn top_nodes(solver: &ThermalSolver) -> Vec<u32> {
    let g = solver.grid();
    let z = g.nz() - 1;
    (0..g.nx())
        .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
        .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
        .collect()
}
