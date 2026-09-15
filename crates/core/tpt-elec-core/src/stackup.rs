// SPDX-License-Identifier: MIT OR Apache-2.0

//! PCB layer stackup model.

use crate::ids::{LayerId, MaterialId};
use crate::units::{Length, Point2};

/// A full PCB layer stackup.
#[derive(Clone, Debug, Default)]
pub struct Stackup {
    /// Layers from top (silkscreen) to bottom.
    pub layers: Vec<Layer>,
    /// Total board thickness.
    pub total_thickness: Length,
    /// Board outline polygon (meters).
    pub outline: Vec<Point2>,
}

impl Stackup {
    /// Creates a stackup from layers; total thickness is derived.
    pub fn from_layers(layers: Vec<Layer>) -> Self {
        let total = layers.iter().map(|l| l.thickness).sum();
        Self {
            layers,
            total_thickness: total,
            outline: Vec::new(),
        }
    }

    /// Returns only electrically significant layers (signal/plane/dielectric).
    pub fn electrical_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(|l| {
            matches!(
                l.layer_type,
                LayerType::Signal | LayerType::Plane | LayerType::Dielectric
            )
        })
    }

    /// Number of copper layers (signal + plane).
    pub fn copper_layer_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|l| matches!(l.layer_type, LayerType::Signal | LayerType::Plane))
            .count()
    }

    /// Total copper thickness [m].
    pub fn total_copper_thickness(&self) -> Length {
        self.layers
            .iter()
            .filter(|l| matches!(l.layer_type, LayerType::Signal | LayerType::Plane))
            .map(|l| l.thickness)
            .sum()
    }

    /// The z-position of the top surface of layer `idx` within the stack [m].
    pub fn z_top_of(&self, idx: usize) -> f64 {
        self.layers[..idx]
            .iter()
            .map(|l| l.thickness.as_meters())
            .sum()
    }

    /// The z-position of the bottom surface of layer `idx` [m].
    pub fn z_bottom_of(&self, idx: usize) -> f64 {
        self.z_top_of(idx) + self.layers[idx].thickness.as_meters()
    }

    /// Convenience: a typical 4-layer FR4 stackup.
    ///
    /// 1 oz outer copper, 0.5 oz inner, 0.2 mm prepreg, 1.0 mm core.
    pub fn four_layer_fr4() -> Self {
        let layers = vec![
            Layer {
                id: LayerId::new(0),
                name: "top".into(),
                layer_type: LayerType::Signal,
                thickness: CopperWeight::OneOz.thickness_m(),
                material: MaterialId::new("copper"),
                copper_weight: Some(CopperWeight::OneOz),
                order: 0,
            },
            Layer {
                id: LayerId::new(1),
                name: "prepreg".into(),
                layer_type: LayerType::Dielectric,
                thickness: Length::mm(0.2),
                material: MaterialId::new("fr4"),
                copper_weight: None,
                order: 1,
            },
            Layer {
                id: LayerId::new(2),
                name: "in1".into(),
                layer_type: LayerType::Plane,
                thickness: CopperWeight::HalfOz.thickness_m(),
                material: MaterialId::new("copper"),
                copper_weight: Some(CopperWeight::HalfOz),
                order: 2,
            },
            Layer {
                id: LayerId::new(3),
                name: "core".into(),
                layer_type: LayerType::Dielectric,
                thickness: Length::mm(1.0),
                material: MaterialId::new("fr4"),
                copper_weight: None,
                order: 3,
            },
            Layer {
                id: LayerId::new(4),
                name: "in2".into(),
                layer_type: LayerType::Plane,
                thickness: CopperWeight::HalfOz.thickness_m(),
                material: MaterialId::new("copper"),
                copper_weight: Some(CopperWeight::HalfOz),
                order: 4,
            },
            Layer {
                id: LayerId::new(5),
                name: "prepreg2".into(),
                layer_type: LayerType::Dielectric,
                thickness: Length::mm(0.2),
                material: MaterialId::new("fr4"),
                copper_weight: None,
                order: 5,
            },
            Layer {
                id: LayerId::new(6),
                name: "bottom".into(),
                layer_type: LayerType::Signal,
                thickness: CopperWeight::OneOz.thickness_m(),
                material: MaterialId::new("copper"),
                copper_weight: Some(CopperWeight::OneOz),
                order: 6,
            },
        ];
        Self::from_layers(layers)
    }
}

