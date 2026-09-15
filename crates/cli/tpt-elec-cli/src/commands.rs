// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI subcommand implementations.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use tpt_elec_gerber::GerberParser;
use tpt_elec_materials::MaterialDatabase;
use tpt_elec_mfg_dfm::{DesignRuleSet, DrcEngine};
use tpt_elec_thermal::{BoundaryCondition, ThermalSolver};

pub fn print_usage() {
    eprintln!(
        "tpt-electronics CLI\n\n\
         COMMANDS: thermal | impedance | drc | report\n\n\
         thermal:   --gerber <f> --power-map <csv> --out <csv>\n\
                    --format <csv|json> --resolution-mm <f>\n\
                    --thickness-mm <f> --h-conv <f> --ambient-c <f> --watch\n\
         impedance: --width-mm <f> --height-mm <f> --er <f> [--suggest --target <ohm>]\n\
         drc:       --kicad <file> --min-width-um <f>\n\
         report:    --out <csv> (reads thermal CSV, writes HTML)"
    );
}

fn flag(args: &[String], name: &str, default: &str) -> String {
    for (i, a) in args.iter().enumerate() {
        if a == name {
            return args
                .get(i + 1)
                .cloned()
                .unwrap_or_else(|| default.to_string());
        }
    }
    default.to_string()
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn flag_f64(args: &[String], name: &str, default: f64) -> f64 {
    for (i, a) in args.iter().enumerate() {
        if a == name {
            return args
                .get(i + 1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(default);
        }
    }
    default
}

fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))
}

