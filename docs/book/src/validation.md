# Validation & standards

How results are checked against industry references (and what is deferred).

## Thermal

- **JEDEC JESD51** — two-resistor models on `ThermalComponentModel`
  (`tpt-elec-thermal`). JESD51-12 LED-specific test deferred.

## Interconnect

- **IPC-2152 / IPC-2221** — trace current capacity & temperature rise
  (`tpt-elec-mfg-dfm`); chart-point cross-checks; full chart-table interpolation
  deferred. Formula-vs-chart optimism is documented.
- **IPC-2141 / IPC-2251** — PCB impedance validation
  (`tpt-elec-si-impedance`).

## EMC

- **CISPR 32**, **FCC Part 15**, **IEC 61000** — emissions/immunity limits in
  `tpt-elec-emc-*` (`EmcStandard`, `EmcLimit`).

## Battery

- **UN GTR 20**, **IEC 62660** — model-level parameters in `tpt-elec-battery-*`
  (physical testing N/A for simulation).

## Signal integrity

- **PCIe CEM**, **DDR4/5 JEDEC**, **USB-IF** — normalized eye masks in
  `tpt-elec-si-eye` (`EyeMask::PcieGen…`, `Ddr4`, …).

## Golden fixtures

`test-data/golden/` holds frozen JSON for eye, impedance, filter, SPICE, and
network results. Refresh them deliberately with `cargo run -p xtask --
regen-goldens` (add `--write` to accept drift after reviewing it).

| Domain | Standard | Where |
|---|---|---|
| Thermal conduction/convection | analytic + JEDEC JESD51 models | `tpt-elec-thermal`, `-convection` |
| Trace current | IPC-2221 formula, IPC-2152 chart points | `tpt-elec-mfg-dfm` |
| Impedance | IPC-2141 / Hammerstad-Jensen | `tpt-elec-si-impedance` |
| Eye masks | PCIe/DDR/USB/Ethernet (normalized) | `tpt-elec-si-eye` |
| Emissions limits | CISPR 32 / FCC Part 15 tables | `tpt-elec-emc-core` |
| Immunity | IEC 61000-4-2/-3/-4/-5 | `tpt-elec-emc-immunity` |
| Shielding | plane-wave SE = R+A+B | `tpt-elec-emc-shielding` |
| Boundary scan | IEEE 1149.1 | `tpt-elec-mfg-test` |
| Battery SOC/runaway | UN GTR 20 / IEC 62660 (models) | `tpt-elec-battery-*` |

## Golden files

`test-data/golden/` contains reference JSON with expected values,
tolerances, and physical sanity checks. Tests fail on drift.

## Benchmarks

Criterion benches in `benches/`: thermal steady state, Joule coupling,
SPICE buck transient, eye analysis. CI records a baseline per push.
