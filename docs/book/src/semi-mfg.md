# Semiconductor & manufacturing

Compact device models, process PDKs, DRC/DFM, boundary scan, and yield.

## Semiconductor devices

- `tpt-elec-semi-core` — `Semiconductor` (Si/SiC/GaN/GaAs/Ge), doping profiles.
- `tpt-elec-semi-mosfet` — Level 1 square-law + Level 3 semi-empirical
  `drain_current`, `gm`, `gds`.
- `tpt-elec-semi-diode` — Shockley compact model (shared lineage with
  spice-models).
- `tpt-elec-semi-bjt` — Ebers–Moll / Gummel–Poon style model.
- `tpt-elec-semi-process` — `ProcessDesignKit`, technology nodes, design rules.

## Manufacturing

- **DRC/DFM** (`tpt-elec-mfg-dfm`): trace width/spacing, via drill, annular
  ring; plus Black's-equation electromigration `MTTF`.
- **Boundary scan** (`tpt-elec-mfg-test`): IEEE 1149.1 chains, test patterns.
- **Yield** (`tpt-elec-mfg-yield`): Poisson / Murphy / Seeds yield models.

## Formats

`OdbppParser` (ZIP, stored + in-tree inflate) and `Ipc2581Parser` (IPC-2581-C
XML) for exchange with fab/assembly tools.

## Try DRC

```console
$ tpt-elec-cli drc --kicad board.kicad_pcb
```

## Standards

IPC-2152 current capacity validation lives in the manufacturing crates (see
[Validation](validation.md)).

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
