# Tutorial: KiCad board → thermal report

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

Coordinates are relative to the board origin (same as KiCad).

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
