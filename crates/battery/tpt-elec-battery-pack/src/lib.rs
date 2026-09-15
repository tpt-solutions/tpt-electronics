// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pack-level series/parallel composition of cells.
//!
//! [`PackBuilder`] assembles `series × parallel` groups, computes pack
//! voltage/capacity/resistance, and aggregates SOC (min rule) and
//! temperature (hottest cell) for BMS-style supervision.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_battery_core::BatteryCell;

/// A configured pack.
#[derive(Clone, Debug)]
pub struct Pack {
    /// Cell description.
    pub cell: BatteryCell,
    /// Cells in series.
    pub series: u32,
    /// Cells in parallel per group.
    pub parallel: u32,
}

impl Pack {
    /// Pack nominal voltage [V].
    pub fn nominal_voltage(&self) -> f64 {
        self.cell.nominal_voltage * self.series as f64
    }

    /// Pack capacity [A·h] (set by parallel count).
    pub fn capacity_ah(&self) -> f64 {
        self.cell.capacity_ah * self.parallel as f64
    }

    /// Pack energy [W·h].
    pub fn energy_wh(&self) -> f64 {
        self.capacity_ah() * self.nominal_voltage()
    }

    /// Pack DC resistance [Ω]: `s·r/p`.
    pub fn internal_resistance(&self) -> f64 {
        self.cell.internal_resistance * self.series as f64 / self.parallel.max(1) as f64
    }

    /// Total cell count.
    pub fn cell_count(&self) -> u32 {
        self.series * self.parallel
    }

    /// Max continuous current [A].
    pub fn max_current(&self) -> f64 {
        self.cell.max_current() * self.parallel as f64
    }

    /// Pack SOC from per-group SOCs: the MIN rule (the weakest group limits
    /// discharge).
    pub fn pack_soc(&self, group_socs: &[f64]) -> f64 {
        group_socs
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min)
            .clamp(0.0, 1.0)
    }

    /// Hottest cell temperature in the pack.
    pub fn hottest_cell(&self, temperatures: &[f64]) -> Option<f64> {
        temperatures
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
            .into()
    }

    /// ΔT spread across the pack [K].
    pub fn temperature_spread(&self, temperatures: &[f64]) -> f64 {
        let (mn, mx) = temperatures
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| {
                (a.min(t), b.max(t))
            });
        mx - mn
    }

    /// Terminal voltage under load [V]: `V_ocv(soc) − I·R_pack`.
    pub fn terminal_voltage(&self, soc: f64, current: f64, cell_ocv: impl Fn(f64) -> f64) -> f64 {
        cell_ocv(soc) * self.series as f64 - current * self.internal_resistance()
    }

    /// Whether a group imbalance trips a typical BMS imbalance alarm
    /// (ΔSOC > 5 %).
    pub fn imbalance_alarm(&self, group_socs: &[f64]) -> bool {
        let (mn, mx) = group_socs
            .iter()
            .fold((1.0f64, 0.0f64), |(a, b), &s| (a.min(s), b.max(s)));
        mx - mn > 0.05
    }
}

/// Pack assembly helper.
pub struct PackBuilder;

impl PackBuilder {
    /// Configures `series × parallel` of a cell.
    pub fn build(cell: BatteryCell, series: u32, parallel: u32) -> Pack {
        Pack {
            cell,
            series: series.max(1),
            parallel: parallel.max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> BatteryCell {
        BatteryCell::typical_nmc(2.0)
    }

    #[test]
    fn series_scales_voltage() {
        let pack = PackBuilder::build(cell(), 13, 1);
        assert!((pack.nominal_voltage() - 48.1).abs() < 1e-9);
        assert!((pack.capacity_ah() - 2.0).abs() < 1e-9);
        assert_eq!(pack.cell_count(), 13);
    }

    #[test]
    fn parallel_scales_capacity() {
        let pack = PackBuilder::build(cell(), 13, 4);
        assert!((pack.capacity_ah() - 8.0).abs() < 1e-9);
        assert!(
            (pack.internal_resistance() - cell().internal_resistance * 13.0 / 4.0).abs() < 1e-15
        );
        assert_eq!(pack.cell_count(), 52);
    }

    #[test]
    fn energy_adds_up() {
        let pack = PackBuilder::build(cell(), 13, 4);
        assert!((pack.energy_wh() - 8.0 * 48.1).abs() < 1e-9);
    }

    #[test]
    fn soc_min_rule() {
        let pack = PackBuilder::build(cell(), 4, 1);
        assert!((pack.pack_soc(&[0.9, 0.8, 0.85, 0.82]) - 0.8).abs() < 1e-12);
    }

    #[test]
    fn thermal_supervision() {
        let pack = PackBuilder::build(cell(), 4, 2);
        let temps = [30.0, 33.0, 41.0, 35.0, 32.0, 31.0, 30.0, 34.0];
        assert!((pack.hottest_cell(&temps).unwrap() - 41.0).abs() < 1e-12);
        assert!((pack.temperature_spread(&temps) - 11.0).abs() < 1e-12);
    }

    #[test]
    fn imbalance_alarm() {
        let pack = PackBuilder::build(cell(), 2, 1);
        assert!(!pack.imbalance_alarm(&[0.80, 0.83]));
        assert!(pack.imbalance_alarm(&[0.80, 0.90]));
    }

    #[test]
    fn terminal_voltage_sags_under_load() {
        let pack = PackBuilder::build(cell(), 13, 1);
        let r = cell().internal_resistance;
        let rest = pack.terminal_voltage(0.5, 0.0, |_| 3.7);
        let loaded = pack.terminal_voltage(0.5, 6.0, |_| 3.7);
        assert!((rest - 48.1).abs() < 1e-9);
        assert!((loaded - (48.1 - 6.0 * r * 13.0)).abs() < 1e-9);
    }
}
