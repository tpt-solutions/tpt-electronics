// SPDX-License-Identifier: MIT OR Apache-2.0

//! Workspace automation for `tpt-electronics`.
//!
//! Run with `cargo run -p xtask -- <task>`.
//!
//! ## `regen-goldens`
//!
//! Verifies — and on request rewrites — the JSON fixtures under
//! `test-data/golden/`.
//!
//! ```text
//! cargo run -p xtask -- regen-goldens            # check only (default)
//! cargo run -p xtask -- regen-goldens --write    # refresh drifted values
//! cargo run -p xtask -- regen-goldens --case butterworth_filter
//! cargo run -p xtask -- regen-goldens --list
//! ```
//!
//! The expected values are recomputed from closed-form analytics in
//! [`reference`], **not** from the crates under test. That distinction is the
//! whole point: a golden regenerated from the implementation stops detecting
//! regressions the moment it is written. Check mode is therefore the default
//! and is safe for CI, while `--write` prints every field it would change so a
//! reviewer can judge the drift before accepting it.

mod json_edit;
mod reference;
mod refmodel;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// What the tool should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Report drift, change nothing, fail on drift.
    Check,
    /// Rewrite drifted numeric leaves in place.
    Write,
}

/// Parsed `regen-goldens` options.
#[derive(Clone, Debug)]
struct Options {
    mode: Mode,
    only: Option<String>,
    list: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("regen-goldens") => match run_regen_goldens(&args[1..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("xtask: {e}");
                ExitCode::FAILURE
            }
        },
        Some("-h") | Some("--help") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown task {other:?}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

/// Prints the available tasks.
fn print_usage() {
    println!("xtask — workspace automation for tpt-electronics\n");
    println!("USAGE:");
    println!("    cargo run -p xtask -- <task> [options]\n");
    println!("TASKS:");
    println!("    regen-goldens        Verify (or refresh) test-data/golden fixtures\n");
    println!("OPTIONS for regen-goldens:");
    println!("    --write              Rewrite drifted values (default: check only)");
    println!("    --case <name>        Restrict to a single case");
    println!("    --list               List every known case and exit");
    println!("    -h, --help           Show this help");
}

/// Parses the flags for `regen-goldens`.
fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut opts = Options {
        mode: Mode::Check,
        only: None,
        list: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--write" => opts.mode = Mode::Write,
            "--list" => opts.list = true,
            "--case" => {
                i += 1;
                let name = args.get(i).ok_or("--case needs a name")?;
                opts.only = Some(name.clone());
            }
            other => return Err(format!("unknown option {other:?}")),
        }
        i += 1;
    }
    Ok(opts)
}

/// Locates the workspace root: xtask's parent directory.
fn workspace_root() -> Result<PathBuf, String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| "CARGO_MANIFEST_DIR is not set; run through cargo".to_string())?;
    let dir = Path::new(&manifest)
        .parent()
        .ok_or_else(|| "xtask has no parent directory".to_string())?;
    if !dir.join("Cargo.toml").is_file() {
        return Err(format!("{} is not the workspace root", dir.display()));
    }
    Ok(dir.to_path_buf())
}

/// Outcome for one case.
struct CaseResult {
    name: &'static str,
    checked: usize,
    /// (path, value-in-file, reference, allowed-delta) for values outside tolerance.
    drifted: Vec<(String, f64, f64, f64)>,
    /// (path, old, new) for values actually rewritten in `--write` mode.
    rewritten: Vec<(String, f64, f64)>,
    notes: Vec<String>,
}

