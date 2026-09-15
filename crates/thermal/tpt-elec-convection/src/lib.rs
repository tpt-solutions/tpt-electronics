// SPDX-License-Identifier: MIT OR Apache-2.0

//! Natural and forced convection correlations.
//!
//! [`ConvectionModel::calculate_h`] returns a heat-transfer coefficient
//! [W/(m²·K)] from classic engineering correlations:
//!
//! * Natural convection — Nusselt from the Rayleigh number,
//!   `Nu = C·Raⁿ` with geometry-dependent `C`/`n`.
//! * Forced convection — Nusselt from Reynolds + Prandtl,
//!   laminar flat plate `Nu = 0.664·Re^½·Pr^⅓`, turbulent
//!   `Nu = 0.037·Re^0.8·Pr^⅓`.
//!
//! Properties are evaluated for air at ~300 K unless a custom fluid is given.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Vector3;

/// Gravitational acceleration [m/s²].
const G: f64 = 9.80665;

/// A coolant fluid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fluid {
    /// Air at 1 atm, ~300 K.
    Air,
    /// Water at ~300 K.
    Water,
    /// User-supplied properties.
    Custom {
        /// Thermal conductivity [W/(m·K)].
        k: f64,
        /// Kinematic viscosity [m²/s].
        nu: f64,
        /// Prandtl number.
        pr: f64,
    },
}

impl Fluid {
    /// Thermal conductivity [W/(m·K)].
    pub fn conductivity(&self) -> f64 {
        match self {
            Fluid::Air => 0.0263,
            Fluid::Water => 0.606,
            Fluid::Custom { k, .. } => *k,
        }
    }

    /// Kinematic viscosity [m²/s].
    pub fn kinematic_viscosity(&self) -> f64 {
        match self {
            Fluid::Air => 1.562e-5,
            Fluid::Water => 8.57e-7,
            Fluid::Custom { nu, .. } => *nu,
        }
    }

    /// Prandtl number.
    pub fn prandtl(&self) -> f64 {
        match self {
            Fluid::Air => 0.71,
            Fluid::Water => 5.83,
            Fluid::Custom { pr, .. } => *pr,
        }
    }

    /// Thermal diffusivity α = ν/Pr [m²/s].
    pub fn thermal_diffusivity(&self) -> f64 {
        self.kinematic_viscosity() / self.prandtl()
    }
}

/// Surface orientation for natural convection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Vertical plate.
    Vertical,
    /// Horizontal plate, heated side facing up.
    HorizontalUp,
    /// Horizontal plate, heated side facing down.
    HorizontalDown,
}

/// Convection regime.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConvectionType {
    /// Buoyancy-driven.
    Natural {
        /// Surface orientation.
        orientation: Orientation,
    },
    /// Externally driven flow.
    Forced {
        /// Approach velocity [m/s].
        velocity: f64,
        /// Flow direction (unit-ish vector; only used for reporting).
        direction: Vector3,
        /// Coolant.
        fluid: Fluid,
    },
}

/// A convection boundary model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvectionModel {
    /// Regime.
    pub convection_type: ConvectionType,
    /// Ambient (coolant) temperature [°C].
    pub ambient_temperature: f64,
}

impl ConvectionModel {
    /// A natural-convection model in air with the given orientation.
    pub fn natural(orientation: Orientation, ambient_c: f64) -> Self {
        Self {
            convection_type: ConvectionType::Natural { orientation },
            ambient_temperature: ambient_c,
        }
    }

    /// A forced-convection model in air.
    pub fn forced(velocity: f64, ambient_c: f64) -> Self {
        Self {
            convection_type: ConvectionType::Forced {
                velocity,
                direction: Vector3::new(1.0, 0.0, 0.0),
                fluid: Fluid::Air,
            },
            ambient_temperature: ambient_c,
        }
    }

    /// Heat-transfer coefficient [W/(m²·K)].
    ///
    /// * `characteristic_length` — plate length along the flow (forced) or
    ///   the vertical/horizontal plate dimension [m].
    /// * `temperature_difference` — surface − ambient [K]. For forced flow it
    ///   enters only via film-temperature property corrections (ignored here),
    ///   so any positive value yields the same `h`.
    pub fn calculate_h(
        &self,
        _surface_area: f64,
        characteristic_length: f64,
        temperature_difference: f64,
    ) -> f64 {
        let l = characteristic_length.max(1e-9);
        match self.convection_type {
            ConvectionType::Natural { orientation } => {
                let dt = temperature_difference.max(0.0);
                let t_film_k = (self.ambient_temperature + dt / 2.0) + 273.15;
                let beta = 1.0 / t_film_k;
                let air = Fluid::Air;
                let ra = G * beta * dt * l.powi(3)
                    / (air.kinematic_viscosity() * air.thermal_diffusivity());
                let (c, n) = match orientation {
                    Orientation::Vertical => (0.59, 0.25),
                    Orientation::HorizontalUp => {
                        if ra > 1.0e7 {
                            (0.15, 1.0 / 3.0)
                        } else {
                            (0.54, 0.25)
                        }
                    }
                    Orientation::HorizontalDown => (0.27, 0.25),
                };
                let nu = c * ra.powf(n);
                nu * air.conductivity() / l
            }
            ConvectionType::Forced {
                velocity, fluid, ..
            } => {
                let re = velocity * l / fluid.kinematic_viscosity();
                let pr = fluid.prandtl();
                let nu = if re < 5.0e5 {
                    // laminar flat plate
                    0.664 * re.sqrt() * pr.powf(1.0 / 3.0)
                } else {
                    // turbulent flat plate
                    0.037 * re.powf(0.8) * pr.powf(1.0 / 3.0)
                };
                nu * fluid.conductivity() / l
            }
        }
    }

