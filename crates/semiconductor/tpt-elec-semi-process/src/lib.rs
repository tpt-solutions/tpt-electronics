// SPDX-License-Identifier: MIT OR Apache-2.0

//! Process design kit (PDK) abstractions.
//!
//! [`ProcessDesignKit`] bundles a [`TechnologyNode`], device models,
//! interconnect RC models (sheet resistance, via resistance, per-mm
//! capacitance), and simple [`DesignRules`] — the wiring a layout tool or
//! parasitic extractor needs before full LVS exists.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;

/// Process node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TechnologyNode {
    /// Micron-scale node (e.g. 0.35 µm).
    Micron(f64),
    /// Nanometer node (e.g. 65 nm).
    Nanometer(f64),
}

impl TechnologyNode {
    /// Feature size in nanometers.
    pub fn nanometers(&self) -> f64 {
        match *self {
            TechnologyNode::Micron(um) => um * 1e3,
            TechnologyNode::Nanometer(nm) => nm,
        }
    }
}

/// Interconnect models per metal layer.
#[derive(Clone, Debug, PartialEq)]
pub struct InterconnectModel {
    /// Layer name.
    pub layer: String,
    /// Sheet resistance [Ω/sq].
    pub sheet_resistance: f64,
    /// Wire capacitance per length [F/m].
    pub capacitance_per_m: f64,
    /// Minimum width [m].
    pub min_width: Length,
}

impl InterconnectModel {
    /// Resistance of a wire [Ω]: `R = R□·L/W`.
    pub fn wire_resistance(&self, length: Length, width: Length) -> f64 {
        self.sheet_resistance * length.as_meters() / width.as_meters().max(1e-12)
    }

    /// Wire capacitance [F].
    pub fn wire_capacitance(&self, length: Length) -> f64 {
        self.capacitance_per_m * length.as_meters()
    }

    /// RC time constant of a wire [s].
    pub fn wire_rc_delay(&self, length: Length, width: Length) -> f64 {
        self.wire_resistance(length, width) * self.wire_capacitance(length) / 2.0
    }
}

/// Via (inter-layer connection) model.
#[derive(Clone, Debug, PartialEq)]
pub struct ViaModel {
    /// Layer pair (e.g. "M1-M2").
    pub layers: String,
    /// Resistance per via [Ω].
    pub resistance: f64,
    /// Minimum via area [m²].
    pub min_area: Length,
}

/// Simple design-rule set.
#[derive(Clone, Debug, PartialEq)]
pub struct DesignRules {
    /// Minimum metal width [m].
    pub min_width: Length,
    /// Minimum metal spacing [m].
    pub min_spacing: Length,
    /// Minimum enclosure of a via by metal [m].
    pub min_enclosure: Length,
    /// Maximum current density [A/m²] (electromigration limit).
    pub max_current_density: f64,
}

impl DesignRules {
    /// Maximum DC current a wire of `width` may carry [A] (EM rule).
    pub fn max_current(&self, width: Length) -> f64 {
        self.max_current_density * width.as_meters() * 35e-9 // 35 nm thick metal
    }
}

/// A process design kit.
#[derive(Clone, Debug)]
pub struct ProcessDesignKit {
    /// Process name (e.g. "generic-0p35um").
    pub process_name: String,
    /// Technology node.
    pub node: TechnologyNode,
    /// Interconnect layers.
    pub interconnect: Vec<InterconnectModel>,
    /// Via models between adjacent layers.
    pub vias: Vec<ViaModel>,
    /// Design rules.
    pub design_rules: DesignRules,
}

impl ProcessDesignKit {
    /// A generic 0.35 µm 2-metal process with textbook parameters.
    pub fn generic_0p35um() -> Self {
        Self {
            process_name: "generic-0p35um".into(),
            node: TechnologyNode::Micron(0.35),
            interconnect: vec![
                InterconnectModel {
                    layer: "metal1".into(),
                    sheet_resistance: 0.08,     // Ω/sq
                    capacitance_per_m: 2.0e-10, // F/m
                    min_width: Length::um(0.6),
                },
                InterconnectModel {
                    layer: "metal2".into(),
                    sheet_resistance: 0.04,
                    capacitance_per_m: 1.0e-10,
                    min_width: Length::um(0.8),
                },
            ],
            vias: vec![ViaModel {
                layers: "metal1-metal2".into(),
                resistance: 5.0,
                min_area: Length::um(0.6),
            }],
            design_rules: DesignRules {
                min_width: Length::um(0.6),
                min_spacing: Length::um(0.6),
                min_enclosure: Length::um(0.2),
                max_current_density: 1.0e9, // A/m² conservative
            },
        }
    }

    /// Looks up an interconnect layer by name.
    pub fn layer(&self, name: &str) -> Option<&InterconnectModel> {
        self.interconnect.iter().find(|m| m.layer == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_conversion() {
        assert!((TechnologyNode::Micron(0.35).nanometers() - 350.0).abs() < 1e-9);
        assert!((TechnologyNode::Nanometer(65.0).nanometers() - 65.0).abs() < 1e-9);
    }

    #[test]
    fn wire_resistance_scales_with_aspect() {
        let pdk = ProcessDesignKit::generic_0p35um();
        let m1 = pdk.layer("metal1").unwrap();
        // 1 mm long, 1 µm wide: R = 0.08·1000 = 80 Ω
        let r = m1.wire_resistance(Length::mm(1.0), Length::um(1.0));
        assert!((r - 80.0).abs() < 1e-9);
        // Doubled width halves R
        let r2 = m1.wire_resistance(Length::mm(1.0), Length::um(2.0));
        assert!((r2 - r / 2.0).abs() < 1e-9);
    }

    #[test]
    fn wire_capacitance_linear() {
        let pdk = ProcessDesignKit::generic_0p35um();
        let m1 = pdk.layer("metal1").unwrap();
        let c = m1.wire_capacitance(Length::mm(2.0));
        assert!((c - 4.0e-13).abs() < 1e-15);
        assert!(m1.wire_rc_delay(Length::mm(1.0), Length::um(1.0)) > 0.0);
    }

    #[test]
    fn electromigration_current_limit() {
        let pdk = ProcessDesignKit::generic_0p35um();
        let imax = pdk.design_rules.max_current(Length::um(1.0));
        // 1e9 A/m² × 1 µm × 35 nm = 35 µA… conservative generic rule
        assert!((imax - 35e-6).abs() < 1e-8, "imax {imax}");
        assert!(pdk.design_rules.max_current(Length::um(100.0)) > imax);
    }

    #[test]
    fn via_model_lookup() {
        let pdk = ProcessDesignKit::generic_0p35um();
        assert_eq!(pdk.vias[0].layers, "metal1-metal2");
        assert!((pdk.vias[0].resistance - 5.0).abs() < 1e-12);
    }
}
