// SPDX-License-Identifier: MIT OR Apache-2.0

//! IPC-2581-C XML parser.
//!
//! Extracts the simulation-relevant subset of an IPC-2581 `Ecad` content
//! file: the layer stackup (`StepLayer` thicknesses/materials), logical
//! nets (`LogNet`), placed components (`Component`), and package pins.
//! A small tolerant XML tokenizer handles the subset (tags, attributes,
//! self-closing elements, comments) without external dependencies.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;

use tpt_elec_core::{Component, Layer, LayerId, LayerType, Length, MaterialId, Point2, Stackup};
use tpt_elec_geometry::{Pad, PadShape};

/// Parse errors.
#[derive(Clone, Debug, PartialEq)]
pub struct Ipc2581Error {
    /// Description.
    pub message: String,
}

impl fmt::Display for Ipc2581Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ipc2581 error: {}", self.message)
    }
}
impl std::error::Error for Ipc2581Error {}

fn err<T>(msg: impl Into<String>) -> Result<T, Ipc2581Error> {
    Err(Ipc2581Error {
        message: msg.into(),
    })
}

/// A parsed XML node.
#[derive(Clone, Debug)]
pub struct XmlNode {
    /// Tag name (case preserved).
    pub tag: String,
    /// Attributes.
    pub attrs: HashMap<String, String>,
    /// Child elements.
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    /// First child with the given tag (case-insensitive).
    pub fn child(&self, tag: &str) -> Option<&XmlNode> {
        self.children
            .iter()
            .find(|c| c.tag.eq_ignore_ascii_case(tag))
    }

    /// All children with the given tag.
    pub fn children_of<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a XmlNode> + 'a {
        self.children
            .iter()
            .filter(move |c| c.tag.eq_ignore_ascii_case(tag))
    }

    /// Attribute lookup (case-insensitive key).
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// All descendants (depth-first) with the given tag.
    pub fn find_all<'a>(&'a self, tag: &str) -> Vec<&'a XmlNode> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            if n.tag.eq_ignore_ascii_case(tag) {
                out.push(n);
            }
            // reverse-push keeps document order in the results
            for c in n.children.iter().rev() {
                stack.push(c);
            }
        }
        out
    }
}

