# Tutorial: KiCad board → thermal report

A narrated end-to-end path. Assumptions: KiCad 7+ with `kicad-cli` on PATH, and
`tpt-elec-cli` installed (`cargo install --path crates/cli/tpt-elec-cli`).

## 0. Prerequisites

```console
$ cargo install --path crates/cli/tpt-elec-cli
$ tpt-elec-cli --help
```

## 1. Export Gerbers from KiCad

In PCB editor: *File → Fabrication Outputs → Gerbers* …, or:

```console
$ kicad-cli pcb export gerbers -o out/ board.kicad_pcb
$ kicad-cli pcb export drill -o out/ board.kicad_pcb
```

Keep at least the top/bottom copper (`.gtl`/`.gbl`) and outline (`.gko`).

## 2. Optional: power map

Place `board.power.csv` next to the `.kicad_pcb` with rows
`x_mm,y_mm,watts`. The KiCad plugin looks for this filename automatically;
if it is absent, the plugin builds the map from footprint properties
(`Power` / `power_w` / `PowerDissipation` / `Pdiss` fields). The CLI also
accepts `--power-map` directly. Without either, use a uniform source
(e.g. `--power-w 0.5`).

## 3. Solve thermal

```console
$ tpt-elec-cli thermal \
    --gerber out/board.gtl \
    --power-map board.power.csv \
    --resolution-mm 1.0 \
    --h-conv 10 \
    --out temps.csv
```

Flags: `--resolution-mm`, `--h-conv`, `--watch` (re-solve on file change),
`--format json`.

## 4. Report / visualize

```console
$ tpt-elec-cli report temps.csv --out report.html
```

Produces a self-contained HTML artifact (SVG heat strip) you can share.

## 5. Optional: impedance + DRC

```console
$ tpt-elec-cli impedance --width-mm 0.3 --height-mm 0.16 --er 4.5
$ tpt-elec-cli drc --kicad board.kicad_pcb
```

## From the KiCad plugin

Copy `kicad-plugin/tpt_elec_plugin.py` into your KiCad plugins path and run
*TPT Electronics: thermal + DRC* from the action plugin menu — it shells out to
the same CLI.

## Try a canned example first

```console
$ cargo run -p example-simple-led-board
$ cargo run -p example-pcie-eye-diagram
```

Full example index: [`examples/README.md`](https://github.com/tpt-solutions/tpt-electronics/blob/main/examples/README.md).

This walkthrough takes a KiCad project from layout to thermal report using
the tpt-electronics CLI.

## Prerequisites

- A saved `.kicad_pcb` file
- `kicad-cli` on PATH (comes with KiCad 7+)
- `tpt-elec-cli` installed (`cargo install --path crates/cli/tpt-elec-cli`)

## 1. Export Gerber files

```console
$ kicad-cli pcb export gerbers -o gerbers/ board.kicad_pcb
```

## 2. Define a power map

Create `power.csv` with one row per heat source:

```csv
x_mm,y_mm,watts
25,20,0.75
40,35,0.3
```

Coordinates are relative to the board origin (same as KiCad). The KiCad
plugin can also generate this automatically from footprint `Power`
properties if no `board.power.csv` exists.

## 3. Run the thermal solver

```console
$ tpt-elec-cli thermal     --gerber gerbers/board-F_Cu.gtl     --power-map power.csv     --out thermal.csv     --resolution-mm 0.5     --h-conv 15 --ambient-c 25
```

## 4. Check trace current capacity

```console
$ tpt-elec-cli drc --kicad board.kicad_pcb --min-width-um 200
```

## 5. Impedance check

```console
$ tpt-elec-cli impedance --suggest --height-mm 0.2 --er 4.4 --target 50
suggested width: 355.0 µm (Z0 = 50.00 Ω, target 50 Ω)
```

## 6. Interpret results

Open `thermal.csv` in your spreadsheet or plotting tool. The `max_temp_c`
value in the CLI output is the hottest cell — for most FR4 designs, anything
under 105 °C at the component body is comfortable with standard lead-free
solder (JEDEC JESD52 gives the moisture-sensitivity levels).
