# xtask

> Workspace automation for tpt-electronics (golden-file regeneration)

Part of [tpt-electronics](https://github.com/tpt-solutions/tpt-electronics) — a fully open-source, MIT-licensed multiphysics simulation engine.

## `regen-goldens`

```console
$ cargo run -p xtask -- regen-goldens                  # verify (default, CI-safe)
$ cargo run -p xtask -- regen-goldens --list           # what is covered, and why
$ cargo run -p xtask -- regen-goldens --case <name>    # a single fixture
$ cargo run -p xtask -- regen-goldens --write          # accept the drift
```

### The important property

Golden files only detect regressions if their expected numbers come from an
authority **independent of the code under test**. `regen-goldens` therefore
recomputes every value from a closed-form identity implemented in
[`reference`](src/reference.rs) — the linear 1-D conduction profile, the lumped
convection law `T = T∞ + P/(h·A)`, the Butterworth prototype `g_k = 2·sin((2k−1)π/2n)`
and `|H|² = 1/(1 + (f/fc)^(2n))` — and never calls the `tpt-elec-*` crates.

Regenerating from the implementation would make the check self-confirming: it
would record whatever the code does today and never fail again. For that
reason:

- **check is the default**, and CI runs only that;
- `--write` is required to change anything;
- each case's `basis` string is printed by `--list` so a reviewer can see
  exactly which identity a value came from;
- the test `every_golden_is_classified` fails if a new golden appears without
  being classified as regenerable or simulation-derived.

### Coverage

`--list` reports all seven fixtures. Three are regenerated from closed forms;
the other four hold simulation output (eye diagram, SPICE startup) or a
rounded design target, so they are **never** rewritten:

| Fixture | Treatment |
|---|---|
| `simple_resistor_board` | analytic — regenerated |
| `butterworth_filter` | analytic — regenerated |
| `rc_lowpass_ac` | parameters only; the test derives the reference itself |
| `ddr4_impedance` | rounded 55 Ω design target — never rewritten |
| `l_network_match` | `\|Γ\|` bound + prose — never rewritten |
| `pcie_gen3_eye` | simulation output — never rewritten |
| `buck_converter_transient` | simulation output — never rewritten |

### Tolerances

Each file declares its own tolerance (`tolerance_c`, `tolerance_db`,
`magnitude_tolerance_rel`, …) and the tool reads it, matching how the in-crate
tests apply it: absolute for °C/dB/V/Ω, relative for component values in
farads. A value only drifts when it exceeds that bound.

### Known cosmetic behaviour

`--write` re-serialises the file it touches through `serde_json`, so JSON
number formatting and array layout are canonicalised (`1.0e8` becomes
`100000000.0`, short arrays are expanded). Key order, prose fields, and all
untouched values are preserved exactly, and only drifted values are rewritten —
but a real `--write` does produce some formatting churn. Check mode, which is
what CI runs, never writes.

## License

Licensed under either of [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE) at your option.
