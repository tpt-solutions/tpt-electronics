// SPDX-License-Identifier: MIT OR Apache-2.0

//! Joule heating benchmark: coupled electro-thermal iterations on a copper
//! bar grid.

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3};
use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
use tpt_elec_joule::{JouleHeatingSolver, VoltageSource};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::BoundaryCondition;

fn bench_joule(c: &mut Criterion) {
    let grid = VoxelGrid::new(
        BoundingBox3::new(Point3::new(0.0, 0.0, 0.0), Point3::new(0.004, 0.004, 0.02)),
        VoxelResolution {
            dx: Length::mm(1.0),
            dy: Length::mm(1.0),
            dz: Length::mm(1.0),
        },
        MaterialId::new("copper"),
    );
    let n = 20; // 2×2×5 cells → 20 total
    let mut solver = JouleHeatingSolver::new(
        grid,
        MaterialDatabase::standard(),
        vec![BoundaryCondition::Convection {
            surface: (0..n).collect(),
            h: 500.0,
            t_ambient: 25.0,
        }],
    );
    c.bench_function("joule_coupled_20_voxels", |b| {
        b.iter(|| {
            solver.solve_coupled(
                &[
                    VoltageSource {
                        nodes: vec![0],
                        voltage: 0.001,
                    },
                    VoltageSource {
                        nodes: vec![n - 1],
                        voltage: 0.0,
                    },
                ],
                8,
                0.05,
            )
        })
    });
}

criterion_group!(benches, bench_joule);
criterion_main!(benches);
