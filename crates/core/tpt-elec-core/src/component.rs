// SPDX-License-Identifier: MIT OR Apache-2.0

//! Component placement and thermal / electrical component models.

use crate::ids::ComponentId;
use crate::units::{Angle, Length, Point2};

/// A component placed on the board.
#[derive(Clone, Debug)]
pub struct Component {
    /// Unique id (usually the reference designator).
    pub id: ComponentId,
    /// Reference designator (e.g. `"R7"`).
    pub reference: String,
    /// Package type.
    pub package: PackageType,
    /// Placement position (board coordinates, meters).
    pub position: Point2,
    /// Placement rotation.
    pub rotation: Angle,
    /// Which side of the board.
    pub side: BoardSide,
    /// Thermal compact model.
    pub thermal_model: ThermalComponentModel,
    /// Electrical model.
    pub electrical_model: ElectricalModel,
}

impl Component {
    /// Creates a component with default (passive) models.
    pub fn new(id: &str, package: PackageType, position: Point2) -> Self {
        Self {
            id: ComponentId::new(id.to_string()),
            reference: id.to_string(),
            package,
            position,
            rotation: Angle::ZERO,
            side: BoardSide::Top,
            thermal_model: ThermalComponentModel::default(),
            electrical_model: ElectricalModel::default(),
        }
    }
}

/// Which side of the board a component is mounted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardSide {
    /// Component side.
    Top,
    /// Solder side.
    Bottom,
}

/// Common package families relevant to thermal modeling.
#[derive(Clone, Debug, PartialEq)]
pub enum PackageType {
    /// Ball grid array.
    Bga {
        /// Body size [mm].
        body_mm: f64,
        /// Ball count.
        ball_count: u32,
    },
    /// Quad flat no-leads.
    Qfn {
        /// Body size [mm].
        body_mm: f64,
    },
    /// Small outline IC.
    Soic {
        /// Pin count.
        pins: u32,
    },
    /// Quad flat package.
    Tqfp {
        /// Pin count.
        pins: u32,
    },
    /// Small transistor packages.
    Sot23,
    /// Chip passives (metric size, e.g. 0402 → 1.0 x 0.5 mm).
    Chip {
        /// Body length [mm].
        length_mm: f64,
        /// Body width [mm].
        width_mm: f64,
    },
    /// Through-hole DIP.
    Dip {
        /// Pin count.
        pins: u32,
    },
    /// Anything else.
    Custom {
        /// Body X extent [mm].
        body_mm_x: f64,
        /// Body Y extent [mm].
        body_mm_y: f64,
    },
}

impl PackageType {
    /// Approximate footprint bounding box side [m] (square estimate).
    pub fn approx_body_m(&self) -> f64 {
        match self {
            PackageType::Bga { body_mm, .. }
            | PackageType::Qfn { body_mm }
            | PackageType::Custom {
                body_mm_x: body_mm, ..
            } => body_mm * 1e-3,
            PackageType::Soic { pins } => *pins as f64 * 1.27e-3,
            PackageType::Tqfp { pins } => *pins as f64 * 0.5e-3,
            PackageType::Sot23 => 2.9e-3,
            PackageType::Chip { length_mm, .. } => length_mm * 1e-3,
            PackageType::Dip { pins } => (*pins as f64 / 4.0) * 2.54e-3,
        }
    }
}

/// Two-resistor style thermal compact model for a component.
///
/// ΘJA junction-to-ambient, ΘJC junction-to-case, ΘJB junction-to-board,
/// all in °C/W.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThermalComponentModel {
    /// Junction-to-ambient thermal resistance [°C/W].
    pub junction_to_ambient: f64,
    /// Junction-to-case thermal resistance [°C/W].
    pub junction_to_case: f64,
    /// Junction-to-board thermal resistance [°C/W].
    pub junction_to_board: f64,
    /// Power dissipation [W].
    pub power_dissipation: f64,
    /// Exposed thermal pad (drives via arrays).
    pub thermal_pad: Option<ThermalPad>,
}