/// Parses a minimal well-formed XML document.
pub fn parse_xml(input: &str) -> Result<XmlNode, Ipc2581Error> {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut pos = 0usize;
    let mut root: Option<XmlNode> = None;
    let mut stack: Vec<XmlNode> = Vec::new();

    while pos < n {
        while pos < n && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= n {
            break;
        }
        if chars[pos] != '<' {
            // text content: skip to the next tag
            while pos < n && chars[pos] != '<' {
                pos += 1;
            }
            continue;
        }
        pos += 1;
        if pos < n && chars[pos] == '?' {
            while pos + 1 < n && !(chars[pos] == '?' && chars[pos + 1] == '>') {
                pos += 1;
            }
            pos += 2;
            continue;
        }
        if pos < n && chars[pos] == '!' {
            while pos < n && chars[pos] != '>' {
                pos += 1;
            }
            pos += 1;
            continue;
        }
        let closing = pos < n && chars[pos] == '/';
        if closing {
            pos += 1;
        }
        // tag name
        let start = pos;
        while pos < n && (chars[pos].is_alphanumeric() || "_-.:".contains(chars[pos])) {
            pos += 1;
        }
        let tag: String = chars[start..pos].iter().collect();

        let mut attrs = HashMap::new();
        loop {
            while pos < n && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos >= n {
                return err("unexpected end of XML");
            }
            if chars[pos] == '>' || (chars[pos] == '/' && pos + 1 < n && chars[pos + 1] == '>') {
                break;
            }
            let kstart = pos;
            while pos < n && (chars[pos].is_alphanumeric() || "_-.:".contains(chars[pos])) {
                pos += 1;
            }
            let key: String = chars[kstart..pos].iter().collect();
            while pos < n && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos < n && chars[pos] == '=' {
                pos += 1;
                while pos < n && chars[pos].is_whitespace() {
                    pos += 1;
                }
                if pos < n && (chars[pos] == '"' || chars[pos] == '\'') {
                    let quote = chars[pos];
                    pos += 1;
                    let vstart = pos;
                    while pos < n && chars[pos] != quote {
                        pos += 1;
                    }
                    attrs.insert(key, chars[vstart..pos].iter().collect());
                    pos += 1;
                }
            } else {
                attrs.insert(key, String::new());
            }
        }
        let self_closing = pos < n && chars[pos] == '/';
        if self_closing {
            pos += 1;
        }
        pos += 1; // consume '>'

        if closing {
            let node = stack.pop().ok_or_else(|| Ipc2581Error {
                message: format!("unbalanced closing tag </{tag}>"),
            })?;
            if let Some(parent) = stack.last_mut() {
                parent.children.push(node);
            } else {
                root = Some(node);
            }
        } else {
            stack.push(XmlNode {
                tag,
                attrs,
                children: Vec::new(),
            });
            if self_closing {
                let node = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
        }
    }

    if !stack.is_empty() {
        return err("unclosed XML elements");
    }
    root.ok_or_else(|| Ipc2581Error {
        message: "no root element".into(),
    })
}

/// A parsed IPC-2581 design.
#[derive(Clone, Debug, Default)]
pub struct Ipc2581Design {
    /// Stackup (from `StepLayer` entries).
    pub stackup: Stackup,
    /// Logical net names.
    pub nets: Vec<String>,
    /// Placed components.
    pub components: Vec<Component>,
    /// Package pads: (owner refDes, pad name, x, y, size) in meters.
    pub pads: Vec<(String, String, f64, f64, f64)>,
}

/// The IPC-2581 parser.
#[derive(Default)]
pub struct Ipc2581Parser;

impl Ipc2581Parser {
    /// Parses an IPC-2581-C content XML document.
    pub fn parse(input: &str) -> Result<Ipc2581Design, Ipc2581Error> {
        let root = parse_xml(input)?;
        let ecad = if root.tag.eq_ignore_ascii_case("Ecad") {
            &root
        } else {
            root.find_all("Ecad").first().ok_or_else(|| Ipc2581Error {
                message: "no Ecad element".into(),
            })?
        };

        let mut design = Ipc2581Design::default();

        // Stackup
        let mut layers = Vec::new();
        for step_layer in ecad.find_all("StepLayer") {
            let name = step_layer.attr("name").unwrap_or("").to_string();
            let thickness = step_layer
                .attr("thickness")
                .and_then(parse_length)
                .unwrap_or(Length::ZERO);
            let material = step_layer
                .child("Material")
                .and_then(|m| m.attr("name"))
                .unwrap_or("")
                .to_string();
            let copper =
                name.to_lowercase().contains("cu") || material.to_uppercase().contains("COPPER");
            layers.push(Layer {
                id: LayerId::new(layers.len() as u32),
                name,
                layer_type: if copper {
                    LayerType::Signal
                } else {
                    LayerType::Dielectric
                },
                thickness,
                material: MaterialId::new(if copper { "copper" } else { "fr4" }),
                copper_weight: None,
                order: layers.len() as u32,
            });
        }
        design.stackup = Stackup::from_layers(layers);

        // Nets
        for net in ecad.find_all("LogNet") {
            if let Some(name) = net.attr("name") {
                design.nets.push(name.to_string());
            }
        }

        // Components
        for comp in ecad.find_all("Component") {
            let ref_des = comp.attr("refDes").unwrap_or("").to_string();
            let mut position = Point2::new(0.0, 0.0);
            if let Some(x) = comp.attr("x").and_then(parse_length) {
                position.x = x.as_meters();
            }
            if let Some(y) = comp.attr("y").and_then(parse_length) {
                position.y = y.as_meters();
            }
            design.components.push(Component::new(
                &ref_des,
                tpt_elec_core::PackageType::Custom {
                    body_mm_x: 1.0,
                    body_mm_y: 0.5,
                },
                position,
            ));
        }

        // Pins → pads
        for pin in ecad.find_all("Pin") {
            let name = pin.attr("name").unwrap_or("").to_string();
            let x = pin
                .attr("x")
                .and_then(parse_length)
                .map(|l| l.as_meters())
                .unwrap_or(0.0);
            let y = pin
                .attr("y")
                .and_then(parse_length)
                .map(|l| l.as_meters())
                .unwrap_or(0.0);
            let size = pin
                .attr("size")
                .and_then(parse_length)
                .map(|l| l.as_meters())
                .unwrap_or(0.5e-3);
            let owner = pin
                .attr("refDes")
                .or_else(|| pin.attr("component"))
                .unwrap_or("")
                .to_string();
            design.pads.push((owner, name, x, y, size));
        }

        Ok(design)
    }
}

/// Parses an IPC-2581 length like `"0.035"` (mm default) or `"35um"`.
fn parse_length(s: &str) -> Option<Length> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num, suffix) = s.split_at(split);
    let mm_per_unit = match suffix.to_lowercase().as_str() {
        "um" | "µm" => 1e-3,
        "mil" => 0.0254,
        "in" | "inch" => 25.4,
        "" => 1.0,
        _ => return None,
    };
    let v: f64 = num.trim().parse().ok()?;
    Some(Length::mm(v * mm_per_unit))
}

