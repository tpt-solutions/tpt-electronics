# KiCad Action Plugin — tpt-electronics

`tpt_elec_plugin.py` registers a PCB-editor action that:

1. Exports the current board to Gerber via `kicad-cli`.
2. Builds a per-component power map from footprint properties
   (`Power` / `power_w` / `PowerDissipation` / `Pdiss`), or uses an
   existing `<board>.power.csv` if present.
3. Runs the `tpt-elec-cli thermal` sweep (Gerber + heat sources → CSV).
4. Writes `<board>.thermal.csv` next to the project for review.
5. Runs `tpt-elec-cli drc` directly against the saved `.kicad_pcb` and
   writes `<board>.drc.txt` next to the project.

## Install

```console
# Build and install the CLI once
$ cargo install --path crates/cli/tpt-elec-cli

# Copy the plugin into KiCad's plugin search path
$ cp kicad-plugin/tpt_elec_plugin.py ~/.kicad_plugins/
# (Windows: %APPDATA%\kicad\6.0\scripting\plugins or 7.0 equivalent)
```

## Manual integration test (requires KiCad GUI)

1. Open `test-data/kicad/demo.kicad_pcb` in the PCB editor.
2. Run **Tools → External Plugins → TPT Electronics: thermal + DRC**.
3. Expected: `[tpt-electronics]` log lines with the max temperature and DRC
   finding count, plus `demo.thermal.csv` and `demo.drc.txt` files beside
   the fixture.

Heat sources come from footprint properties. Add a `Power` field (in watts)
to any footprint that dissipates power — the plugin collects all of them
into an `x_mm,y_mm,watts` map automatically. Alternatively, drop a
`<board>.power.csv` beside the `.kicad_pcb` for hand-authored sources.
