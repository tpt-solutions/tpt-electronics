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
         COMMANDS: thermal | impedance | drc | report | spice | rf | power | emc | battery | sweep\n\n\
         thermal:   --gerber <f> --power-map <csv> --out <csv>\n\
                    --format <csv|json> --resolution-mm <f>\n\
                    --thickness-mm <f> --h-conv <f> --ambient-c <f> --watch\n\
         impedance: --width-mm <f> --height-mm <f> --er <f> [--suggest --target <ohm>]\n\
         drc:       --kicad <file> --min-width-um <f>\n\
         report:    --out <csv> (reads thermal CSV, writes HTML)\n\
         spice:     --netlist <f> --analysis dc|ac|tran|noise [--node <name>]\n\
                    [--fstart <hz>] [--fstop <hz>] [--ppd <n>]\n\
                    [--tstop <s>] [--tstep <s>] [--format csv|json]\n\
         rf:        match --source <ohm> --load <ohm> --freq <hz> [--pi --q <n>]\n\
                    | filter --type butterworth|chebyshev1|chebyshev2|bessel\n\
                      --order <n> --response lp|hp|bp|bs --cutoff <hz>\n\
                      [--ripple-db <f>] [--stopband-db <f>] [--center <hz>]\n\
                      [--bandwidth <hz>] [--z0 <ohm>]\n\
         power:     buck --vin <v> --vout <v> --iout <a> --fsw <hz>\n\
                    | compensator --type i|ii|iii --fc <hz> --pm <deg>\n\
                      [--gain <f>] [--pole1 <hz>] [--pole2 <hz>]\n\
         emc:       radiated --freq-mhz <f> --loop-area-mm2 <f> --current-a <f>\n\
                    --rise-ns <f> [--harmonics <n>] [--standard cispr32-b|...]\n\
                    | conducted --fsw <hz> --di-dt <a/s> --l-nh <f>\n\
                      [--harmonics <n>] [--standard cispr32-b|...]\n\
         battery:   runaway --cells <n> --init <i[,j..]> [--horizon <s>] [--dt <s>]\n\
                    | soc [--capacity <ah>] [--initial <frac>] [--current <a>]\n\
                          [--dt <s>] [--steps <n>]\n\
         sweep:     --param name=start..stop:points --objective impedance|thermal_max"
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

fn flag_u32(args: &[String], name: &str, default: u32) -> u32 {
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

fn flag_usize(args: &[String], name: &str, default: usize) -> usize {
    flag_u32(args, name, default as u32) as usize
}

fn opt_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn read_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))
}

#[allow(dead_code)]
fn json_out(format: &str, json: &str, text: &str) {
    if format == "json" {
        println!("{json}");
    } else {
        println!("{text}");
    }
}

