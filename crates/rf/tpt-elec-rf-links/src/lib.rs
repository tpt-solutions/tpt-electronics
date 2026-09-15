// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end RF chain composition: antennas, filters, mixers, amplifiers,
//! and link budgets evaluated stage-by-stage.
//!
//! [`LinkChain`] collects stages, evaluates gain/NF/IIP3 with the Friis
//! relations (via `tpt-elec-rf-mixer`), and folds in the free-space link
//! budget from `tpt-elec-rf-antenna`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_rf_antenna::{Antenna, LinkBudget};
use tpt_elec_rf_mixer::CascadeCalculator;

/// One stage in an RF chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// Amplifier (LNA, driver, PA).
    Amplifier {
        /// Gain [dB].
        gain_db: f64,
        /// Noise figure [dB].
        nf_db: f64,
        /// Output third-order intercept [dBm].
        oip3_dbm: f64,
    },
    /// Passive filter (or any lossy passive).
    Filter {
        /// Insertion loss (positive number) [dB].
        insertion_loss_db: f64,
    },
    /// Mixer.
    Mixer {
        /// Conversion gain [dB] (usually negative).
        gain_db: f64,
        /// Noise figure [dB].
        nf_db: f64,
        /// Input third-order intercept [dBm].
        iip3_dbm: f64,
    },
    /// Attenuator/cable.
    Loss {
        /// Loss (positive number) [dB].
        loss_db: f64,
    },
}

impl Stage {
    /// Gain contribution [dB].
    pub fn gain_db(&self) -> f64 {
        match *self {
            Stage::Amplifier { gain_db, .. } | Stage::Mixer { gain_db, .. } => gain_db,
            Stage::Filter { insertion_loss_db } => -insertion_loss_db,
            Stage::Loss { loss_db } => -loss_db,
        }
    }

    /// Noise figure contribution [dB].
    pub fn nf_db(&self) -> f64 {
        match *self {
            Stage::Amplifier { nf_db, .. } | Stage::Mixer { nf_db, .. } => nf_db,
            // Passive at 290 K: NF = loss
            Stage::Filter { insertion_loss_db } => insertion_loss_db,
            Stage::Loss { loss_db } => loss_db,
        }
    }

    /// IIP3 contribution [dBm].
    pub fn iip3_dbm(&self) -> Option<f64> {
        match *self {
            Stage::Amplifier {
                gain_db,
                nf_db: _,
                oip3_dbm,
            } => Some(oip3_dbm - gain_db),
            Stage::Mixer { iip3_dbm, .. } => Some(iip3_dbm),
            _ => None,
        }
    }
}

/// Evaluated chain results.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChainResult {
    /// Total gain source→load [dB].
    pub gain_db: f64,
    /// System noise figure [dB].
    pub nf_db: f64,
    /// System input-referred IIP3 [dBm].
    pub iip3_dbm: f64,
    /// Spurious-free dynamic range [dB] (2·(IIP3−NF_floor)−NF… classic SFDR
    /// = 2/3·(IIP3 − noise floor) reported here in dB).
    pub sfdr_db: f64,
}

/// A composed RF receive chain.
#[derive(Clone, Debug, Default)]
pub struct LinkChain {
    /// Ordered stages (antenna-side first).
    pub stages: Vec<Stage>,
}

impl LinkChain {
    /// An empty chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a stage.
    pub fn push(&mut self, stage: Stage) -> &mut Self {
        self.stages.push(stage);
        self
    }

    /// Standard front end: filter → LNA → mixer.
    pub fn typical_receiver(lna_gain_db: f64, lna_nf_db: f64) -> Self {
        let mut chain = Self::new();
        chain.push(Stage::Filter {
            insertion_loss_db: 1.0,
        });
        chain.push(Stage::Amplifier {
            gain_db: lna_gain_db,
            nf_db: lna_nf_db,
            oip3_dbm: 30.0,
        });
        chain.push(Stage::Mixer {
            gain_db: -7.0,
            nf_db: 7.5,
            iip3_dbm: 22.0,
        });
        chain
    }

