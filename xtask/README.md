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

`--list` reports all eight fixtures. Five are checked against an independent
reference, one holds no derived value, one is frozen from an external
implementation, and one is genuinely blocked.

| Fixture | Treatment |
|---|---|
| `simple_resistor_board` | closed form (conduction, `T∞ + P/hA`) — regenerated |
| `butterworth_filter` | closed form (prototype `g_k`, `\|H\|² = 1/(1+(f/fc)²ⁿ)`) — regenerated |
| `ddr4_impedance` | independent Hammerstad-Jensen implementation — regenerated |
| `pcie_gen3_eye` | independent PRBS-7 + channel + eye metrics — regenerated |
| `rc_lowpass_ac` | parameters only; the test derives the reference itself |
| `l_network_match` | specification only — nothing to regenerate |
| `elliptic_pole_zero` | frozen from `scipy.signal.ellipap`; checked in-crate |
| `buck_converter_transient` | **blocked** — needs an external solver |

### Three grades of independence

Worth being precise about, because the guarantees differ:

- **Closed forms** (`simple_resistor_board`, `butterworth_filter`): the
  reference is a short identity stated in the module docs. Anyone can check it
  by hand, so it catches both implementation drift *and* a wrong model.
- **Independent reimplementations** (`ddr4_impedance`, `pcie_gen3_eye`, in
  [`refmodel`](src/refmodel.rs)): a second implementation of the same published
  model. This catches implementation drift and transcription errors — the
  common bug class — but **not** a shared misunderstanding of the model, since
  both sides implement the same equations.
- **External implementations** (`elliptic_pole_zero`): the numbers were produced
  by `scipy.signal.ellipap`, which shares no code and no author with this
  workspace, and are frozen into the golden so the check needs no SciPy at test
  time. Because elliptic synthesis has no closed form to hand-check against,
  this is the only kind of reference available for it.

The buck converter is the one fixture where a second implementation of our own
model proves nothing: reproducing our own solver tells you only that it is
deterministic. It needs a genuinely different engine (ngspice/LTspice) run on
the same netlist, which is why it stays blocked.

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
