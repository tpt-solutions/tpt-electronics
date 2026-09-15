# RF & microwave

- `tpt-elec-rf-core` — Smith chart maps and L/π network synthesis. Every
  synthesized network is verified by an ABCD-chain simulation, so |Γ| is
  guaranteed at the design frequency.
- `tpt-elec-rf-filters` — Butterworth (closed form), Chebyshev I
  (0.1/0.5/1/2/3 dB tables), Bessel ladders; LP/HP transformations; each
  filter ships with ABCD-simulated S-parameters (−3 dB at the Butterworth
  corner and equal-ripple at the Chebyshev corner are asserted in tests).
- `tpt-elec-rf-antenna` — dipole/patch/monopole/Yagi/helical reference
  patterns, effective aperture, Friis FSPL, link budgets.
- `tpt-elec-rf-mixer` — conversion gain, IM3 arithmetic, image rejection
  from I/Q imbalance, Friis NF/IIP3 cascades.
- `tpt-elec-rf-links` — stage-by-stage chain composition with SFDR.