impl ThermalComponentModel {
    /// Steady-state junction temperature estimate [°C] above ambient when
    /// cooled only through ΘJA.
    pub fn junction_temperature(&self, ambient_c: f64) -> f64 {
        ambient_c + self.power_dissipation * self.junction_to_ambient
    }
}

/// An exposed thermal pad under a package.
#[derive(Clone, Debug, PartialEq)]
pub struct ThermalPad {
    /// Pad width (X) [m].
    pub width: Length,
    /// Pad height (Y) [m].
    pub height: Length,
    /// Optional via array under the pad.
    pub via_array: Option<ViaArray>,
}

impl ThermalPad {
    /// Pad area [m²].
    pub fn area(&self) -> f64 {
        self.width.as_meters() * self.height.as_meters()
    }
}

/// An array of thermal vias under a thermal pad.
#[derive(Clone, Debug, PartialEq)]
pub struct ViaArray {
    /// Rows of vias.
    pub rows: u32,
    /// Columns of vias.
    pub cols: u32,
    /// Center-to-center pitch [m].
    pub pitch: Length,
    /// Drill diameter [m].
    pub drill_diameter: Length,
    /// Copper plating thickness [m].
    pub plating_thickness: Length,
}

impl ViaArray {
    /// Total number of vias.
    pub fn count(&self) -> u32 {
        self.rows * self.cols
    }

    /// Total plated copper cross-section of all vias [m²].
    ///
    /// Annulus area × board span is not known here; this is the single-via
    /// plating cross-section summed over the array.
    pub fn total_plated_area(&self) -> f64 {
        let d = self.drill_diameter.as_meters();
        let annulus = std::f64::consts::PI
            * (((d / 2.0 + self.plating_thickness.as_meters()).powi(2)) - (d / 2.0).powi(2));
        annulus * self.count() as f64
    }
}

/// Simplified electrical model of a component.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElectricalModel {
    /// Terminal-to-terminal resistance [Ω].
    pub resistance_ohm: f64,
    /// Nominal operating current [A].
    pub current_a: f64,
    /// Nominal operating voltage [V].
    pub voltage_v: f64,
}

impl ElectricalModel {
    /// Dissipated power from the electrical model [W] (V·I, falls back to I²R).
    pub fn power(&self) -> f64 {
        if self.voltage_v > 0.0 {
            self.voltage_v * self.current_a
        } else {
            self.current_a * self.current_a * self.resistance_ohm
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junction_temperature_estimate() {
        let model = ThermalComponentModel {
            junction_to_ambient: 40.0,
            power_dissipation: 0.75,
            ..Default::default()
        };
        assert!((model.junction_temperature(25.0) - 55.0).abs() < 1e-12);
    }

    #[test]
    fn via_array_plated_area() {
        let array = ViaArray {
            rows: 3,
            cols: 3,
            pitch: Length::mm(1.0),
            drill_diameter: Length::mm(0.3),
            plating_thickness: Length::um(25.0),
        };
        assert_eq!(array.count(), 9);
        let single_annulus =
            std::f64::consts::PI * ((0.15e-3f64 + 25e-6f64).powi(2) - 0.15e-3f64.powi(2));
        assert!((array.total_plated_area() - 9.0 * single_annulus).abs() < 1e-18);
    }

    #[test]
    fn package_body_estimates() {
        assert!(
            (PackageType::Bga {
                body_mm: 10.0,
                ball_count: 144
            }
            .approx_body_m()
                - 0.01)
                .abs()
                < 1e-9
        );
        assert!(
            PackageType::Chip {
                length_mm: 1.0,
                width_mm: 0.5
            }
            .approx_body_m()
                > 0.0
        );
    }

    #[test]
    fn electrical_power_fallback() {
        let m = ElectricalModel {
            resistance_ohm: 10.0,
            current_a: 0.5,
            voltage_v: 0.0,
        };
        assert!((m.power() - 2.5).abs() < 1e-12);
        let m2 = ElectricalModel {
            resistance_ohm: 0.0,
            current_a: 0.5,
            voltage_v: 3.3,
        };
        assert!((m2.power() - 1.65).abs() < 1e-12);
    }
}
