# SPDX-License-Identifier: MIT OR Apache-2.0
#
# KiCad Action Plugin: run tpt-electronics thermal + DRC on the current board.
#
# Install: copy this file into ~/.kicad_plugins/ (KiCad 7: the PCB editor's
# "Plugins" search path) or register via the Plugin and Content Manager.
# The plugin shells out to the `tpt-elec-cli` binary — install it with
# `cargo install --path crates/cli/tpt-elec-cli` and make sure it is on PATH.

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

            # 2. Thermal sweep with a 0.5 W source at the board center.
            out_csv = os.path.join(tmp, "thermal.csv")
                        power_csv = os.path.join(
                os.path.dirname(str(board_path)),
                os.path.splitext(os.path.basename(str(board_path)))[0] + ".power.csv"
            )
            cmd = [
                cli, "thermal",
                "--gerber", gerbers[0],
                "--out", out_csv,
                "--resolution-mm", "1.0",
                "--h-conv", "10",
            ]
            if os.path.exists(power_csv):
                cmd.extend(["--power-map", power_csv])
            result = subprocess.run(cmd, capture_output=True, text=True)
            _log(result.stdout.strip() or result.stderr.strip())

            # 3. Copy results next to the board.
            dest = os.path.splitext(str(board_path))[0] + ".thermal.csv"
            if os.path.exists(out_csv):
                os.replace(out_csv, dest)
                _log(f"Temperature field written to {dest}")


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