fn to_exit(result: Result<bool, String>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
// ---------------------------------------------------------------------------
// thermal
// ---------------------------------------------------------------------------

pub fn thermal(args: &[String]) -> ExitCode {
    to_exit(thermal_impl(args))
}

fn thermal_impl(args: &[String]) -> Result<bool, String> {
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
    let power_map = opt_flag(args, "--power-map").map(PathBuf::from);

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
    Ok(true)
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

// ---------------------------------------------------------------------------
// impedance
// ---------------------------------------------------------------------------

pub fn impedance(args: &[String]) -> ExitCode {
    to_exit(impedance_impl(args))
}

fn impedance_impl(args: &[String]) -> Result<bool, String> {
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
    Ok(true)
}

// ---------------------------------------------------------------------------
// drc
// ---------------------------------------------------------------------------

pub fn drc(args: &[String]) -> ExitCode {
    to_exit(drc_impl(args))
}

fn drc_impl(args: &[String]) -> Result<bool, String> {
    let board_path = opt_flag(args, "--kicad").ok_or("--kicad <file> is required")?;
    let src = read_file(board_path)?;
    let board = tpt_elec_kicad::KiCadParser::parse_pcb(&src).map_err(|e| e.to_string())?;
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
    Ok(result.passed())
}

// ---------------------------------------------------------------------------
// report
// ---------------------------------------------------------------------------

pub fn report(args: &[String]) -> ExitCode {
    to_exit(report_impl(args))
}

fn report_impl(args: &[String]) -> Result<bool, String> {
    let csv_path = flag(args, "--out", "thermal.csv");
    let csv = read_file(&csv_path)?;
    let mut temps = Vec::new();
    for line in csv.lines().skip(1) {
        if let Some(field) = line.split(',').nth(3) {
            if let Ok(t) = field.trim().parse::<f64>() {
                temps.push(t);
            }
        }
    }
    if temps.is_empty() {
        return Err("no data".into());
    }
    let min = temps.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = temps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let html_path = PathBuf::from(csv_path.replace(".csv", ".html"));
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset='utf-8'><title>Thermal Report</title></head>\n\
         <body><h1>tpt-electronics thermal report</h1>\n\
         <p>Min: {min:.1} &deg;C | Max: <b>{max:.1} &deg;C</b> | Cells: {n}</p>\n\
         <svg width='400' height='40' xmlns='http://www.w3.org/2000/svg'>\n\
         <defs><linearGradient id='g' x1='0' x2='1'>\n\
         <stop offset='0%' stop-color='#3b82f6'/>\n\
         <stop offset='50%' stop-color='#eab308'/>\n\
         <stop offset='100%' stop-color='#ef4444'/>\n\
         </linearGradient></defs>\n\
         <rect width='400' height='40' fill='url(#g)'/>\n\
         <text x='4' y='26' font-size='12' fill='#111'>{min:.0}°C</text>\n\
         <text x='360' y='26' font-size='12' fill='#111'>{max:.0}°C</text>\n\
         </svg>\n\
         </body></html>",
        min = min,
        max = max,
        n = temps.len()
    );
    fs::write(&html_path, &html).map_err(|e| format!("write {html_path:?}: {e}"))?;
    println!("report written to {}", html_path.display());
    Ok(true)
}

// ---------------------------------------------------------------------------
// spice
// ---------------------------------------------------------------------------

/// Dispatch for `spice` (separated for unit tests).
pub fn spice(args: &[String]) -> ExitCode {
    to_exit(spice_impl(args))
}

fn spice_impl(args: &[String]) -> Result<bool, String> {
    let netlist_path = opt_flag(args, "--netlist").ok_or("--netlist <file> is required")?;
    let src = read_file(netlist_path)?;
    let circuit =
        tpt_elec_spice_netlist::SpiceNetlistParser::parse(&src).map_err(|e| e.to_string())?;
    let analyzer = tpt_elec_spice_analysis::SpiceAnalyzer::new(circuit);
    let format = flag(args, "--format", "text");
    let analysis = flag(args, "--analysis", "dc").to_lowercase();

    match analysis.as_str() {
        "dc" | "op" => {
            let op = analyzer.dc_operating_point().map_err(|e| e.to_string())?;
            let mut parts = Vec::new();
            for (i, v) in op.node_voltages.iter().enumerate().skip(1) {
                let name = analyzer
                    .circuit()
                    .nodes()
                    .get(i)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("n{i}"));
                parts.push(format!("\"{name}\":{v:.6}"));
                if format != "json" {
                    println!("{name} = {v:.6} V");
                }
            }
            if format == "json" {
                println!("{{{}}}", parts.join(","));
            } else {
                println!("({} Newton iterations)", op.iterations);
            }
            Ok(true)
        }
        "ac" => {
            let fstart = flag_f64(args, "--fstart", 1e3);
            let fstop = flag_f64(args, "--fstop", 1e6);
            let ppd = flag_u32(args, "--ppd", 10);
            let ac = analyzer
                .ac_analysis(fstart, fstop, ppd)
                .map_err(|e| e.to_string())?;
            let node = resolve_node(&analyzer, args)?.max(1);
            write_waveform_csv(
                opt_flag(args, "--out"),
                "frequency_hz,magnitude_v,phase_deg",
                &ac.frequencies
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| {
                        let v = ac.node_voltages.get(i).and_then(|r| r.get(node)).copied();
                        match v {
                            Some(z) => format!("{f:.6},{:.6},{:.4}", z.abs(), z.arg().to_degrees()),
                            None => format!("{f:.6},0,0"),
                        }
                    })
                    .collect::<Vec<_>>(),
                &format,
                || {
                    if let Some(_last) = ac.frequencies.last().copied() {
                        let m = ac
                            .node_voltages
                            .last()
                            .and_then(|r| r.get(node))
                            .map(|z| z.abs())
                            .unwrap_or(0.0);
                        format!(
                            "ac: {} points, |V(node)|@fstop = {m:.4} V",
                            ac.frequencies.len()
                        )
                    } else {
                        "ac: empty".into()
                    }
                },
            )?;
            Ok(true)
        }
        "tran" | "transient" => {
            let tstop = flag_f64(args, "--tstop", 1e-3);
            let tstep = flag_f64(args, "--tstep", tstop / 200.0);
            let tr = analyzer
                .transient(tstop, tstep, 4.0)
                .map_err(|e| e.to_string())?;
            let node = resolve_node(&analyzer, args)?.max(1);
            write_waveform_csv(
                opt_flag(args, "--out"),
                "time_s,voltage_v",
                &tr.times
                    .iter()
                    .enumerate()
                    .map(|(i, &t)| {
                        let v = tr.node_voltages.get(i).and_then(|r| r.get(node)).copied();
                        format!("{t:.9},{:.6}", v.unwrap_or(0.0))
                    })
                    .collect::<Vec<_>>(),
                &format,
                || {
                    let vmax = tr
                        .waveform(node)
                        .iter()
                        .cloned()
                        .fold(f64::NEG_INFINITY, f64::max);
                    let vmin = tr
                        .waveform(node)
                        .iter()
                        .cloned()
                        .fold(f64::INFINITY, f64::min);
                    format!(
                        "tran: {} samples, node range [{vmin:.4}, {vmax:.4}] V",
                        tr.times.len()
                    )
                },
            )?;
            Ok(true)
        }
        "noise" => {
            let fstart = flag_f64(args, "--fstart", 1e3);
            let fstop = flag_f64(args, "--fstop", 1e6);
            let points = flag_usize(args, "--points", 21);
            let node = resolve_node(&analyzer, args)?.max(1);
            let noise = analyzer
                .noise_analysis(node, (fstart, fstop), points)
                .map_err(|e| e.to_string())?;
            if format == "json" {
                println!(
                    "{{\"total_rms_v\":{:.6e},\"points\":{}}}",
                    noise.total_rms,
                    noise.spots.len()
                );
            } else {
                println!(
                    "noise: {:.6e} V RMS over {}–{} Hz ({} points)",
                    noise.total_rms,
                    fstart,
                    fstop,
                    noise.spots.len()
                );
            }
            Ok(true)
        }
        other => Err(format!(
            "unknown analysis {other:?} (expected dc|ac|tran|noise)"
        )),
    }
}

fn resolve_node(
    analyzer: &tpt_elec_spice_analysis::SpiceAnalyzer,
    args: &[String],
) -> Result<usize, String> {
    if let Some(name) = opt_flag(args, "--node") {
        return analyzer
            .circuit()
            .node_by_name(name)
            .ok_or_else(|| format!("unknown node {name:?}"));
    }
    Ok(1)
}

fn write_waveform_csv<F>(
    out: Option<&str>,
    header: &str,
    rows: &[String],
    format: &str,
    summary: F,
) -> Result<(), String>
where
    F: FnOnce() -> String,
{
    if let Some(path) = out {
        let mut body = String::from(header);
        body.push('\n');
        for r in rows {
            body.push_str(r);
            body.push('\n');
        }
        fs::write(path, body).map_err(|e| format!("write {path}: {e}"))?;
        eprintln!("wrote {path}");
    }
    if format == "json" {
        println!(
            "{{\"points\":{},\"csv\":{}}}",
            rows.len(),
            serde_json_escape(header)
        );
    } else {
        println!("{}", summary());
    }
    Ok(())
}

