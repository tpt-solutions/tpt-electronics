// SPDX-License-Identifier: MIT OR Apache-2.0

//! KiCad native-format parser.
//!
//! * [`KiCadParser::parse_pcb`] reads `.kicad_pcb` S-expressions (KiCad 5/6/7
//!   subset): general board thickness, layers, nets, footprints (pads and
//!   placement), segments (traces), vias, and zones.
//! * [`KiCadParser::parse_schematic`] reads KiCad's exported netlist
//!   (`.net`): components with values, nets with pin membership, and
//!   identification of power nets.
//!
//! S-expression tokenizing is hand-rolled and allocation-light; quoted
//! strings, escaped quotes, and comments-free structure are supported.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;

use tpt_elec_core::{
    Angle, Component, CopperWeight, Layer, LayerId, LayerType, Length, MaterialId, NetId, Point2,
    Stackup,
};
use tpt_elec_geometry::{DrillHole, Pad, PadShape, Trace, ViaType};

/// Parse errors.
#[derive(Clone, Debug, PartialEq)]
pub struct KiCadError {
    /// Description.
    pub message: String,
}

impl fmt::Display for KiCadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kicad parse error: {}", self.message)
    }
}

impl std::error::Error for KiCadError {}

fn err<T>(message: impl Into<String>) -> Result<T, KiCadError> {
    Err(KiCadError {
        message: message.into(),
    })
}

/// A parsed S-expression node.
#[derive(Clone, Debug)]
pub enum Sexp {
    /// An atom (number or bare word).
    Atom(String),
    /// A parenthesized list.
    List(Vec<Sexp>),
}

impl Sexp {
    /// The head atom of a list node (empty for bare atoms).
    pub fn head(&self) -> &str {
        match self {
            Sexp::Atom(a) => a,
            Sexp::List(items) => items
                .first()
                .and_then(|s| match s {
                    Sexp::Atom(a) => Some(a.as_str()),
                    _ => None,
                })
                .unwrap_or(""),
        }
    }

    /// First argument atom of a list with head `name`.
    pub fn arg(&self, name: &str) -> Option<&str> {
        if let Sexp::List(items) = self {
            if self.head() == name {
                if let Some(Sexp::Atom(a)) = items.get(1) {
                    return Some(a);
                }
            }
        }
        None
    }

    /// Named keyword argument, e.g. `(layer "F.Cu")` → the layer value.
    pub fn keyword(&self, key: &str) -> Option<&str> {
        if let Sexp::List(items) = self {
            for item in items.iter().skip(1) {
                if let Sexp::List(kv) = item {
                    if kv.first().and_then(|s| match s {
                        Sexp::Atom(a) => Some(a.as_str()),
                        _ => None,
                    }) == Some(key)
                    {
                        if let Some(Sexp::Atom(a)) = kv.get(1) {
                            return Some(a);
                        }
                    }
                }
            }
        }
        None
    }

    /// All child lists (excluding the head atom).
    pub fn children(&self) -> Box<dyn Iterator<Item = &Sexp> + '_> {
        match self {
            Sexp::List(items) => Box::new(items.iter().skip(1)),
            Sexp::Atom(_) => Box::new([].iter()),
        }
    }

    /// All child lists with a given head.
    pub fn lists<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.children().filter(move |c| c.head() == name)
    }

    /// Float value of a child atom (position 1) for a list with head `name`.
    pub fn float_arg(&self, name: &str) -> Option<f64> {
        self.arg(name).and_then(|s| s.parse().ok())
    }
}

