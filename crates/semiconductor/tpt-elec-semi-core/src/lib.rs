// SPDX-License-Identifier: MIT OR Apache-2.0

//! Semiconductor fundamentals: material parameters and doping profiles.
//!
//! [`SemiMaterial`] carries bandgap and effective density-of-states values
//! for the common families; [`Semiconductor::intrinsic_carrier_density`]
//! evaluates `ni = √(Nc·Nv)·e^(−Eg/2kT)`; [`DopingProfile`] covers the
//! uniform/Gaussian/exponential profiles used by compact models.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;
use tpt_elec_spice_models::{BOLTZMANN, ELEMENTARY_CHARGE};

/// Semiconductor material parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum SemiMaterial {
    /// Silicon.
    Silicon,
    /// 4H silicon carbide (polytype noted).
    SiliconCarbide {
        /// Polytype ("4H", "6H").
        polytype: String,
    },
    /// Gallium nitride.
    GalliumNitride,
    /// Gallium arsenide.
    GalliumArsenide,
    /// Germanium.
    Germanium,
}

impl SemiMaterial {
    /// Bandgap at 300 K [eV].
    pub fn bandgap_ev(&self) -> f64 {
        match self {
            SemiMaterial::Silicon => 1.12,
            SemiMaterial::SiliconCarbide { .. } => 3.26,
            SemiMaterial::GalliumNitride => 3.39,
            SemiMaterial::GalliumArsenide => 1.42,
            SemiMaterial::Germanium => 0.66,
        }
    }

    /// Effective density of states, conduction band [cm⁻³] at 300 K.
    pub fn nc_cm3(&self) -> f64 {
        match self {
            SemiMaterial::Silicon => 2.8e19,
            SemiMaterial::SiliconCarbide { .. } => 1.7e19,
            SemiMaterial::GalliumNitride => 2.2e18,
            SemiMaterial::GalliumArsenide => 4.7e17,
            SemiMaterial::Germanium => 1.0e19,
        }
    }

    /// Effective density of states, valence band [cm⁻³] at 300 K.
    pub fn nv_cm3(&self) -> f64 {
        match self {
            SemiMaterial::Silicon => 1.04e19,
            SemiMaterial::SiliconCarbide { .. } => 2.5e19,
            SemiMaterial::GalliumNitride => 4.6e19,
            SemiMaterial::GalliumArsenide => 7.0e18,
            SemiMaterial::Germanium => 6.0e18,
        }
    }

    /// Relative permittivity.
    pub fn relative_permittivity(&self) -> f64 {
        match self {
            SemiMaterial::Silicon => 11.7,
            SemiMaterial::SiliconCarbide { .. } => 9.7,
            SemiMaterial::GalliumNitride => 9.0,
            SemiMaterial::GalliumArsenide => 12.9,
            SemiMaterial::Germanium => 16.0,
        }
    }
}

/// Dopant type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DopingType {
    /// n-type donors.
    Donor,
    /// p-type acceptors.
    Acceptor,
}

/// A doping profile.
#[derive(Clone, Debug, PartialEq)]
pub enum DopingProfile {
    /// Constant concentration [cm⁻³].
    Uniform {
        /// Concentration.
        concentration: f64,
    },
    /// Gaussian: peak at the surface decaying to `depth` [cm⁻³, m].
    Gaussian {
        /// Surface concentration [cm⁻³].
        peak: f64,
        /// Junction/characteristic depth [m].
        depth: Length,
    },
    /// Exponential decay from the surface.
    Exponential {
        /// Surface concentration [cm⁻³].
        surface: f64,
        /// Decay length [m].
        decay_length: Length,
    },
}

impl DopingProfile {
    /// Concentration [cm⁻³] at depth `x` [m].
    pub fn concentration_at(&self, x: f64) -> f64 {
        match self {
            DopingProfile::Uniform { concentration } => *concentration,
            DopingProfile::Gaussian { peak, depth } => {
                let sigma = depth.as_meters() / 2.5;
                peak * (-(x * x) / (2.0 * sigma * sigma)).exp()
            }
            DopingProfile::Exponential {
                surface,
                decay_length,
            } => surface * (-x / decay_length.as_meters().max(1e-15)).exp(),
        }
    }
}

/// A doped semiconductor region.
#[derive(Clone, Debug)]
pub struct Semiconductor {
    /// Material.
    pub material: SemiMaterial,
    /// Doping type (for uniform profiles).
    pub doping_type: DopingType,
    /// Doping profile.
    pub doping: DopingProfile,
    /// Temperature [K].
    pub temperature: f64,
}

