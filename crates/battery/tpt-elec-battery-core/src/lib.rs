// SPDX-License-Identifier: MIT OR Apache-2.0

//! Battery cell models and equivalent circuit models (ECM).
//!
//! [`EquivalentCircuitModel`] implements the standard Thevenin ECM:
//! open-circuit voltage (SOC/temperature dependent), series resistance R0,
//! and an arbitrary number of RC pairs. [`step`] advances the RC state for
//! a constant current over a time step (exact for the linear network).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Cell chemistry.
#[derive(Clone, Debug, PartialEq)]
pub enum BatteryChemistry {
    /// Lithium-ion family with a cathode designation.
    LithiumIon {
        /// Cathode chemistry.
        cathode: CathodeMaterial,
    },
    /// Lithium polymer (pouch).
    LithiumPolymer,
    /// Nickel metal hydride.
    NickelMetalHydride,
    /// Lead acid.
    LeadAcid,
    /// Solid state.
    SolidState,
}

/// Lithium-ion cathode materials.
#[derive(Clone, Debug, PartialEq)]
pub enum CathodeMaterial {
    /// LiNiMnCoO2 (NMC) with ratio, e.g. "111", "811".
    Nmc {
        /// Stoichiometry.
        ratio: String,
    },
    /// LiFePO4.
    Lfp,
    /// LiCoO2.
    Lco,
    /// LiNiCoAlO2.
    Nca,
    /// Li4Ti5O12 anode chemistry (LTO).
    Lto,
}

impl CathodeMaterial {
    /// Nominal cell voltage [V].
    pub fn nominal_voltage(&self) -> f64 {
        match self {
            CathodeMaterial::Nmc { .. } => 3.7,
            CathodeMaterial::Lfp => 3.3,
            CathodeMaterial::Lco => 3.7,
            CathodeMaterial::Nca => 3.6,
            CathodeMaterial::Lto => 2.4,
        }
    }
}

/// A battery cell.
#[derive(Clone, Debug, PartialEq)]
pub struct BatteryCell {
    /// Chemistry.
    pub chemistry: BatteryChemistry,
    /// Nominal capacity [Ah].
    pub capacity_ah: f64,
    /// Nominal voltage [V].
    pub nominal_voltage: f64,
    /// DC internal resistance [Ohm].
    pub internal_resistance: f64,
    /// Max continuous discharge C-rate.
    pub max_c_rate: f64,
}

impl BatteryCell {
    /// A typical NMC-811 cell scaled with capacity.
    pub fn typical_nmc(capacity_ah: f64) -> Self {
        Self {
            chemistry: BatteryChemistry::LithiumIon {
                cathode: CathodeMaterial::Nmc {
                    ratio: "811".into(),
                },
            },
            capacity_ah,
            nominal_voltage: 3.7,
            internal_resistance: 25.0e-3 / capacity_ah.max(0.1) * 2.0,
            max_c_rate: 3.0,
        }
    }

    /// Max continuous current [A].
    pub fn max_current(&self) -> f64 {
        self.capacity_ah * self.max_c_rate
    }

    /// Stored energy [Wh].
    pub fn energy_wh(&self) -> f64 {
        self.capacity_ah * self.nominal_voltage
    }
}

/// Open-circuit voltage curve: (SOC, V) table with linear interpolation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OcvCurve {
    /// SOC breakpoints (0..1), ascending.
    pub soc: Vec<f64>,
    /// OCV [V] at each breakpoint.
    pub voltage: Vec<f64>,
    /// Reference temperature [C].
    pub temperature_c: f64,
}

impl OcvCurve {
    /// Builds a curve from equal SOC steps across `v_min..v_max` with the
    /// characteristic Li-ion knee.
    pub fn lithium_ion(v_min: f64, v_max: f64, points: usize) -> Self {
        let n = points.max(4);
        let mut soc = Vec::with_capacity(n);
        let mut voltage = Vec::with_capacity(n);
        for i in 0..n {
            let x = i as f64 / (n - 1) as f64;
            soc.push(x);
            let shaped = (x + 0.08 * (std::f64::consts::PI * x).sin()).clamp(0.0, 1.0);
            voltage.push(v_min + shaped * (v_max - v_min));
        }
        voltage[n - 1] = v_max;
        Self {
            soc,
            voltage,
            temperature_c: 25.0,
        }
    }

    /// OCV [V] at the given SOC (clamped, linear interpolation).
    pub fn voltage_at_soc(&self, soc: f64, _temperature_c: f64) -> f64 {
        if self.soc.is_empty() {
            return 0.0;
        }
        let s = soc.clamp(0.0, 1.0);
        for w in self.soc.windows(2).zip(self.voltage.windows(2)) {
            let (s0, s1) = (w.0[0], w.0[1]);
            if s <= s1 {
                let f = (s - s0) / (s1 - s0).max(1e-12);
                return w.1[0] + f * (w.1[1] - w.1[0]);
            }
        }
        *self.voltage.last().unwrap_or(&0.0)
    }
}

/// One RC branch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RcPair {
    /// Resistance [Ohm].
    pub r: f64,
    /// Capacitance [F].
    pub c: f64,
}

