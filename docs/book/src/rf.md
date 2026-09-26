# RF & microwave

Match networks, synthesize filters, and budget links with `tpt-elec-rf-*`.

## Matching

```rust
use tpt_elec_rf_core::{MatchingDesigner, MatchingTopology};

let net = MatchingDesigner::l_network(
    complex!(50.0, 0.0),
    complex!(25.0, 15.0),
    MatchingTopology::LowPass,
    2.4e9,
).unwrap();
```

Pi networks are ABCD-validated: `MatchingDesigner::pi_network`.

## Smith chart

`SmithChart::impedance_to_reflection` / `reflection_to_impedance` convert
impedance ↔ reflection coefficient for plotting or network analysis.

## Filters

`tpt-elec-rf-filters` synthesizes Butterworth, Chebyshev-I, Bessel
low/high/band-pass responses, and Chebyshev-II (pole/zero, even orders) and
Elliptic/Cauer (pole/zero, orders 1–10, low-pass only). The elliptic design is
the Zolotarev construction and is checked against `scipy.signal.ellipap` and
against the equiripple definition (see `rfcs/0004`).

## Antennas & links

`AntennaType` (dipole, monopole, patch, yagi, helical), `LinkBudget`, and
`free_space_path_loss` for range/coverage estimates. Compose the full chain
(antenna → filter → mixer → budget) with `tpt-elec-rf-links`.

## Try it

```console
$ cargo run -p example-wifi-antenna
```

See also [Signal & power integrity](si.md) for board-level transmission lines.

- `tpt-elec-rf-core` — Smith chart maps and L/π network synthesis. Every
  synthesized network is verified by an ABCD-chain simulation, so |Γ| is
  guaranteed at the design frequency.
- `tpt-elec-rf-filters` — Butterworth (closed form), Chebyshev I
  (0.1/0.5/1/2/3 dB tables), Chebyshev II (pole/zero, even orders),
  Bessel ladders; LP/HP transformations; each filter ships with
  ABCD-simulated S-parameters (−3 dB at the Butterworth corner and
  equal-ripple at the Chebyshev corner are asserted in tests).
- `tpt-elec-rf-antenna` — dipole/patch/monopole/Yagi/helical reference
  patterns, effective aperture, Friis FSPL, link budgets.
- `tpt-elec-rf-mixer` — conversion gain, IM3 arithmetic, image rejection
  from I/Q imbalance, Friis NF/IIP3 cascades.
- `tpt-elec-rf-links` — stage-by-stage chain composition with SFDR.
