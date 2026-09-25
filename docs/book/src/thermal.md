# Thermal analysis

Steady-state, transient, convection, and Joule heating with the
`tpt-elec-thermal` family.

## Steady state

`ThermalSolver::solve_steady_state` assembles \([K]\), applies boundary
conditions, and runs a Conjugate Gradient solve. Anisotropic tensors handle
PCB substrates (in-plane ≫ through-plane).

Boundary conditions: `FixedTemperature`, `Convection`, `Radiation`,
`HeatFlux`, `HeatSource`.

```rust
let result = solver.solve_steady_state(&mut problem)?;
let t_max = result.max_temperature;
```

## Transient

`tpt-elec-transient` solves \([C]\{\dot T\} + [K]\{T\} = \{Q(t)\}\) with
explicit/implicit integration → `TransientResult` (time history).

## Convection

`tpt-elec-convection` → `calculate_h`: Nusselt from Rayleigh (natural) or
Reynolds + Prandtl (forced).

## Joule heating (coupled)

`tpt-elec-joule` iterates electrical solve → \(\rho(T)\) update → thermal →
converge (`CoupledResult`). Power density uses the **current** temperature
field (B4 fix).

## From Gerber

Rasterize copper (respecting `Polarity::Clear` cutouts — B3 fix), map to
voxels (`tpt-elec-geometry`), attach materials (`tpt-elec-materials`), solve.

CLI end-to-end: `tpt-elec-cli thermal …` (see [Tutorial](tutorial.md)).

## Validation

JEDEC JESD51 two-resistor checks (see [Validation](validation.md)).

## Model

Boards are voxelized (`tpt-elec-geometry::VoxelGrid`) and assembled into a
7-point finite-volume stiffness matrix with harmonic-mean material pairing
(`tpt-elec-thermal`). Anisotropic substrates — FR4's in-plane vs
through-thickness conductivity — are handled by evaluating the material
tensor along each axis.

The system `[K]{T} = {Q}` is solved with Conjugate Gradient on a CSR
matrix. Dirichlet (fixed-temperature) conditions use a symmetric penalty
so the matrix stays SPD; radiation is linearized
(`h_r = εσ(T_s²+T_∞²)(T_s+T_∞)`) and iterated to convergence.

## Boundary conditions

| BC | Meaning |
|---|---|
| `FixedTemperature` | heat sink / plane clamp |
| `Convection` | `h·A·(T−T∞)` on surface cells |
| `Radiation` | linearized Stefan–Boltzmann, iterated |
| `HeatFlux` | prescribed W/m² |
| `HeatSource` | lumped component power |

## Transients

`tpt-elec-transient` integrates `[C]{dT/dt} + [K]{T} = {Q(t)}` with
implicit (backward) Euler — unconditionally stable — or explicit Euler
with a lumped-mass stability check. The lumped capacitance uses
`C = ρ·cp·V` per cell.

## Electro-thermal coupling

`tpt-elec-joule` iterates electrical solve → edge power `G(Vᵢ−Vⱼ)²` →
thermal solve → resistivity update `ρ(T) = ρ₀(1+αΔT)` until the
temperature field converges.

## Validation

* 1-D slab conduction against the analytic linear profile.
* Lumped single-cell convection against `T = T∞ + P/(h·A)`.
* Golden file `test-data/golden/thermal/simple_resistor_board.json`.
* JEDEC JESD51-style two-resistor models on `ThermalComponentModel`.