fn serde_json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// rf
// ---------------------------------------------------------------------------

pub fn rf(args: &[String]) -> ExitCode {
    to_exit(rf_impl(args))
}

fn rf_impl(args: &[String]) -> Result<bool, String> {
    let sub = args.first().map(String::as_str).unwrap_or("match");
    match sub {
        "match" => {
            let rest = &args[1..];
            let zs = flag_f64(rest, "--source", 50.0);
            let zl = flag_f64(rest, "--load", 50.0);
            let freq = flag_f64(rest, "--freq", 1e9);
            let z0 = flag_f64(rest, "--z0", 50.0);
            use tpt_elec_core::Complex;
            use tpt_elec_rf_core::{MatchingDesigner, MatchingTopology};
            let z_load = Complex::real(zl);
            let nets = if has_flag(rest, "--pi") {
                let q = flag_f64(rest, "--q", 2.0);
                vec![MatchingDesigner::pi_network(
                    Complex::real(zs),
                    z_load,
                    freq,
                    q,
                    z0,
                )]
            } else {
                MatchingDesigner::l_network(Complex::real(zs), z_load, freq, z0)
            };
            if nets.is_empty() {
                return Err("no matching network for those terminations".into());
            }
            let net = &nets[0];
            let gamma = net.simulate_gamma(z_load).abs();
            let format = flag(rest, "--format", "text");
            if format == "json" {
                let els: Vec<String> = net.elements.iter().map(|e| format!("\"{e:?}\"")).collect();
                println!(
                    "{{\"topology\":\"{:?}\",\"gamma\":{gamma:.6},\"elements\":[{}]}}",
                    net.topology,
                    els.join(",")
                );
            } else {
                println!(
                    "topology: {:?}  |Γ| = {gamma:.4}  ({} solution(s))",
                    match net.topology {
                        MatchingTopology::LNetwork => "L",
                        MatchingTopology::PiNetwork => "π",
                        MatchingTopology::TNetwork => "T",
                        MatchingTopology::StubMatch => "stub",
                        MatchingTopology::TransformerMatch => "λ/4",
                    },
                    nets.len()
                );
                for (i, el) in net.elements.iter().enumerate() {
                    println!("  [{i}] {el:?}");
                }
            }
            Ok(true)
        }
        "filter" => {
            use tpt_elec_rf_filters::{FilterResponse, FilterSynthesizer, FilterType};
            let rest = &args[1..];
            let kind = flag(rest, "--type", "butterworth").to_lowercase();
            let order = flag_u32(rest, "--order", 3);
            let response_kind = flag(rest, "--response", "lp").to_lowercase();
            let cutoff = flag_f64(rest, "--cutoff", 1e6);
            let z0 = flag_f64(rest, "--z0", 50.0);
            let filter_type = match kind.as_str() {
                "butterworth" => FilterType::Butterworth { order },
                "chebyshev1" | "chebyshev-i" => FilterType::ChebyshevType1 {
                    order,
                    ripple_db: flag_f64(rest, "--ripple-db", 1.0),
                },
                "chebyshev2" | "chebyshev-ii" => FilterType::ChebyshevType2 {
                    order,
                    stopband_db: flag_f64(rest, "--stopband-db", 40.0),
                },
                "bessel" => FilterType::Bessel { order },
                "elliptic" | "cauer" => FilterType::Elliptic {
                    order,
                    passband_ripple_db: flag_f64(rest, "--ripple-db", 1.0),
                    stopband_attenuation_db: flag_f64(rest, "--stopband-db", 40.0),
                },
                other => return Err(format!("unknown filter type {other:?}")),
            };
            let response = match response_kind.as_str() {
                "lp" | "lowpass" => FilterResponse::LowPass { cutoff },
                "hp" | "highpass" => FilterResponse::HighPass { cutoff },
                "bp" | "bandpass" => FilterResponse::BandPass {
                    center: flag_f64(rest, "--center", cutoff),
                    bandwidth: flag_f64(rest, "--bandwidth", cutoff / 10.0),
                },
                "bs" | "bandstop" | "notch" => FilterResponse::BandStop {
                    center: flag_f64(rest, "--center", cutoff),
                    bandwidth: flag_f64(rest, "--bandwidth", cutoff / 10.0),
                },
                other => return Err(format!("unknown response {other:?}")),
            };
            let filter = FilterSynthesizer::synthesize(filter_type, response, z0)?;
            let format = flag(rest, "--format", "text");
            if format == "json" {
                println!(
                    "{{\"elements\":{},\"impedance\":{z0}}}",
                    filter.components.len()
                );
            } else {
                println!(
                    "filter: {} ladder element(s), Z0 = {z0} Ω",
                    filter.components.len()
                );
                for (i, el) in filter.components.iter().enumerate() {
                    println!("  [{i}] {el:?}");
                }
            }
            Ok(true)
        }
        other => Err(format!(
            "unknown rf subcommand {other:?} (expected match|filter)"
        )),
    }
}

// ---------------------------------------------------------------------------
// power
// ---------------------------------------------------------------------------

pub fn power(args: &[String]) -> ExitCode {
    to_exit(power_impl(args))
}

