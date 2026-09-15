// SPDX-License-Identifier: MIT OR Apache-2.0

//! Steady-state thermal FEM benchmark: 50×50×4 voxel board (10 k cells).

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3};
use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

fn bench_solver() -> ThermalSolver {
    let grid = VoxelGrid::new(
        BoundingBox3::new(Point3::new(0.0, 0.0, 0.0), Point3::new(0.05, 0.05, 0.004)),
        VoxelResolution {
            dx: Length::mm(1.0),
            dy: Length::mm(1.0),
            dz: Length::mm(1.0),
        },
        MaterialId::new("fr4"),
    );
    let mut solver = ThermalSolver::new(grid, MaterialDatabase::standard());
    let (surface, center) = {
        let g = solver.grid();
        let z = g.nz() - 1;
        let surface: Vec<u32> = (0..g.nx())
            .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
            .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
            .collect();
        let center = g.index(g.nx() / 2, g.ny() / 2, z).map(|i| i as u32);
        (surface, center)
    };
    solver.add_boundary_condition(BoundaryCondition::Convection {
        surface,
        h: 15.0,
        t_ambient: 25.0,
    });
    if let Some(center) = center {
        solver.add_boundary_condition(BoundaryCondition::HeatSource {
            nodes: vec![center],
            power: 1.0,
        });
    }
    solver
}

fn bench_thermal(c: &mut Criterion) {
    let solver = bench_solver();
    c.bench_function("thermal_steady_state_10k_voxels", |b| {
        b.iter(|| solver.solve_steady_state())
    });
}

criterion_group!(benches, bench_thermal);
criterion_main!(benches);