/// Runs the `regen-goldens` task.
fn run_regen_goldens(args: &[String]) -> Result<ExitCode, String> {
    let opts = parse_options(args)?;
    let root = workspace_root()?;
    let cases = reference::cases();

    if opts.list {
        println!("Regenerable (independent reference):");
        for c in &cases {
            println!("  {:<28} {}", c.name, c.basis);
        }
        println!("\nSpecification only (no derived numeric value):");
        for (name, _path, why) in reference::specification_only() {
            println!("  {:<28} {}", name, why);
        }
        println!("\nSimulation-derived (never rewritten by this tool):");
        for (name, _path, why) in reference::simulation_derived() {
            println!("  {:<28} {}", name, why);
        }
        return Ok(ExitCode::SUCCESS);
    }

    let selected: Vec<&reference::Case> = match &opts.only {
        Some(name) => {
            let found: Vec<&reference::Case> = cases.iter().filter(|c| c.name == name).collect();
            if found.is_empty() {
                let known: Vec<&str> = cases.iter().map(|c| c.name).collect();
                return Err(format!(
                    "unknown case {name:?}; regenerable cases are {known:?} \
                     (simulation-derived fixtures are never rewritten)"
                ));
            }
            found
        }
        None => cases.iter().collect(),
    };

    let mut results = Vec::new();
    for case in selected {
        results.push(process_case(&root, case, opts.mode)?);
    }
    report(&results, opts.mode);

    if results.iter().any(|r| !r.notes.is_empty()) {
        return Ok(ExitCode::FAILURE);
    }
    if opts.mode == Mode::Check && results.iter().any(|r| !r.drifted.is_empty()) {
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Reads, checks and optionally rewrites one case.
fn process_case(root: &Path, case: &reference::Case, mode: Mode) -> Result<CaseResult, String> {
    let path = root.join(case.path);
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", case.path))?;
    let mut doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", case.path))?;

    let mut result = CaseResult {
        name: case.name,
        checked: 0,
        drifted: Vec::new(),
        rewritten: Vec::new(),
        notes: Vec::new(),
    };

    // Each file declares its own tolerance; that is the reviewer's contract.
    let tolerance = match json_edit::tolerance(&doc) {
        Some((_, t)) if t > 0.0 => t,
        _ => {
            result
                .notes
                .push("no positive top-level tolerance field".to_string());
            return Ok(result);
        }
    };

    for leaf in &case.leaves {
        result.checked += 1;
        match json_edit::get_number(&doc, leaf.path) {
            Some(cur) => {
                let limit = leaf.allowed(cur, tolerance);
                if (cur - leaf.value).abs() > limit {
                    result
                        .drifted
                        .push((leaf.path.to_string(), cur, leaf.value, limit));
                }
            }
            None => {
                result
                    .drifted
                    .push((leaf.path.to_string(), f64::NAN, leaf.value, tolerance));
            }
        }
    }

    if mode == Mode::Write && !result.drifted.is_empty() {
        // Only touch the values that actually drifted. Rewriting every leaf
        // would restate the ones that are already correct at full precision and
        // bury the real change in formatting noise.
        for leaf in &case.leaves {
            let Some((_, cur, _, _)) = result.drifted.iter().find(|(p, _, _, _)| *p == leaf.path)
            else {
                continue;
            };
            let _ = cur;
            json_edit::set_number(&mut doc, leaf.path, leaf.value)
                .map_err(|e| format!("{}: {e}", case.path))?;
            result
                .rewritten
                .push((leaf.path.to_string(), *cur, leaf.value));
        }
        let out = json_edit::to_string_pretty_like(&doc, &raw);
        std::fs::write(&path, out).map_err(|e| format!("{}: {e}", case.path))?;
    }

    Ok(result)
}

/// Formats a float compactly: keeps tiny and huge values legible
/// (`1.968e-11`) and trims trailing zeros elsewhere (`1025`, `0.2`).
fn num(v: f64) -> String {
    if v.is_nan() {
        return "missing".to_string();
    }
    if !v.is_finite() {
        return v.to_string();
    }
    let a = v.abs();
    if a != 0.0 && (a < 1e-3 || a >= 1e6) {
        return format!("{v:.3e}");
    }
    let s = format!("{v:.6}");
    // Drop trailing zeros only when a decimal point is present, so integral
    // values keep their ".0" and never collapse to "1025.".
    match s.find('.') {
        Some(dot) => {
            let trimmed = s[dot..].trim_end_matches('0');
            if trimmed == "." {
                s[..dot].to_string()
            } else {
                format!("{}{}", &s[..dot], trimmed)
            }
        }
        None => s,
    }
}

/// Prints a human-readable summary.
fn report(results: &[CaseResult], mode: Mode) {
    println!(
        "xtask regen-goldens ({})",
        match mode {
            Mode::Check => "check",
            Mode::Write => "regen",
        }
    );
    for r in results {
        if !r.notes.is_empty() {
            println!("  {:<24} skipped: {}", r.name, r.notes.join("; "));
            continue;
        }
        if r.drifted.is_empty() {
            println!("  {:<24} ok ({} values)", r.name, r.checked);
            continue;
        }
        match mode {
            Mode::Check => {
                println!(
                    "  {:<24} DRIFT ({} of {} values)",
                    r.name,
                    r.drifted.len(),
                    r.checked
                );
                for (path, cur, want, limit) in &r.drifted {
                    println!(
                        "      {path:<38} file={}  reference={}  tol={}",
                        num(*cur),
                        num(*want),
                        num(*limit)
                    );
                }
            }
            Mode::Write => {
                println!("  {:<24} rewrote {} value(s)", r.name, r.rewritten.len());
                for (path, from, to) in &r.rewritten {
                    println!("      {path:<38} {} -> {}", num(*from), num(*to));
                }
            }
        }
    }
    if mode == Mode::Check && results.iter().any(|r| !r.drifted.is_empty()) {
        println!("\nRun with --write to accept the references above, after reviewing them.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_check_mode() {
        assert_eq!(parse_options(&[]).expect("parse").mode, Mode::Check);
        assert_eq!(
            parse_options(&["--write".into()]).expect("parse").mode,
            Mode::Write
        );
    }

    #[test]
    fn rejects_unknown_options() {
        let err = parse_options(&["--nope".into()]).expect_err("must reject");
        assert!(err.contains("--nope"), "{err}");
    }

    #[test]
    fn case_filter_requires_a_name() {
        assert!(parse_options(&["--case".into()]).is_err());
        let o = parse_options(&["--case".into(), "butterworth_filter".into()]).expect("parse");
        assert_eq!(o.only.as_deref(), Some("butterworth_filter"));
    }

    #[test]
    fn workspace_root_contains_the_manifest() {
        let root = workspace_root().expect("root");
        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("test-data").is_dir());
    }
}
