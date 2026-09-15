# Circuit simulation (SPICE)

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
