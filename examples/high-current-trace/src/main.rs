// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: high-current trace — Joule heating coupled electro-thermal solve
//! plus the IPC-2152/2221 chart cross-check.

use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point3};
use tpt_elec_geometry::{VoxelGrid, VoxelResolution};
use tpt_elec_joule::{JouleHeatingSolver, VoltageSource};
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_mfg_dfm::{trace_width_for_current, TraceLayer};
use tpt_elec_thermal::BoundaryCondition;

fn main() {
    // A 20 mm long, 2 mm wide, 2 oz copper power trace carrying ~5 A.
    let width = Length::mm(2.0);
    let length = Length::mm(20.0);
    let thickness = Length::um(70.0);
    let cells = 10;

    let grid = VoxelGrid::new(
        BoundingBox3::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(width.as_meters(), width.as_meters(), length.as_meters()),
        ),
        VoxelResolution {
            dx: Length::mm(2.0),
            dy: Length::mm(2.0),
            dz: Length::mm(2.0),
        },
        MaterialId::new("copper"),
    );
    let _ = cells;

    let mut solver = JouleHeatingSolver::new(
        grid,
        MaterialDatabase::standard(),
        vec![
            BoundaryCondition::Convection {
                surface: (0..10).collect(),
                h: 15.0,
                t_ambient: 25.0,
            },
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
                voltage: 0.005,
            }, // 5 mV drop
            VoltageSource {
                nodes: vec![9],
                voltage: 0.0,
            },
        ],
        12,
        0.1,
    );

    println!("high-current trace (2 × 2 mm cross-section, 20 mm long):");
    println!("  current: {:.2} A", result.current_a);
    println!("  dissipated: {:.2} mW", result.total_power * 1e3);
    println!(
        "  max copper temperature: {:.1} °C",
        result.thermal.max_temp
    );
    println!("  converged in {} iterations", result.iterations);

    // What does the IPC-2152/2221 chart formula recommend for 5 A?
    let recommended = trace_width_for_current(5.0, thickness, TraceLayer::External, 20.0);
    println!(
        "  IPC-2221 width for 5 A @ ΔT 20 °C: {:.2} mm (formula is optimistic vs charts)",
        recommended.as_mm()
    );
}
