// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tpt-elec-cli` — command-line front end for tpt-electronics.
//!
//! Phase 1 milestone (spec §10): takes Gerber + a power map and produces a
//! thermal CSV.
//!
//! ```text
//! tpt-elec-cli thermal \
//!   --gerber top.gtl --gerber bottom.gbl \
//!   --power-map power.csv \
//!   --out thermal.csv \
//!   --resolution-mm 1.0 --thickness-mm 1.6 \
//!   --h-conv 10 --ambient-c 25
//! ```
//!
//! The power map CSV has `x_mm,y_mm,watts` rows (header optional).

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

struct Args {
    command: String,
    gerbers: Vec<PathBuf>,
    power_map: Option<PathBuf>,
    out: PathBuf,
    resolution_mm: f64,
    thickness_mm: f64,
    h_conv: f64,
    ambient_c: f64,
}

fn print_usage() {
    eprintln!(
        "tpt-electronics CLI\n\n\
         USAGE:\n    \
         tpt-elec-cli thermal [OPTIONS]\n\n\
         OPTIONS:\n    \
         --gerber <file>      Copper-layer Gerber file (repeatable; first = top)\n    \
         --power-map <csv>    Heat sources: rows `x_mm,y_mm,watts`\n    \
         --out <csv>          Output temperature field CSV\n    \
         --resolution-mm <f>  Voxel edge length [default 1.0]\n    \
         --thickness-mm <f>   Substrate thickness [default 1.6]\n    \
         --h-conv <f>         Natural-convection coefficient W/(m2*K) [default 10]\n    \
         --ambient-c <f>      Ambient temperature [default 25]\n    \
         --help               Show this message"
    );
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut command = String::new();
    let mut gerbers = Vec::new();
    let mut power_map = None;
    let mut out = PathBuf::from("thermal.csv");
    let mut resolution_mm = 1.0;
    let mut thickness_mm = 1.6;
    let mut h_conv = 10.0;
    let mut ambient_c = 25.0;

    let items: Vec<String> = args.collect();
    let mut i = 0;
    while i < items.len() {
        match items[i].as_str() {
            "--help" | "-h" => return Err("help".to_string()),
            "--gerber" => {
                i += 1;
                gerbers.push(PathBuf::from(
                    items.get(i).ok_or("--gerber needs a file")?.clone(),
                ));
            }
            "--power-map" => {
                i += 1;
                power_map = Some(PathBuf::from(
                    items.get(i).ok_or("--power-map needs a file")?,
                ));
            }
            "--out" => {
                i += 1;
                out = PathBuf::from(items.get(i).ok_or("--out needs a file")?);
            }
            "--resolution-mm" => {
                i += 1;
                resolution_mm = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--resolution-mm needs a number")?;
            }
            "--thickness-mm" => {
                i += 1;
                thickness_mm = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--thickness-mm needs a number")?;
            }
            "--h-conv" => {
                i += 1;
                h_conv = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--h-conv needs a number")?;
            }
            "--ambient-c" => {
                i += 1;
                ambient_c = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--ambient-c needs a number")?;
            }
            other if command.is_empty() && !other.starts_with('-') => command = other.to_string(),
            other => return Err(format!("unknown or misplaced argument {other:?}")),
        }
        i += 1;
    }
    if command.is_empty() {
        return Err("no command given".to_string());
    }
    Ok(Args {
        command,
        gerbers,
        power_map,
        out,
        resolution_mm,
        thickness_mm,
        h_conv,
        ambient_c,
    })
}

fn read_file(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn run_thermal(args: &Args) -> Result<(), String> {
    if args.gerbers.is_empty() {
        return Err("at least one --gerber is required".to_string());
    }
    let top_src = read_file(&args.gerbers[0])?;
    let gerber =
        GerberParser::parse(&top_src).map_err(|e| format!("{}: {e}", args.gerbers[0].display()))?;

    let materials = MaterialDatabase::standard();
    let mut solver = ThermalSolver::from_gerber_scaled(
        &gerber,
        &materials,
        args.resolution_mm * 1e-3,
        args.thickness_mm * 1e-3,
        35e-6,
    );
    solver.set_ambient(args.ambient_c);

    // Convection over the whole top surface.
    let top_nodes: Vec<u32> = {
        let g = solver.grid();
        let z = g.nz() - 1;
        (0..g.nx())
            .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
            .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
            .collect()
    };
    solver.add_boundary_condition(BoundaryCondition::Convection {
        surface: top_nodes,
        h: args.h_conv,
        t_ambient: args.ambient_c,
    });

    // Heat sources from the power map.
    if let Some(map_path) = &args.power_map {
        let csv = read_file(map_path)?;
        let mut count = 0;
        for (lineno, line) in csv.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("x_mm") {
                continue;
            }
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            if parts.len() < 3 {
                return Err(format!(
                    "{}:{lineno}: expected x_mm,y_mm,watts",
                    map_path.display()
                ));
            }
            let x = parts[0]
                .parse::<f64>()
                .map_err(|_| format!("{}:{lineno}: bad x", map_path.display()))?;
            let y = parts[1]
                .parse::<f64>()
                .map_err(|_| format!("{}:{lineno}: bad y", map_path.display()))?;
            let w = parts[2]
                .parse::<f64>()
                .map_err(|_| format!("{}:{lineno}: bad watts", map_path.display()))?;
            // from_gerber uses meters with mm-file input; power map is in mm.
            solver.add_heat_source_near(x * 1e-3, y * 1e-3, w);
            count += 1;
        }
        eprintln!("loaded {count} heat sources");
    }

    let result = solver.solve_steady_state();

    // Emit CSV: x_mm,y_mm,z_mm,temperature_c
    let g = solver.grid();
    let mut out_lines = Vec::with_capacity(result.temperatures.len() + 1);
    out_lines.push("x_mm,y_mm,z_mm,temperature_c".to_string());
    for (i, t) in result.temperatures.iter().enumerate() {
        let nx = g.nx() as usize;
        let ny = g.ny() as usize;
        let z = (i / (nx * ny)) as u32;
        let y = ((i % (nx * ny)) / nx) as u32;
        let x = (i % nx) as u32;
        let c = g.center_of(x, y, z);
        out_lines.push(format!(
            "{:.3},{:.3},{:.4},{:.2}",
            c.x * 1e3,
            c.y * 1e3,
            c.z * 1e3,
            t
        ));
    }
    fs::write(&args.out, out_lines.join("\n") + "\n")
        .map_err(|e| format!("cannot write {}: {e}", args.out.display()))?;

    println!(
        "max temperature: {:.1} °C at cell {:?} ({} cells, residual {:.1e})",
        result.max_temp,
        result.max_temp_location,
        result.temperatures.len(),
        result.residual
    );
    println!("temperature field written to {}", args.out.display());
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) if e == "help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n");
            print_usage();
            return ExitCode::FAILURE;
        }
    };
    match args.command.as_str() {
        "thermal" => match run_thermal(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("error: unknown command {other:?}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}
