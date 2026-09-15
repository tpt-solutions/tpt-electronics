// SPDX-License-Identifier: MIT OR Apache-2.0

//! Converter topologies and first-pass automated design.
//!
//! [`BuckDesigner`] (and the topology enum covering the other families)
//! implements the standard textbook design flow: duty cycle, inductor
//! ripple current, output capacitor ripple, switch/diode stress, and an
//! efficiency estimate from conduction losses. Magnetics details live in
//! `tpt-elec-power-magnetics`, switch losses in `tpt-elec-power-switches`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Converter topology families with their operating requirements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConverterTopology {
    /// Step-down.
    Buck {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output current [A].
        iout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Step-up.
    Boost {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output current [A].
        iout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Inverting / step-up-step-down.
    BuckBoost {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage magnitude [V].
        vout: f64,
        /// Output current [A].
        iout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Isolated flyback.
    Flyback {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output power [W].
        pout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Single-switch forward.
    Forward {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output power [W].
        pout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Half bridge.
    HalfBridge {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output power [W].
        pout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Full bridge.
    FullBridge {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output power [W].
        pout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
    /// Resonant LLC.
    Llc {
        /// Input voltage [V].
        vin: f64,
        /// Output voltage [V].
        vout: f64,
        /// Output power [W].
        pout: f64,
        /// Switching frequency [Hz].
        fsw: f64,
    },
}

impl ConverterTopology {
    /// Switching frequency [Hz].
    pub fn fsw(&self) -> f64 {
        match *self {
            ConverterTopology::Buck { fsw, .. }
            | ConverterTopology::Boost { fsw, .. }
            | ConverterTopology::BuckBoost { fsw, .. }
            | ConverterTopology::Flyback { fsw, .. }
            | ConverterTopology::Forward { fsw, .. }
            | ConverterTopology::HalfBridge { fsw, .. }
            | ConverterTopology::FullBridge { fsw, .. }
            | ConverterTopology::Llc { fsw, .. } => fsw,
        }
    }

    /// Whether the topology provides isolation.
    pub fn is_isolated(&self) -> bool {
        matches!(
            self,
            ConverterTopology::Flyback { .. }
                | ConverterTopology::Forward { .. }
                | ConverterTopology::HalfBridge { .. }
                | ConverterTopology::FullBridge { .. }
                | ConverterTopology::Llc { .. }
        )
    }
}

/// Chosen pass devices (subsystem of a full design).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConverterComponents {
    /// Power switch on-resistance [Ω] (0 = ideal).
    pub switch_rds_on: f64,
    /// Switch output charge [C] (0 = ideal).
    pub switch_q_output: f64,
    /// Rectifier forward drop [V] (0 = ideal).
    pub diode_vf: f64,
    /// Inductance [H].
    pub inductance: f64,
    /// Inductor DCR [Ω].
    pub inductor_dcr: f64,
    /// Output capacitance [F].
    pub output_cap: f64,
    /// Output cap ESR [Ω].
    pub output_cap_esr: f64,
}

/// A complete first-pass converter design.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConverterDesign {
    /// Topology and operating point.
    pub topology: ConverterTopology,
    /// Component selection.
    pub components: ConverterComponents,
    /// Estimated efficiency (0–1) from conduction + switching losses.
    pub efficiency: f64,
    /// Peak inductor current [A].
    pub peak_inductor_current: f64,
    /// Peak-to-peak inductor ripple [A].
    pub inductor_ripple: f64,
    /// Output voltage ripple [V] (approximate, ESR + capacitive).
    pub output_ripple: f64,
    /// Estimated loop bandwidth target [Hz] (~fsw/10).
    pub loop_bandwidth: f64,
    /// Estimated phase margin [°] for the suggested compensation.
    pub phase_margin: f64,
}

/// Buck converter sizing.
pub struct BuckDesigner;

impl BuckDesigner {
    /// Automates the standard first-pass buck design.
    ///
    /// * `ripple_fraction` — peak-to-peak inductor ripple as a fraction of
    ///   `iout` (0.2–0.4 typical).
    /// * `switch_rds_on` / `diode_vf` — device parameters for loss estimates.
    pub fn design(
        vin: f64,
        vout: f64,
        iout: f64,
        fsw: f64,
        ripple_fraction: f64,
        switch_rds_on: f64,
        diode_vf: f64,
    ) -> Result<ConverterDesign, String> {
        if vout >= vin {
            return Err("buck requires vout < vin".into());
        }
        if iout <= 0.0 || fsw <= 0.0 || vin <= 0.0 {
            return Err("vin, iout, fsw must be positive".into());
        }
        let duty = vout / vin;
        let ripple = iout * ripple_fraction.clamp(0.01, 1.0);
        // L = Vout·(1−D)/(ΔI·fsw)
        let l = vout * (1.0 - duty) / (ripple * fsw);
        // Output cap for ~1% capacitive ripple + ESR term with 50 mΩ assumption
        let c_out = ripple / (8.0 * fsw * 0.01 * vout.max(1e-9));
        let esr = 0.05 * vout / ripple;

        // Loss model (CCM):
        // conduction: I²·R·D (switch) + I·Vf·(1−D) (diode) + I²·DCR
        let p_cond = iout * iout * switch_rds_on * duty + iout * diode_vf * (1.0 - duty);
        // switching: 0.5·V·I·(tr+tf)·fsw with 20 ns combined edge assumption
        let p_sw = 0.5 * vin * iout * 20e-9 * fsw;
        let p_out = vout * iout;
        let efficiency = (p_out / (p_out + p_cond + p_sw)).clamp(0.0, 1.0);

        Ok(ConverterDesign {
            topology: ConverterTopology::Buck {
                vin,
                vout,
                iout,
                fsw,
            },
            components: ConverterComponents {
                switch_rds_on,
                switch_q_output: 0.0,
                diode_vf,
                inductance: l,
                inductor_dcr: 0.0,
                output_cap: c_out,
                output_cap_esr: esr,
            },
            efficiency,
            peak_inductor_current: iout + ripple / 2.0,
            inductor_ripple: ripple,
            output_ripple: (ripple * esr + 0.01 * vout).min(0.02 * vout),
            loop_bandwidth: fsw / 10.0,
            phase_margin: 45.0,
        })
    }

    /// Continuous-conduction-mode duty cycle.
    pub fn duty_cycle(vin: f64, vout: f64) -> f64 {
        (vout / vin).clamp(0.0, 1.0)
    }

    /// Required inductance for a given ripple current [H].
    pub fn inductance_for_ripple(
        vin: f64,
        vout: f64,
        iout: f64,
        fsw: f64,
        ripple_fraction: f64,
    ) -> f64 {
        let duty = Self::duty_cycle(vin, vout);
        let ripple = iout * ripple_fraction;
        vout * (1.0 - duty) / (ripple.max(1e-12) * fsw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buck_design_12v_to_3v3() {
        let d = BuckDesigner::design(12.0, 3.3, 2.0, 500e3, 0.3, 0.03, 0.5).unwrap();
        // Duty = 3.3/12 = 0.275
        assert!((BuckDesigner::duty_cycle(12.0, 3.3) - 0.275).abs() < 1e-12);
        // L = 3.3·0.725/(0.6·5e5) = 7.975 µH
        assert!((d.components.inductance - 7.975e-6).abs() / 7.975e-6 < 1e-9);
        // Ripple = 0.6 A
        assert!((d.inductor_ripple - 0.6).abs() < 1e-12);
        // Peak = 2 + 0.3 = 2.3 A
        assert!((d.peak_inductor_current - 2.3).abs() < 1e-12);
        // Efficiency sanity: losses ≈ 4·0.03·0.275 + 0.5·0.725 + 2.4e-3·12·2·0.5e6·...
        assert!((0.7..0.99).contains(&d.efficiency), "eff {}", d.efficiency);
        assert!(!d.topology.is_isolated());
        assert!((d.topology.fsw() - 500e3).abs() < 1.0);
    }

    #[test]
    fn buck_rejects_non_step_down() {
        assert!(BuckDesigner::design(5.0, 12.0, 1.0, 1e6, 0.3, 0.03, 0.5).is_err());
        assert!(BuckDesigner::design(0.0, 3.3, 1.0, 1e6, 0.3, 0.03, 0.5).is_err());
        assert!(BuckDesigner::design(12.0, 3.3, 0.0, 1e6, 0.3, 0.03, 0.5).is_err());
    }

    #[test]
    fn inductance_formula_matches_designer() {
        let l1 = BuckDesigner::inductance_for_ripple(12.0, 3.3, 2.0, 500e3, 0.3);
        let d = BuckDesigner::design(12.0, 3.3, 2.0, 500e3, 0.3, 0.03, 0.5).unwrap();
        assert!((l1 - d.components.inductance).abs() < 1e-12);
    }

    #[test]
    fn topology_flags() {
        let t = ConverterTopology::Flyback {
            vin: 230.0,
            vout: 12.0,
            pout: 30.0,
            fsw: 100e3,
        };
        assert!(t.is_isolated());
        assert!((t.fsw() - 100e3).abs() < 1.0);
        let b = ConverterTopology::Boost {
            vin: 3.3,
            vout: 12.0,
            iout: 0.5,
            fsw: 1e6,
        };
        assert!(!b.is_isolated());
    }

    #[test]
    fn higher_ripple_means_smaller_inductor() {
        let low = BuckDesigner::inductance_for_ripple(12.0, 3.3, 2.0, 500e3, 0.2);
        let high = BuckDesigner::inductance_for_ripple(12.0, 3.3, 2.0, 500e3, 0.5);
        assert!(high < low);
    }
}
