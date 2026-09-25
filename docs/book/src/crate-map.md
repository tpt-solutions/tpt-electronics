# Which crate do I need?

Pick by job — full workspace map lives in the repo `README.md` crate table.

| You want to… | Reach for |
|---|---|
| Mesh / stackup / IDs | `tpt-elec-core`, `tpt-elec-geometry` |
| Materials DB | `tpt-elec-materials` |
| Parse Gerber / drill | `tpt-elec-gerber` |
| Parse `.kicad_pcb` / netlist | `tpt-elec-kicad` |
| Steady/transient thermal | `tpt-elec-thermal`, `tpt-elec-transient` |
| Convection coefficients | `tpt-elec-convection` |
| Joule heating coupled solve | `tpt-elec-joule` |
| SPICE end-to-end | `tpt-elec-spice-*` (netlist → analysis) |
| Touchstone S-params | `tpt-elec-touchstone` |
| Trace / diff-pair impedance | `tpt-elec-si-impedance` |
| Crosstalk | `tpt-elec-si-crosstalk` |
| Eye diagram / masks | `tpt-elec-si-eye` |
| PDN / decoupling | `tpt-elec-pi-pdn`, `tpt-elec-pi-decoupling` |
| Matching networks / Smith | `tpt-elec-rf-core` |
| Filter synthesis | `tpt-elec-rf-filters` |
| Antenna / link budget | `tpt-elec-rf-antenna`, `tpt-elec-rf-links` |
| Converter design | `tpt-elec-power-*` |
| Semiconductor devices | `tpt-elec-semi-*` |
| EMC limits / emissions | `tpt-elec-emc-*` |
| Battery cell / pack / BMS | `tpt-elec-battery-*` |
| DRC / yield / JTAG | `tpt-elec-mfg-*` |
| ODB++ / IPC-2581 | `tpt-elec-odbpp`, `tpt-elec-ipc2581` |
| Browser / WASM | `tpt-elec-wasm` |
| Headless CLI | `tpt-elec-cli` |

**Start here if unsure:** Gerber thermal → `tpt-elec-gerber` +
`tpt-elec-thermal` + CLI; board-level SI → `tpt-elec-si-impedance` +
`tpt-elec-si-eye`; RF match → `tpt-elec-rf-core`.

| I want to… | Use |
|---|---|
| Simulate board temperature from a Gerber file | `tpt-elec-thermal` + `tpt-elec-gerber` |
| Model transient heating / power cycling | `tpt-elec-transient` |
| Model Joule heating (current + thermal coupled) | `tpt-elec-joule` |
| Check trace width for current (IPC-2152) | `tpt-elec-mfg-dfm` |
| Estimate electromigration lifetime | `tpt-elec-mfg-dfm` (Black's equation) |
| Simulate a circuit (DC/AC/transient) | `tpt-elec-spice-netlist` + `-spice-analysis` |
| Check 50 Ω microstrip impedance | `tpt-elec-si-impedance` |
| Solve inverse: find width for target impedance | `tpt-elec-si-impedance::suggest_microstrip` |
| Monte Carlo impedance tolerances | `tpt-elec-si-impedance::monte_carlo_microstrip` |
| Analyze an eye diagram against a standard mask | `tpt-elec-si-eye` |
| Model PDN impedance / decoupling caps | `tpt-elec-pi-pdn` |
| Design an L or π matching network | `tpt-elec-rf-core` |
| Synthesize a Butterworth/Chebyshev filter | `tpt-elec-rf-filters` |
| Predict radiated emissions | `tpt-elec-emc-emissions` |
| Check shielding effectiveness | `tpt-elec-emc-shielding` |
| Model battery SOC / thermal runaway | `tpt-elec-battery-bms` / `-thermal` |
| Design-rule-check a KiCad board | `tpt-elec-mfg-dfm` + `tpt-elec-kicad` |
| Run WASM in the browser | `tpt-elec-wasm` |
