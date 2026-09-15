// SPDX-License-Identifier: MIT OR Apache-2.0

//! Material property database for electronics simulation.
//!
//! [`MaterialDatabase::standard`] provides the built-in library (copper, FR4,
//! aluminum, SAC305 solder, silicon, AlN, …). All thermal conductivities are
//! SI (W/(m·K)) and may be isotropic, orthotropic, or full tensors — PCB
//! substrates conduct differently in-plane versus through-thickness.
//!
//! Property values are typical datasheet/IPC figures for simulation, not
//! guaranteed min/max spec limits.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;

use tpt_elec_core::MaterialId;

/// Thermal conductivity model.
#[derive(Clone, Debug, PartialEq)]
pub enum ThermalConductivity {
    /// Same in every direction [W/(m·K)].
    Isotropic(f64),
    /// Orthotropic: distinct x/y/z values [W/(m·K)].
    Anisotropic {
        /// X conductivity.
        kx: f64,
        /// Y conductivity.
        ky: f64,
        /// Z conductivity.
        kz: f64,
    },
    /// Full 3×3 tensor [W/(m·K)].
    Tensor([[f64; 3]; 3]),
}

/// Conductivity axes for orthotropic lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// In-plane X.
    X,
    /// In-plane Y.
    Y,
    /// Through-thickness Z.
    Z,
}

impl ThermalConductivity {
    /// Conductivity along one axis [W/(m·K)].
    pub fn along(&self, axis: Axis) -> f64 {
        match self {
            ThermalConductivity::Isotropic(k) => *k,
            ThermalConductivity::Anisotropic { kx, ky, kz } => match axis {
                Axis::X => *kx,
                Axis::Y => *ky,
                Axis::Z => *kz,
            },
            ThermalConductivity::Tensor(t) => match axis {
                Axis::X => t[0][0],
                Axis::Y => t[1][1],
                Axis::Z => t[2][2],
            },
        }
    }

    /// Effective scalar conductivity (mean of the diagonal).
    pub fn effective(&self) -> f64 {
        (self.along(Axis::X) + self.along(Axis::Y) + self.along(Axis::Z)) / 3.0
    }
}

/// Coefficient of thermal expansion per axis [ppm/K].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cte {
    /// X direction.
    pub x: f64,
    /// Y direction.
    pub y: f64,
    /// Z direction.
    pub z: f64,
}

/// Coarse material classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialCategory {
    /// Copper and copper alloys.
    Copper,
    /// Aluminum and alloys.
    Aluminum,
    /// FR-4 glass epoxy.
    Fr4,
    /// Ceramic-filled PTFE / hydrocarbon laminates.
    Rogers,
    /// Polyimide (flex).
    Polyimide,
    /// Liquid photoimageable solder mask.
    SolderMask,
    /// Solder alloys.
    Solder,
    /// Thermal interface materials.
    ThermalInterface,
    /// Ceramics (alumina, AlN, …).
    Ceramic,
    /// Silicon.
    Silicon,
    /// Air / trapped gas.
    Air,
    /// Mold compound.
    MoldCompound,
}

/// A material with (optionally temperature dependent) properties.
#[derive(Clone, Debug)]
pub struct Material {
    /// Stable identifier (`"copper"`, `"fr4"`, …).
    pub id: MaterialId,
    /// Human-readable name.
    pub name: String,
    /// Classification.
    pub category: MaterialCategory,
    /// Thermal conductivity [W/(m·K)].
    pub thermal_conductivity: ThermalConductivity,
    /// Electrical resistivity at 20 °C [Ω·m].
    pub electrical_resistivity: f64,
    /// Temperature coefficient of resistivity [1/K] (α in ρ = ρ₀(1 + α·ΔT)).
    pub temperature_coefficient: f64,
    /// Specific heat capacity [J/(kg·K)].
    pub specific_heat: f64,
    /// Density [kg/m³].
    pub density: f64,
    /// Coefficient of thermal expansion.
    pub cte: Cte,
    /// Maximum recommended operating temperature [°C].
    pub max_operating_temp: f64,
    /// Relative permittivity at 1 GHz (dielectrics; `None` for conductors).
    pub dielectric_constant: Option<f64>,
    /// Loss tangent at 1 GHz (dielectrics).
    pub loss_tangent: Option<f64>,
}

