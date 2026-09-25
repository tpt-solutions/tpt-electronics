# EMC & battery

Predict emissions/immunity, estimate shielding, and model battery packs.

## EMC standards & limits

`tpt-elec-emc-core` defines `EmcStandard` (CISPR 32 A/B, FCC Part 15 A/B,
MIL-STD-461, DO-160, IEC 61000, automotive), `EmcLimit`, and `EmcTestType`.

## Emissions

`tpt-elec-emc-emissions`:

- Radiated: trapezoidal waveform harmonic spectrum.
- Conducted: di/dt and parasitic inductance paths.

CLI:

```console
$ tpt-elec-cli emc --standard cispr32-b --waveform clock.json
```

## Immunity

Radiated/conducted immunity, ESD, surge, and EFT test modeling in
`tpt-elec-emc-immunity`.

## Shielding & grounding

Plane-wave shielding \(SE = R + A + B\) (`ShieldingCalculator`), shield materials
(copper → mu-metal, paint), plus ground-loop / return-path modeling in
`tpt-elec-emc-grounding`.

## Battery

- `tpt-elec-battery-core` — cell, chemistry, ECM, OCV, `terminal_voltage`.
- `tpt-elec-battery-thermal` — `ThermalRunawayModel`, `propagation_risk`,
  adjacency.
- `tpt-elec-battery-bms` — SOC estimators (Coulomb, KF/EKF/UKF, neural).
- `tpt-elec-battery-pack` — series/parallel composition, pack SOC/thermal.

## Try it

Battery thermal runaway propagation runs end-to-end in unit tests
(`tpt-elec-battery-thermal`). See [Validation](validation.md) for UN GTR 20 /
IEC 62660 notes.

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
