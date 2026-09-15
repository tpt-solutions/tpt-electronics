# WASM & integrations

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
