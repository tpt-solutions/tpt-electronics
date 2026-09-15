// SPDX-License-Identifier: MIT OR Apache-2.0

//! Battery-aware power delivery helpers bridging `tpt-elec-power-core` and
//! `tpt-elec-battery-core`.
//!
//! Answers the system-level questions: how long can a pack run a converter
//! load, what input voltage window does the converter see over the SOC
//! range, and what pack current does a given load power draw at the input.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_battery_core::{BatteryCell, EquivalentCircuitModel};
use tpt_elec_power_core::BuckDesigner;

/// A battery feeding a converter load.
pub struct BatteryPoweredConverter {
    /// The cell (single-cell model; scale via series/parallel counts).
    pub cell: BatteryCell,
    /// Cell ECM for terminal-voltage estimates.
    pub ecm: EquivalentCircuitModel,
    /// Number of cells in series.
    pub series: u32,
    /// Number of cells in parallel.
    pub parallel: u32,
    /// Converter input→output efficiency (0..1].
    pub converter_efficiency: f64,
}

impl BatteryPoweredConverter {
    /// Builds a bridge with a one-RC ECM derived from the cell resistance.
    pub fn new(cell: BatteryCell, series: u32, parallel: u32, converter_efficiency: f64) -> Self {
        let r0 = cell.internal_resistance * series as f64 / parallel.max(1) as f64;
        Self {
            ecm: EquivalentCircuitModel::one_rc(r0, r0 * 0.5, 2000.0),
            cell,
            series: series.max(1),
            parallel: parallel.max(1),
            converter_efficiency: converter_efficiency.clamp(0.05, 1.0),
        }
    }

    /// Pack nominal voltage [V].
    pub fn pack_voltage(&self) -> f64 {
        self.cell.nominal_voltage * self.series as f64
    }

    /// Pack capacity [A·h].
    pub fn pack_capacity_ah(&self) -> f64 {
        self.cell.capacity_ah * self.parallel as f64
    }

    /// Pack energy [W·h].
    pub fn pack_energy_wh(&self) -> f64 {
        self.pack_capacity_ah() * self.pack_voltage()
    }

    /// Input current [A] the pack must supply for an output power draw.
    ///
    /// Includes the converter efficiency and the SOC-dependent terminal
    /// voltage (iterative, since I depends on V which depends on I).
    pub fn input_current_at(&self, output_power_w: f64, soc: f64) -> f64 {
        let p_in = output_power_w / self.converter_efficiency.max(1e-3);
        // Start from the nominal point and refine.
        let v_nominal = self.pack_voltage() * 0.9;
        let mut i = p_in / v_nominal.max(1e-3);
        for _ in 0..8 {
            let r_pack =
                self.cell.internal_resistance * self.series as f64 / self.parallel.max(1) as f64;
            let v_terminal = self.pack_voltage() * (0.75 + 0.25 * soc) - i * r_pack;
            i = p_in / v_terminal.max(1.0);
        }
        i
    }

    /// Runtime [s] delivering `output_power_w` from a full pack down to the
    /// cutoff SOC, using average-power coulomb counting.
    pub fn runtime_seconds(&self, output_power_w: f64, cutoff_soc: f64) -> f64 {
        let usable_ah = self.pack_capacity_ah() * (1.0 - cutoff_soc);
        let v_nominal = self.pack_voltage() * (0.78 + 0.22 * (1.0 + cutoff_soc) / 2.0);
        let i_avg = output_power_w / self.converter_efficiency.max(1e-3) / v_nominal.max(1.0);
        usable_ah * 3600.0 / i_avg.max(1e-9)
    }

    /// Whether a buck converter can regulate across the whole SOC window.
    ///
    /// Returns the SOC at which `v_pack(soc)` drops below `vout + margin`,
    /// or `None` if the pack always stays above it (buck remains valid).
    pub fn buck_dropout_soc(&self, vout: f64, margin: f64) -> Option<f64> {
        let v_max = self.pack_voltage() * 1.0; // full SOC (highest cell voltage)
        let v_min = self.pack_voltage() * 0.75; // empty
        let v_required = vout + margin;
        if v_min >= v_required {
            return None;
        }
        // Linear SOC→voltage interpolation between the window endpoints.
        let soc_at_dropout = ((v_required - v_min) / (v_max - v_min).max(1e-9)).clamp(0.0, 1.0);
        Some(soc_at_dropout)
    }