/// Tokenizes S-expression text into a tree.
pub fn parse_sexp(input: &str) -> Result<Sexp, KiCadError> {
    let mut pos = 0usize;
    let bytes: Vec<char> = input.chars().collect();
    let n = bytes.len();
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
    let mut current = String::new();
    let mut in_string = false;
    let mut complete: Vec<Sexp> = Vec::new();

    while pos < n {
        let c = bytes[pos];
        if in_string {
            match c {
                '\\' => {
                    if pos + 1 < n {
                        current.push(bytes[pos + 1]);
                        pos += 1;
                    }
                }
                '"' => {
                    in_string = false;
                    stack.last_mut().unwrap().push(Sexp::Atom(current.clone()));
                    current.clear();
                }
                _ => current.push(c),
            }
        } else {
            match c {
                '"' => in_string = true,
                '(' => stack.push(Vec::new()),
                ')' => {
                    if !current.is_empty() {
                        stack.last_mut().unwrap().push(Sexp::Atom(current.clone()));
                        current.clear();
                    }
                    if stack.len() < 2 {
                        return err("unbalanced ')' in s-expression");
                    }
                    let done = stack.pop().unwrap();
                    let node = Sexp::List(done);
                    if stack.len() == 1 {
                        complete.push(node);
                    } else {
                        stack.last_mut().unwrap().push(node);
                    }
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        stack.last_mut().unwrap().push(Sexp::Atom(current.clone()));
                        current.clear();
                    }
                }
                _ => current.push(c),
            }
        }
        pos += 1;
    }
    if in_string {
        return err("unterminated string literal");
    }
    if stack.len() != 1 {
        return err("unbalanced '(' in s-expression");
    }
    if complete.len() == 1 {
        Ok(complete.pop().unwrap())
    } else {
        err("expected a single top-level expression")
    }
}

/// A routed via.
#[derive(Clone, Debug)]
pub struct Via {
    /// Position [m].
    pub position: Point2,
    /// Drill diameter [m].
    pub drill: Length,
    /// Pad diameter [m].
    pub diameter: Length,
    /// Net.
    pub net: NetId,
    /// Layers spanned (indices into the stackup copper order).
    pub layers: (u32, u32),
}

impl Via {
    /// Classification from layer span.
    pub fn via_type(&self) -> ViaType {
        if self.drill.as_meters() < 0.2e-3 {
            ViaType::MicroVia
        } else {
            ViaType::ThroughHole
        }
    }
}

/// A copper pour zone.
#[derive(Clone, Debug)]
pub struct Zone {
    /// Net name.
    pub net: NetId,
    /// Layer name.
    pub layer: String,
    /// Outline polygon [m].
    pub polygon: Vec<Point2>,
    /// Whether the zone is actually filled.
    pub filled: bool,
    /// Whether thermal reliefs are enabled.
    pub thermal_relief: bool,
}

/// A net class declaration.
#[derive(Clone, Debug)]
pub struct NetClass {
    /// Class name.
    pub name: String,
    /// Minimum trace width [m].
    pub min_width: Option<Length>,
    /// Clearance [m].
    pub clearance: Option<Length>,
}

/// A parsed `.kicad_pcb` board.
#[derive(Clone, Debug, Default)]
pub struct ParsedKiCadBoard {
    /// Derived stackup (copper + dielectric layers, total thickness).
    pub stackup: Stackup,
    /// Placed components.
    pub components: Vec<Component>,
    /// Copper segments.
    pub traces: Vec<Trace>,
    /// Vias.
    pub vias: Vec<Via>,
    /// Zones.
    pub zones: Vec<Zone>,
    /// Nets referenced by traces/pads.
    pub nets: Vec<NetId>,
    /// Net classes (from `(net_setup)` best-effort; may be empty).
    pub net_classes: Vec<NetClass>,
}

/// A schematic netlist entry.
#[derive(Clone, Debug)]
pub struct SchematicNetlist {
    /// Components: (ref, value, footprint).
    pub components: Vec<(String, String, String)>,
    /// Nets: (name, member refs).
    pub nets: Vec<(String, Vec<String>)>,
    /// Names of nets identified as power/ground rails.
    pub power_nets: Vec<String>,
}

/// The KiCad parser.
#[derive(Default)]
pub struct KiCadParser;