pub fn thermal(args: &[String]) -> ExitCode {
    match thermal_impl(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn thermal_impl(args: &[String]) -> Result<ExitCode, String> {
    let mut gerber_files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--gerber" {
            i += 1;
            if let Some(f) = args.get(i) {
                gerber_files.push(f.clone());
            }
        }
        i += 1;
    }
    if gerber_files.is_empty() {
        return Err("at least one --gerber is required".into());
    }
    if gerber_files.len() > 1 {
        eprintln!(
            "note: {} additional --gerber ignored (top only)",
            gerber_files.len() - 1
        );
    }
    let top_src = read_file(&gerber_files[0])?;
    let gerber = GerberParser::parse(&top_src).map_err(|e| format!("{}: {e}", gerber_files[0]))?;
    let materials = MaterialDatabase::standard();
    let resolution = flag_f64(args, "--resolution-mm", 1.0) * 1e-3;
    let thickness = flag_f64(args, "--thickness-mm", 1.6) * 1e-3;
    let h_conv = flag_f64(args, "--h-conv", 10.0);
    let ambient = flag_f64(args, "--ambient-c", 25.0);
    let format = flag(args, "--format", "csv");
    let watch = has_flag(args, "--watch");

    let out_path = PathBuf::from(flag(args, "--out", "thermal.csv"));
    let power_map = args
        .iter()
        .position(|a| a == "--power-map")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let solve_once = || -> Result<tpt_elec_thermal::ThermalResult, String> {
        let mut solver =
            ThermalSolver::from_gerber_scaled(&gerber, &materials, resolution, thickness, 35e-6);
        solver.set_ambient(ambient);
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
            h: h_conv,
            t_ambient: ambient,
        });
        if let Some(pm) = &power_map {
            let csv = read_file(pm.to_str().unwrap_or(""))?;
            for (ln, line) in csv.lines().enumerate() {
                let l = line.trim();
                if l.is_empty() || l.starts_with('#') || l.starts_with("x_mm") {
                    continue;
                }
                let p: Vec<&str> = l.split(',').map(str::trim).collect();
                if p.len() < 3 {
                    continue;
                }
                let x: f64 = p[0].parse().map_err(|_| format!("{pm:?}:{ln}: bad x"))?;
                let y: f64 = p[1].parse().map_err(|_| format!("{pm:?}:{ln}: bad y"))?;
                let w: f64 = p[2]
                    .parse()
                    .map_err(|_| format!("{pm:?}:{ln}: bad watts"))?;
                solver.add_heat_source_near(x * 1e-3, y * 1e-3, w);
            }
        }
        Ok(solver.solve_steady_state())
    };

    if watch {
        let mut last = file_signature(&gerber_files);
        eprintln!("watching — Ctrl-C to stop");
        loop {
            match solve_once() {
                Ok(r) => println!("max: {:.1} °C", r.max_temp),
                Err(e) => eprintln!("error: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            loop {
                let cur = file_signature(&gerber_files);
                if cur != last {
                    last = cur;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    let result = solve_once()?;

    match format.as_str() {
        "json" => println!(
            "{{\"max_temp_c\":{:.2},\"cells\":{},\"residual\":{:.1e}}}",
            result.max_temp,
            result.temperatures.len(),
            result.residual
        ),
        _ => println!(
            "max temperature: {:.1} °C at {:?} ({} cells)",
            result.max_temp,
            result.max_temp_location,
            result.temperatures.len()
        ),
    }
    write_thermal_csv(&result, &out_path)?;
    eprintln!("field written to {}", out_path.display());
    Ok(ExitCode::SUCCESS)
}

fn write_thermal_csv(
    result: &tpt_elec_thermal::ThermalResult,
    path: &PathBuf,
) -> Result<(), String> {
    let mut lines = vec!["x_mm,y_mm,z_mm,temperature_c".to_string()];
    for (i, t) in result.temperatures.iter().enumerate() {
        lines.push(format!("{i},0,0,{t:.2}"));
    }
    fs::write(path, lines.join("\n") + "\n").map_err(|e| format!("write {path:?}: {e}"))
}

fn file_signature(paths: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for path in paths {
        if let Ok(meta) = fs::metadata(path) {
            meta.len().hash(&mut hasher);
            meta.modified().ok().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn impedance(args: &[String]) -> ExitCode {
    use tpt_elec_si_impedance::ImpedanceCalculator;
    let thickness = tpt_elec_core::Length::um(35.0);
    let h_mm = flag_f64(args, "--height-mm", 0.2);
    let er = flag_f64(args, "--er", 4.4);
    if has_flag(args, "--suggest") {
        let target = flag_f64(args, "--target", 50.0);
        let w = ImpedanceCalculator::suggest_microstrip(
            target,
            thickness,
            tpt_elec_core::Length::mm(h_mm),
            er,
        );
        println!("suggested width: {:.1} µm (target {target} Ω)", w.as_um());
    } else {
        let w_mm = flag_f64(args, "--width-mm", 0.35);
        let line = ImpedanceCalculator::microstrip(
            tpt_elec_core::Length::mm(w_mm),
            thickness,
            tpt_elec_core::Length::mm(h_mm),
            er,
        );
        println!("Z0 = {:.2} Ω", line.z0);
    }
    ExitCode::SUCCESS
}

pub fn drc(args: &[String]) -> ExitCode {
    let board_path = match args
        .iter()
        .position(|a| a == "--kicad")
        .and_then(|i| args.get(i + 1))
    {
        Some(p) => p.clone(),
        None => {
            eprintln!("error: --kicad <file> is required");
            return ExitCode::FAILURE;
        }
    };
    let src = match read_file(&board_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let board = match tpt_elec_kicad::KiCadParser::parse_pcb(&src) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let min_w = flag_f64(args, "--min-width-um", 150.0);
    let rules = DesignRuleSet {
        min_trace_width: tpt_elec_core::Length::um(min_w),
        ..Default::default()
    };
    let result = DrcEngine::new(rules).run(&board);
    for finding in &result.findings {
        println!("{finding}");
    }
    println!(
        "{}: {} finding(s), {}",
        board_path,
        result.findings.len(),
        if result.passed() { "PASS" } else { "FAIL" }
    );
    if result.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

pub fn report(args: &[String]) -> ExitCode {
    let csv_path = flag(args, "--out", "thermal.csv");
    let csv = match read_file(&csv_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut temps = Vec::new();
    for line in csv.lines().skip(1) {
        if let Some(field) = line.split(',').nth(3) {
            if let Ok(t) = field.trim().parse::<f64>() {
                temps.push(t);
            }
        }
    }
    if temps.is_empty() {
        eprintln!("error: no data");
        return ExitCode::FAILURE;
    }
    let min = temps.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let html_path = PathBuf::from(csv_path.replace(".csv", ".html"));
    let html =
        format!(
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>Thermal Report</title></head>\n\
         <body><h1>tpt-electronics thermal report</h1>\n\
         <p>Min: {min:.1} &deg;C | Max: <b>{max:.1} &deg;C</b> | Cells: {n}</p></body></html>",
        min = min, max = max, n = temps.len()
    );
    if let Err(e) = fs::write(&html_path, &html) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    println!("report written to {}", html_path.display());
    ExitCode::SUCCESS
}
