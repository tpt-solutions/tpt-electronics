// SPDX-License-Identifier: MIT OR Apache-2.0

//! Inductors, cores, and magnetic design.
//!
//! * [`CoreLossCalculator::steinmetz`] — classic Steinmetz volumetric core
//!   loss `Pv = k·f^α·B^β` with per-material constants.
//! * [`Inductor::for_inductance`] — turns from the AL value, peak flux from
//!   `B = L·I/(N·Ae)`, saturation margin check.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;

/// Magnetic core shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum CoreType {
    /// Toroid.
    Toroid {
        /// Outer diameter [m].
        od: Length,
        /// Inner diameter [m].
        id: Length,
        /// Height [m].
        height: Length,
    },
    /// E core (size string, e.g. "E25/13/7").
    ECore {
        /// Size designation.
        size: String,
    },
    /// PQ core.
    PQCore {
        /// Size designation.
        size: String,
    },
    /// RM core.
    RMCore {
        /// Size designation.
        size: String,
    },
    /// Custom geometry (caller supplies Ae/Ve directly).
    Custom,
}

/// Magnetic core materials with Steinmetz parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum CoreMaterial {
    /// MnZn/NiZn ferrites (e.g. "3F3", "N87").
    Ferrite {
        /// Grade name.
        material: String,
    },
    /// Iron powder cores.
    IronPowder {
        /// Grade name.
        material: String,
    },
    /// Kool Mu.
    KoolMu,
    /// Molypermalloy (MPP).
    Mpp,
    /// Air core (no core loss).
    Air,
}

impl CoreMaterial {
    /// Steinmetz constants for `Pv = k·f^α·B^β` with f in Hz, B in T,
    /// Pv in W/m³ (typical datasheet-fit values at ~100 °C).
    pub fn steinmetz_constants(&self) -> (f64, f64, f64) {
        match self {
            // Power ferrite: Pv[mW/cm³] ≈ k_f·f^α·B^β with f in kHz, B in mT
            // → converted here to SI (W/m³, Hz, T).
            // 3F3-like: 25e3 mW/cm³ at 300 kHz/100 mT → k_SI ≈ 3.2
            CoreMaterial::Ferrite { .. } => (3.2, 1.5, 2.5),
            CoreMaterial::IronPowder { .. } => (12.0, 1.3, 2.0),
            CoreMaterial::KoolMu => (9.0, 1.35, 2.1),
            CoreMaterial::Mpp => (6.0, 1.4, 2.2),
            CoreMaterial::Air => (0.0, 1.0, 1.0),
        }
    }
}

/// A magnetic core with its geometry numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct MagneticCore {
    /// Shape.
    pub core_type: CoreType,
    /// Material.
    pub material: CoreMaterial,
    /// Inductance factor [nH/turn²].
    pub al_value: f64,
    /// Effective cross-section [m²].
    pub ae: f64,
    /// Effective magnetic path length [m].
    pub le: f64,
    /// Effective volume [m³].
    pub ve: f64,
}

/// Winding description.
#[derive(Clone, Debug, PartialEq)]
pub struct Winding {
    /// Number of turns.
    pub turns: u32,
    /// Conductor diameter [m] (0 for foil/flat).
    pub wire_diameter: Length,
}

/// An inductor built from a core and winding.
#[derive(Clone, Debug, PartialEq)]
pub struct Inductor {
    /// Achieved inductance [H].
    pub inductance: f64,
    /// RMS current rating [A].
    pub current_rating: f64,
    /// Saturation current [A] (at which L drops ~10–30%).
    pub saturation_current: f64,
    /// DC resistance [Ω].
    pub dcr: f64,
    /// Core.
    pub core: MagneticCore,
    /// Winding.
    pub winding: Winding,
}

/// Core loss evaluation.
pub struct CoreLossCalculator;

impl CoreLossCalculator {
    /// Steinmetz volumetric core loss [W/m³].
    ///
    /// * `frequency` — ripple frequency [Hz]
    /// * `b_peak` — peak AC flux density (half of ΔB) [T]
    pub fn steinmetz(frequency: f64, b_peak: f64, material: &CoreMaterial) -> f64 {
        let (k, alpha, beta) = material.steinmetz_constants();
        k * frequency.powf(alpha) * b_peak.powf(beta)
    }

    /// Total core loss [W] over the effective volume.
    pub fn total_loss(frequency: f64, b_peak: f64, core: &MagneticCore) -> f64 {
        Self::steinmetz(frequency, b_peak, &core.material) * core.ve
    }
}

