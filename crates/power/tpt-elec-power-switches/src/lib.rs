// SPDX-License-Identifier: MIT OR Apache-2.0

//! Power switch and rectifier loss models.
//!
//! * [`PowerSwitch`] — MOSFET conduction (`I²·R·D`) and switching
//!   (`½·V·I·(tr+tf)·fsw`) losses, plus gate-drive loss.
//! * [`PowerDiode`] — forward conduction and reverse-recovery losses.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A power MOSFET model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSwitch {
    /// On-resistance at operating temperature [Ω].
    pub rds_on: f64,
    /// Total gate charge [C].
    pub gate_charge: f64,
    /// Output charge [C] (Coss-related switching loss term).
    pub output_charge: f64,
    /// Rise time [s].
    pub rise_time: f64,
    /// Fall time [s].
    pub fall_time: f64,
    /// Max VDS rating [V].
    pub vds_rating: f64,
    /// Thermal resistance junction-to-ambient [°C/W].
    pub r_theta_ja: f64,
}

/// A power rectifier model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerDiode {
    /// Forward voltage at rated current [V].
    pub vf: f64,
    /// Reverse recovery charge [C] (0 for Schottky).
    pub reverse_recovery_charge: f64,
    /// Max VRRM rating [V].
    pub vrrm_rating: f64,
}

/// Loss breakdown for one switching device.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LossBreakdown {
    /// Conduction loss [W].
    pub conduction: f64,
    /// Switching (voltage-current overlap) loss [W].
    pub switching: f64,
    /// Output-charge loss [W].
    pub output_charge_loss: f64,
    /// Gate-drive loss [W].
    pub gate_drive: f64,
}

impl LossBreakdown {
    /// Total [W].
    pub fn total(&self) -> f64 {
        self.conduction + self.switching + self.output_charge_loss + self.gate_drive
    }
}

impl PowerSwitch {
    /// Full loss breakdown for a hard-switched PWM leg.
    ///
    /// * `current_rms` — conduction RMS current [A]
    /// * `voltage` — blocking voltage at turn-off [V]
    /// * `current_off` — current commutated at turn-off [A]
    /// * `duty` — on-duty fraction
    /// * `fsw` — switching frequency [Hz]
    /// * `gate_drive_v` — gate swing [V]
    pub fn losses(
        &self,
        current_rms: f64,
        voltage: f64,
        current_off: f64,
        duty: f64,
        fsw: f64,
        gate_drive_v: f64,
    ) -> LossBreakdown {
        LossBreakdown {
            conduction: current_rms * current_rms * self.rds_on * duty,
            switching: 0.5 * voltage * current_off * (self.rise_time + self.fall_time) * fsw,
            output_charge_loss: self.output_charge * voltage * fsw,
            gate_drive: self.gate_charge * gate_drive_v * fsw,
        }
    }

    /// Junction temperature [°C] at ambient for a given dissipation.
    pub fn junction_temperature(&self, power_w: f64, ambient_c: f64) -> f64 {
        ambient_c + power_w * self.r_theta_ja
    }

    /// Whether the switch blocks more than its rating.
    pub fn overstressed(&self, voltage: f64) -> bool {
        voltage > self.vds_rating
    }
}

impl PowerDiode {
    /// Diode loss breakdown [W].
    ///
    /// * `i_avg` — average forward current [A]
    /// * `duty` — conduction duty
    /// * `v_reverse` — reverse voltage when blocking [V]
    /// * `fsw` — switching frequency [Hz]
    pub fn losses(&self, i_avg: f64, duty: f64, v_reverse: f64, fsw: f64) -> LossBreakdown {
        LossBreakdown {
            conduction: i_avg * self.vf * duty,
            switching: self.reverse_recovery_charge * v_reverse * fsw,
            output_charge_loss: 0.0,
            gate_drive: 0.0,
        }
    }

    /// Whether the rectifier sees more than its reverse rating.
    pub fn overstressed(&self, v_reverse: f64) -> bool {
        v_reverse > self.vrrm_rating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mosfet() -> PowerSwitch {
        PowerSwitch {
            rds_on: 0.01,
            gate_charge: 30e-9,
            output_charge: 15e-9,
            rise_time: 5e-9,
            fall_time: 5e-9,
            vds_rating: 40.0,
            r_theta_ja: 40.0,
        }
    }

    #[test]
    fn conduction_loss_is_i2r_times_duty() {
        let m = mosfet();
        let l = m.losses(2.0, 12.0, 2.0, 0.5, 100e3, 10.0);
        assert!((l.conduction - 0.02).abs() < 1e-12); // 4·0.01·0.5
        assert!(l.total() > l.conduction);
    }

    #[test]
    fn switching_loss_formula() {
        let m = mosfet();
        let l = m.losses(2.0, 12.0, 2.0, 0.5, 100e3, 10.0);
        // 0.5·12·2·10e-9·1e5 = 12 mW
        assert!((l.switching - 12e-3).abs() < 1e-9);
        // Gate drive: 30 nC·10 V·100 kHz = 30 mW
        assert!((l.gate_drive - 30e-3).abs() < 1e-9);
        // Output charge: 15 nC·12 V·100 kHz = 18 mW
        assert!((l.output_charge_loss - 18e-3).abs() < 1e-9);
    }

    #[test]
    fn junction_temperature() {
        let m = mosfet();
        assert!((m.junction_temperature(0.5, 25.0) - 45.0).abs() < 1e-12);
        assert!(m.junction_temperature(2.0, 25.0) > 100.0);
    }

    #[test]
    fn stress_checks() {
        let m = mosfet();
        assert!(!m.overstressed(12.0));
        assert!(m.overstressed(48.0));
    }

    #[test]
    fn diode_losses() {
        let d = PowerDiode {
            vf: 0.5,
            reverse_recovery_charge: 0.0, // Schottky
            vrrm_rating: 30.0,
        };
        let l = d.losses(1.0, 0.5, 12.0, 100e3);
        assert!((l.conduction - 0.25).abs() < 1e-12);
        assert_eq!(l.switching, 0.0); // no Qrr
        assert!(!d.overstressed(12.0));
        assert!(d.overstressed(40.0));

        let fast = PowerDiode {
            vf: 0.7,
            reverse_recovery_charge: 50e-9,
            vrrm_rating: 100.0,
        };
        let l2 = fast.losses(1.0, 0.5, 12.0, 100e3);
        // Qrr·V·f = 50 nC·12·1e5 = 60 mW
        assert!((l2.switching - 60e-3).abs() < 1e-9);
    }

    #[test]
    fn loss_breakdown_sums() {
        let l = LossBreakdown {
            conduction: 1.0,
            switching: 2.0,
            output_charge_loss: 3.0,
            gate_drive: 4.0,
        };
        assert!((l.total() - 10.0).abs() < 1e-12);
    }
}
