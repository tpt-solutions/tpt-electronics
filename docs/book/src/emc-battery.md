# EMC & battery

## EMC

- `tpt-elec-emc-core` — CISPR 32 / FCC Part 15 limit lines (Class A/B,
  radiated at 3 m and conducted) plus IEC 61000-4 severity tables.
- `tpt-elec-emc-emissions` — trapezoidal clock harmonic envelope
  (flat → −20 dB/dec → −40 dB/dec) and small-loop radiation; conducted
  noise from `L·di/dt`.
- `tpt-elec-emc-immunity` — port-level immunity plans (ESD/surge/EFT/RF).
- `tpt-elec-emc-shielding` — plane-wave `SE = R + A + B` with skin-depth
  models for copper/aluminum/steel/mu-metal/paint.
- `tpt-elec-emc-grounding` — return-path inductance, slot crossings,
  ground-loop coupling.

## Battery

- `tpt-elec-battery-core` — cell/chemistry tables, OCV curves, Thevenin
  ECM with exact RC stepping and coulomb counting.
- `tpt-elec-battery-thermal` — cell-to-cell runaway propagation over an
  adjacency matrix (barrier studies).
- `tpt-elec-battery-bms` — coulomb counting and an EKF over `[SOC, V_rc]`
  with voltage-correction updates.
- `tpt-elec-battery-pack` — s/p composition, min-rule SOC, thermal
  supervision, imbalance alarms.