/// One layer of the board stackup.
#[derive(Clone, Debug)]
pub struct Layer {
    /// Layer identifier.
    pub id: LayerId,
    /// Human-readable name.
    pub name: String,
    /// Functional type.
    pub layer_type: LayerType,
    /// Thickness [m].
    pub thickness: Length,
    /// Material reference.
    pub material: MaterialId,
    /// Copper weight for copper layers.
    pub copper_weight: Option<CopperWeight>,
    /// Stacking order (0 = top).
    pub order: u32,
}

/// Functional layer classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerType {
    /// Routing layer.
    Signal,
    /// Power/ground pour.
    Plane,
    /// Insulating substrate/prepreg.
    Dielectric,
    /// Top solder mask.
    SolderMaskTop,
    /// Bottom solder mask.
    SolderMaskBottom,
    /// Top silkscreen.
    SilkscreenTop,
    /// Bottom silkscreen.
    SilkscreenBottom,
    /// Surface finish (ENIG, HASL, …).
    SurfaceFinish,
}

impl LayerType {
    /// Whether this layer conducts.
    pub fn is_conductive(&self) -> bool {
        matches!(
            self,
            LayerType::Signal | LayerType::Plane | LayerType::SurfaceFinish
        )
    }
}

/// Copper foil weight; thickness per IPC-4562 nominal values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CopperWeight {
    /// 0.5 oz/ft² ≈ 17.5 µm.
    HalfOz,
    /// 1 oz/ft² ≈ 35 µm.
    OneOz,
    /// 2 oz/ft² ≈ 70 µm.
    TwoOz,
    /// 3 oz/ft² ≈ 105 µm.
    ThreeOz,
    /// Arbitrary foil thickness in µm.
    Custom(f64),
}

impl CopperWeight {
    /// Nominal foil thickness [m].
    pub fn thickness_m(&self) -> Length {
        let um = match self {
            CopperWeight::HalfOz => 17.5,
            CopperWeight::OneOz => 35.0,
            CopperWeight::TwoOz => 70.0,
            CopperWeight::ThreeOz => 105.0,
            CopperWeight::Custom(um) => *um,
        };
        Length::um(um)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_weight_thicknesses() {
        assert!((CopperWeight::OneOz.thickness_m().as_um() - 35.0).abs() < 1e-9);
        assert!((CopperWeight::HalfOz.thickness_m().as_um() - 17.5).abs() < 1e-9);
        assert!((CopperWeight::Custom(50.0).thickness_m().as_um() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn four_layer_stackup_geometry() {
        let s = Stackup::four_layer_fr4();
        assert_eq!(s.copper_layer_count(), 4);
        assert_eq!(s.electrical_layers().count(), 7);
        // 35 + 200 + 17.5 + 1000 + 17.5 + 200 + 35 µm
        let expected_mm = 1.505;
        assert!((s.total_thickness.as_mm() - expected_mm).abs() < 1e-6);
        assert!((s.total_copper_thickness().as_um() - 105.0).abs() < 1e-6);
        // bottom of top copper sits exactly at copper thickness
        assert!((s.z_bottom_of(0) - 35e-6).abs() < 1e-12);
    }

    #[test]
    fn layer_type_conductive() {
        assert!(LayerType::Signal.is_conductive());
        assert!(!LayerType::Dielectric.is_conductive());
        assert!(!LayerType::SolderMaskTop.is_conductive());
    }
}