    /// Convenience: designs a buck for this pack and reports the design.
    pub fn design_buck(
        &self,
        vout: f64,
        iout: f64,
        fsw: f64,
    ) -> Result<tpt_elec_power_core::ConverterDesign, String> {
        let v_full = self.pack_voltage();
        let designer_ok = v_full > vout;
        if !designer_ok {
            return Err("pack voltage below buck input requirement".into());
        }
        BuckDesigner::design(
            (self.pack_voltage() * 0.9).max(vout * 1.2), // nominal operating point
            vout,
            iout,
            fsw,
            0.3,
            0.02,
            0.5,
        )
    }

    /// Topology recommendation for the pack window.
    pub fn recommended_topology(&self, vout: f64) -> &'static str {
        let v_min = self.pack_voltage() * 0.75;
        let v_max = self.pack_voltage();
        if v_min > vout {
            "buck"
        } else if v_max < vout {
            "boost"
        } else {
            "buck-boost"
        }
    }

    /// Full topology list helper (re-exports [`ConverterTopology`] variants).
    pub fn topology_families() -> [&'static str; 8] {
        [
            "buck",
            "boost",
            "buck-boost",
            "flyback",
            "forward",
            "half-bridge",
            "full-bridge",
            "llc",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_power_core::ConverterTopology;

    fn bridge() -> BatteryPoweredConverter {
        let cell = BatteryCell::typical_nmc(2.0);
        BatteryPoweredConverter::new(cell, 3, 1, 0.9)
    }

    #[test]
    fn pack_scaling() {
        let b = bridge();
        assert!((b.pack_voltage() - 11.1).abs() < 1e-9);
        assert!((b.pack_capacity_ah() - 2.0).abs() < 1e-9);
        // 3S1P: 22.2 Wh
        assert!((b.pack_energy_wh() - 22.2).abs() < 1e-9);
    }

    #[test]
    fn input_current_includes_efficiency() {
        let b = bridge();
        let i = b.input_current_at(9.0, 0.8);
        // 9 W out / 0.9 = 10 W in at ~11 V ≈ 0.95 A (with IR sag a bit more)
        assert!((0.8..1.4).contains(&i), "i = {i}");
        // Heavier load → more current
        assert!(b.input_current_at(18.0, 0.8) > i);
    }

    #[test]
    fn runtime_scales_with_power() {
        let b = bridge();
        let light = b.runtime_seconds(5.0, 0.1);
        let heavy = b.runtime_seconds(15.0, 0.1);
        assert!(light > heavy * 2.5, "{light} vs {heavy}");
        assert!(light > 0.0);
    }

    #[test]
    fn buck_dropout_mid_soc() {
        let b = bridge(); // 3S NMC: 11.1 V nominal, 8.3–11.1 V window
        let dropout = b.buck_dropout_soc(9.5, 0.5);
        // 9.5 V + 0.5 margin = 10 V lies inside the window
        let soc = dropout.expect("should drop out inside the window");
        assert!((0.0..1.0).contains(&soc));
        // A low-vout buck never drops out
        assert!(b.buck_dropout_soc(5.0, 0.5).is_none());
    }

    #[test]
    fn topology_recommendation() {
        let b = bridge();
        assert_eq!(b.recommended_topology(5.0), "buck");
        assert_eq!(b.recommended_topology(15.0), "boost");
        assert_eq!(b.recommended_topology(10.0), "buck-boost");
        assert_eq!(BatteryPoweredConverter::topology_families().len(), 8);
    }

    #[test]
    fn design_buck_uses_pack_window() {
        let b = bridge();
        let d = b.design_buck(5.0, 2.0, 500e3).unwrap();
        match d.topology {
            ConverterTopology::Buck {
                vin,
                vout,
                iout,
                fsw,
            } => {
                assert!((vin - 11.1 * 0.9).abs() < 1e-9);
                assert!((vout - 5.0).abs() < 1e-12);
                assert!((iout - 2.0).abs() < 1e-12);
                assert!((fsw - 500e3).abs() < 1.0);
            }
            other => panic!("expected buck, got {other:?}"),
        }
        assert!(d.efficiency > 0.5);
        assert!(b.design_buck(15.0, 1.0, 500e3).is_err());
    }
}