impl Semiconductor {
    /// Builds a uniformly doped region.
    pub fn uniform(material: SemiMaterial, doping_type: DopingType, concentration: f64) -> Self {
        Self {
            material,
            doping_type,
            doping: DopingProfile::Uniform { concentration },
            temperature: 300.0,
        }
    }

    /// Intrinsic carrier density `ni = √(Nc·Nv)·e^(−Eg/2kT)` [cm⁻³].
    pub fn intrinsic_carrier_density(&self) -> f64 {
        let m = &self.material;
        (m.nc_cm3() * m.nv_cm3()).sqrt()
            * (-(m.bandgap_ev() * ELEMENTARY_CHARGE) / (2.0 * BOLTZMANN * self.temperature)).exp()
    }

    /// Bulk majority carrier density [cm⁻³] at the surface.
    pub fn majority_carrier_density(&self) -> f64 {
        let d = self.doping.concentration_at(0.0);
        let ni = self.intrinsic_carrier_density();
        match self.doping_type {
            // n ≈ ND/2 + sqrt((ND/2)² + ni²) (exact-ish for all ranges)
            DopingType::Donor => d / 2.0 + (d * d / 4.0 + ni * ni).sqrt(),
            DopingType::Acceptor => d / 2.0 + (d * d / 4.0 + ni * ni).sqrt(),
        }
    }

    /// Built-in potential of a junction against intrinsic material [V]:
    /// `Vbi = (kT/q)·ln(N·N/(ni²))` using surface concentrations.
    pub fn built_in_potential(&self, other: &Semiconductor) -> f64 {
        let ni = self.intrinsic_carrier_density();
        let n1 = self.majority_carrier_density();
        let n2 = other.majority_carrier_density();
        BOLTZMANN * self.temperature / ELEMENTARY_CHARGE * (n1 * n2 / (ni * ni)).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silicon_intrinsic_density() {
        let si = Semiconductor::uniform(SemiMaterial::Silicon, DopingType::Donor, 1e16);
        // ni(Si, 300 K) from Eg = 1.12 eV: ≈ 6.7 × 10^9 cm⁻³
        // (textbooks quote ~1e10 with slightly different parameter sets)
        let ni = si.intrinsic_carrier_density();
        assert!((0.4e10..2.0e10).contains(&ni), "ni = {ni}");
    }

    #[test]
    fn wide_bandgap_materials_have_tiny_ni() {
        let si = Semiconductor::uniform(SemiMaterial::Silicon, DopingType::Donor, 1e15);
        let sic = Semiconductor::uniform(
            SemiMaterial::SiliconCarbide {
                polytype: "4H".into(),
            },
            DopingType::Donor,
            1e15,
        );
        assert!(sic.intrinsic_carrier_density() < si.intrinsic_carrier_density() / 1e6);
    }

    #[test]
    fn majority_density_doping_dominates() {
        let si = Semiconductor::uniform(SemiMaterial::Silicon, DopingType::Donor, 1e17);
        let n = si.majority_carrier_density();
        assert!((n - 1e17).abs() / 1e17 < 1e-6);
    }

    #[test]
    fn doping_profiles() {
        let uniform = DopingProfile::Uniform {
            concentration: 1e16,
        };
        assert_eq!(uniform.concentration_at(0.0), 1e16);
        assert_eq!(uniform.concentration_at(10e-6), 1e16);

        let gauss = DopingProfile::Gaussian {
            peak: 1e20,
            depth: Length::um(1.0),
        };
        assert!((gauss.concentration_at(0.0) - 1e20).abs() < 1e-9);
        assert!(gauss.concentration_at(2e-6) < 1e15); // ~6 decades of decay

        let exp = DopingProfile::Exponential {
            surface: 1e18,
            decay_length: Length::um(0.5),
        };
        assert!((exp.concentration_at(0.5e-6) - 1e18 * std::f64::consts::E.powi(-1)).abs() < 1e15);
    }

    #[test]
    fn built_in_potential_of_pn_junction() {
        let n_side = Semiconductor::uniform(SemiMaterial::Silicon, DopingType::Donor, 1e17);
        let p_side = Semiconductor::uniform(SemiMaterial::Silicon, DopingType::Acceptor, 1e16);
        let vbi = n_side.built_in_potential(&p_side);
        // Classic textbook: ~0.7–0.85 V for these dopings
        assert!((0.6..0.95).contains(&vbi), "Vbi = {vbi}");
    }

    #[test]
    fn permittivity_table() {
        assert!((SemiMaterial::Silicon.relative_permittivity() - 11.7).abs() < 1e-9);
        assert!(
            SemiMaterial::Germanium.relative_permittivity()
                > SemiMaterial::Silicon.relative_permittivity()
        );
    }
}
