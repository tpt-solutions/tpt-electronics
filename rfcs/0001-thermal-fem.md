# RFC 0001: Thermal FEM

- **Status:** Implemented
- **Crate:** `tpt-elec-thermal` (+ `-geometry`, `-transient`, `-joule`)

## Summary

Voxel-based finite-volume thermal solver with Conjugate Gradient, anisotropic
material tensors, and penalty Dirichlet conditions.

## Motivation

PCB thermal analysis without GPL meshing tools (GMSH/Netgen) and without
proprietary suites. Voxel meshes keep assembly trivial and parallelizable.

## Detailed design

* 7-point stencil, harmonic-mean pairing across material interfaces.
* CSR sparse matrix + CG (`tpt-elec-core::linalg`); penalty scale = 10⁶ ×
  max edge conductance.
* Radiation linearized per-iteration: `h_r = εσ(Ts²+T∞²)(Ts+T∞)`.
* Transients: lumped capacitance `C = ρcpV`, backward Euler via CG on
  `K + C/Δt`.

## Alternatives

Tetrahedral FEM (rejected: GPL ecosystem, complexity), lumped RC networks
(rejected: no spatial field), FFT methods (rejected: non-rect geometry).

## Unresolved questions

GPU compute (wgpu) for transient sweeps — deferred until the WASM viewer
exercises the CPU path in production.
