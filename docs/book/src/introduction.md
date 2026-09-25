# Introduction

**tpt-electronics** is a multiphysics simulation engine for electronic systems —
thermal, circuit (SPICE), signal/power integrity, RF, power conversion, EMC,
battery, and manufacturing — written in pure Rust and dual-licensed
MIT OR Apache-2.0.

## Why this exists

Commercial multiphysics stacks are closed, expensive, and hard to script.
tpt-electronics is fully open-source, headless-first (CLI + library + WASM),
and designed so each domain is an independent crate you can `cargo add` (once
published) or path-depend today.

## What you can do today

- Gerber → thermal temperature field (CSV/HTML report)
- SPICE netlist → DC/AC/transient/noise
- Impedance, eye diagrams, PDN
- RF matching / filters / link budget
- Buck converter design + compensator
- CISPR 32 emissions, shielding
- Battery SOC + runaway propagation
- DRC, yield, boundary scan

## How the book is organized

1. [Which crate do I need?](crate-map.md) — decision table
2. [Tutorial](tutorial.md) — narrated KiCad → report walkthrough
3. Domain chapters (thermal, SPICE, SI, RF, power, EMC/battery, semi/mfg)
4. [WASM](wasm.md) & [Validation](validation.md)

## Quick start

See [Getting started](getting-started.md) for paste-able commands
(`cargo generate`, CLI install, first thermal run).

`tpt-electronics` is a fully open-source, MIT-licensed multiphysics
simulation engine for electronic systems, written in pure Rust.

It solves the problems that kill hardware projects:

| Problem | Crates |
|---|---|
| Thermal runaway | `tpt-elec-thermal`, `-transient`, `-joule` |
| Circuit behavior | `tpt-elec-spice-*` |
| Signal integrity | `tpt-elec-si-*` |
| Power delivery | `tpt-elec-pi-*`, `tpt-elec-power-*` |
| EMC compliance | `tpt-elec-emc-*` |
| Battery safety | `tpt-elec-battery-*` |
| Manufacturability | `tpt-elec-mfg-*` |

## Design principles

1. **Zero runtime dependencies.** The linear algebra, complex arithmetic,
   and even the ZIP reader for ODB++ live in-tree. The dependency graph is
   auditable in minutes, and `cargo-deny` enforces the permissive-only
   license chain.
2. **Standards-anchored validation.** Every numeric crate carries tests
   against analytic solutions or published chart/standard points (JEDEC
   JESD51, IPC-2152/2221, CISPR 32, IEC 61000, IEEE 1149.1, UN GTR 20).
3. **Golden files over snapshots.** `test-data/golden` holds expected
   values with explicit tolerances and physical sanity checks.
4. **No GPL traps.** Elmer FEM, OpenFOAM, GMSH, and Netgen are explicitly
   banned in `deny.toml`; meshing is voxel/hexahedral only.