fn power_impl(args: &[String]) -> Result<bool, String> {
    let sub = args.first().map(String::as_str).unwrap_or("buck");
    match sub {
        "buck" => {
            let rest = &args[1..];
            let vin = flag_f64(rest, "--vin", 12.0);
            let vout = flag_f64(rest, "--vout", 3.3);
            let iout = flag_f64(rest, "--iout", 2.0);
            let fsw = flag_f64(rest, "--fsw", 500e3);
            let ripple = flag_f64(rest, "--ripple-frac", 0.3);
            let rds = flag_f64(rest, "--rds", 0.03);
            let vf = flag_f64(rest, "--vf", 0.5);
            let d =
                tpt_elec_power_core::BuckDesigner::design(vin, vout, iout, fsw, ripple, rds, vf)?;
            let format = flag(rest, "--format", "text");
            if format == "json" {
                println!(
                    "{{\"duty\":{:.6},\"inductance_h\":{:.6e},\"output_cap_f\":{:.6e},\
                     \"efficiency\":{:.4},\"ripple_a\":{:.4},\"peak_i_a\":{:.4}}}",
                    vout / vin,
                    d.components.inductance,
                    d.components.output_cap,
                    d.efficiency,
                    d.inductor_ripple,
                    d.peak_inductor_current
                );
            } else {
                println!(
                    "buck {vin} V → {vout} V @ {iout} A, fsw = {fsw} Hz\n  \
                     duty = {:.1} %\n  L = {:.2} µH\n  C_out = {:.1} µF\n  \
                     ripple = {:.2} A pp\n  peak I_L = {:.2} A\n  efficiency ≈ {:.1} %\n  \
                     loop BW target ≈ {:.0} Hz",
                    100.0 * vout / vin,
                    d.components.inductance * 1e6,
                    d.components.output_cap * 1e6,
                    d.inductor_ripple,
                    d.peak_inductor_current,
                    100.0 * d.efficiency,
                    d.loop_bandwidth
                );
            }
            Ok(true)
        }
        "compensator" => {
            use tpt_elec_power_control::{CompensatorDesigner, CompensatorType, TransferFunction};
            let rest = &args[1..];
            let ty = flag(rest, "--type", "ii").to_uppercase();
            let ct = match ty.as_str() {
                "I" | "TYPEI" | "TYPE_I" => CompensatorType::TypeI,
                "II" | "TYPEII" | "TYPE_II" => CompensatorType::TypeII,
                "III" | "TYPEIII" | "TYPE_III" => CompensatorType::TypeIII,
                other => return Err(format!("unknown compensator type {other:?}")),
            };
            let fc = flag_f64(rest, "--fc", 10e3);
            let pm = flag_f64(rest, "--pm", 60.0);
            // Default plant: G0 / ((1+s/wp1)(1+s/wp2)) — voltage-mode buck-ish.
            let g0 = flag_f64(rest, "--gain", 50.0);
            let wp1 = std::f64::consts::TAU * flag_f64(rest, "--pole1", 20e3);
            let wp2 = std::f64::consts::TAU * flag_f64(rest, "--pole2", 200e3);
            // numerator [g0], denominator [(1/w1)(1/w2), (1/w1+1/w2), 1] scaled:
            // G(s) = g0 / ((1+s/wp1)(1+s/wp2)) = g0 / (s²/(wp1 wp2) + s(1/wp1+1/wp2) + 1)
            let den = vec![1.0 / (wp1 * wp2), 1.0 / wp1 + 1.0 / wp2, 1.0];
            let plant = TransferFunction {
                numerator: vec![g0],
                denominator: den,
            };
            let comp = CompensatorDesigner::design(&plant, ct, fc, pm);
            let loop_sys = tpt_elec_power_control::ControlLoop::new(plant, comp.clone());
            let margins = loop_sys.stability_margins();
            let format = flag(rest, "--format", "text");
            if format == "json" {
                println!(
                    "{{\"type\":\"{ty}\",\"fc_hz\":{fc},\"phase_margin_deg\":{:.2},\
                     \"gain_margin_db\":{:.2},\"gain_crossover_hz\":{:.1}}}",
                    margins.phase_margin, margins.gain_margin, margins.gain_crossover_hz
                );
            } else {
                println!(
                    "Type {ty} compensator @ fc = {fc} Hz, target PM = {pm}°\n  \
                     numerator:   {:?}\n  denominator: {:?}\n  \
                     achieved PM = {:.1}°  GM = {:.1} dB  crossover = {:.0} Hz",
                    comp.numerator,
                    comp.denominator,
                    margins.phase_margin,
                    margins.gain_margin,
                    margins.gain_crossover_hz
                );
            }
            Ok(true)
        }
        other => Err(format!(
            "unknown power subcommand {other:?} (expected buck|compensator)"
        )),
    }
}

// ---------------------------------------------------------------------------
// emc
// ---------------------------------------------------------------------------

pub fn emc(args: &[String]) -> ExitCode {
    to_exit(emc_impl(args))
}

fn parse_standard(name: &str) -> Result<tpt_elec_emc_core::EmcStandard, String> {
    use tpt_elec_emc_core::EmcStandard;
    Ok(match name.to_lowercase().replace('_', "-").as_str() {
        "cispr32-a" | "cispr-32-a" => EmcStandard::Cispr32ClassA,
        "cispr32-b" | "cispr-32-b" | "cispr32" | "cispr-32" => EmcStandard::Cispr32ClassB,
        "fcc15-a" | "fcc-part15-a" | "fcc-a" => EmcStandard::FccPart15ClassA,
        "fcc15-b" | "fcc-part15-b" | "fcc-b" | "fcc15" => EmcStandard::FccPart15ClassB,
        "mil461" | "mil-std-461" | "milstd461" => EmcStandard::MilStd461 {
            requirement: "RE102".into(),
        },
        "do160" | "do-160" => EmcStandard::Do160 {
            section: "25".into(),
        },
        "iec61000" | "iec-61000" => EmcStandard::Iec61000 {
            part: "4-3".into(),
        },
        "automotive" | "cispr25" => EmcStandard::Automotive {
            standard: "CISPR 25".into(),
        },
        other => {
            return Err(format!(
                "unknown standard {other:?} (cispr32-a|cispr32-b|fcc15-a|fcc15-b|mil461|do160|iec61000|automotive)"
            ))
        }
    })
}