impl RcPair {
    /// Time constant [s].
    pub fn tau(&self) -> f64 {
        self.r * self.c
    }
}

/// Thevenin equivalent circuit model.
#[derive(Clone, Debug, PartialEq)]
pub struct EquivalentCircuitModel {
    /// Open-circuit voltage curve.
    pub ocv: OcvCurve,
    /// Ohmic (series) resistance [Ohm].
    pub r0: f64,
    /// Diffusion/polarization RC pairs.
    pub rc_pairs: Vec<RcPair>,
}

impl EquivalentCircuitModel {
    /// A common 1-RC model for an NMC cell.
    pub fn one_rc(r0: f64, r1: f64, c1: f64) -> Self {
        Self {
            ocv: OcvCurve::lithium_ion(3.0, 4.2, 21),
            r0,
            rc_pairs: vec![RcPair { r: r1, c: c1 }],
        }
    }

    /// Terminal voltage [V] for the given RC branch states.
    ///
    /// `rc_states[i]` is the voltage across RC pair i [V]. Positive
    /// `current` discharges.
    pub fn terminal_voltage(&self, soc: f64, current: f64, rc_states: &[f64]) -> f64 {
        let ocv = self.ocv.voltage_at_soc(soc, 25.0);
        let mut v = ocv - current * self.r0;
        for state in rc_states {
            v -= *state;
        }
        v
    }

    /// Advances RC states over `dt` for constant current (exact exponential
    /// update). Positive current discharges (polarizes the branches).
    pub fn step(&self, current: f64, dt: f64, rc_states: &[f64]) -> Vec<f64> {
        self.rc_pairs
            .iter()
            .zip(rc_states)
            .map(|(pair, &v)| {
                let tau = pair.tau().max(1e-9);
                let target = current * pair.r;
                target + (v - target) * (-dt / tau).exp()
            })
            .collect()
    }

    /// SOC update for coulomb counting.
    pub fn update_soc(&self, soc: f64, current: f64, dt: f64, capacity_ah: f64) -> f64 {
        (soc - current * dt / (capacity_ah * 3600.0)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemistry_voltages() {
        assert!((CathodeMaterial::Lfp.nominal_voltage() - 3.3).abs() < 1e-12);
        assert!(
            CathodeMaterial::Nmc {
                ratio: "811".into()
            }
            .nominal_voltage()
                > 3.5
        );
    }

    #[test]
    fn cell_basics() {
        let cell = BatteryCell::typical_nmc(2.0);
        assert!((cell.max_current() - 6.0).abs() < 1e-9);
        assert!((cell.energy_wh() - 7.4).abs() < 1e-9);
    }

    #[test]
    fn ocv_interpolation_monotonic() {
        let ocv = OcvCurve::lithium_ion(3.0, 4.2, 21);
        for w in ocv.voltage.windows(2) {
            assert!(w[1] >= w[0]);
        }
        assert!((ocv.voltage_at_soc(0.0, 25.0) - 3.0).abs() < 1e-9);
        assert!((ocv.voltage_at_soc(1.0, 25.0) - 4.2).abs() < 1e-9);
        let mid = ocv.voltage_at_soc(0.5, 25.0);
        assert!((3.5..4.0).contains(&mid), "mid {mid}");
        assert!((ocv.voltage_at_soc(-0.5, 25.0) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn ecm_terminal_voltage_and_polarization() {
        let ecm = EquivalentCircuitModel::one_rc(0.03, 0.02, 2000.0);
        let states = vec![0.0];
        let soc = 0.5;
        assert!(
            (ecm.terminal_voltage(soc, 0.0, &states) - ecm.ocv.voltage_at_soc(soc, 25.0)).abs()
                < 1e-12
        );
        // 1 A discharge: instant drop = I*R0 = 30 mV
        let v1 = ecm.terminal_voltage(soc, 1.0, &states);
        assert!((v1 - (ecm.ocv.voltage_at_soc(soc, 25.0) - 0.03)).abs() < 1e-12);
        // After 5 tau the RC branch polarizes to I*R1 = 20 mV
        let states5 = ecm.step(1.0, 5.0 * ecm.rc_pairs[0].tau(), &states);
        assert!((states5[0] - 0.02).abs() < 2e-4); // 5 tau leaves 0.7% of the gap
        let v2 = ecm.terminal_voltage(soc, 1.0, &states5);
        assert!(v2 < v1);
        // Relaxing back at zero current
        let relaxed = ecm.step(0.0, 5.0 * ecm.rc_pairs[0].tau(), &states5);
        assert!(relaxed[0].abs() < 2e-4); // same 5-tau residue
    }

    #[test]
    fn coulomb_counting_soc() {
        let ecm = EquivalentCircuitModel::one_rc(0.03, 0.02, 2000.0);
        assert!(ecm.update_soc(1.0, 2.0, 3600.0, 2.0) < 1e-9);
        assert!((ecm.update_soc(1.0, 2.0, 1800.0, 2.0) - 0.5).abs() < 1e-9);
        assert!((ecm.update_soc(0.95, -2.0, 3600.0, 2.0) - 1.0).abs() < 1e-9);
    }
}
