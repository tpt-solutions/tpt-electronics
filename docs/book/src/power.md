# Power electronics

- `tpt-elec-power-core` — topology enum and a buck first-pass design
  (duty, ripple, L/C sizing, loss/efficiency estimates).
- `tpt-elec-power-magnetics` — Steinmetz core loss `Pv = k·f^α·B^β` with
  per-material constants, `L = AL·N²` turns selection, saturation checks.
- `tpt-elec-power-switches` — conduction/switching/gate/output-charge
  losses, junction temperature, stress limits.
- `tpt-elec-power-control` — transfer functions, Bode sweeps, gain/phase
  margins, k-factor Type II/III compensator synthesis.
- `tpt-elec-power-battery` — pack-to-converter bridge: input current,
  runtime, buck dropout SOC, topology recommendation.

Run `cargo run -p example-buck-converter` for the end-to-end milestone.
