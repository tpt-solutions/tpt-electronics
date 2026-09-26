# RFC 0004: RF matching

- **Status:** Implemented
- **Crates:** `tpt-elec-rf-core`, `tpt-elec-rf-filters`, `tpt-elec-rf-mixer`, `tpt-elec-rf-links`, `tpt-elec-rf-antenna`

## Summary

Smith-chart mathematics, L/π matching synthesis, Butterworth/Chebyshev/Bessel
ladder filters with ABCD-verified S-parameters, and Friis chain arithmetic.

## Motivation

Antenna matching and filter design are the RF tasks EDA tools gate behind
$50k seats; the closed forms are entirely public and testable.

## Detailed design

* L-networks: Q method with opposite-sign reactances; complex loads are
  pre-resonated with a series element placed at the load side.
* π-networks: virtual-resistance method with loaded-Q floor.
* Verification: every returned network is simulated through its ABCD chain
  and tests require |Γ| < 10⁻³ at the design frequency — synthesis and
  validation can never silently diverge.
* Filters: g-value prototypes + LP→HP transformation; the Butterworth −3 dB
  corner and Chebyshev equal-ripple edge are asserted through the same ABCD
  simulation. Chebyshev-II uses a response-based pole/zero design (even
  orders 2–12). Elliptic (Cauer) is implemented as a Zolotarev pole/zero
  design (orders 1–10), verified case-for-case against `scipy.signal.ellipap`
  and against the equiripple definition.

## Alternatives

Real-frequency technique (deferred), optimization-based matching (deferred),
elliptic synthesis (implemented; see `tpt-elec-rf-filters::elliptic`).

## Unresolved questions

Stub and transformer topologies are enum slots; distributed-element
synthesis awaits use cases.
