# RFC 0005: Battery Kalman SOC

- **Status:** Implemented
- **Crates:** `tpt-elec-battery-bms`, `tpt-elec-battery-core`, `tpt-elec-battery-thermal`, `tpt-elec-battery-pack`

## Summary

Extended Kalman filter over the 1-RC equivalent circuit model for SOC
estimation, with coulomb counting, runaway propagation simulation, and
pack composition.

## Motivation

Coulomb counting drifts with current-sensor bias; pure OCV lookup fails
under load. The KF fuses both and is the industry-standard estimator that a
BMS study needs.

## Detailed design

* State `[SOC, V_rc]`; measurement `V = OCV(SOC) − I·R0 − V_rc`.
* `dV/dSOC` via central difference on the OCV table.
* Diagonal covariance propagation with process noise; Joseph-lite update.
* `track_soc` helper runs a synthetic drive cycle through the ECM and
  reports final |SOC error| (asserted < 2 % in tests with a 15 % initial
  guess error).

## Alternatives

UKF (enum slot, deferred), neural estimators (deferred), dual estimation of
capacity (deferred).

## Unresolved questions

Per-cell temperature-dependent OCV and aging models.
