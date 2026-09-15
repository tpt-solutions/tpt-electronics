// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tpt-elec-cli` — command-line front end for tpt-electronics.
//!
//! Subcommands: `thermal`, `impedance`, `drc`, `report`

#![forbid(unsafe_code)]

mod commands;

use std::process::ExitCode;

fn main() -> ExitCode {
    let items: Vec<String> = std::env::args().skip(1).collect();
    let command = items.first().map(|s| s.as_str()).unwrap_or("");
    let rest: Vec<String> = items.iter().skip(1).cloned().collect();
    match command {
        "thermal" => commands::thermal(&rest),
        "impedance" => commands::impedance(&rest),
        "drc" => commands::drc(&rest),
        "report" => commands::report(&rest),
        "help" | "--help" | "-h" => {
            commands::print_usage();
            ExitCode::SUCCESS
        }
        "" => {
            commands::print_usage();
            ExitCode::FAILURE
        }
        other => {
            eprintln!("error: unknown command {other:?}");
            commands::print_usage();
            ExitCode::FAILURE
        }
    }
}
