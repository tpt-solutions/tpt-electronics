# RFC 0003: SI eye diagram

- **Status:** Implemented
- **Crates:** `tpt-elec-si-eye`, `tpt-elec-si-impedance`, `tpt-elec-touchstone`

## Summary

UI-folding eye analysis with jitter decomposition and normalized standard
masks, backed by Hammerstad-Jensen impedance calculators and a Touchstone
reader.

## Motivation

Serial-link compliance (PCIe/DDR/USB/Ethernet) needs a repeatable,
dependency-free eye/mask checker for CI, not just lab scopes.

## Detailed design

* Waveforms fold into a UI window; crossings interpolate linearly at the
  amplitude midpoint; eye width = UI − crossing spread; eye height = min-high
  − max-low at UI centers.
* Jitter: RJ ≈ pₚₚ/6 (3σ Gaussian allowance), DJ = remainder.
* Masks normalized to (UI fraction, amplitude fraction) so one checker
  serves all standards; per-standard forbidden rectangles from the spec
  intent.

## Alternatives

Full statistical (DuBost-DFE) analysis (deferred), bit-error-rate
integration (deferred), importing scope CSVs (planned via Touchstone/CSV).

## Unresolved questions

PAM4 (Gen 6) needs a three-level eye model; the mask enum reserves the slot.