impl Material {
    /// Electrical conductivity at temperature `t_c` [°C] [S/m].
    pub fn conductivity_at(&self, t_c: f64) -> f64 {
        1.0 / self.resistivity_at(t_c)
    }

    /// Electrical resistivity at temperature `t_c` [°C] [Ω·m]:
    /// ρ(T) = ρ₀ · (1 + α·(T − 20)).
    pub fn resistivity_at(&self, t_c: f64) -> f64 {
        self.electrical_resistivity * (1.0 + self.temperature_coefficient * (t_c - 20.0))
    }
}

/// A searchable library of materials.
#[derive(Clone, Debug, Default)]
pub struct MaterialDatabase {
    materials: HashMap<MaterialId, Material>,
}

impl MaterialDatabase {
    /// An empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts (or replaces) a material.
    pub fn insert(&mut self, material: Material) {
        self.materials.insert(material.id.clone(), material);
    }

    /// Looks a material up by id.
    pub fn get(&self, id: &MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    /// Number of materials.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Iterates over all materials.
    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.materials.values()
    }

    /// Builds the standard built-in library.
    pub fn standard() -> Self {
        let mut db = Self::new();
        db.insert(Material {
            id: MaterialId::new("copper"),
            name: "Electrolytic copper (C11000)".into(),
            category: MaterialCategory::Copper,
            thermal_conductivity: ThermalConductivity::Isotropic(385.0),
            electrical_resistivity: 1.68e-8,
            temperature_coefficient: 0.00393,
            specific_heat: 385.0,
            density: 8960.0,
            cte: Cte {
                x: 17.0,
                y: 17.0,
                z: 17.0,
            },
            max_operating_temp: 200.0,
            dielectric_constant: None,
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("fr4"),
            name: "FR-4 glass epoxy".into(),
            category: MaterialCategory::Fr4,
            thermal_conductivity: ThermalConductivity::Anisotropic {
                kx: 0.6,
                ky: 0.6,
                kz: 0.3,
            },
            electrical_resistivity: 1.0e12,
            temperature_coefficient: 0.0,
            specific_heat: 600.0,
            density: 1900.0,
            cte: Cte {
                x: 14.0,
                y: 14.0,
                z: 50.0,
            },
            max_operating_temp: 130.0,
            dielectric_constant: Some(4.4),
            loss_tangent: Some(0.02),
        });
        db.insert(Material {
            id: MaterialId::new("aluminum"),
            name: "Aluminum 6061".into(),
            category: MaterialCategory::Aluminum,
            thermal_conductivity: ThermalConductivity::Isotropic(205.0),
            electrical_resistivity: 2.65e-8,
            temperature_coefficient: 0.00429,
            specific_heat: 900.0,
            density: 2700.0,
            cte: Cte {
                x: 23.6,
                y: 23.6,
                z: 23.6,
            },
            max_operating_temp: 200.0,
            dielectric_constant: None,
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("sac305"),
            name: "SAC305 solder (Sn-3.0Ag-0.5Cu)".into(),
            category: MaterialCategory::Solder,
            thermal_conductivity: ThermalConductivity::Isotropic(50.0),
            electrical_resistivity: 1.1e-7,
            temperature_coefficient: 0.0037,
            specific_heat: 220.0,
            density: 7400.0,
            cte: Cte {
                x: 21.7,
                y: 21.7,
                z: 21.7,
            },
            max_operating_temp: 120.0,
            dielectric_constant: None,
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("silicon"),
            name: "Silicon".into(),
            category: MaterialCategory::Silicon,
            thermal_conductivity: ThermalConductivity::Isotropic(148.0),
            electrical_resistivity: 2.3e3,
            temperature_coefficient: -0.007,
            specific_heat: 700.0,
            density: 2330.0,
            cte: Cte {
                x: 2.6,
                y: 2.6,
                z: 2.6,
            },
            max_operating_temp: 200.0,
            dielectric_constant: Some(11.7),
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("aln"),
            name: "Aluminum nitride".into(),
            category: MaterialCategory::Ceramic,
            thermal_conductivity: ThermalConductivity::Isotropic(170.0),
            electrical_resistivity: 1.0e13,
            temperature_coefficient: 0.0,
            specific_heat: 740.0,
            density: 3260.0,
            cte: Cte {
                x: 4.5,
                y: 4.5,
                z: 4.5,
            },
            max_operating_temp: 800.0,
            dielectric_constant: Some(8.8),
            loss_tangent: Some(0.001),
        });
        db.insert(Material {
            id: MaterialId::new("alumina"),
            name: "96% alumina (Al2O3)".into(),
            category: MaterialCategory::Ceramic,
            thermal_conductivity: ThermalConductivity::Isotropic(24.0),
            electrical_resistivity: 1.0e14,
            temperature_coefficient: 0.0,
            specific_heat: 780.0,
            density: 3750.0,
            cte: Cte {
                x: 6.8,
                y: 6.8,
                z: 6.8,
            },
            max_operating_temp: 1000.0,
            dielectric_constant: Some(9.4),
            loss_tangent: Some(0.0004),
        });
        db.insert(Material {
            id: MaterialId::new("solder_mask"),
            name: "Liquid photoimageable solder mask".into(),
            category: MaterialCategory::SolderMask,
            thermal_conductivity: ThermalConductivity::Isotropic(0.25),
            electrical_resistivity: 1.0e12,
            temperature_coefficient: 0.0,
            specific_heat: 1000.0,
            density: 1200.0,
            cte: Cte {
                x: 50.0,
                y: 50.0,
                z: 150.0,
            },
            max_operating_temp: 110.0,
            dielectric_constant: Some(3.5),
            loss_tangent: Some(0.03),
        });
        db.insert(Material {
            id: MaterialId::new("polyimide"),
            name: "Polyimide flex substrate".into(),
            category: MaterialCategory::Polyimide,
            thermal_conductivity: ThermalConductivity::Isotropic(0.15),
            electrical_resistivity: 1.0e14,
            temperature_coefficient: 0.0,
            specific_heat: 1100.0,
            density: 1420.0,
            cte: Cte {
                x: 16.0,
                y: 16.0,
                z: 60.0,
            },
            max_operating_temp: 200.0,
            dielectric_constant: Some(3.4),
            loss_tangent: Some(0.02),
        });
        db.insert(Material {
            id: MaterialId::new("rogers4350b"),
            name: "Rogers RO4350B".into(),
            category: MaterialCategory::Rogers,
            thermal_conductivity: ThermalConductivity::Isotropic(0.66),
            electrical_resistivity: 1.0e12,
            temperature_coefficient: 0.0,
            specific_heat: 900.0,
            density: 2100.0,
            cte: Cte {
                x: 10.0,
                y: 10.0,
                z: 30.0,
            },
            max_operating_temp: 280.0,
            dielectric_constant: Some(3.48),
            loss_tangent: Some(0.0037),
        });
        db.insert(Material {
            id: MaterialId::new("air"),
            name: "Air (1 atm, 20 °C)".into(),
            category: MaterialCategory::Air,
            thermal_conductivity: ThermalConductivity::Isotropic(0.026),
            electrical_resistivity: 1.0e14,
            temperature_coefficient: 0.0,
            specific_heat: 1005.0,
            density: 1.204,
            cte: Cte {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            max_operating_temp: 1000.0,
            dielectric_constant: Some(1.0),
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("tim"),
            name: "Generic thermal interface material".into(),
            category: MaterialCategory::ThermalInterface,
            thermal_conductivity: ThermalConductivity::Isotropic(3.0),
            electrical_resistivity: 1.0e12,
            temperature_coefficient: 0.0,
            specific_heat: 1000.0,
            density: 2500.0,
            cte: Cte {
                x: 30.0,
                y: 30.0,
                z: 30.0,
            },
            max_operating_temp: 150.0,
            dielectric_constant: None,
            loss_tangent: None,
        });
        db.insert(Material {
            id: MaterialId::new("mold_compound"),
            name: "Epoxy mold compound".into(),
            category: MaterialCategory::MoldCompound,
            thermal_conductivity: ThermalConductivity::Isotropic(0.8),
            electrical_resistivity: 1.0e12,
            temperature_coefficient: 0.0,
            specific_heat: 900.0,
            density: 1900.0,
            cte: Cte {
                x: 12.0,
                y: 12.0,
                z: 40.0,
            },
            max_operating_temp: 175.0,
            dielectric_constant: Some(3.7),
            loss_tangent: Some(0.01),
        });
        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_database_contains_builtins() {
        let db = MaterialDatabase::standard();
        assert!(db.len() >= 13);
        let cu = db.get(&MaterialId::new("copper")).unwrap();
        assert_eq!(cu.category, MaterialCategory::Copper);
        assert!((cu.thermal_conductivity.along(Axis::Z) - 385.0).abs() < 1e-9);
        let fr4 = db.get(&MaterialId::new("fr4")).unwrap();
        assert_eq!(fr4.category, MaterialCategory::Fr4);
    }

    #[test]
    fn fr4_is_anisotropic() {
        let db = MaterialDatabase::standard();
        let fr4 = db.get(&MaterialId::new("fr4")).unwrap();
        match &fr4.thermal_conductivity {
            ThermalConductivity::Anisotropic { kx, ky, kz } => {
                assert!((kx - 0.6).abs() < 1e-9);
                assert!((ky - 0.6).abs() < 1e-9);
                assert!((kz - 0.3).abs() < 1e-9);
            }
            other => panic!("FR4 should be anisotropic, got {other:?}"),
        }
        // In-plane conducts ~2x better than through-thickness.
        assert!(fr4.thermal_conductivity.along(Axis::X) > fr4.thermal_conductivity.along(Axis::Z));
    }

    #[test]
    fn tensor_axis_lookup() {
        let k = ThermalConductivity::Tensor([[1.0, 0.1, 0.0], [0.1, 2.0, 0.0], [0.0, 0.0, 3.0]]);
        assert!((k.along(Axis::Z) - 3.0).abs() < 1e-12);
        assert!((k.effective() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn copper_resistivity_temperature_dependence() {
        let db = MaterialDatabase::standard();
        let cu = db.get(&MaterialId::new("copper")).unwrap();
        // ρ(20°C) = 1.68e-8 Ω·m
        assert!((cu.resistivity_at(20.0) - 1.68e-8).abs() < 1e-12);
        // ρ(120°C) = ρ₀ (1 + 0.00393 · 100)
        let expected = 1.68e-8 * (1.0 + 0.00393 * 100.0);
        assert!((cu.resistivity_at(120.0) - expected).abs() < 1e-14);
        // Conductivity is the reciprocal.
        assert!((cu.conductivity_at(20.0) - 1.0 / 1.68e-8).abs() < 1.0);
    }

    #[test]
    fn insert_and_lookup_custom() {
        let mut db = MaterialDatabase::new();
        assert!(db.is_empty());
        db.insert(Material {
            id: MaterialId::new("peek"),
            name: "PEEK".into(),
            category: MaterialCategory::Polyimide,
            thermal_conductivity: ThermalConductivity::Isotropic(0.25),
            electrical_resistivity: 1e15,
            temperature_coefficient: 0.0,
            specific_heat: 1300.0,
            density: 1300.0,
            cte: Cte::default(),
            max_operating_temp: 250.0,
            dielectric_constant: Some(3.2),
            loss_tangent: Some(0.003),
        });
        assert_eq!(db.len(), 1);
        assert!(db.get(&MaterialId::new("peek")).is_some());
        assert_eq!(db.iter().count(), 1);
    }

    #[test]
    fn dielectric_properties_present() {
        let db = MaterialDatabase::standard();
        let rogers = db.get(&MaterialId::new("rogers4350b")).unwrap();
        assert!((rogers.dielectric_constant.unwrap() - 3.48).abs() < 1e-9);
        let cu = db.get(&MaterialId::new("copper")).unwrap();
        assert!(cu.dielectric_constant.is_none());
    }
}
