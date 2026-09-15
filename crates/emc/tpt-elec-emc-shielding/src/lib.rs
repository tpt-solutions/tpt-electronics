// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shielding effectiveness for plane waves: `SE = R + A + B` [dB].
//!
//! * A (absorption) = 131.4·t·√(f·µr·σr) with t in meters, f in Hz.
//! * R (reflection, plane wave) = 168 − 10·log10(f·µr/σr).
//! * B (multiple-reflection correction, thin shields) = 20·log10|1 − e^(−2t/δ)|
//!   and is negligible for A > 15 dB.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;

/// Shield materials with relative conductivity (vs copper) and permeability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShieldMaterial {
    /// Copper: σr = 1, µr = 1.
    Copper,
    /// Aluminum: σr ≈ 0.61, µr = 1.
    Aluminum,
    /// Steel: σr ≈ 0.1, µr ≈ 100 (alloy dependent).
    Steel,
    /// Mu-metal: σr ≈ 0.03, µr ≈ 20 000 (low-field).
    MuMetal,
    /// Spray/conductive paint.
    ConductivePaint {
        /// Relative conductivity.
        conductivity: f64,
    },
    /// Fully custom material.
    Custom {
        /// Relative conductivity.
        conductivity: f64,
        /// Relative permeability.
        permeability: f64,
    },
}

impl ShieldMaterial {
    /// (σr, µr).
    pub fn properties(&self) -> (f64, f64) {
        match self {
            ShieldMaterial::Copper => (1.0, 1.0),
            ShieldMaterial::Aluminum => (0.61, 1.0),
            ShieldMaterial::Steel => (0.1, 100.0),
            ShieldMaterial::MuMetal => (0.03, 20_000.0),
            ShieldMaterial::ConductivePaint { conductivity } => (*conductivity, 1.0),
            ShieldMaterial::Custom {
                conductivity,
                permeability,
            } => (*conductivity, *permeability),
        }
    }
}

/// Shielding effectiveness breakdown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShieldingEffectiveness {
    /// Reflection loss [dB].
    pub reflection_db: f64,
    /// Absorption loss [dB].
    pub absorption_db: f64,
    /// Multiple-reflection correction [dB].
    pub multiple_reflection_db: f64,
}

impl ShieldingEffectiveness {
    /// Total SE [dB].
    pub fn total_db(&self) -> f64 {
        self.reflection_db + self.absorption_db + self.multiple_reflection_db
    }
}

/// Shielding calculations.
pub struct ShieldingCalculator;

impl ShieldingCalculator {
    /// Skin depth δ = 1/√(π·f·µ·σ) [m].
    pub fn skin_depth(material: ShieldMaterial, frequency: f64) -> f64 {
        let (sigma_r, mu_r) = material.properties();
        let mu = mu_r * 4.0e-7 * std::f64::consts::PI;
        let sigma = sigma_r * 5.8e7;
        1.0 / (std::f64::consts::PI * frequency * mu * sigma).sqrt()
    }

    /// Full plane-wave shielding breakdown.
    pub fn plane_wave_shielding(
        material: ShieldMaterial,
        thickness: Length,
        frequency: f64,
    ) -> ShieldingEffectiveness {
        let (sigma_r, mu_r) = material.properties();
        let t = thickness.as_meters();
        let delta = Self::skin_depth(material, frequency);
        // Absorption: A = 8.69·(t/δ) = 131.4·t·√(f·µr·σr)
        let a = 8.686 * (t / delta).max(0.0);
        // Reflection (plane wave, far field)
        let r = 168.0 - 10.0 * (frequency * mu_r / sigma_r.max(1e-9)).log10();
        // Multiple reflection correction: negligible once A > 15 dB
        let b = if a < 15.0 {
            let x = -2.0 * t / delta;
            20.0 * (1.0 - x.exp()).abs().log10()
        } else {
            0.0
        };
        ShieldingEffectiveness {
            reflection_db: r,
            absorption_db: a,
            multiple_reflection_db: b,
        }
    }

    /// Total shielding effectiveness [dB] (convenience).
    pub fn total_se(material: ShieldMaterial, thickness: Length, frequency: f64) -> f64 {
        Self::plane_wave_shielding(material, thickness, frequency).total_db()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_skin_depth_classic_values() {
        // Copper: 66 µm at 1 MHz, 21 µm at 10 MHz, 6.6 µm at 100 MHz
        let d1 = ShieldingCalculator::skin_depth(ShieldMaterial::Copper, 1e6);
        assert!((d1 - 66e-6).abs() / 66e-6 < 0.05, "{d1}");
        let d2 = ShieldingCalculator::skin_depth(ShieldMaterial::Copper, 1e8);
        assert!((d2 - 6.6e-6).abs() / 6.6e-6 < 0.05, "{d2}");
    }

    #[test]
    fn copper_1mm_plane_wave_near_130db() {
        // Copper 1 mm at 1 MHz: A ≈ 131 dB, R ≈ 108 dB, B ≈ 0 → SE ≈ 239 dB
        let se =
            ShieldingCalculator::plane_wave_shielding(ShieldMaterial::Copper, Length::mm(1.0), 1e6);
        assert!(
            (se.absorption_db - 131.4).abs() < 2.0,
            "A = {}",
            se.absorption_db
        );
        assert!(
            (se.reflection_db - 108.0).abs() < 2.0,
            "R = {}",
            se.reflection_db
        );
        assert!(se.multiple_reflection_db.abs() < 0.1); // A ≫ 15 dB
        assert!(se.total_db() > 200.0);
    }

    #[test]
    fn thin_shield_multiple_reflection_counts() {
        // 0.1 mm copper at 1 MHz: t/δ = 1.5 → A ≈ 13 dB, B applies
        let se =
            ShieldingCalculator::plane_wave_shielding(ShieldMaterial::Copper, Length::mm(0.1), 1e6);
        assert!(se.absorption_db < 15.0);
        assert!(se.multiple_reflection_db < 0.0); // correction is negative
    }

    #[test]
    fn steel_trades_conductivity_for_permeability() {
        // At 10 kHz steel beats copper on absorption (µr ≫ 1)
        let cu = ShieldingCalculator::plane_wave_shielding(
            ShieldMaterial::Copper,
            Length::mm(0.5),
            10e3,
        );
        let st =
            ShieldingCalculator::plane_wave_shielding(ShieldMaterial::Steel, Length::mm(0.5), 10e3);
        assert!(st.absorption_db > cu.absorption_db);
        // But loses on reflection at low frequency
        assert!(st.reflection_db < cu.reflection_db);
    }

    #[test]
    fn mu_metal_shields_magnetics_at_low_frequency() {
        let se = ShieldingCalculator::total_se(ShieldMaterial::MuMetal, Length::mm(1.0), 60.0);
        assert!(se > 20.0, "SE = {se}");
    }

    #[test]
    fn conductive_paint_is_weak() {
        let se = ShieldingCalculator::total_se(
            ShieldMaterial::ConductivePaint { conductivity: 0.01 },
            Length::um(50.0),
            1e9,
        );
        // Real enclosure paints deliver 20–60 dB
        assert!((10.0..80.0).contains(&se), "SE = {se}");
    }
}
