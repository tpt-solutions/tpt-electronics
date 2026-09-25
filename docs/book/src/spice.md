# Circuit simulation (SPICE)

Netlist → MNA → DC/AC/transient/noise, all in pure Rust (`tpt-elec-spice-*`).

## Pipeline

1. **Parse** — `SpiceNetlistParser` (`tpt-elec-spice-netlist`): R/C/L/M/D,
   `.model`, `.tran`, `.ac`, `.subckt`/`.ends`.
2. **Build** — `Circuit` / `MnaMatrix` (`tpt-elec-spice-core`).
3. **Analyze** — `SpiceAnalyzer` (`tpt-elec-spice-analysis`):
   - `dc_operating_point` (Newton–Raphson)
   - `ac_analysis` (complex MNA per frequency)
   - `transient` (trapezoidal/Gear, adaptive step)
   - `noise_analysis` (thermal/shot/flicker via `tpt-elec-spice-noise`)

Models: MOSFET Level 1–3 (BSIM3/4 subsets + EKV charge-sheet documented),
Shockley diode, BJT.

## CLI

```console
$ tpt-elec-cli spice --netlist buck.net --analysis tran --out result.json
```

JSON output: `--format json` for `jq`/Python.

## Golden test

`test-data/golden/spice/buck_converter_transient.json` — buck end-to-end
cross-checked against the power-converter example.

## Fixtures

`test-data/spice/rc_lowpass.net`, `buck.net` (note: workspace path contains
spaces — quote paths).

The SPICE engine is a clean-room MNA implementation:

- `tpt-elec-spice-core` — circuit graph, node table, waveforms (DC,
  PULSE, SIN, PWL), MNA matrices, analysis descriptors.
- `tpt-elec-spice-models` — Level 1–3 MOSFET, Shockley diode with series
  resistance and junction capacitance, Ebers-Moll BJT, single-pole op-amp.
- `tpt-elec-spice-analysis` — Newton-Raphson DC with junction limiting
  (pnjlim) and adaptive damping, complex LU AC sweep, trapezoidal
  transient with step-halving on Newton failure, resistor-noise transfer.
- `tpt-elec-spice-netlist` — classic netlist syntax with SPICE unit
  suffixes (`10k`, `100n`, `1.5meg`), `.model` cards, `.subckt` flattening.
- `tpt-elec-spice-noise` — thermal, shot, and flicker densities.

## Convergence

Hard-switching circuits (the buck example) exercise the pathological
paths: companion current return legs, junction limiting, damped Newton
with full-step escapes, and step halving. The buck golden test
(`test-data/golden/spice/buck_converter_transient.json`) locks this in.