/// Collects the design's pads as geometry pads.
pub fn geometry_pads(design: &Ipc2581Design) -> Vec<Pad> {
    design
        .pads
        .iter()
        .enumerate()
        .map(|(i, (_, _, x, y, size))| Pad {
            id: i as u64,
            shape: PadShape::Circle {
                diameter: Length::meters(*size),
            },
            position: Point2::new(*x, *y),
            layer: 0,
            drill: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- IPC-2581 demo content -->
<Ecad name="demo">
  <StackupGroup name="stackup">
    <Stackup>
      <StepLayer name="F.Cu" thickness="0.035" material="COPPER"/>
      <StepLayer name="dielectric" thickness="0.2" material="FR4"/>
      <StepLayer name="B.Cu" thickness="0.035" material="COPPER"/>
    </Stackup>
  </StackupGroup>
  <LogNet id="1" name="GND"/>
  <LogNet id="2" name="VCC"/>
  <Component refDes="R1" x="10.0" y="20.0"/>
  <Component refDes="C1" x="30.0" y="5.0"/>
  <Pin refDes="R1" name="1" x="9.6" y="20.0" size="0.8"/>
  <Pin refDes="R1" name="2" x="10.4" y="20.0" size="0.8"/>
</Ecad>"#;

    #[test]
    fn xml_tokenizer_basics() {
        let doc = parse_xml("<a x='1'><b q=\"2\"/><c>t</c></a>").unwrap();
        assert_eq!(doc.tag, "a");
        assert_eq!(doc.attr("x"), Some("1"));
        assert_eq!(doc.children.len(), 2);
        assert_eq!(doc.child("B").unwrap().attr("q"), Some("2"));
        assert!(parse_xml("<a><b></a>").is_err());
        // comments tolerated
        assert!(parse_xml("<!-- hi --><a/>").is_ok());
    }

    #[test]
    fn parses_stackup_nets_components() {
        let d = Ipc2581Parser::parse(IPC).unwrap();
        assert_eq!(d.stackup.copper_layer_count(), 2);
        assert!((d.stackup.total_thickness.as_mm() - 0.27).abs() < 1e-6);
        assert_eq!(d.nets, vec!["GND", "VCC"]);
        assert_eq!(d.components.len(), 2);
        let r1 = &d.components[0];
        assert_eq!(r1.reference, "R1");
        assert!((r1.position.x - 10e-3).abs() < 1e-9);
        assert!((r1.position.y - 20e-3).abs() < 1e-9);
    }

    #[test]
    fn parses_pins() {
        let d = Ipc2581Parser::parse(IPC).unwrap();
        assert_eq!(d.pads.len(), 2);
        assert!((d.pads[0].2 - 9.6e-3).abs() < 1e-9);
        assert!((d.pads[0].4 - 0.8e-3).abs() < 1e-9);
        let pads = geometry_pads(&d);
        assert_eq!(pads.len(), 2);
        assert!(matches!(pads[0].shape, PadShape::Circle { .. }));
    }

    #[test]
    fn rejects_non_ipc_documents() {
        assert!(Ipc2581Parser::parse("<html><body/></html>").is_err());
        assert!(Ipc2581Parser::parse("").is_err());
    }

    #[test]
    fn length_parsing_units() {
        assert!((parse_length("0.035").unwrap().as_mm() - 0.035).abs() < 1e-12);
        assert!((parse_length("35um").unwrap().as_mm() - 0.035).abs() < 1e-9);
        assert!((parse_length("1.6").unwrap().as_mm() - 1.6).abs() < 1e-12);
        assert!((parse_length("40mil").unwrap().as_mm() - 1.016).abs() < 1e-9);
        assert!(parse_length("5ly").is_none());
    }
}
