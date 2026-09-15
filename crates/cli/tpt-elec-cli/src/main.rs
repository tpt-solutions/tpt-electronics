// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tpt-elec-cli` — command-line front end for tpt-electronics.
//!
//! Subcommands:
//!
//! ```text
//! tpt-elec-cli thermal --gerber top.gtl --power-map power.csv --out thermal.csv
//! tpt-elec-cli impedance --width-mm 0.35 --height-mm 0.2 --er 4.4
//! tpt-elec-cli impedance --suggest --height-mm 0.2 --er 4.4 --target 50
//! tpt-elec-cli drc --kicad board.kicad_pcb
//! ```

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_mfg_dfm::{DesignRuleSet, DrcEngine};
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
    format: String,
    kicad: Option<PathBuf>,
    width_mm: f64,
    height_mm: f64,
    er: f64,
    target_ohm: f64,
    suggest: bool,
    min_width_um: f64,
}

fn print_usage() {
    eprintln!(
        "tpt-electronics CLI\n\n\
         USAGE:\n    \
         tpt-elec-cli <COMMAND> [OPTIONS]\n\n\
         COMMANDS:\n    \
         thermal      Gerber + power map → temperature field\n    \
         impedance    Microstrip impedance (forward or inverse)\n    \
         drc          Design-rule check a .kicad_pcb file\n\n\
         THERMAL OPTIONS:\n    \
         --gerber <file>      Copper Gerber (repeatable; first = top)\n    \
         --power-map <csv>    Heat sources: `x_mm,y_mm,watts` rows\n    \
         --out <csv>          Output file\n    \
         --format <csv|json>  Output format [csv]\n    \
         --resolution-mm <f>  Voxel edge [1.0]\n    \
         --thickness-mm <f>   Substrate thickness [1.6]\n    \
         --h-conv <f>         Convection W/(m²·K) [10]\n    \
         --ambient-c <f>      Ambient [25]\n\n\
         IMPEDANCE OPTIONS:\n    \
         --width-mm <f>       Trace width [mm]\n    \
         --height-mm <f>      Dielectric height [mm]\n    \
         --er <f>             Relative permittivity [4.4]\n    \
         --suggest            Solve inverse: find width for target\n    \
         --target <ohm>       Target impedance for --suggest [50]\n\n\
         DRC OPTIONS:\n    \
         --kicad <file>       .kicad_pcb file to check\n    \
         --min-width-um <f>   Min trace width [µm, 150]"
    );
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let items: Vec<String> = args.collect();
    let mut command = String::new();
    let mut gerbers = Vec::new();
    let mut power_map = None;
    let mut out = PathBuf::from("thermal.csv");
    let mut resolution_mm = 1.0f64;
    let mut thickness_mm = 1.6f64;
    let mut h_conv = 10.0f64;
    let mut ambient_c = 25.0f64;
    let mut format = "csv".to_string();
    let mut kicad: Option<PathBuf> = None;
    let mut width_mm = 0.0f64;
    let mut height_mm = 0.0f64;
    let mut er = 4.4f64;
    let mut target_ohm = 50.0f64;
    let mut suggest = false;
    let mut min_width_um = 150.0f64;

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
            "--format" => {
                i += 1;
                format = items.get(i).ok_or("--format needs a value")?.clone();
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
            "--kicad" => {
                i += 1;
                kicad = Some(PathBuf::from(items.get(i).ok_or("--kicad needs a file")?));
            }
            "--width-mm" => {
                i += 1;
                width_mm = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--width-mm needs a number")?;
            }
            "--height-mm" => {
                i += 1;
                height_mm = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--height-mm needs a number")?;
            }
            "--er" => {
                i += 1;
                er = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--er needs a number")?;
            }
            "--target" => {
                i += 1;
                target_ohm = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--target needs a number")?;
            }
            "--suggest" => suggest = true,
            "--min-width-um" => {
                i += 1;
                min_width_um = items
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .ok_or("--min-width-um needs a number")?;
            }
            other if command.is_empty() && !other.starts_with('-') => command = other.to_string(),
            other => return Err(format!("unknown or misplaced argument {other:?}")),
        }
        i += 1;
    }
    if command.is_empty() {
        return Err("no command given".into());
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
        format,
        kicad,
        width_mm,
        height_mm,
        er,
        target_ohm,
        suggest,
        min_width_um,
    })
}

