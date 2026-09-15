# Introduction

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
