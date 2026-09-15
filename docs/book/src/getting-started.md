# Getting started

```console
$ cargo build --workspace
$ cargo test --workspace
$ cargo run -p example-pcie-eye-diagram
```

## Thermal from a Gerber file

```rust
use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

let gerber = GerberParser::parse(include_str!("board.gtl"))?;
let mut solver = ThermalSolver::from_gerber(&gerber, &MaterialDatabase::standard(), 1e-3);
solver.add_boundary_condition(BoundaryCondition::HeatSource {
    nodes: vec![0],
    power: 1.0,
});
let result = solver.solve_steady_state();
println!("max temperature: {:.1} °C", result.max_temp);
```

## CLI

```console
$ tpt-elec-cli thermal --gerber board.gtl --power-map power.csv --out thermal.csv
max temperature: 63.2 °C at cell (14, 12, 3)
```

The power map is a CSV of `x_mm,y_mm,watts` rows (header optional).
