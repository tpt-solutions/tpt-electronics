# Signal & power integrity

Transmission lines, impedance, crosstalk, eye diagrams, and PDN with
`tpt-elec-si-*` / `tpt-elec-touchstone`.

## Impedance

`ImpedanceCalculator::microstrip` (Hammerstad–Jensen), `::stripline`, and
`::differential_pair` (odd/even mode). Inverse solve:

```rust
let w = imp.suggest(50.0, 4.5, 0.16); // Ω, εr, height → width [m]
```

CLI: `tpt-elec-cli impedance --suggest --target 50 …`.

## Crosstalk

Near-end (NEXT) and far-end (FEXT) coupled-line models in `tpt-elec-si-crosstalk`.

## Eye diagrams

`tpt-elec-si-eye` overlays unit intervals, measures eye height/width and jitter
separation, and checks `EyeMask` compliance (PCIe Gen1–6, DDR4/5, USB3/4,
10G Ethernet, custom).

```console
$ cargo run -p example-pcie-eye-diagram
```

## PDN

`tpt-elec-pi-pdn` builds impedance profiles (VRM + bulk + MLCC + plane
capacitance) and `optimize_decoupling`. Placement helpers live in
`tpt-elec-pi-decoupling`.

## Touchstone

Parse `.sNp` with `TouchstoneParser` (`# GHz S MA R 50` and variants).

## Standards

Masks and impedance targets are cross-checked against PCIe CEM, JEDEC, and
USB-IF (see [Validation](validation.md)).

## Impedance

`tpt-elec-si-impedance` implements Hammerstad-Jensen microstrip with a
Bahl-style thickness correction, symmetric/asymmetric stripline, and
edge-coupled differential pairs (`Z0e·Z0o = Z0²`).

Note: the spec's worked example (0.2 mm trace over 0.2 mm FR4) is
physically ≈ 65–71 Ω, not 50 Ω. The tests assert the correct physics.

## Crosstalk

Coupling factor from modes: `k = (Z0e − Z0o)/(Z0e + Z0o)`. NEXT saturates
at `(kL+kC)/4`; FEXT is `(kL−kC)·len/(2·v·tr)` — zero in homogeneous
(stripline) media.

## Eye diagrams

PRBS-7 waveforms fold into a unit-interval window; eye height/width and
jitter (deterministic + random) are extracted from crossing statistics.
Standard masks (PCIe Gen1-6, DDR4/5, USB3/4, 10GbE) are normalized to UI
and amplitude.

## PDN

Parallel branches — VRM (R + inductive beyond loop bandwidth), plane
capacitance, and decoupling capacitors with ESR/ESL/mount inductance —
swept over a log grid. The greedy optimizer damps the VRM-plane
anti-resonance first.
