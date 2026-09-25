# AGENTS.md — tpt-electronics

## What this is

Rust monorepo (~50 crates) for multiphysics electronics simulation.
Workspace root at repo root; non-Rust tools (`vscode-extension/`, `kicad-plugin/`) sit outside Cargo.

## Quick commands

```console
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo deny check licenses
```

CI enforces `RUSTFLAGS: -D warnings`; match it locally to catch failures early.

Single-crate: `cargo test -p tpt-elec-thermal`
Single-example: `cargo run --example high-current-trace`
WASM check: `cargo check -p tpt-elec-wasm --target wasm32-unknown-unknown`

## Contribution rules (enforced by CI)

- **No CLA, no DCO, no PRs:** contributions are not accepted. The repo is dual-licensed `MIT OR Apache-2.0`, maintained by its author; external feedback happens through GitHub issues (see `CONTRIBUTING.md`). CI does not check commit sign-off.
- **SPDX header:** every new Rust source file starts with `// SPDX-License-Identifier: MIT OR Apache-2.0` (template: `docs/templates/source-header.rs`).
- **License chain:** `MIT OR Apache-2.0` only. `cargo-deny` (`deny.toml`) rejects GPL/LGPL crates and bans `elmer-fem`, `openfoam`, `gmsh`, `netgen`, `ngspice-sys` by name.

## Toolchain

- Rust edition 2021, MSRV 1.75 (CI has a dedicated MSRV job).
- `unsafe_code = "forbid"` at workspace level.
- `clippy.toml`: `too-many-arguments-threshold = 8`.
- `rustfmt.toml`: `max_width = 100`, `use_field_init_shorthand = true`.

## Workspace layout

| Directory | Domain |
|---|---|
| `crates/core/*` | Foundation types, units, linalg, geometry, materials |
| `crates/formats/*` | Gerber, KiCad, ODB++, IPC-2581, Touchstone, SPICE |
| `crates/thermal/*` | Steady-state, transient, convection, Joule heating |
| `crates/circuit/*` | MNA, SPICE models, DC/AC/transient/noise |
| `crates/signal-integrity/*` | T-lines, impedance, crosstalk, eye, PDN |
| `crates/rf/*` | Smith, matching, filters, antennas, mixers |
| `crates/power/*` | Converters, magnetics, switches, control, battery |
| `crates/semiconductor/*` | MOSFET, diode, BJT, PDK abstractions |
| `crates/emc/*` | Standards, emissions, immunity, shielding |
| `crates/battery/*` | Cell models, thermal runaway, BMS, pack |
| `crates/manufacturing/*` | DRC/DFM, boundary scan, yield |
| `crates/cli/*` | `tpt-elec-cli` binary (Gerber → thermal CSV, impedance, DRC) |

## Non-obvious conventions

- `[workspace.package]` and `[workspace.dependencies]` are the source of truth for version, edition, rust-version, license, and metadata. Crates repeat via `*.workspace = true`.
- `Cargo.lock` is gitignored (library workspace); do not commit it.
- Examples under `examples/*` are workspace members and runnable with `cargo run --example <name>`.
- Benchmarks under `benches/` use criterion; run with `cargo bench`.
- Test fixtures live in `test-data/` (gerber, kicad, odbpp, touchstone).

## Stable vs Alpha

See `README.md` for per-crate status. Alpha crates (`🚧`) have unstable APIs; check RFCs in `rfcs/` before changing public surfaces.
