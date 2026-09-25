# SPDX-License-Identifier: MIT OR Apache-2.0
#
# KiCad Action Plugin: run tpt-electronics thermal + DRC on the current board.
#
# Install: copy this file into ~/.kicad_plugins/ (KiCad 7: the PCB editor's
# "Plugins" search path) or register via the Plugin and Content Manager.
# The plugin shells out to the `tpt-elec-cli` binary — install it with
# `cargo install --path crates/cli/tpt-elec-cli` and make sure it is on PATH.
#
# Power sources are collected from footprint properties (Power / power_w /
# PowerDissipation / Pdiss) or from an existing <board>.power.csv; if
# neither is present the CLI uses its default (no --power-map flag).

import os
import subprocess
import tempfile

import pcbnew  # type: ignore (provided by KiCad's Python)


class TptElecPlugin(pcbnew.ActionPlugin):
    def defaults(self):
        self.name = "TPT Electronics: thermal + DRC"
        self.category = "Simulation"
        self.description = (
            "Export the board to Gerber, run the tpt-electronics thermal "
            "solver, and run the DRC engine. Results open in your editor."
        )
        self.show_toolbar_button = True

    def Run(self):
        cli = _find_cli()
        board_path = pcbnew.GetBoard().GetFileName()
        if not board_path:
            _log("Save the board first (needs a .kicad_pcb path).")
            return

        with tempfile.TemporaryDirectory(prefix="tpt-elec-") as tmp:
            # 1. Export Gerber + drill files with KiCad's CLI.
            subprocess.run(
                [
                    "kicad-cli", "pcb", "export", "gerbers",
                    "-o", tmp,
                    board_path,
                ],
                check=False,
            )
            gerbers = sorted(
                os.path.join(tmp, f)
                for f in os.listdir(tmp)
                if f.endswith((".gtl", ".gbl", ".gko"))
            )
            if not gerbers:
                _log("Gerber export produced no files — is kicad-cli on PATH?")
                return

            # 2. Build a power map from footprint properties, then thermal sweep.
            out_csv = os.path.join(tmp, "thermal.csv")
            power_csv = _build_power_csv(board_path, tmp)
            cmd = [
                cli, "thermal",
                "--gerber", gerbers[0],
                "--out", out_csv,
                "--resolution-mm", "1.0",
                "--h-conv", "10",
            ]
            if power_csv:
                cmd.extend(["--power-map", power_csv])
            result = subprocess.run(cmd, capture_output=True, text=True)
            _log(result.stdout.strip() or result.stderr.strip())

            # 3. Copy results next to the board.
            dest = os.path.splitext(str(board_path))[0] + ".thermal.csv"
            if os.path.exists(out_csv):
                os.replace(out_csv, dest)
                _log(f"Temperature field written to {dest}")

            # 4. Run the DRC engine directly against the saved .kicad_pcb.
            drc_result = subprocess.run(
                [cli, "drc", "--kicad", str(board_path)],
                capture_output=True, text=True,
            )
            drc_out = (drc_result.stdout or drc_result.stderr).strip()
            _log(drc_out)
            drc_dest = os.path.splitext(str(board_path))[0] + ".drc.txt"
            with open(drc_dest, "w", encoding="utf-8") as f:
                f.write(drc_out + "\n")
            _log(f"DRC report written to {drc_dest}")


_POWER_KEYS = ("Power", "power", "power_w", "PowerDissipation", "Pdiss")


def _build_power_csv(board_path, tmp):
    """Return a path to an x_mm,y_mm,watts CSV, or None if no sources found.

    Priority:
      1. Existing `<board>.power.csv` next to the .kicad_pcb (hand-authored).
      2. Per-footprint properties scanned via the pcbnew API.
      3. None → CLI falls back to its default (no power-map flag).
    """
    base = os.path.splitext(str(board_path))[0]
    hand = base + ".power.csv"
    if os.path.exists(hand):
        return hand

    board = pcbnew.GetBoard()
    if board is None:
        return None

    rows = []
    for fp in board.GetFootprints():
        ref = fp.GetReference()
        pos = fp.GetPosition()
        # pcbnew VECTOR2I: coordinates in internal units (nm on modern KiCad).
        x_mm = pos.x / 1e6
        y_mm = pos.y / 1e6

        # Try known property names first, then fall back to a "Power" field.
        watts = None
        for key in _POWER_KEYS:
            try:
                val = fp.GetFieldByName(key)
                if val is not None and str(val.GetText()).strip():
                    watts = float(str(val.GetText()).replace("W", "").strip())
                    break
            except (AttributeError, ValueError, TypeError):
                continue

        if watts is None:
            # Older KiCad: Properties() returns a dict-like object.
            try:
                props = fp.Properties()
                for key in _POWER_KEYS:
                    if key in props:
                        raw = str(props[key]).replace("W", "").strip()
                        if raw:
                            watts = float(raw)
                            break
            except (AttributeError, ValueError, TypeError, KeyError):
                pass

        if watts is not None and watts > 0:
            rows.append(f"{x_mm:.4f},{y_mm:.4f},{watts:.6f}")

    if not rows:
        return None

    out = os.path.join(tmp, "component_power.csv")
    with open(out, "w", encoding="utf-8") as f:
        f.write("x_mm,y_mm,watts\n")
        for r in rows:
            f.write(r + "\n")
    _log(f"power map: {len(rows)} component(s) from footprint properties")
    return out


def _find_cli():
    for candidate in ("tpt-elec-cli", os.path.expanduser("~/.cargo/bin/tpt-elec-cli")):
        if _which(candidate):
            return candidate
    raise RuntimeError("tpt-elec-cli not found — cargo install --path crates/cli/tpt-elec-cli")


def _which(name):
    for path_dir in os.environ.get("PATH", "").split(os.pathsep):
        full = os.path.join(path_dir, name)
        if os.path.isfile(full):
            return full
    return None


def _log(message):
    print(f"[tpt-electronics] {message}")


TptElecPlugin().register()
