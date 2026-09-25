// SPDX-License-Identifier: MIT OR Apache-2.0

//! `tpt-elec-cli` — command-line front end for tpt-electronics.
//!
//! Subcommands: `thermal`, `impedance`, `drc`, `report`, `spice`, `rf`,
//! `power`, `emc`, `battery`, `sweep`

#![forbid(unsafe_code)]

mod commands;

use std::process::ExitCode;

fn main() -> ExitCode {
    let items: Vec<String> = std::env::args().skip(1).collect();
    commands::dispatch(&items)
}
