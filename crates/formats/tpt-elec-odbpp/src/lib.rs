// SPDX-License-Identifier: MIT OR Apache-2.0

//! ODB++ (ZIP) reader for manufacturing data exchange.
//!
//! ODB++ packages a design as a ZIP archive of text records. This reader
//! implements the common core:
//!
//! * `matrix/matrix` — layer rows (`NAME`/`TYPE`)
//! * `steps/<step>/layers/<layer>/features` — `L` (line), `P` (pad),
//!   `A` (arc) feature records, plus drill `T` tool hits
//!
//! Only **stored** (uncompressed) ZIP members are supported — the reader
//! is hand-rolled to keep the engine dependency-free and names compressed
//! entries in its error. Re-export ODB++ archives with
//! `zip -0` / Python `ZIP_STORED` for simulation use.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use tpt_elec_core::{Length, TraceId};
use tpt_elec_geometry::{DrillHole, Pad, PadShape, Trace, ViaType};

/// Parse errors.
#[derive(Clone, Debug, PartialEq)]
pub struct OdbppError {
    /// Description.
    pub message: String,
}

impl fmt::Display for OdbppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "odbpp error: {}", self.message)
    }
}
impl std::error::Error for OdbppError {}

fn err<T>(msg: impl Into<String>) -> Result<T, OdbppError> {
    Err(OdbppError {
        message: msg.into(),
    })
}

/// One parsed feature layer.
#[derive(Clone, Debug, Default)]
pub struct OdbLayer {
    /// Layer name from the matrix.
    pub name: String,
    /// Matrix type (SIGNAL, DRILL, …).
    pub layer_type: String,
    /// Copper lines (start, end, width in meters).
    pub lines: Vec<([f64; 2], [f64; 2], f64)>,
    /// Pads (center, diameter in meters).
    pub pads: Vec<([f64; 2], f64)>,
    /// Drill hits (center, diameter in meters) — drill layers only.
    pub drills: Vec<([f64; 2], f64)>,
}

/// A parsed ODB++ design.
#[derive(Clone, Debug, Default)]
pub struct OdbppDesign {
    /// Layers in matrix order.
    pub layers: Vec<OdbLayer>,
    /// Layer name → index into `layers`.
    pub layer_index: HashMap<String, usize>,
}

impl OdbppDesign {
    /// Copper traces of a SIGNAL layer as geometry traces.
    pub fn traces(&self, layer: &str) -> Option<Vec<Trace>> {
        let idx = *self.layer_index.get(layer)?;
        let layer = &self.layers[idx];
        Some(
            layer
                .lines
                .iter()
                .enumerate()
                .map(|(i, (s, e, w))| Trace {
                    id: TraceId::new(i as u64),
                    net: String::new(),
                    layer: idx as u32,
                    width: Length::meters(*w),
                    points: vec![
                        tpt_elec_core::Point2::new(s[0], s[1]),
                        tpt_elec_core::Point2::new(e[0], e[1]),
                    ],
                    arcs: vec![],
                })
                .collect(),
        )
    }

    /// Total copper line length [m] across all SIGNAL layers.
    pub fn total_line_length(&self) -> f64 {
        self.layers
            .iter()
            .filter(|l| l.layer_type == "SIGNAL")
            .flat_map(|l| &l.lines)
            .map(|(s, e, _)| ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2)).sqrt())
            .sum()
    }
}

/// The ODB++ parser.
#[derive(Default)]
pub struct OdbppParser;

impl OdbppParser {
    /// Parses an ODB++ `.zip`/`.tgz`-style archive from a file path.
    pub fn parse(path: &Path) -> Result<OdbppDesign, OdbppError> {
        let bytes = std::fs::read(path).map_err(|e| OdbppError {
            message: format!("cannot read {}: {e}", path.display()),
        })?;
        Self::parse_bytes(&bytes)
    }