impl KiCadParser {
    /// Parses a `.kicad_pcb` document.
    pub fn parse_pcb(input: &str) -> Result<ParsedKiCadBoard, KiCadError> {
        let root = parse_sexp(input)?;
        if root.head() != "kicad_pcb" {
            return err("not a kicad_pcb document");
        }
        let mut board = ParsedKiCadBoard::default();
        let mut board_thickness_mm = 1.6;
        let mut copper_layer_names: Vec<String> = Vec::new();
        let mut net_names: HashMap<u64, String> = HashMap::new();
        let mut trace_id: u64 = 0;

        for section in root.children() {
            match section.head() {
                "general" => {
                    if let Some(t) = section.float_arg("thickness") {
                        board_thickness_mm = t;
                    }
                }
                "layers" => {
                    for layer in section.children() {
                        let name = layer.arg("layers").unwrap_or("").to_string();
                        let _ = name;
                        if let Some(lname) = layer_arg_name(layer) {
                            if lname.ends_with(".Cu") {
                                copper_layer_names.push(lname);
                            }
                        }
                    }
                }
                "net" => {
                    if let (Some(code), Some(name)) = (
                        section.arg("net").and_then(|s| s.parse::<u64>().ok()),
                        section.keyword("name"),
                    ) {
                        net_names.insert(code, name.to_string());
                    } else if let Sexp::List(items) = section {
                        // (net 1 "GND") legacy form
                        if let (Some(Sexp::Atom(code)), Some(Sexp::Atom(name))) =
                            (items.get(1), items.get(2))
                        {
                            if let Ok(code) = code.parse::<u64>() {
                                net_names.insert(code, name.trim_matches('"').to_string());
                            }
                        }
                    }
                }
                "footprint" | "module" => {
                    if let Some(c) = parse_footprint(section, &net_names) {
                        board.components.push(c);
                    }
                }
                "segment" => {
                    let start = section.lists("start").next().and_then(sexp_point2);
                    let end = section.lists("end").next().and_then(sexp_point2);
                    let width = section
                        .lists("width")
                        .next()
                        .and_then(|s| s.float_arg("width"));
                    let net_code = section.lists("net").next().and_then(|s| s.float_arg("net"));
                    let layer = section.lists("layer").next().and_then(|s| s.arg("layer"));
                    if let (Some(start), Some(end), Some(width)) = (start, end, width) {
                        let net = net_code
                            .and_then(|c| net_names.get(&(c as u64)))
                            .cloned()
                            .unwrap_or_default();
                        // Layer name → stackup copper index (F.Cu = 0)
                        let layer_idx = layer
                            .and_then(|l| copper_layer_index(&copper_layer_names, l))
                            .unwrap_or(0);
                        board.traces.push(Trace {
                            id: tpt_elec_core::TraceId::new(trace_id),
                            net,
                            layer: layer_idx,
                            width: Length::mm(width),
                            points: vec![start, end],
                            arcs: vec![],
                        });
                        trace_id += 1;
                    }
                }
                "via" => {
                    let at = section.lists("at").next().and_then(sexp_point2);
                    let size = section
                        .lists("size")
                        .next()
                        .and_then(|s| s.float_arg("size"));
                    let drill = section
                        .lists("drill")
                        .next()
                        .and_then(|s| s.float_arg("drill"));
                    let net_code = section.lists("net").next().and_then(|s| s.float_arg("net"));
                    if let (Some(at), Some(size), Some(drill)) = (at, size, drill) {
                        let net = net_code
                            .and_then(|c| net_names.get(&(c as u64)))
                            .cloned()
                            .unwrap_or_default();
                        board.vias.push(Via {
                            position: at,
                            drill: Length::mm(drill),
                            diameter: Length::mm(size),
                            net: NetId::new(net),
                            layers: (0, copper_layer_names.len().saturating_sub(1) as u32),
                        });
                    }
                }
                "zone" => {
                    let net = section
                        .lists("net_name")
                        .next()
                        .and_then(|s| s.arg("net_name"))
                        .unwrap_or("")
                        .to_string();
                    let layer = section
                        .lists("layers")
                        .next()
                        .and_then(|s| s.arg("layers"))
                        .unwrap_or("F.Cu")
                        .to_string();
                    let filled = section.keyword("filled_polygon").is_some()
                        || section
                            .lists("fill")
                            .next()
                            .map(|f| f.keyword("yes").is_some())
                            .unwrap_or(false);
                    let thermal = section.lists("thermal_bridge_angle").next().is_some()
                        || section.keyword("thermal_gap").is_some();
                    let mut polygon = Vec::new();
                    for poly in section.lists("polygon") {
                        for pts in poly.lists("pts") {
                            for xy in pts.lists("xy") {
                                if let Some(p) = sexp_point2(xy) {
                                    polygon.push(p);
                                }
                            }
                        }
                    }
                    board.zones.push(Zone {
                        net: NetId::new(net),
                        layer,
                        polygon,
                        filled,
                        thermal_relief: thermal,
                    });
                }
                _ => {}
            }
        }

        board.nets = {
            let mut v: Vec<String> = net_names.into_values().collect();
            v.sort();
            v.into_iter().map(NetId::new).collect()
        };
        board.stackup = synth_stackup(copper_layer_names.len(), board_thickness_mm);
        Ok(board)
    }