fn emc_impl(args: &[String]) -> Result<bool, String> {
    use tpt_elec_emc_core::{EmcLimit, EmcTestType};
    use tpt_elec_emc_emissions::EmissionsPredictor;
    let sub = args.first().map(String::as_str).unwrap_or("radiated");
    let rest = &args[1..];
    let standard = parse_standard(&flag(rest, "--standard", "cispr32-b"))?;
    let format = flag(rest, "--format", "text");

    match sub {
        "radiated" => {
            let freq_mhz = flag_f64(rest, "--freq-mhz", 100.0);
            let loop_area_mm2 = flag_f64(rest, "--loop-area-mm2", 100.0);
            let current = flag_f64(rest, "--current-a", 0.1);
            let rise_ns = flag_f64(rest, "--rise-ns", 2.0);
            let harmonics = flag_u32(rest, "--harmonics", 30);
            let spectrum = EmissionsPredictor::radiated_emissions(
                freq_mhz * 1e6,
                loop_area_mm2 * 1e-6,
                current,
                rise_ns * 1e-9,
                harmonics,
            );
            let limit = EmcLimit::for_standard(standard.clone(), EmcTestType::RadiatedEmissions)
                .ok_or_else(|| format!("no radiated limit for {standard:?}"))?;
            let margin = spectrum.worst_margin_db(&limit);
            if let Some(out) = opt_flag(rest, "--out") {
                fs::write(out, spectrum.to_csv_with_limit(&limit))
                    .map_err(|e| format!("write {out}: {e}"))?;
                eprintln!("wrote {out}");
            }
            if format == "json" {
                println!(
                    "{{\"worst_margin_db\":{margin:.2},\"harmonics\":{},\"compliant\":{}}}",
                    spectrum.harmonics.len(),
                    margin >= 0.0
                );
            } else {
                println!(
                    "radiated emissions vs {:?}: worst margin {margin:.1} dB ({})",
                    limit.standard,
                    if margin >= 0.0 { "PASS" } else { "FAIL" }
                );
                for h in spectrum.harmonics.iter().take(5) {
                    println!(
                        "  H{:<3} {:>10.3} MHz  {:7.1} dBµV/m",
                        h.number,
                        h.frequency / 1e6,
                        h.amplitude_dbuv
                    );
                }
                if spectrum.harmonics.len() > 5 {
                    println!("  … {} more", spectrum.harmonics.len() - 5);
                }
            }
            if margin >= 0.0 {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        "conducted" => {
            let fsw = flag_f64(rest, "--fsw", 100e3);
            let di_dt = flag_f64(rest, "--di-dt", 1e6);
            let l_nh = flag_f64(rest, "--l-nh", 100.0);
            let harmonics = flag_u32(rest, "--harmonics", 30);
            let spectrum =
                EmissionsPredictor::conducted_emissions(fsw, di_dt, l_nh * 1e-9, harmonics);
            let limit = EmcLimit::for_standard(standard.clone(), EmcTestType::ConductedEmissions)
                .ok_or_else(|| format!("no conducted limit for {standard:?}"))?;
            let margin = spectrum.worst_margin_db(&limit);
            if let Some(out) = opt_flag(rest, "--out") {
                fs::write(out, spectrum.to_csv_with_limit(&limit))
                    .map_err(|e| format!("write {out}: {e}"))?;
                eprintln!("wrote {out}");
            }
            if format == "json" {
                println!(
                    "{{\"worst_margin_db\":{margin:.2},\"harmonics\":{},\"compliant\":{}}}",
                    spectrum.harmonics.len(),
                    margin >= 0.0
                );
            } else {
                println!(
                    "conducted emissions vs {:?}: worst margin {margin:.1} dB ({})",
                    limit.standard,
                    if margin >= 0.0 { "PASS" } else { "FAIL" }
                );
            }
            if margin >= 0.0 {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        other => Err(format!(
            "unknown emc subcommand {other:?} (expected radiated|conducted)"
        )),
    }
}

// ---------------------------------------------------------------------------
// battery
// ---------------------------------------------------------------------------

pub fn battery(args: &[String]) -> ExitCode {
    to_exit(battery_impl(args))
}

fn battery_impl(args: &[String]) -> Result<bool, String> {
    let sub = args.first().map(String::as_str).unwrap_or("soc");
    let rest = &args[1..];
    let format = flag(rest, "--format", "text");
    match sub {
        "runaway" => {
            use tpt_elec_battery_thermal::{AdjacencyMatrix, ThermalRunawayModel};
            let cells = flag_usize(rest, "--cells", 6);
            if cells == 0 {
                return Err("--cells must be ≥ 1".into());
            }
            let init: Vec<usize> = opt_flag(rest, "--init")
                .unwrap_or("0")
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .filter(|&i| i < cells)
                .collect();
            if init.is_empty() {
                return Err("--init indices out of range".into());
            }
            let horizon = flag_f64(rest, "--horizon", 300.0);
            let dt = flag_f64(rest, "--dt", 0.5);
            let g = flag_f64(rest, "--conductance", 0.5);
            let model = ThermalRunawayModel {
                onset_temperature: flag_f64(rest, "--onset-c", 150.0),
                heat_generation_rate: flag_f64(rest, "--heat-w", 200.0),
                propagation_threshold: flag_f64(rest, "--threshold-c", 140.0),
                cell_heat_capacity: flag_f64(rest, "--capacity-jk", 800.0),
                ambient_conductance: g,
                ambient_c: flag_f64(rest, "--ambient-c", 25.0),
            };
            let adjacency = AdjacencyMatrix::chain(cells, g);
            let mut temps = vec![25.0f64; cells];
            for &i in &init {
                temps[i] = model.onset_temperature;
            }
            let risk = model.propagation_risk(&temps, &init, &adjacency, horizon, dt);
            if format == "json" {
                println!(
                    "{{\"will_propagate\":{},\"runaway_cells\":{:?},\"final_temps\":[{}]}}",
                    risk.will_propagate,
                    risk.runaway_cells,
                    risk.final_temperatures
                        .iter()
                        .map(|t| format!("{t:.1}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            } else {
                println!(
                    "runaway: {} cell(s) initiated → {} in runaway after {horizon} s ({})",
                    init.len(),
                    risk.runaway_cells.len(),
                    if risk.will_propagate {
                        "PROPAGATED"
                    } else {
                        "contained"
                    }
                );
                for (i, t) in risk.final_temperatures.iter().enumerate() {
                    let mark = if risk.runaway_cells.contains(&i) {
                        " ↯"
                    } else {
                        ""
                    };
                    println!("  cell {i}: {t:.1} °C{mark}");
                }
            }
            Ok(true)
        }
        "soc" => {
            use tpt_elec_battery_bms::KalmanFilterEstimator;
            use tpt_elec_battery_core::EquivalentCircuitModel;
            let capacity = flag_f64(rest, "--capacity", 2.0);
            let initial = flag_f64(rest, "--initial", 0.5);
            let current = flag_f64(rest, "--current", 2.0);
            let dt = flag_f64(rest, "--dt", 10.0);
            let steps = flag_usize(rest, "--steps", 60);
            let model = EquivalentCircuitModel::one_rc(
                flag_f64(rest, "--r0", 0.03),
                flag_f64(rest, "--r1", 0.02),
                flag_f64(rest, "--c1", 2000.0),
            );
            let mut kf = KalmanFilterEstimator::new(model.clone(), capacity, initial);
            let mut true_soc = flag_f64(rest, "--true-soc", initial);
            let mut rc = vec![0.0; model.rc_pairs.len()];
            for _ in 0..steps {
                true_soc = model.update_soc(true_soc, current, dt, capacity);
                rc = model.step(current, dt, &rc);
                let v = model.terminal_voltage(true_soc, current, &rc);
                kf.predict(current, dt);
                kf.update(v, current);
            }
            let est = kf.soc();
            let err = (est - true_soc).abs();
            if format == "json" {
                println!(
                    "{{\"soc_estimate\":{est:.4},\"soc_true\":{true_soc:.4},\"abs_error\":{err:.4}}}"
                );
            } else {
                println!(
                    "SOC after {steps} steps @ {current} A: estimate {:.1} %  \
                     (true {:.1} %, |err| {:.1} %)",
                    100.0 * est,
                    100.0 * true_soc,
                    100.0 * err
                );
            }
            Ok(true)
        }
        other => Err(format!(
            "unknown battery subcommand {other:?} (expected runaway|soc)"
        )),
    }
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

pub fn sweep(args: &[String]) -> ExitCode {
    to_exit(sweep_impl(args))
}

/// Parses `name=start..stop:points` (points optional, default 5).
fn parse_sweep_spec(spec: &str) -> Result<(String, f64, f64, usize), String> {
    let (name, range) = spec
        .split_once('=')
        .ok_or_else(|| format!("bad --param {spec:?} (want name=start..stop:points)"))?;
    let (span, points) = match range.split_once(':') {
        Some((s, p)) => (
            s,
            p.parse::<usize>()
                .map_err(|_| format!("bad points in {spec:?}"))?,
        ),
        None => (range, 5),
    };
    let (a, b) = span
        .split_once("..")
        .ok_or_else(|| format!("bad range in {spec:?} (want start..stop)"))?;
    let start: f64 = a.parse().map_err(|_| format!("bad start in {spec:?}"))?;
    let stop: f64 = b.parse().map_err(|_| format!("bad stop in {spec:?}"))?;
    if points == 0 || start == stop {
        return Err(format!("empty sweep in {spec:?}"));
    }
    Ok((name.to_string(), start, stop, points))
}

fn sweep_impl(args: &[String]) -> Result<bool, String> {
    let spec = opt_flag(args, "--param").ok_or("--param name=start..stop:points is required")?;
    let (name, start, stop, points) = parse_sweep_spec(spec)?;
    let objective = flag(args, "--objective", "impedance");
    let format = flag(args, "--format", "csv");
    let values: Vec<f64> = (0..points)
        .map(|i| start + (stop - start) * i as f64 / (points - 1) as f64)
        .collect();

    let mut rows = Vec::new();
    match objective.as_str() {
        "impedance" => {
            use tpt_elec_si_impedance::ImpedanceCalculator;
            let h_mm = flag_f64(args, "--height-mm", 0.2);
            let er = flag_f64(args, "--er", 4.4);
            let thickness = tpt_elec_core::Length::um(35.0);
            for &v in &values {
                let line = ImpedanceCalculator::microstrip(
                    tpt_elec_core::Length::mm(v),
                    thickness,
                    tpt_elec_core::Length::mm(h_mm),
                    er,
                );
                rows.push(format!("{v:.6},{:.4}", line.z0));
            }
            let header = format!("{name},z0_ohm");
            emit_sweep(&format, &header, &rows, &objective, &values)?;
        }
        "thermal_max" | "thermal" => {
            // Sweep power (watts) at a fixed Gerber if provided.
            let gerber = opt_flag(args, "--gerber")
                .ok_or("thermal_max objective requires --gerber <file>")?;
            let src = read_file(gerber)?;
            let gerber_p = GerberParser::parse(&src).map_err(|e| format!("{gerber}: {e}"))?;
            let materials = MaterialDatabase::standard();
            let resolution = flag_f64(args, "--resolution-mm", 2.0) * 1e-3;
            let thickness = flag_f64(args, "--thickness-mm", 1.6) * 1e-3;
            let ambient = flag_f64(args, "--ambient-c", 25.0);
            for &v in &values {
                let mut solver = ThermalSolver::from_gerber_scaled(
                    &gerber_p, &materials, resolution, thickness, 35e-6,
                );
                solver.set_ambient(ambient);
                let top: Vec<u32> = {
                    let g = solver.grid();
                    let z = g.nz() - 1;
                    (0..g.nx())
                        .flat_map(|x| (0..g.ny()).map(move |y| (x, y)))
                        .filter_map(|(x, y)| g.index(x, y, z).map(|i| i as u32))
                        .collect()
                };
                solver.add_boundary_condition(BoundaryCondition::Convection {
                    surface: top,
                    h: flag_f64(args, "--h-conv", 10.0),
                    t_ambient: ambient,
                });
                // Center heat source of `v` watts.
                let (cx, cy) = {
                    let g = solver.grid();
                    (
                        (g.nx() / 2) as f64 * resolution,
                        (g.ny() / 2) as f64 * resolution,
                    )
                };
                solver.add_heat_source_near(cx, cy, v);
                let r = solver.solve_steady_state();
                rows.push(format!("{v:.6},{:.2}", r.max_temp));
            }
            let header = format!("{name},max_temp_c");
            emit_sweep(&format, &header, &rows, &objective, &values)?;
        }
        other => {
            return Err(format!(
                "unknown objective {other:?} (impedance|thermal_max)"
            ))
        }
    }
    Ok(true)
}

fn emit_sweep(
    format: &str,
    header: &str,
    rows: &[String],
    objective: &str,
    values: &[f64],
) -> Result<(), String> {
    if format == "json" {
        let pts: Vec<String> = rows
            .iter()
            .map(|r| {
                let nums: Vec<&str> = r.split(',').collect();
                let y = nums.get(1).copied().unwrap_or("0");
                format!(
                    "{{\"x\":{},\"y\":{y}}}",
                    nums.first().copied().unwrap_or("0")
                )
            })
            .collect();
        println!(
            "{{\"objective\":\"{objective}\",\"param\":{}}}",
            pts.join(",")
        );
    } else {
        println!("{header}");
        for r in rows {
            println!("{r}");
        }
        eprintln!(
            "{} point(s), range [{}, {}]",
            values.len(),
            values[0],
            values[values.len() - 1]
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// dispatch + tests
// ---------------------------------------------------------------------------

/// Routes argv (without the program name) to a subcommand.
pub fn dispatch(items: &[String]) -> ExitCode {
    let command = items.first().map(|s| s.as_str()).unwrap_or("");
    let rest: Vec<String> = items.iter().skip(1).cloned().collect();
    match command {
        "thermal" => thermal(&rest),
        "impedance" => impedance(&rest),
        "drc" => drc(&rest),
        "report" => report(&rest),
        "spice" => spice(&rest),
        "rf" => rf(&rest),
        "power" => power(&rest),
        "emc" => emc(&rest),
        "battery" => battery(&rest),
        "sweep" => sweep(&rest),
        "help" | "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        "" => {
            print_usage();
            ExitCode::FAILURE
        }
        other => {
            eprintln!("error: unknown command {other:?}");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        // Support double-quoted tokens so fixture paths with spaces survive.
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut in_quotes = false;
        for c in s.chars() {
            match c {
                '"' => in_quotes = !in_quotes,
                c if c.is_whitespace() && !in_quotes => {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    fn netlist_fixture() -> String {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../test-data/spice/rc_lowpass.net"
        )
        .to_string()
    }

    fn spice_netlist(extra: &str) -> Vec<String> {
        args(&format!("--netlist \"{}\" {extra}", netlist_fixture()))
    }

    fn ok(r: Result<bool, String>) -> bool {
        match r {
            Ok(_) => true,
            Err(e) => {
                eprintln!("unexpected err: {e}");
                false
            }
        }
    }

    fn err(r: Result<bool, String>) -> bool {
        r.is_err()
    }

    fn exit_is(code: ExitCode, expect_success: bool) -> bool {
        // ExitCode has no public comparator; re-dispatch help is success and
        // unknown is failure via to_exit semantics. Use a side channel: run
        // through to_exit on a known Result instead where possible.
        // Here we only need equality with SUCCESS/FAILURE construction:
        format!("{code:?}")
            == format!(
                "{:?}",
                if expect_success {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            )
    }

    #[test]
    fn flag_parsing() {
        let a = args("--foo 1.5 --bar baz");
        assert_eq!(flag(&a, "--foo", "0"), "1.5");
        assert_eq!(flag(&a, "--bar", ""), "baz");
        assert_eq!(flag(&a, "--missing", "d"), "d");
        assert_eq!(flag_f64(&a, "--foo", 0.0), 1.5);
        assert_eq!(flag_f64(&a, "--nope", 2.0), 2.0);
        assert!(has_flag(&args("--watch"), "--watch"));
        assert!(!has_flag(&a, "--watch"));
        assert_eq!(opt_flag(&a, "--bar"), Some("baz"));
        assert_eq!(opt_flag(&a, "--none"), None);
        assert_eq!(flag_u32(&args("--n 7"), "--n", 1), 7);
        assert_eq!(flag_usize(&args("--n 3"), "--n", 9), 3);
    }

    #[test]
    fn to_exit_maps_results() {
        assert!(exit_is(to_exit(Ok(true)), true));
        assert!(exit_is(to_exit(Ok(false)), false));
        assert!(exit_is(to_exit(Err("x".into())), false));
    }

    #[test]
    fn dispatch_unknown_and_help() {
        assert!(exit_is(dispatch(&args("definitely-not-a-command")), false));
        assert!(exit_is(dispatch(&[]), false));
        assert!(exit_is(dispatch(&args("help")), true));
        assert!(exit_is(dispatch(&args("--help")), true));
    }

    #[test]
    fn thermal_requires_gerber() {
        assert!(err(thermal_impl(&[])));
        assert!(err(thermal_impl(&args("--format json"))));
        assert!(exit_is(dispatch(&args("thermal --format json")), false));
    }

    #[test]
    fn impedance_defaults_and_suggest() {
        assert!(ok(impedance_impl(&[])));
        assert!(ok(impedance_impl(&args("--suggest --target 50"))));
        assert!(ok(impedance_impl(&args(
            "--width-mm 0.3 --height-mm 0.2 --er 4.4"
        ))));
        assert!(exit_is(dispatch(&args("impedance --width-mm 0.35")), true));
    }

    #[test]
    fn drc_requires_kicad() {
        assert!(err(drc_impl(&[])));
        assert!(err(drc_impl(&args("--kicad missing.kicad_pcb"))));
    }

    #[test]
    fn report_requires_data() {
        assert!(err(report_impl(&args("--out definitely-missing.csv"))));
    }

    #[test]
    fn spice_dc_ac_tran_from_fixture() {
        assert!(ok(spice_impl(&spice_netlist("--analysis dc"))));
        assert!(ok(spice_impl(&spice_netlist(
            "--analysis ac --fstart 1k --fstop 1meg --ppd 5"
        ))));
        assert!(ok(spice_impl(&spice_netlist(
            "--analysis tran --tstop 10u --tstep 100n --node out"
        ))));
        assert!(ok(spice_impl(&spice_netlist(
            "--analysis noise --points 5"
        ))));
        assert!(err(spice_impl(&args(
            "--netlist missing.net --analysis dc"
        ))));
        assert!(err(spice_impl(&spice_netlist("--analysis bogus"))));
        assert!(err(spice_impl(&spice_netlist(
            "--analysis ac --node nosuch"
        ))));
        assert!(exit_is(dispatch(&args("spice")), false));
    }

    #[test]
    fn spice_json_output() {
        assert!(ok(spice_impl(&spice_netlist(
            "--analysis dc --format json"
        ))));
    }

    #[test]
    fn rf_match_and_filter() {
        assert!(ok(rf_impl(&args(
            "match --source 50 --load 10 --freq 100e6"
        ))));
        assert!(ok(rf_impl(&args(
            "match --pi --source 50 --load 5 --freq 1e8 --q 3"
        ))));
        assert!(ok(rf_impl(&args(
            "filter --type butterworth --order 3 --response lp --cutoff 1e6"
        ))));
        assert!(ok(rf_impl(&args(
            "filter --type chebyshev1 --order 3 --ripple-db 1 --cutoff 1e6 --format json"
        ))));
        assert!(ok(rf_impl(&args(
            "filter --type chebyshev2 --order 4 --stopband-db 40 --cutoff 1e6"
        ))));
        assert!(err(rf_impl(&args("filter --type nope --order 3"))));
        assert!(err(rf_impl(&args("filter --type elliptic --order 3"))));
        assert!(err(rf_impl(&args("bogus"))));
        assert!(exit_is(dispatch(&args("rf match --load 20")), true));
    }

    #[test]
    fn power_buck_and_compensator() {
        assert!(ok(power_impl(&args(
            "buck --vin 12 --vout 3.3 --iout 2 --fsw 500e3"
        ))));
        assert!(err(power_impl(&args("buck --vin 5 --vout 12"))));
        assert!(ok(power_impl(&args(
            "compensator --type ii --fc 10e3 --pm 60 --format json"
        ))));
        assert!(ok(power_impl(&args(
            "compensator --type iii --fc 5e3 --pm 45"
        ))));
        assert!(err(power_impl(&args("compensator --type iv"))));
        assert!(err(power_impl(&args("bogus"))));
        assert!(exit_is(
            dispatch(&args("power buck --vin 12 --vout 5")),
            true
        ));
    }

    #[test]
    fn emc_radiated_and_conducted() {
        // Soft-fail (non-compliant) is Ok(false); validation errors are Err.
        let r = emc_impl(&args(
            "radiated --freq-mhz 100 --loop-area-mm2 100 --current-a 0.1 --rise-ns 2",
        ));
        assert!(r.is_ok(), "{r:?}");
        let r = emc_impl(&args(
            "radiated --freq-mhz 100 --standard cispr32-b --format json",
        ));
        assert!(r.is_ok(), "{r:?}");
        let r = emc_impl(&args(
            "conducted --fsw 100e3 --di-dt 1e6 --l-nh 100 --standard cispr32-b",
        ));
        assert!(r.is_ok(), "{r:?}");
        assert!(err(emc_impl(&args("radiated --standard bogus"))));
        assert!(err(emc_impl(&args("immunity"))));
        assert!(emc_impl(&args("radiated --freq-mhz 50")).is_ok());
    }

    #[test]
    fn battery_runaway_and_soc() {
        assert!(ok(battery_impl(&args(
            "runaway --cells 5 --init 0 --horizon 60 --dt 0.5"
        ))));
        assert!(ok(battery_impl(&args(
            "runaway --cells 4 --init 0,1 --format json"
        ))));
        assert!(err(battery_impl(&args("runaway --cells 0"))));
        assert!(err(battery_impl(&args("runaway --init 99 --cells 3"))));
        assert!(ok(battery_impl(&args(
            "soc --capacity 2 --initial 0.5 --current 2 --steps 30 --format json"
        ))));
        assert!(err(battery_impl(&args("pack"))));
        assert!(ok(battery_impl(&args("soc --steps 5"))));
        assert!(exit_is(dispatch(&args("battery soc --steps 5")), true));
    }

    #[test]
    fn sweep_param_parser_and_impedance() {
        let (n, a, b, p) = parse_sweep_spec("w=0.2..0.5:4").unwrap();
        assert_eq!(n, "w");
        assert!((a - 0.2).abs() < 1e-12);
        assert!((b - 0.5).abs() < 1e-12);
        assert_eq!(p, 4);
        let (_, _, _, p2) = parse_sweep_spec("w=0.2..0.4").unwrap();
        assert_eq!(p2, 5);
        assert!(parse_sweep_spec("noequals").is_err());
        assert!(parse_sweep_spec("w=bad..0.4").is_err());
        assert!(parse_sweep_spec("w=0.2..0.4:0").is_err());

        assert!(ok(sweep_impl(&args(
            "--param w=0.2..0.4:3 --objective impedance --height-mm 0.2 --er 4.4"
        ))));
        assert!(err(sweep_impl(&args(
            "--param w=0.2..0.4:3 --objective bogus"
        ))));
        assert!(err(sweep_impl(&args("--objective impedance"))));
        assert!(err(sweep_impl(&args(
            "--param w=0.2..0.4:3 --objective thermal_max"
        ))));
        assert!(exit_is(dispatch(&args("sweep --param w=0.2..0.3:2")), true));
    }
}