    /// Rayleigh number for the current natural-convection setup.
    pub fn rayleigh(&self, characteristic_length: f64, temperature_difference: f64) -> f64 {
        let dt = temperature_difference.max(0.0);
        let t_film_k = (self.ambient_temperature + dt / 2.0) + 273.15;
        let beta = 1.0 / t_film_k;
        let air = Fluid::Air;
        G * beta * dt * characteristic_length.powi(3)
            / (air.kinematic_viscosity() * air.thermal_diffusivity())
    }

    /// Reynolds number for the current forced-convection setup.
    pub fn reynolds(&self, characteristic_length: f64) -> f64 {
        match self.convection_type {
            ConvectionType::Forced {
                velocity, fluid, ..
            } => velocity * characteristic_length / fluid.kinematic_viscosity(),
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_convection_air_horizontal_board() {
        // 100 mm × 100 mm PCB, 30 K rise: h ≈ 5–7 W/m²K
        let m = ConvectionModel::natural(Orientation::HorizontalUp, 25.0);
        let h = m.calculate_h(0.01, 0.1, 30.0);
        assert!((4.5..8.0).contains(&h), "h = {h} outside expected 4.5–8");
    }

    #[test]
    fn natural_convection_vertical_vs_down() {
        let up = ConvectionModel::natural(Orientation::HorizontalUp, 25.0);
        let down = ConvectionModel::natural(Orientation::HorizontalDown, 25.0);
        let h_up = up.calculate_h(0.01, 0.1, 30.0);
        let h_down = down.calculate_h(0.01, 0.1, 30.0);
        assert!(h_up > h_down * 1.5, "up {h_up} should beat down {h_down}");
    }

    #[test]
    fn forced_beats_natural_and_scales() {
        let nat = ConvectionModel::natural(Orientation::HorizontalUp, 25.0);
        let slow = ConvectionModel::forced(1.0, 25.0);
        let fast = ConvectionModel::forced(3.0, 25.0);
        let h_nat = nat.calculate_h(0.01, 0.1, 30.0);
        let h_slow = slow.calculate_h(0.01, 0.1, 30.0);
        let h_fast = fast.calculate_h(0.01, 0.1, 30.0);
        assert!(h_slow > h_nat, "1 m/s {h_slow} > natural {h_nat}");
        assert!(h_fast > h_slow, "3 m/s {h_fast} > 1 m/s {h_slow}");
        // h roughly scales with sqrt(v) in the laminar regime
        let ratio = h_fast / h_slow;
        assert!((ratio - 3.0f64.sqrt()).abs() < 0.3, "ratio {ratio}");
    }

    #[test]
    fn rayleigh_number_order_of_magnitude() {
        // Ra for 0.1 m plate, 30 K in air: g·β·ΔT·L³/(ν·α) ≈ 2.7e6
        let m = ConvectionModel::natural(Orientation::HorizontalUp, 25.0);
        let ra = m.rayleigh(0.1, 30.0);
        assert!((1.5e6..4.5e6).contains(&ra), "ra = {ra}");
    }

    #[test]
    fn reynolds_number() {
        let m = ConvectionModel::forced(2.0, 25.0);
        let re = m.reynolds(0.1);
        let expected = 2.0 * 0.1 / 1.562e-5;
        assert!((re - expected).abs() < 1.0);
        // Laminar flat-plate regime switch at Re = 5e5
        assert!(re < 5.0e5);
    }

    #[test]
    fn water_properties_differ_from_air() {
        let air = ConvectionModel {
            convection_type: ConvectionType::Forced {
                velocity: 1.0,
                direction: Vector3::new(1., 0., 0.),
                fluid: Fluid::Air,
            },
            ambient_temperature: 25.0,
        };
        let water = ConvectionModel {
            convection_type: ConvectionType::Forced {
                velocity: 1.0,
                direction: Vector3::new(1., 0., 0.),
                fluid: Fluid::Water,
            },
            ambient_temperature: 25.0,
        };
        assert!(water.calculate_h(0.01, 0.1, 30.0) > air.calculate_h(0.01, 0.1, 30.0));
    }

    #[test]
    fn custom_fluid() {
        let f = Fluid::Custom {
            k: 0.5,
            nu: 1.0e-6,
            pr: 10.0,
        };
        let m = ConvectionModel {
            convection_type: ConvectionType::Forced {
                velocity: 1.0,
                direction: Vector3::new(1., 0., 0.),
                fluid: f,
            },
            ambient_temperature: 25.0,
        };
        let h = m.calculate_h(0.01, 0.1, 30.0);
        assert!(h > 0.0 && h.is_finite());
    }
}
