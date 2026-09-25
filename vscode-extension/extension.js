// SPDX-License-Identifier: MIT OR Apache-2.0
// VS Code extension scaffold: surface tpt-electronics thermal/impedance
// results in the editor.
//
// Build & install:
//   npm install -g @vscode/vsce
//   cd vscode-extension && vsce package
//   code --install-extension tpt-electronics-0.0.1.vsix
//
// The extension shells out to `tpt-elec-cli` (same contract as the KiCad
// plugin) and shows a heat-map preview for the active Gerber file.

const vscode = require("vscode");
const { execFile } = require("child_process");

function activate(context) {
  const runThermal = vscode.commands.registerCommand(
    "tpt-electronics.thermal",
    async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !editor.document.fileName.match(/\.(gtl|gbl|gko)$/i)) {
        vscode.window.showErrorMessage("Open a Gerber file (.gtl/.gbl/.gko) first.");
        return;
      }
      const gerber = editor.document.fileName;
      const out = gerber.replace(/\.(gtl|gbl|gko)$/i, ".thermal.csv");
      const cfg = vscode.workspace.getConfiguration("tpt-electronics");
      const cli = cfg.get("cliPath", "tpt-elec-cli");

      const channel = vscode.window.createOutputChannel("tpt-electronics");
      channel.show(true);
      channel.appendLine(`Running ${cli} thermal on ${gerber}…`);

      execFile(
        cli,
        ["thermal", "--gerber", gerber, "--out", out, "--resolution-mm", "1.0"],
        (error, stdout, stderr) => {
          if (error) {
            channel.appendLine(`Failed: ${error.message}\n${stderr}`);
            vscode.window.showErrorMessage("tpt-elec-cli failed (see output).");
            return;
          }
          channel.appendLine(stdout);
          vscode.window.showInformationMessage(
            `Max temperature written to ${out}`
          );
          vscode.commands.executeCommand("vscode.open", vscode.Uri.file(out));
        }
      );
    }
  );

  const runDrc = vscode.commands.registerCommand(
    "tpt-electronics.drc",
    async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !editor.document.fileName.match(/\.kicad_pcb$/i)) {
        vscode.window.showErrorMessage("Open a .kicad_pcb file first.");
        return;
      }
      const board = editor.document.fileName;
      const cfg = vscode.workspace.getConfiguration("tpt-electronics");
      const cli = cfg.get("cliPath", "tpt-elec-cli");

      const channel = vscode.window.createOutputChannel("tpt-electronics");
      channel.show(true);
      channel.appendLine(`Running ${cli} drc on ${board}…`);

      execFile(cli, ["drc", "--kicad", board], (error, stdout, stderr) => {
        const output = stdout || stderr;
        channel.appendLine(output);
        if (error && !stdout) {
          vscode.window.showErrorMessage("tpt-elec-cli drc failed (see output).");
          return;
        }
        const lastLine = output.trim().split("\n").pop() || "";
        vscode.window.showInformationMessage(`DRC: ${lastLine}`);
      });
    }
  );

  const runImpedance = vscode.commands.registerCommand(
    "tpt-electronics.impedance",
    async () => {
      const width = await vscode.window.showInputBox({
        prompt: "Trace width [mm]",
        value: "0.35",
      });
      const height = await vscode.window.showInputBox({
        prompt: "Dielectric height [mm]",
        value: "0.2",
      });
      const er = await vscode.window.showInputBox({
        prompt: "Substrate er",
        value: "4.4",
      });
      if (!width || !height || !er) {
        return;
      }
      const z = microstripQuick(parseFloat(width), parseFloat(height), parseFloat(er));
      vscode.window.showInformationMessage(`Z0 ≈ ${z.toFixed(1)} Ω`);
    }
  );

  context.subscriptions.push(runThermal, runDrc, runImpedance);
}

// Quick Hammerstad-Jensen approximation so the command works without the
// CLI installed (education mode); the CLI remains authoritative.
function microstripQuick(wMm, hMm, er) {
  const u = wMm / hMm;
  const erEff = (er + 1) / 2 + ((er - 1) / 2) / Math.sqrt(1 + 12 / u);
  return u >= 1
    ? (377 / (Math.sqrt(erEff) * (u + 1.393 + 0.667 * Math.log(u + 1.444))))
    : (60 * Math.log(6 / u + Math.sqrt(1 + 4 / (u * u))) / Math.sqrt(erEff));
}

function deactivate() {}

module.exports = { activate, deactivate };
