# Semiconductor & manufacturing

- `tpt-elec-semi-core` — bandgap/density-of-states tables, intrinsic
  carrier density, doping profiles, built-in potential.
- `tpt-elec-semi-mosfet` / `-diode` / `-bjt` — standalone device analysis
  wrapping the SPICE compact models.
- `tpt-elec-semi-process` — PDK abstractions: wire R/C, vias,
  electromigration current limits.
- `tpt-elec-mfg-dfm` — DRC engine (width/clearance/drill/annular ring)
  over KiCad boards plus the IPC-2221 trace-current formula with IPC-2152
  chart-point validation.
- `tpt-elec-mfg-test` — IEEE 1149.1 chain modeling and pattern generation
  (EXTEST/SAMPLE, walking patterns, stuck-at frames).
- `tpt-elec-mfg-yield` — Poisson/Murphy/Seeds yield models and composite
  board yield.
