# KiCad Action Plugin — tpt-electronics

`tpt_elec_plugin.py` registers a PCB-editor action that:

1. Exports the current board to Gerber via `kicad-cli`.
2. Runs the `tpt-elec-cli thermal` sweep (Gerber + heat sources → CSV).
3. Writes `<board>.thermal.csv` next to the project for review.

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
3. Expected: a `[tpt-electronics]` log line with the max temperature and a
   `demo.thermal.csv` file beside the fixture.

The plugin is a scaffold: heat sources are currently hardcoded to a 0.5 W
point at the board center — extend `Run()` to read a component power map
from schematic properties for real use.