    /// Parses a KiCad-exported netlist document (`.net`).
    pub fn parse_schematic(input: &str) -> Result<SchematicNetlist, KiCadError> {
        let root = parse_sexp(input)?;
        if root.head() != "export" {
            return err("not a KiCad netlist export");
        }
        let mut result = SchematicNetlist {
            components: Vec::new(),
            nets: Vec::new(),
            power_nets: Vec::new(),
        };
        for section in root.children() {
            match section.head() {
                "components" => {
                    // (comp (ref "R1") (value "10k") (footprint "..."))
                    for comp in section.lists("comp") {
                        let rf = child_atom(comp, "ref").unwrap_or("").to_string();
                        let value = child_atom(comp, "value").unwrap_or("").to_string();
                        let footprint = child_atom(comp, "footprint").unwrap_or("").to_string();
                        result.components.push((rf, value, footprint));
                    }
                }
                "nets" => {
                    // (net (code "1") (name "GND") (node (ref "R1") (pin "2")) ...)
                    for net in section.lists("net") {
                        let name = child_atom(net, "name").unwrap_or("").to_string();
                        let mut members = Vec::new();
                        for node in net.lists("node") {
                            if let Some(rf) = child_atom(node, "ref") {
                                members.push(rf.to_string());
                            }
                        }
                        if is_power_net(&name) {
                            result.power_nets.push(name.clone());
                        }
                        result.nets.push((name, members));
                    }
                }
                _ => {}
            }
        }
        Ok(result)
    }
}

