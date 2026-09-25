# Changelog — tpt-elec-py

All notable changes to this crate are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0]

### Added

- `pyo3` extension module `tpt_elec_py`.
- `synthesize_filter()` and `filter_types()` over `tpt-elec-rf-filters`,
  returning prototype g-values, ladder elements and a swept S-parameter
  response for scripting parametric studies.
- `tpt_elec_py.impedance` submodule: `microstrip`, `stripline`,
  `differential_pair`, `suggest_microstrip_width`,
  `monte_carlo_microstrip`, `reflection_to_impedance` and
  `impedance_to_reflection`.
- `tpt_elec_py.thermal` submodule: `default_materials()` and
  `solve_steady_state()` for voxel-grid steady-state conduction with
  convection and point heat sources.
- `tpt_elec_py.spice` submodule: `parse_netlist`, `dc_operating_point`,
  `ac_analysis` and `transient`. The netlist text is the unit of input, so
  each call is self-contained and no circuit handle has to be kept alive.
- `tpt_elec_py.pyi` type stubs covering the whole API, with a drift guard in
  `smoke_test.py` that fails if the stub and the built module disagree.
- `smoke_test.py` covering every exposed entry point, including the buck
  converter transient against the crate's own golden fixture.

### Known limitations

- `elliptic`/`cauer` is accepted by `synthesize_filter` but raises
  `ValueError`; elliptic synthesis is still deferred in
  `tpt-elec-rf-filters` (see `rfcs/0004`).
- Only low-pass elliptic synthesis would be supported even once implemented;
  the other approximations already cover low/high/band-pass/band-stop.
- `spice.transient` accuracy depends on `t_step_s`. The buck-converter golden
  is reproduced at `t_step_s = 20e-9, max_step_factor = 5.0`; much coarser
  steps drift well outside the 0.05 V tolerance.
- `spice.ac_analysis` exposes `magnitude` (linear |V|) and `magnitude_db`;
  `magnitude_at` in the Rust crate is linear, so the binding derives the dB
  form itself rather than mislabelling it.
- `spice` bindings are not a class hierarchy: passing node names in upper or
  lower case both work because the parser normalises them.
