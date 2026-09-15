// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example: simple LED board — one hot LED on a 2-layer FR4 board.
//!
//! Demonstrates the minimal tpt-electronics thermal workflow: Gerber in,
//! steady-state temperatures out.

use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

fn main() {
    let gerber_src = "\
G04 simple LED board - top copper*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,2.000*%
G01*
D10*
X20000000Y15000000D03*
M02*
";
    let gerber = GerberParser::parse(gerber_src).unwrap();
    let mut solver = ThermalSolver::from_gerber(&gerber, &MaterialDatabase::standard(), 0.75e-3);
    solver.set_ambient(25.0);

    // A 1 W LED near the board center.
    solver.add_boundary_condition(BoundaryCondition::HeatSource {
        nodes: solver_top_center(&solver),
        power: 1.0,
    });
    solver.add_boundary_condition(BoundaryCondition::Convection {
        surface: top_surface(&solver),
        h: 15.0,
        t_ambient: 25.0,
    });

    let result = solver.solve_steady_state();
    println!("LED board steady state:");
    println!("  cells: {}", result.temperatures.len());
    println!(
        "  max temperature: {:.1} °C at {:?}",
        result.max_temp, result.max_temp_location
    );
    println!(
        "  LED junction estimate (θJA 60 °C/W): {:.1} °C",
        25.0 + 1.0 * 60.0
    );
}

fn top_center(solver: &ThermalSolver) -> (u32, u32, u32) {
    let g = solver.grid();
    (
        (g.nx() / 2).min(g.nx() - 1),
        (g.ny() / 2).min(g.ny() - 1),
        g.nz() - 1,
    )
}

fn solver_top_center(solver: &ThermalSolver) -> Vec<u32> {
    let (x, y, _) = top_center(solver);
    vec![solver.grid().index(x, y, solver.grid().nz() - 1).unwrap() as u32]
}

fn top_surface(solver: &ThermalSolver) -> Vec<u32> {
    let g = solver.grid();
    let z = g.nz() - 1;
    (0..g.nx())
        .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
        .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
        .collect()
}
