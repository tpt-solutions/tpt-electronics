# tpt-elec-py

> Python bindings for tpt-electronics thermal, impedance and filter simulation

Part of [tpt-electronics](https://github.com/tpt-solutions/tpt-electronics) — a fully open-source, MIT-licensed multiphysics simulation engine.

`python` · `pyo3` · `notebook` · `parametric-sweep` · `optimisation`

## Status

🚧 Alpha · `pyo3` extension module, API may still change

## What is exposed

| Python | Backing crate |
|---|---|
| `tpt_elec_py.synthesize_filter(...)`, `tp.filters.*` | `tpt-elec-rf-filters` |
| `tpt_elec_py.filter_types()` | `tpt-elec-rf-filters` |
| `tpt_elec_py.impedance.*` | `tpt-elec-si-impedance`, `tpt-elec-rf-core` |
| `tpt_elec_py.thermal.*` | `tpt-elec-thermal` |
| `tpt_elec_py.spice.*` | `tpt-elec-spice-netlist`, `tpt-elec-spice-analysis` |

## Type stubs

`tpt_elec_py.pyi` types the whole API for editors and `mypy`. It is
hand-maintained, so `smoke_test.py` walks it and fails if it drifts from the
built module in either direction — a stub that names a function the module
does not have, or misses one it does.

## Building

```console
$ cargo build -p tpt-elec-py
```

On Windows CPython only auto-detects `.pyd`, so copy the artefact once:

```console
$ Copy-Item target\debug\tpt_elec_py.dll target\debug\tpt_elec_py.pyd
```

## Testing

`smoke_test.py` exercises every entry point against the built module and
locates it automatically:

```console
$ cargo build -p tpt-elec-py
$ python crates/python/tpt-elec-py/smoke_test.py
```

## Example

```python
import tpt_elec_py as tp

# Filter synthesis: g-values, ladder elements and a swept response.
lp = tp.synthesize_filter("butterworth", order=5, response="lowpass",
                          cutoff_hz=100e6)
print(lp["g_values"])
print(lp["insertion_loss_db"][200])

# Controlled impedance.
trace = tp.impedance.microstrip(width_mm=0.35, thickness_mm=0.035,
                                height_mm=0.2, er=4.4)
print(trace["z0"])

# Inverse design: width for a target impedance.
print(tp.impedance.suggest_microstrip_width(50.0, 0.035, 0.2, 4.4))

# Steady-state thermal.
r = tp.thermal.solve_steady_state(
    size_mm=(50.0, 50.0, 1.6), cell_mm=2.5,
    heat_sources=[(25.0, 25.0, 0.0, 3.0)],
    ambient_c=25.0, convection_h=10.0)
print(r["max_temp_c"])

# SPICE: the netlist text is the input, so each call is self-contained.
net = open("test-data/spice/rc_lowpass.net").read()
ac = tp.spice.ac_analysis(net, 1e3, 1e7, 10, node="out")
print(ac["magnitude_db"][0])
```

## Transient step size

`spice.transient` takes an initial `t_step_s` and a `max_step_factor` cap on
adaptive step growth. **A coarse step is the usual cause of a transient that
disagrees with the Rust golden.** The buck-converter fixture is reproduced
with `t_step_s=20e-9, max_step_factor=5.0`; `smoke_test.py` asserts the
Python binding matches that golden to within its 0.05 V tolerance.

## Notes

- `pyo3` needs `unsafe` for its generated FFI glue, so this crate overrides
  the workspace `forbid(unsafe_code)` lint locally. There is no hand-written
  `unsafe` in this crate.
- `elliptic` / `cauer` is low-pass only, orders 1-10; other shapes raise
  `ValueError`. The others cover high-pass/band-pass/band-stop.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
