# RFC 0002: SPICE MNA

- **Status:** Implemented
- **Crates:** `tpt-elec-spice-core/-models/-analysis/-noise/-netlist`

## Summary

Modified Nodal Analysis circuit simulator with Newton-Raphson DC, complex LU
AC sweep, and trapezoidal transient, covering R/L/C/D/M/Q/V/I/X and op-amp
macromodels.

## Motivation

Pure-Rust circuit simulation for converter and SI front-end studies with no
NGSpice linkage and no proprietary model libraries.

## Detailed design

* Unknown vector `[V₁..Vₙ, I_branch]`; branch rows for voltage sources,
  inductors, and op-amp outputs.
* Nonlinear devices stamp conductance + VCCS + companion current terms;
  **both legs** of every companion source are stamped (the buck converter's
  non-ground return paths exposed a missing cathode/source return early on).
* Diode junction limiting (pnjlim-style geometric walk) plus damped Newton
  with full-step escapes on 2-cycle stalls; transient steps halve on
  failure and grow after repeated easy steps.
* AC uses a dense complex LU with per-device `jω` stamps; op-amps get their
  single-pole rolloff.

## Alternatives

Sparse solver libraries (rejected: zero-dependency policy), relaxation
(iteration-entrance) methods (deferred), Verilog-A (out of scope).

## Unresolved questions

BSIM3/4 and EKV parameter sets (enum slots currently fall back to Level 1).