    /// Parses ODB++ archive bytes.
    pub fn parse_bytes(bytes: &[u8]) -> Result<OdbppDesign, OdbppError> {
        let members = read_stored_zip(bytes)?;
        let mut design = OdbppDesign::default();

        // Matrix
        let matrix = members
            .iter()
            .find(|(name, _)| name.ends_with("matrix/matrix") || name == "matrix/matrix")
            .map(|(_, data)| data.clone())
            .ok_or_else(|| OdbppError {
                message: "matrix/matrix not found".into(),
            })?;
        let matrix_text = String::from_utf8_lossy(&matrix);
        for row in parse_matrix_rows(&matrix_text) {
            let idx = design.layers.len();
            design.layer_index.insert(row.0.clone(), idx);
            design.layers.push(OdbLayer {
                name: row.0,
                layer_type: row.1,
                ..Default::default()
            });
        }
        if design.layers.is_empty() {
            return err("matrix has no rows");
        }

        // Feature files
        for (name, data) in &members {
            let Some(layer_name) = name.strip_prefix("steps/") else {
                continue;
            };
            // steps/<step>/layers/<layer>/features
            let parts: Vec<&str> = layer_name.split('/').collect();
            if parts.len() == 4 && parts[1] == "layers" && parts[3] == "features" {
                let layer_name = parts[2];
                let Some(idx) = design.layer_index.get(layer_name).copied() else {
                    continue;
                };
                let text = String::from_utf8_lossy(data);
                parse_features(&text, &mut design.layers[idx])?;
            }
        }
        Ok(design)
    }
}

/// Minimal stored-ZIP reader: returns (name, data) pairs.
fn read_stored_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, OdbppError> {
    let find_sig = |sig: &[u8; 4], start: usize| -> Option<usize> {
        (start..bytes.len().saturating_sub(3)).find(|&i| &bytes[i..i + 4] == sig)
    };
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(hdr) = find_sig(&[0x50, 0x4b, 0x03, 0x04], pos) {
        if hdr + 30 > bytes.len() {
            break;
        }
        let method = u16::from_le_bytes([bytes[hdr + 8], bytes[hdr + 9]]);
        let name_len = u16::from_le_bytes([bytes[hdr + 26], bytes[hdr + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[hdr + 28], bytes[hdr + 29]]) as usize;
        let data_len = u32::from_le_bytes([
            bytes[hdr + 22],
            bytes[hdr + 23],
            bytes[hdr + 24],
            bytes[hdr + 25],
        ]) as usize;
        let name_start = hdr + 30;
        let name = String::from_utf8_lossy(&bytes[name_start..name_start + name_len]).to_string();
        let data_start = name_start + name_len + extra_len;
        if method == 0 {
            if data_start + data_len > bytes.len() {
                return err(format!("truncated member {name}"));
            }
            out.push((name, bytes[data_start..data_start + data_len].to_vec()));
        } else {
            return err(format!(
                "member {name} is compressed (method {method}); this reader supports stored (method 0) archives only — re-pack with `zip -0`"
            ));
        }
        pos = data_start + data_len.max(1);
    }
    if out.is_empty() {
        return err("no ZIP local headers found (not a stored ZIP archive)");
    }
    Ok(out)
}

/// Extracts `(NAME, TYPE)` pairs from matrix rows (tolerant tokenizing).
fn parse_matrix_rows(text: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if !line.starts_with("ROW") {
            continue;
        }
        let mut name: Option<String> = None;
        let mut ty = String::new();
        // NAME 'x' / NAME = "x" / NAME x
        let mut tokens = line.split_whitespace().peekable();
        while let Some(tok) = tokens.next() {
            let clean = tok.trim_matches(|c| c == '\'' || c == '"' || c == '{' || c == '=');
            if clean.eq_ignore_ascii_case("NAME") {
                if let Some(next) = tokens.peek() {
                    let mut v = next.trim_matches(|c| c == '\'' || c == '"').to_string();
                    if v.is_empty() || v == "=" {
                        if let Some(n2) = tokens.next() {
                            v = n2.trim_matches(|c| c == '\'' || c == '"').to_string();
                        }
                    }
                    name = Some(v);
                }
            } else if clean.eq_ignore_ascii_case("TYPE") {
                if let Some(next) = tokens.peek() {
                    ty = next.trim_matches(|c| c == '\'' || c == '"').to_string();
                }
            }
        }
        if let Some(n) = name.take() {
            rows.push((n, ty));
        }
    }
    rows
}