    /// Evaluates gain, NF, IIP3, and SFDR. An empty chain is a 0 dB,
    /// 0 dB NF pass-through.
    pub fn evaluate(&self) -> ChainResult {
        if self.stages.is_empty() {
            return ChainResult {
                gain_db: 0.0,
                nf_db: 0.0,
                iip3_dbm: f64::INFINITY,
                sfdr_db: f64::INFINITY,
            };
        }
        let gains: Vec<f64> = self.stages.iter().map(|s| s.gain_db()).collect();
        let nfs: Vec<f64> = self.stages.iter().map(|s| s.nf_db()).collect();
        let gain = CascadeCalculator::cascade_gain_db(&gains);
        let nf = CascadeCalculator::cascade_nf_db(&nfs, &gains);

        // Friis IIP3 over the stages that intercept (passives pass through
        // with shifted intercepts: IIP3_out = IIP3 − preceding gain… handled
        // by using OIP3 refs where present, IIP3 for the mixer).
        let mut iip3_inv = 0.0f64;
        let mut g_prod = 1.0f64;
        for (i, stage) in self.stages.iter().enumerate() {
            if i > 0 {
                g_prod += 10f64.powf(gains[i - 1] / 10.0) * g_prod;
            }
            if let Some(iip3) = stage.iip3_dbm() {
                iip3_inv += g_prod / 10f64.powf(iip3 / 10.0);
            }
        }
        let iip3 = if iip3_inv > 0.0 {
            10.0 * (1.0 / iip3_inv).log10()
        } else {
            f64::INFINITY
        };

        // SFDR: 2/3·(IIP3 − (−174 + 10log10(BW=1 Hz) + NF)) with 1 Hz BW.
        let noise_floor = -174.0 + nf;
        let sfdr = 2.0 / 3.0 * (iip3 - noise_floor);

        ChainResult {
            gain_db: gain,
            nf_db: nf,
            iip3_dbm: iip3,
            sfdr_db: sfdr,
        }
    }

    /// Adds the antennas and path: end-to-end link budget check.
    ///
    /// Returns the received power [dBm] of a `tx_power_dbm` transmitter.
    pub fn received_power_dbm(
        &self,
        tx_antenna: &Antenna,
        rx_antenna: &Antenna,
        tx_power_dbm: f64,
        distance_m: f64,
        frequency_hz: f64,
    ) -> f64 {
        let lb = LinkBudget::evaluate(
            tx_power_dbm,
            tx_antenna.gain_dbi,
            rx_antenna.gain_dbi,
            distance_m,
            frequency_hz,
            0.0, // sensitivity folded in by the caller
        );
        lb.rx_sensitivity_dbm + lb.margin_db + self.evaluate().gain_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_receiver_gain() {
        let chain = LinkChain::typical_receiver(20.0, 1.0);
        let r = chain.evaluate();
        // Gain: −1 + 20 − 7 = 12 dB
        assert!((r.gain_db - 12.0).abs() < 1e-9);
        // NF: Friis — dominated by the LNA
        assert!((1.0..2.5).contains(&r.nf_db), "nf {}", r.nf_db);
    }

    #[test]
    fn lossy_filter_before_lna_degrades_nf_exactly() {
        let mut chain = LinkChain::new();
        chain.push(Stage::Filter {
            insertion_loss_db: 3.0,
        });
        chain.push(Stage::Amplifier {
            gain_db: 20.0,
            nf_db: 1.0,
            oip3_dbm: 30.0,
        });
        let r = chain.evaluate();
        // Friis: F = 2 + (1.2589 − 1)/G1 with G1 = 0.5 (the 3 dB loss)
        // → F = 2.518 → 4.01 dB. The passive loss counts with full weight.
        assert!((r.nf_db - 4.011).abs() < 0.05, "nf {}", r.nf_db);
    }

    #[test]
    fn stronger_lna_improves_nf_but_hurts_iip3() {
        let low = LinkChain::typical_receiver(12.0, 1.0).evaluate();
        let high = LinkChain::typical_receiver(25.0, 1.0).evaluate();
        assert!(high.nf_db < low.nf_db);
        assert!(high.iip3_dbm < low.iip3_dbm);
        // Both report a finite SFDR
        assert!(low.sfdr_db.is_finite() && high.sfdr_db.is_finite());
    }

    #[test]
    fn received_power_budget() {
        let tx = Antenna::design(tpt_elec_rf_antenna::AntennaType::Isotropic, 2.4e9);
        let rx = Antenna::design(tpt_elec_rf_antenna::AntennaType::Isotropic, 2.4e9);
        let chain = LinkChain::new();
        let p = chain.received_power_dbm(&tx, &rx, 0.0, 10.0, 2.4e9);
        // FSPL(10 m, 2.4 GHz) = 60.05 dB → P_rx = −60.05 dBm
        assert!((p + 60.05).abs() < 0.1, "p = {p}");
    }
}
