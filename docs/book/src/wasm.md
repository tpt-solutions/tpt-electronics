# WASM & integrations

Run thermal and impedance solvers in the browser (or any wasm host) with
`tpt-elec-wasm`.

## Build the demo

```console
$ cargo check -p tpt-elec-wasm --target wasm32-unknown-unknown
$ cd demo && wasm-pack build --target web
```

Open `demo/index.html` after `wasm-pack build` produces `demo/pkg/`.

## Bindings

- `WasmThermalSolver` — construct from Gerber data + stackup JSON, `solve()`,
  `get_temperature_map()`.
- `WasmImpedanceCalculator::microstrip` / `::differential` — Hammerstad-Jensen
  width/impedance.
- `WasmSpiceAnalyzer` — parse a SPICE netlist, then `dc_operating_point()`,
  `ac_analysis(f0, f1, ppd, node)` (interleaved f/mag), `transient(node,
  t_stop, t_step)` (interleaved t/v; uses `.tran` when times are ≤ 0).
- `WasmEyeDiagram` — fold a sampled waveform into an eye (`eye_height`,
  `eye_width`, jitter metrics) and check masks by name (`check_mask("pcie-gen3",
  amp)`) or custom rectangle (`check_mask_custom`).
- `WasmPdnAnalysis` — JSON-configured PDN; `impedance_profile()` returns
  interleaved f/|Z|, plus `peak_impedance`, `peak_frequency`, `target_met`,
  and `optimize_decoupling(available_json, budget)`.

## Repeated solves

State is rebuilt per `solve()` call (B5 regression covered by tests): calling
`solve()` twice does not stack boundary conditions or heat sources.

## CI

The `wasm32` job in `ci.yml` runs `cargo check --target wasm32-unknown-unknown`
so bindings stay green. GitHub Pages deploys the demo via `docs.yml` /
benchmark artifacts (see `demo/`).

## Extending

SPICE, eye-diagram, and PDN bindings ship with the crate so `demo/` can grow
into a full interactive playground (see `todo.md` post-v0.1).

## Browser build

```console
$ rustup target add wasm32-unknown-unknown
$ cargo check -p tpt-elec-wasm --target wasm32-unknown-unknown
$ wasm-pack build crates/core/tpt-elec-wasm --target web --out-dir demo/pkg
```

`demo/index.html` is the thermal viewer: drop a `.gtl` file, set a heat
source, and solve in-browser (no server).

## KiCad plugin

`kicad-plugin/tpt_elec_plugin.py` exports Gerber via `kicad-cli`, runs
`tpt-elec-cli thermal`, and drops a CSV next to the project.

## VS Code extension

`vscode-extension/` adds thermal sweeps for the active Gerber file and a
quick microstrip impedance command. Package with `vsce`.
