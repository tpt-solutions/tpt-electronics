# Which crate do I need?

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
