# VS Code extension — tpt-electronics

Scaffold that surfaces tpt-electronics results inside VS Code:

- **tpt-electronics: Run thermal sweep** — runs `tpt-elec-cli thermal` on the
  active Gerber file and opens the resulting temperature CSV.
- **tpt-electronics: Run DRC** — runs `tpt-elec-cli drc` on the active
  `.kicad_pcb` file and shows the pass/fail summary and finding count.
- **tpt-electronics: Quick microstrip impedance** — prompts for width,
  height, and er; returns Z₀ inline (offline approximation, no CLI needed).

## Install (development)

```console
$ npm install -g @vscode/vsce
$ cd vscode-extension
$ node --check extension.js     # syntax smoke test (CI runs this)
$ vsce package
$ code --install-extension tpt-electronics-0.0.1.vsix
```

## Manual integration test (requires VS Code GUI)

1. Open a folder containing `test-data/gerber/board.gtl` and
   `test-data/kicad/demo.kicad_pcb`.
2. Run the three commands from the command palette.
3. Expected: `board.thermal.csv` opens with a
   `x_mm,y_mm,z_mm,temperature_c` table, the DRC command reports a
   finding count and PASS/FAIL, and the impedance dialog reports ≈ 50 Ω
   for the defaults.