fn read_file(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn run_thermal(args: &Args) -> Result<(), String> {
    if args.gerbers.is_empty() {
        return Err("at least one --gerber is required".into());
    }
    if args.gerbers.len() > 1 {
        eprintln!(
            "note: {} additional --gerber file(s) ignored (top layer only in this release)",
            args.gerbers.len() - 1
        );
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

    if let Some(map_path) = &args.power_map {
        let csv = read_file(map_path)?;
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
            solver.add_heat_source_near(x * 1e-3, y * 1e-3, w);
        }
    }

    let result = solver.solve_steady_state();

    match args.format.as_str() {
        "json" => {
            let g = solver.grid();
            println!(
                "{{\"max_temp_c\":{:.2},\"cells\":{},\"grid\":[{},{},{}],\"residual\":{:.1e}}}",
                result.max_temp,
                result.temperatures.len(),
                g.nx(),
                g.ny(),
                g.nz(),
                result.residual
            );
        }
        _ => {
            println!(
                "max temperature: {:.1} °C at cell {:?} ({} cells)",
                result.max_temp,
                result.max_temp_location,
                result.temperatures.len()
            );
        }
    }

    let g = solver.grid();
    let mut lines = Vec::with_capacity(result.temperatures.len() + 1);
    lines.push("x_mm,y_mm,z_mm,temperature_c".to_string());
    let nx = g.nx() as usize;
    let ny = g.ny() as usize;
    for (idx, t) in result.temperatures.iter().enumerate() {
        let z = (idx / (nx * ny)) as u32;
        let y = ((idx % (nx * ny)) / nx) as u32;
        let x = (idx % nx) as u32;
        let c = g.center_of(x, y, z);
        lines.push(format!(
            "{:.3},{:.3},{:.4},{:.2}",
            c.x * 1e3,
            c.y * 1e3,
            c.z * 1e3,
            t
        ));
    }
    fs::write(&args.out, lines.join("\n") + "\n")
        .map_err(|e| format!("cannot write {}: {e}", args.out.display()))?;
    eprintln!("field written to {}", args.out.display());
    Ok(())
}

fn run_impedance(args: &Args) -> Result<(), String> {
    use tpt_elec_si_impedance::ImpedanceCalculator;
    let thickness = Length::um(35.0);
    if args.suggest {
        let w = ImpedanceCalculator::suggest_microstrip(
            args.target_ohm,
            thickness,
            Length::mm(args.height_mm),
            args.er,
        );
        let z = ImpedanceCalculator::microstrip(w, thickness, Length::mm(args.height_mm), args.er);
        println!(
            "suggested width: {:.1} µm (Z0 = {:.2} Ω, target {:.0} Ω)",
            w.as_um(),
            z.z0,
            args.target_ohm
        );
    } else {
        if args.width_mm <= 0.0 || args.height_mm <= 0.0 {
            return Err("--width-mm and --height-mm required for forward mode".into());
        }
        let line = ImpedanceCalculator::microstrip(
            Length::mm(args.width_mm),
            thickness,
            Length::mm(args.height_mm),
            args.er,
        );
        println!("Z0 = {:.2} Ω", line.z0);
    }
    Ok(())
}

fn run_drc(args: &Args) -> Result<(), String> {
    let board_path = args.kicad.as_ref().ok_or("--kicad <file> is required")?;
    let src = read_file(board_path)?;
    let board = tpt_elec_kicad::KiCadParser::parse_pcb(&src)
        .map_err(|e| format!("{}: {e}", board_path.display()))?;
    let rules = DesignRuleSet {
        min_trace_width: Length::um(args.min_width_um),
        ..Default::default()
    };
    let result = DrcEngine::new(rules).run(&board);
    for finding in &result.findings {
        println!("{finding}");
    }
    println!(
        "{}: {} finding(s), {}",
        board_path.display(),
        result.findings.len(),
        if result.passed() { "PASS" } else { "FAIL" }
    );
    if result.passed() {
        Ok(())
    } else {
        Err("DRC failed".into())
    }
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(a) if a.command == "thermal" => match run_thermal(&a) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(a) if a.command == "impedance" => match run_impedance(&a) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(a) if a.command == "drc" => match run_drc(&a) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(a) => {
            eprintln!("error: unknown command {:?}\n", a.command);
            print_usage();
            ExitCode::FAILURE
        }
        Err(e) if e == "help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

use tpt_elec_core::Length;
