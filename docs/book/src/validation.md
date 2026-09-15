# Validation & standards

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