impl Inductor {
    /// Sizes an inductor on a given core.
    ///
    /// Picks the nearest turn count reaching `inductance` from
    /// `L = AL·N²`, computes the peak flux at `peak_current` and the DCR
    /// estimate from the mean turn length and wire diameter.
    pub fn for_inductance(
        core: MagneticCore,
        target_inductance: f64,
        peak_current: f64,
        wire_diameter: Length,
        mean_turn_length: Length,
    ) -> Self {
        let n = ((target_inductance / (core.al_value * 1e-9)).sqrt())
            .ceil()
            .max(1.0) as u32;
        let l = core.al_value * 1e-9 * (n as f64).powi(2);
        // Peak flux: B = L·I/(N·Ae)
        let _b = l * peak_current / (n as f64 * core.ae.max(1e-12));
        // DCR ≈ ρ·len/area (copper)
        let rho = 1.72e-8;
        let area = std::f64::consts::PI * (wire_diameter.as_meters() / 2.0).powi(2);
        let dcr = rho * n as f64 * mean_turn_length.as_meters() / area.max(1e-12);
        // Saturation current: where B reaches ~0.3 T for ferrite-style cores
        let b_sat = 0.3;
        let i_sat = b_sat * n as f64 * core.ae.max(1e-12) / l.max(1e-15);
        Self {
            inductance: l,
            current_rating: i_sat * 0.8,
            saturation_current: i_sat,
            dcr,
            core,
            winding: Winding {
                turns: n,
                wire_diameter,
            },
        }
    }

    /// Peak AC flux density at the given operating point [T].
    pub fn peak_flux(&self, peak_current: f64) -> f64 {
        self.inductance * peak_current / (self.winding.turns as f64 * self.core.ae.max(1e-12))
    }

    /// Whether the inductor saturates at `current`.
    pub fn saturates_at(&self, current: f64) -> bool {
        current > self.saturation_current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e_core() -> MagneticCore {
        MagneticCore {
            core_type: CoreType::ECore { size: "E25".into() },
            material: CoreMaterial::Ferrite {
                material: "3F3".into(),
            },
            al_value: 1800.0, // nH/N²
            ae: 52.0e-6,      // m²
            le: 57.0e-3,      // m
            ve: 3.0e-6,       // m³
        }
    }

    #[test]
    fn steinmetz_ferrite_reference_point() {
        // At 100 kHz, 100 mT: Pv = 3.2·(1e5)^1.5·(0.1)^2.5
        let pv = CoreLossCalculator::steinmetz(
            1e5,
            0.1,
            &CoreMaterial::Ferrite {
                material: "3F3".into(),
            },
        );
        let expected = 3.2 * 1e5f64.powf(1.5) * 0.1f64.powf(2.5);
        assert!((pv - expected).abs() / expected < 1e-12);
        // Order of magnitude: tens of kW/m³ (tens of mW/cm³) ✓
        assert!((1e4..1e6).contains(&pv), "pv = {pv}");
        // Loss grows with frequency and flux
        let pv2 = CoreLossCalculator::steinmetz(
            3e5,
            0.1,
            &CoreMaterial::Ferrite {
                material: "3F3".into(),
            },
        );
        assert!(pv2 > pv);
        // Air core: zero loss
        assert_eq!(
            CoreLossCalculator::steinmetz(1e6, 0.1, &CoreMaterial::Air),
            0.0
        );
    }

    #[test]
    fn inductor_turns_from_al() {
        // L = AL·N²: 1800 nH/N² × 9² = 145.8 µH
        let ind =
            Inductor::for_inductance(e_core(), 100e-6, 2.0, Length::mm(0.5), Length::mm(30.0));
        assert_eq!(ind.winding.turns, 8); // ceil(sqrt(100e-6/1800e-9)) = ceil(7.45) = 8
        assert!((ind.inductance - 1800e-9 * 64.0).abs() < 1e-12);
        assert!(!ind.saturates_at(1.0));
    }

    #[test]
    fn saturation_current_consistent_with_flux() {
        let ind =
            Inductor::for_inductance(e_core(), 100e-6, 2.0, Length::mm(0.5), Length::mm(30.0));
        // At i_sat, B should equal ~0.3 T
        let b = ind.peak_flux(ind.saturation_current);
        assert!((b - 0.3).abs() / 0.3 < 0.05, "B(isat) = {b}");
        assert!(ind.saturates_at(ind.saturation_current * 1.2));
    }

    #[test]
    fn dcr_reasonable() {
        let ind =
            Inductor::for_inductance(e_core(), 100e-6, 2.0, Length::mm(0.5), Length::mm(30.0));
        // 8 turns × 30 mm of 0.5 mm wire: R = 1.72e-8·0.24/1.96e-7 ≈ 21 mΩ
        assert!((0.005..0.1).contains(&ind.dcr), "dcr = {}", ind.dcr);
    }
}