fn layer_arg_name(layer: &Sexp) -> Option<String> {
    // (0 "F.Cu" signal)  or  (layer "F.Cu")
    layer.arg("layers").map(|s| s.to_string()).or_else(|| {
        if let Sexp::List(items) = layer {
            match items.get(1) {
                Some(Sexp::Atom(a)) => Some(a.trim_matches('"').to_string()),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn copper_layer_index(copper: &[String], name: &str) -> Option<u32> {
    copper.iter().position(|l| l == name).map(|i| i as u32)
}

/// Parses an `(x y …)` positional list into a point, converting KiCad
/// millimeters to meters.
fn sexp_point2(list: &Sexp) -> Option<Point2> {
    if let Sexp::List(items) = list {
        match (items.get(1), items.get(2)) {
            (Some(Sexp::Atom(x)), Some(Sexp::Atom(y))) => Some(Point2::new(
                x.parse::<f64>().ok()? * 1e-3,
                y.parse::<f64>().ok()? * 1e-3,
            )),
            _ => None,
        }
    } else {
        None
    }
}

/// First positional atom of the first child list with head `name`,
/// e.g. the `"R1"` of `(comp (ref "R1") ...)`.
fn child_atom<'a>(parent: &'a Sexp, name: &'a str) -> Option<&'a str> {
    parent.lists(name).next()?.arg(name)
}

fn parse_footprint(fp: &Sexp, _nets: &HashMap<u64, String>) -> Option<Component> {
    // (property "Reference" "R1" (at ...)) - the value is positional (item 2).
    let prop_value = |prop_name: &str| -> Option<String> {
        fp.lists("property")
            .find(|p| p.arg("property") == Some(prop_name))
            .and_then(|p| match p {
                Sexp::List(items) => items.get(2).and_then(|s| match s {
                    Sexp::Atom(a) => Some(a.to_string()),
                    _ => None,
                }),
                _ => None,
            })
    };
    let reference = prop_value("Reference")
        .or_else(|| child_atom(fp, "reference").map(String::from))
        .unwrap_or_else(|| "??".to_string());
    let at = fp.lists("at").next().and_then(|s| {
        if let Sexp::List(items) = s {
            match (items.get(1), items.get(2), items.get(3)) {
                (Some(Sexp::Atom(x)), Some(Sexp::Atom(y)), rot) => Some((
                    Point2::new(x.parse::<f64>().ok()? * 1e-3, y.parse::<f64>().ok()? * 1e-3),
                    rot.and_then(|r| match r {
                        Sexp::Atom(a) => a.parse::<f64>().ok(),
                        _ => None,
                    })
                    .unwrap_or(0.0),
                )),
                _ => None,
            }
        } else {
            None
        }
    })?;

    // Heuristic package classification from reference/footprint strings.
    let package = classify_package(&reference, fp.arg("footprint").unwrap_or(""));

    let mut comp = Component::new(&reference, package, at.0);
    comp.rotation = Angle::degrees(at.1);
    comp.reference = reference;
    Some(comp)
}

fn classify_package(value: &str, footprint: &str) -> tpt_elec_core::PackageType {
    let hay = format!("{} {}", value, footprint).to_uppercase();
    if hay.contains("BGA") {
        tpt_elec_core::PackageType::Bga {
            body_mm: 10.0,
            ball_count: 144,
        }
    } else if hay.contains("QFN") {
        tpt_elec_core::PackageType::Qfn { body_mm: 5.0 }
    } else if hay.contains("SOIC") {
        tpt_elec_core::PackageType::Soic { pins: 8 }
    } else if hay.contains("TQFP") || hay.contains("QFP") {
        tpt_elec_core::PackageType::Tqfp { pins: 32 }
    } else if hay.contains("SOT-23") || hay.contains("SOT23") {
        tpt_elec_core::PackageType::Sot23
    } else if hay.contains("DIP") {
        tpt_elec_core::PackageType::Dip { pins: 8 }
    } else {
        tpt_elec_core::PackageType::Chip {
            length_mm: 1.6,
            width_mm: 0.8,
        }
    }
}

fn is_power_net(name: &str) -> bool {
    let up = name.to_uppercase();
    up.contains("GND")
        || up.contains("VCC")
        || up.contains("VDD")
        || up.contains("VSS")
        || up.contains("+3V3")
        || up.contains("+5V")
        || up.contains("+12V")
        || up.contains("VBUS")
        || up.starts_with('/')
}

/// Builds a synthetic stackup for a board with `copper_count` layers.
fn synth_stackup(copper_count: usize, thickness_mm: f64) -> Stackup {
    let mut layers = Vec::new();
    let cw = CopperWeight::OneOz;
    let diel = if copper_count <= 1 {
        thickness_mm
    } else {
        (thickness_mm - copper_count as f64 * cw.thickness_m().as_mm()) / (copper_count - 1) as f64
    };
    for i in 0..copper_count {
        layers.push(Layer {
            id: LayerId::new(layers.len() as u32),
            name: format!("copper{i}"),
            layer_type: if i == 0 || i == copper_count - 1 {
                LayerType::Signal
            } else {
                LayerType::Plane
            },
            thickness: cw.thickness_m(),
            material: MaterialId::new("copper"),
            copper_weight: Some(cw),
            order: layers.len() as u32,
        });
        if i + 1 < copper_count {
            layers.push(Layer {
                id: LayerId::new(layers.len() as u32),
                name: format!("dielectric{i}"),
                layer_type: LayerType::Dielectric,
                thickness: Length::mm(diel),
                material: MaterialId::new("fr4"),
                copper_weight: None,
                order: layers.len() as u32,
            });
        }
    }
    Stackup::from_layers(layers)
}

/// Extracts the pads of a footprint into geometry pads (helper used by
/// consumers that need drill/pad data).
pub fn footprint_pads(fp: &Sexp, copper_layers: u32) -> Vec<Pad> {
    let mut pads = Vec::new();
    for (idx, pad) in fp.lists("pad").enumerate() {
        let at = pad.lists("at").next().and_then(sexp_point2);
        let size = pad.lists("size").next().and_then(|s| {
            if let Sexp::List(items) = s {
                match (items.get(1), items.get(2)) {
                    (Some(Sexp::Atom(w)), Some(Sexp::Atom(h))) => {
                        Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?))
                    }
                    _ => None,
                }
            } else {
                None
            }
        });
        let drill = pad.lists("drill").next().and_then(|d| d.float_arg("drill"));
        // (pad "N" TYPE SHAPE ...) - type and shape are positional.
        let (kind, shape_str) = match pad {
            Sexp::List(items) => (
                items.get(2).and_then(|s| match s {
                    Sexp::Atom(a) => Some(a.to_string()),
                    _ => None,
                }),
                items.get(3).and_then(|s| match s {
                    Sexp::Atom(a) => Some(a.to_string()),
                    _ => None,
                }),
            ),
            _ => (None, None),
        };
        let kind = kind.unwrap_or_else(|| "smd".to_string());
        let shape_str = shape_str.unwrap_or_else(|| "circle".to_string());
        let (Some(at), Some((w, h))) = (at, size) else {
            continue;
        };
        let shape = match shape_str.as_str() {
            "rect" => PadShape::Rectangle {
                width: Length::mm(w),
                height: Length::mm(h),
            },
            "roundrect" => PadShape::RoundedRectangle {
                width: Length::mm(w),
                height: Length::mm(h),
                radius: Length::mm((w.min(h)) * 0.25),
            },
            "oval" => PadShape::Oblong {
                width: Length::mm(w),
                height: Length::mm(h),
            },
            "custom" => PadShape::Circle {
                diameter: Length::mm(w),
            },
            _ => PadShape::Circle {
                diameter: Length::mm(w),
            },
        };
        let drill_hole = drill.map(|d| DrillHole {
            diameter: Length::mm(d),
            plating_thickness: Length::um(25.0),
            is_via: false,
            via_type: ViaType::ThroughHole,
        });
        pads.push(Pad {
            id: idx as u64,
            shape,
            position: at,
            layer: if kind == "smd" {
                0
            } else {
                copper_layers.saturating_sub(1)
            },
            drill: if kind == "smd" { None } else { drill_hole },
        });
    }
    pads
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCB: &str = r#"(kicad_pcb (version 20221018)
  (general (thickness 1.6))
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
  )
  (net 0 "")
  (net 1 "GND")
  (net 2 "VCC")
  (footprint "Resistor_SMD:R_0603"
    (layer "F.Cu")
    (at 10 20 90)
    (property "Reference" "R1" (at 0 0) (layer "F.SilkS"))
    (property "Value" "10k" (at 0 0) (layer "F.Fab"))
    (pad "1" smd roundrect (at -0.75 0) (size 0.8 0.8) (layers "F.Cu"))
    (pad "2" smd roundrect (at 0.75 0) (size 0.8 0.8) (layers "F.Cu"))
  )
  (segment (start 5 5) (end 30 5) (width 0.25) (layer "F.Cu") (net 2))
  (via (at 15 10) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
  (zone (net 1) (net_name "GND") (layers "F.Cu") (fill yes) (thermal_gap 0.5)
    (polygon (pts (xy 2 2) (xy 44 2) (xy 44 28) (xy 2 28))))
)"#;

    const NETLIST: &str = r#"(export (version E)
  (components
    (comp (ref "R1")
      (value "10k")
      (footprint "Resistor_SMD:R_0603")
    )
    (comp (ref "C1")
      (value "100n")
      (footprint "Capacitor_SMD:C_0402")
    )
  )
  (nets
    (net (code "1") (name "GND")
      (node (ref "R1") (pin "2"))
      (node (ref "C1") (pin "1"))
    )
    (net (code "2") (name "VCC")
      (node (ref "R1") (pin "1"))
    )
  )
)"#;

    #[test]
    fn sexp_tokenizer_basics() {
        let s = parse_sexp("(a (b \"c d\") 42)").unwrap();
        assert_eq!(s.head(), "a");
        // arg() returns the positional item after the head atom.
        assert_eq!(s.float_arg("a"), None); // item 1 is a list, not an atom
        let inner: Vec<_> = s.lists("b").collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].arg("b"), Some("c d"));
        assert_eq!(child_atom(&s, "b"), Some("c d"));
        assert!(parse_sexp("(unbalanced").is_err());
        assert!(parse_sexp("(bad \"string)").is_err());
    }

    #[test]
    fn parses_board_metadata() {
        let b = KiCadParser::parse_pcb(PCB).unwrap();
        assert!((b.stackup.total_thickness.as_mm() - 1.6).abs() < 1e-9);
        assert_eq!(b.stackup.copper_layer_count(), 2);
        assert_eq!(b.nets.len(), 3);
        assert!(b.nets.iter().any(|n| n.value() == "GND"));
        assert!(b.nets.iter().any(|n| n.value() == "VCC"));
    }

    #[test]
    fn parses_traces_vias_zones() {
        let b = KiCadParser::parse_pcb(PCB).unwrap();
        assert_eq!(b.traces.len(), 1);
        let t = &b.traces[0];
        assert_eq!(t.net, "VCC");
        assert!((t.width.as_mm() - 0.25).abs() < 1e-9);
        assert!((t.points[1].x - 30.0e-3).abs() < 1e-9);

        assert_eq!(b.vias.len(), 1);
        let v = &b.vias[0];
        assert!((v.drill.as_mm() - 0.3).abs() < 1e-9);
        assert_eq!(v.net.value(), "GND");
        assert_eq!(v.via_type(), ViaType::ThroughHole);

        assert_eq!(b.zones.len(), 1);
        let z = &b.zones[0];
        assert_eq!(z.net.value(), "GND");
        assert_eq!(z.polygon.len(), 4);
        assert!(z.thermal_relief);
    }

    #[test]
    fn parses_footprint_component() {
        let b = KiCadParser::parse_pcb(PCB).unwrap();
        assert_eq!(b.components.len(), 1);
        let c = &b.components[0];
        assert_eq!(c.reference, "R1");
        assert!((c.position.x - 10.0e-3).abs() < 1e-12);
        assert!((c.rotation.as_degrees() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn extracts_footprint_pads() {
        let root = parse_sexp(PCB).unwrap();
        let fps: Vec<_> = root.lists("footprint").collect();
        assert_eq!(fps.len(), 1);
        let pads = footprint_pads(fps[0], 2);
        assert_eq!(pads.len(), 2);
        assert!(matches!(pads[0].shape, PadShape::RoundedRectangle { .. }));
        assert!(pads[0].drill.is_none()); // smd
        assert!((pads[1].position.x - 0.75e-3).abs() < 1e-12);
    }

    #[test]
    fn parses_netlist() {
        let nl = KiCadParser::parse_schematic(NETLIST).unwrap();
        assert_eq!(nl.components.len(), 2);
        assert_eq!(nl.components[0].0, "R1");
        assert_eq!(nl.components[0].1, "10k");
        assert_eq!(nl.nets.len(), 2);
        let gnd = nl.nets.iter().find(|(n, _)| n == "GND").unwrap();
        assert_eq!(gnd.1, vec!["R1", "C1"]);
        assert!(nl.power_nets.contains(&"GND".to_string()));
        assert!(nl.power_nets.contains(&"VCC".to_string()));
    }

    #[test]
    fn rejects_non_pcb_documents() {
        assert!(KiCadParser::parse_pcb("(schematic)").is_err());
        assert!(KiCadParser::parse_schematic("(kicad_pcb)").is_err());
    }

    #[test]
    fn stackup_synth_sums_to_thickness() {
        let s = synth_stackup(4, 1.6);
        assert!((s.total_thickness.as_mm() - 1.6).abs() < 1e-6);
        assert_eq!(s.copper_layer_count(), 4);
        assert_eq!(
            s.layers
                .iter()
                .filter(|l| l.layer_type == LayerType::Dielectric)
                .count(),
            3
        );
    }
}