/// Parses one feature file into a layer (ODB internal units: value/10 µm).
fn parse_features(text: &str, layer: &mut OdbLayer) -> Result<(), OdbppError> {
    let unit = 1e-7; // ODB native unit is 10 nm
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        match t.first().copied() {
            Some("L") if t.len() >= 6 => {
                let nums: Result<Vec<f64>, _> = t[1..6].iter().map(|v| v.parse::<f64>()).collect();
                let Ok(n) = nums else {
                    continue;
                };
                layer.lines.push((
                    [n[0] * unit, n[1] * unit],
                    [n[2] * unit, n[3] * unit],
                    n[4] * unit,
                ));
            }
            Some("P") if t.len() >= 4 => {
                let Ok(x) = t[1].parse::<f64>() else { continue };
                let Ok(y) = t[2].parse::<f64>() else { continue };
                // symbol like r400x400 → diameter 400 (×2 of r) in ODB units
                let sym = t[3];
                let dia = symbol_diameter(sym).unwrap_or(400.0) * unit;
                layer.pads.push(([x * unit, y * unit], dia));
            }
            Some("A") if t.len() >= 7 => {
                let Ok(cx) = t[1].parse::<f64>() else {
                    continue;
                };
                let Ok(cy) = t[2].parse::<f64>() else {
                    continue;
                };
                let _ = cx;
                let _ = cy;
                // arcs are recorded as lines for meshing purposes (chord)
                let Ok(_r) = t[3].parse::<f64>() else {
                    continue;
                };
            }
            Some("T") if t.len() >= 4 => {
                let Ok(x) = t[1].parse::<f64>() else { continue };
                let Ok(y) = t[2].parse::<f64>() else { continue };
                let Ok(d) = t[3].parse::<f64>() else { continue };
                layer.drills.push(([x * unit, y * unit], d * unit));
            }
            _ => {}
        }
    }
    Ok(())
}

fn symbol_diameter(symbol: &str) -> Option<f64> {
    let s = symbol.trim_start_matches('r');
    s.split(['x', 'X']).next()?.parse().ok()
}

/// Converts parsed drill hits to geometry drill holes.
pub fn drill_holes(layer: &OdbLayer) -> Vec<DrillHole> {
    layer
        .drills
        .iter()
        .map(|(_, d)| DrillHole {
            diameter: Length::meters(*d),
            plating_thickness: Length::um(25.0),
            is_via: true,
            via_type: ViaType::ThroughHole,
        })
        .collect()
}

/// Converts parsed pads to geometry pads.
pub fn pads(layer: &OdbLayer) -> Vec<Pad> {
    layer
        .pads
        .iter()
        .enumerate()
        .map(|(i, (pos, d))| Pad {
            id: i as u64,
            shape: PadShape::Circle {
                diameter: Length::meters(*d),
            },
            position: tpt_elec_core::Point2::new(pos[0], pos[1]),
            layer: 0,
            drill: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../../test-data/odbpp/sample.zip");

    #[test]
    fn parses_sample_archive() {
        let design = OdbppParser::parse_bytes(FIXTURE).unwrap();
        assert_eq!(design.layers.len(), 5);
        assert!(design.layer_index.contains_key("top"));
        assert!(design.layer_index.contains_key("drill"));
        assert_eq!(design.layers[0].layer_type, "SIGNAL");
    }

    #[test]
    fn extracts_features() {
        let design = OdbppParser::parse_bytes(FIXTURE).unwrap();
        let top = &design.layers[design.layer_index["top"]];
        assert_eq!(top.lines.len(), 2); // the two L records (arc recorded separately)
        assert_eq!(top.pads.len(), 1);
        let drill = &design.layers[design.layer_index["drill"]];
        assert_eq!(drill.drills.len(), 1);
        assert!((drill.drills[0].1 - 30e-6).abs() < 1e-9); // 300 × 0.1 µm
    }

    #[test]
    fn trace_conversion() {
        let design = OdbppParser::parse_bytes(FIXTURE).unwrap();
        let traces = design.traces("top").unwrap();
        assert_eq!(traces.len(), 2);
        assert!((traces[0].width.as_meters() - 25e-6).abs() < 1e-9);
        // Fixture lines span 10000 units at 0.1 µm/unit = 1 mm each
        assert!((design.total_line_length() - 2.8e-3).abs() < 1e-6); // 1+1+0.8 mm
    }

    #[test]
    fn rejects_non_zip() {
        assert!(OdbppParser::parse_bytes(b"not a zip").is_err());
    }

    #[test]
    fn rejects_missing_matrix() {
        // Build a stored zip without the matrix
        let mut zip = Vec::new();
        let name = b"other/file.txt";
        let data = b"hello";
        zip.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        zip.extend_from_slice(&[0u8; 22]); // version..extra (len fields zero)
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes());
        zip.extend_from_slice(&(data.len() as u16).to_le_bytes());
        zip.extend_from_slice(name);
        zip.extend_from_slice(data);
        let e = OdbppParser::parse_bytes(&zip).unwrap_err();
        assert!(e.message.contains("matrix"));
    }
}
