// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gerber RS-274X and Excellon drill file parser.
//!
//! Implements the industry-standard subset of [The Gerber File Format
//! Specification (RS-274X)] needed for thermal/geometry extraction:
//! coordinate format (`%FS`), units (`%MO`), aperture definitions (`%AD`),
//! aperture macros (`%AM`), D01/D02/D03 operations, linear and circular
//! interpolation (`G01/G02/G03`), regions (`G36/G37`), polarity (`%LP`), and
//! end of file (`M02`).
//!
//! Aperture-macro flashes are approximated by their bounding primitives for
//! meshing purposes; full macro expression evaluation is intentionally out of
//! scope for thermal extraction.
//!
//! [spec]: https://www.ucamco.com/en/girbserber

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;

use tpt_elec_core::{Angle, Length, Point2};

/// Parser errors with the offending command.
#[derive(Clone, Debug, PartialEq)]
pub struct GerberError {
    /// Human-readable problem description.
    pub message: String,
    /// The command that failed to parse.
    pub command: String,
}

impl fmt::Display for GerberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "gerber parse error at `{}`: {}",
            self.command, self.message
        )
    }
}

impl std::error::Error for GerberError {}

/// Coordinate units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// Millimeters.
    Millimeters,
    /// Inches.
    Inches,
}

/// Zero-omission mode of the coordinate format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZeroOmission {
    /// Leading zeros omitted (`L`).
    Leading,
    /// Trailing zeros omitted (`T`).
    Trailing,
}

/// `%FS` coordinate format: number of integer/decimal digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    /// Digits before the decimal point (1–6).
    pub integer: u32,
    /// Digits after the decimal point (4–6 typical).
    pub decimal: u32,
    /// Zero-omission mode.
    pub omission: ZeroOmission,
}

impl Default for FormatSpec {
    fn default() -> Self {
        Self {
            integer: 3,
            decimal: 6,
            omission: ZeroOmission::Leading,
        }
    }
}

/// Aperture definitions.
#[derive(Clone, Debug, PartialEq)]
pub enum Aperture {
    /// `C` — round.
    Circle {
        /// Diameter [m].
        diameter: Length,
        /// Optional drill hole [m].
        hole: Option<Length>,
    },
    /// `R` — rectangle.
    Rectangle {
        /// X extent [m].
        width: Length,
        /// Y extent [m].
        height: Length,
        /// Optional drill hole [m].
        hole: Option<Length>,
    },
    /// `O` — obround.
    Obround {
        /// X extent [m].
        width: Length,
        /// Y extent [m].
        height: Length,
    },
    /// `P` — regular polygon.
    Polygon {
        /// Number of vertices (3–12).
        vertices: u32,
        /// Circumscribed circle diameter [m].
        diameter: Length,
        /// Rotation.
        rotation: Angle,
    },
    /// Referenced aperture macro.
    Macro {
        /// Macro name.
        name: String,
        /// Modified parameters from the `%AD` line.
        parameters: Vec<f64>,
    },
}

impl Aperture {
    /// Approximate footprint half-extent [m] (used for meshing).
    pub fn approx_half_extent(&self) -> f64 {
        match self {
            Aperture::Circle { diameter, .. } => diameter.as_meters() / 2.0,
            Aperture::Rectangle { width, height, .. } | Aperture::Obround { width, height } => {
                width.as_meters().max(height.as_meters()) / 2.0
            }
            Aperture::Polygon { diameter, .. } => diameter.as_meters() / 2.0,
            Aperture::Macro { .. } => 1.0e-3,
        }
    }
}

/// Image polarity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Polarity {
    /// `D` — draw copper.
    #[default]
    Dark,
    /// `C` — clear (remove).
    Clear,
}

/// Interpolation mode between coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterpolationMode {
    /// `G01` straight line.
    #[default]
    Linear,
    /// `G02` clockwise arc.
    ClockwiseCircular,
    /// `G03` counterclockwise arc.
    CounterclockwiseCircular,
}

/// One aperture-macro primitive (simplified storage).
#[derive(Clone, Debug, PartialEq)]
pub struct MacroPrimitive {
    /// Gerber primitive code (1 = circle, 2/20 = line/vector line,
    /// 21 = rectangle, 4/22 = outline, …).
    pub code: f64,
    /// Raw numeric parameters.
    pub parameters: Vec<f64>,
}

/// A defined aperture macro.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApertureMacro {
    /// Macro name.
    pub name: String,
    /// Primitives in definition order.
    pub primitives: Vec<MacroPrimitive>,
}

/// A graphical primitive extracted from the image.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// D01 in linear mode.
    Line {
        /// Start [m].
        start: Point2,
        /// End [m].
        end: Point2,
        /// Trace width [m] (current aperture extent).
        width: Length,
    },
    /// D01 in circular mode.
    Arc {
        /// Start [m].
        start: Point2,
        /// End [m].
        end: Point2,
        /// Arc center [m].
        center: Point2,
        /// Sweep direction.
        clockwise: bool,
        /// Trace width [m].
        width: Length,
    },
    /// D03 aperture stamp.
    Flash {
        /// Position [m].
        position: Point2,
        /// Aperture used.
        aperture: Aperture,
    },
    /// G36/G37 closed region.
    Region {
        /// Outline vertices in draw order [m].
        boundary: Vec<Point2>,
        /// Region polarity.
        polarity: Polarity,
    },
}

/// An aperture id reference (D-code number).
pub type ApertureId = u32;

/// Parser state and the result of parsing.
#[derive(Clone, Debug, Default)]
pub struct GerberState {
    /// Units in effect.
    pub units: Option<Units>,
    /// Coordinate format in effect.
    pub format: Option<FormatSpec>,
    /// Defined aperture macros by name.
    pub aperture_macros: HashMap<String, ApertureMacro>,
    /// Defined apertures by D-code.
    pub apertures: HashMap<ApertureId, Aperture>,
    /// Current polarity.
    pub polarity: Polarity,
    /// Current interpolation mode.
    pub interpolation: InterpolationMode,
}

/// The parsed image of one Gerber layer.
#[derive(Clone, Debug, Default)]
pub struct ParsedGerber {
    /// Free-comment layer name from `G04` (first comment line).
    pub layer_name: String,
    /// Units the file declared.
    pub units: Option<Units>,
    /// Extracted primitives in draw order.
    pub primitives: Vec<Primitive>,
    /// Apertures the file defined.
    pub apertures: HashMap<ApertureId, Aperture>,
}

impl ParsedGerber {
    /// Image bounding box [m] including aperture extents.
    pub fn bounding_box(&self) -> Option<(Point2, Point2)> {
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut any = false;
        let mut grow = |p: Point2, r: f64| {
            min.x = min.x.min(p.x - r);
            min.y = min.y.min(p.y - r);
            max.x = max.x.max(p.x + r);
            max.y = max.y.max(p.y + r);
            any = true;
        };
        for prim in &self.primitives {
            match prim {
                Primitive::Line { start, end, width } => {
                    let r = width.as_meters() / 2.0;
                    grow(*start, r);
                    grow(*end, r);
                }
                Primitive::Arc {
                    start, end, center, ..
                } => {
                    let r = center.distance_to(start).max(center.distance_to(end));
                    grow(*center, r);
                    grow(*start, 0.0);
                    grow(*end, 0.0);
                }
                Primitive::Flash { position, aperture } => {
                    grow(*position, aperture.approx_half_extent());
                }
                Primitive::Region { boundary, .. } => {
                    for p in boundary {
                        grow(*p, 0.0);
                    }
                }
            }
        }
        if any {
            Some((min, max))
        } else {
            None
        }
    }
}

/// The Gerber/Excellon parser.
#[derive(Default)]
pub struct GerberParser {
    state: GerberState,
}

impl GerberParser {
    /// Parses a Gerber RS-274X file.
    pub fn parse(input: &str) -> Result<ParsedGerber, GerberError> {
        let mut parser = GerberParser::default();
        parser.parse_inner(input)
    }

    fn parse_inner(&mut self, input: &str) -> Result<ParsedGerber, GerberError> {
        let mut result = ParsedGerber::default();
        let mut current_pos = Point2::new(0.0, 0.0);
        let mut current_aperture: Option<ApertureId> = None;
        let mut region_mode = false;
        let mut region_points: Vec<Point2> = Vec::new();
        let mut region_polarity = Polarity::Dark;
        let mut first_comment: Option<String> = None;

        for raw in input.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            // Extended commands: multiple %...% blocks can share a line.
            if line.starts_with('%') {
                let blocks = split_extended(raw);
                for block in blocks {
                    if block.starts_with("AM") {
                        let (name, prims) = parse_macro(&block)?;
                        self.state.aperture_macros.insert(
                            name.clone(),
                            ApertureMacro {
                                name,
                                primitives: prims,
                            },
                        );
                        continue;
                    }
                    let body = block.trim_end_matches('*');
                    if body.is_empty() {
                        continue;
                    }
                    if body.starts_with("FS") {
                        self.state.format = Some(parse_fs(body)?);
                    } else if body.starts_with("MO") {
                        self.state.units = match body.trim_start_matches("MO") {
                            "MM" => Some(Units::Millimeters),
                            "IN" => Some(Units::Inches),
                            other => return Err(err(&format!("unknown units {other:?}"), &block)),
                        };
                    } else if body.starts_with("AD") {
                        let (dcode, aperture) = parse_ad(body, &self.state.aperture_macros)?;
                        self.state.apertures.insert(dcode, aperture);
                    } else if body.starts_with("LP") {
                        self.state.polarity = match body.trim_start_matches("LP") {
                            "D" => Polarity::Dark,
                            "C" => Polarity::Clear,
                            other => return Err(err("bad LP polarity", other)),
                        };
                    }
                    // Other extended commands (SR, OF, IP, IR, AS, MI, SF…)
                    // do not affect geometry extraction and are ignored.
                }
                continue;
            }

            // In-function commands
            let cmd = line.trim_end_matches('*').trim();
            if cmd.is_empty() {
                continue;
            }
            if cmd.starts_with("G04") || cmd.starts_with("G4") {
                if first_comment.is_none() {
                    first_comment = Some(cmd.trim_start_matches("G04").trim().to_string());
                }
                continue;
            }
            if cmd.starts_with('G') && !cmd.starts_with("G0") && !cmd.starts_with("G3") {
                // Any other G we ignore (G54 legacy prefix handled below).
            }
            // G-codes may be prefixed to an operation, e.g. `G01X…D01*`
            let mut work = cmd.to_string();
            for g in [
                "G03", "G02", "G01", "G54", "G75", "G74", "G36", "G37", "G70", "G71", "G90", "G91",
            ] {
                if work.starts_with(g) {
                    match g {
                        "G01" => self.state.interpolation = InterpolationMode::Linear,
                        "G02" => self.state.interpolation = InterpolationMode::ClockwiseCircular,
                        "G03" => {
                            self.state.interpolation = InterpolationMode::CounterclockwiseCircular
                        }
                        "G36" => {
                            region_mode = true;
                            region_points.clear();
                            region_polarity = self.state.polarity;
                        }
                        "G37" => {
                            if region_mode && region_points.len() >= 3 {
                                result.primitives.push(Primitive::Region {
                                    boundary: region_points.clone(),
                                    polarity: region_polarity,
                                });
                            }
                            region_mode = false;
                            region_points.clear();
                        }
                        _ => {}
                    }
                    work = work[g.len()..].trim().to_string();
                    break;
                }
            }
            if work.is_empty() {
                continue;
            }
            if work == "M02" || work == "M00" || work == "M30" {
                break;
            }
            // D-code only (aperture select): "D10"
            if let Some(rest) = work.strip_prefix('D') {
                if let Ok(d) = rest.parse::<u32>() {
                    if matches!(d, 1..=3) {
                        // operation without coordinates: use current pos
                        let op = d;
                        self.apply_operation(
                            op,
                            current_pos,
                            None,
                            &mut current_pos,
                            &mut current_aperture,
                            &mut result,
                            region_mode,
                            &mut region_points,
                        )?;
                    } else if self.state.apertures.contains_key(&d) {
                        current_aperture = Some(d);
                    }
                    continue;
                }
            }
            if work.starts_with('X') || work.starts_with('Y') {
                let (op, coords) = split_operation(&work)?;
                let (pos, offsets) = self.to_meters(&coords)?;
                self.apply_operation(
                    op,
                    pos,
                    offsets,
                    &mut current_pos,
                    &mut current_aperture,
                    &mut result,
                    region_mode,
                    &mut region_points,
                )?;
            }
        }

        result.layer_name = first_comment.unwrap_or_default();
        result.units = self.state.units;
        result.apertures = self.state.apertures.clone();
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_operation(
        &self,
        op: u32,
        new_pos: Point2,
        offsets: Option<(f64, f64)>,
        current_pos: &mut Point2,
        current_aperture: &mut Option<ApertureId>,
        result: &mut ParsedGerber,
        region_mode: bool,
        region_points: &mut Vec<Point2>,
    ) -> Result<(), GerberError> {
        let width = current_aperture
            .and_then(|d| self.state.apertures.get(&d))
            .map(|a| Length::meters(2.0 * a.approx_half_extent()))
            .unwrap_or_else(|| Length::mm(0.1));
        match op {
            1 => {
                if region_mode {
                    if region_points.is_empty() {
                        region_points.push(*current_pos);
                    }
                    region_points.push(new_pos);
                } else {
                    let prim = match self.state.interpolation {
                        InterpolationMode::Linear => Primitive::Line {
                            start: *current_pos,
                            end: new_pos,
                            width,
                        },
                        InterpolationMode::ClockwiseCircular
                        | InterpolationMode::CounterclockwiseCircular => {
                            // Center comes from I/J offsets; fall back to the
                            // chord midpoint when a file omits them.
                            let clockwise = matches!(
                                self.state.interpolation,
                                InterpolationMode::ClockwiseCircular
                            );
                            let center = match offsets {
                                Some((di, dj)) => {
                                    Point2::new(current_pos.x + di, current_pos.y + dj)
                                }
                                None => arc_center_from(*current_pos, new_pos),
                            };
                            Primitive::Arc {
                                start: *current_pos,
                                end: new_pos,
                                center,
                                clockwise,
                                width,
                            }
                        }
                    };
                    result.primitives.push(prim);
                }
            }
            2 => {
                if region_mode && region_points.len() < 2 {
                    region_points.clear();
                }
            }
            3 if !region_mode => {
                let aperture = current_aperture
                    .and_then(|d| self.state.apertures.get(&d))
                    .cloned()
                    .unwrap_or(Aperture::Circle {
                        diameter: Length::mm(0.5),
                        hole: None,
                    });
                result.primitives.push(Primitive::Flash {
                    position: new_pos,
                    aperture,
                });
            }
            _ => {}
        }
        *current_pos = new_pos;
        Ok(())
    }

    /// Converts parsed integer coordinates to meters using the active format.
    /// Returns the position and the I/J arc offsets (in meters), if present.
    fn to_meters(&self, coords: &Coords) -> Result<(Point2, Option<(f64, f64)>), GerberError> {
        let fmt = self.state.format.ok_or_else(|| {
            err(
                "coordinate seen before %FS format specification",
                "coordinate",
            )
        })?;
        let unit_scale = match self.state.units.unwrap_or(Units::Millimeters) {
            Units::Millimeters => 1.0e-3,
            Units::Inches => 0.0254,
        };
        let scale = 10f64.powi(fmt.decimal as i32);
        let conv = |raw: Option<i64>| raw.map(|v| v as f64 / scale * unit_scale);
        let pos = Point2::new(conv(coords.x).unwrap_or(0.0), conv(coords.y).unwrap_or(0.0));
        let offsets = match (coords.i, coords.j) {
            (Some(i), Some(j)) => {
                Some((i as f64 / scale * unit_scale, j as f64 / scale * unit_scale))
            }
            (Some(i), None) => Some((i as f64 / scale * unit_scale, 0.0)),
            (None, Some(j)) => Some((0.0, j as f64 / scale * unit_scale)),
            (None, None) => None,
        };
        Ok((pos, offsets))
    }
}

#[derive(Debug)]
struct Coords {
    x: Option<i64>,
    y: Option<i64>,
    i: Option<i64>,
    j: Option<i64>,
    #[allow(dead_code)] // written by split_operation, read via the returned op
    op: u32,
}

fn err(message: &str, command: &str) -> GerberError {
    GerberError {
        message: message.to_string(),
        command: command.to_string(),
    }
}

/// Splits a raw line into `%…%` extended blocks (handling multi-block lines).
fn split_extended(raw: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = raw.trim();
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            blocks.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            // unterminated; treat the rest as one block
            blocks.push(after.to_string());
            break;
        }
    }
    blocks
        .into_iter()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect()
}

fn parse_fs(body: &str) -> Result<FormatSpec, GerberError> {
    // e.g. FSLAX36Y36
    let spec = body.trim_start_matches("FS");
    let omission = if spec.contains('L') {
        ZeroOmission::Leading
    } else if spec.contains('T') {
        ZeroOmission::Trailing
    } else {
        ZeroOmission::Leading
    };
    let take_digits = |s: &str| -> Result<(u32, u32), GerberError> {
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() != 2 {
            return Err(err("FS needs two digits after X/Y", body));
        }
        Ok((
            digits.as_bytes()[0] as char as u32 - '0' as u32,
            digits.as_bytes()[1] as char as u32 - '0' as u32,
        ))
    };
    let xi = spec.find('X').ok_or_else(|| err("missing X in FS", body))?;
    let yi = spec.find('Y').ok_or_else(|| err("missing Y in FS", body))?;
    let (integer, decimal) = take_digits(&spec[xi + 1..yi])?;
    Ok(FormatSpec {
        integer,
        decimal,
        omission,
    })
}

fn parse_number(s: &str) -> Result<f64, GerberError> {
    s.parse::<f64>().map_err(|_| err("bad number", s))
}

fn parse_ad(
    body: &str,
    macros: &HashMap<String, ApertureMacro>,
) -> Result<(u32, Aperture), GerberError> {
    // ADD10C,0.25  |  ADD12R,1.2X0.8  |  ADD15O,2X1  |  ADD16P,6X2.4X30  |  ADD20MYMACRO,0.5
    let rest = body.trim_start_matches("AD");
    let dcode_str: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let dcode = dcode_str
        .parse::<u32>()
        .map_err(|_| err("bad D-code in AD", body))?;
    let after_d = &rest[dcode_str.len() + rest.find(|c: char| c.is_ascii_digit()).unwrap_or(0)..];
    let after_d = after_d.trim_start_matches(|c: char| c.is_ascii_digit());
    let (template, params_str) = match after_d.split_once(',') {
        Some((t, p)) => (t, Some(p)),
        None => (after_d, None),
    };
    let template = template.trim();
    let mut params: Vec<f64> = Vec::new();
    if let Some(p) = params_str {
        for part in p.split('X') {
            params.push(parse_number(part.trim())?);
        }
    }
    let aperture = match template.chars().next() {
        Some('C') => Aperture::Circle {
            diameter: Length::mm(params.first().copied().unwrap_or(0.0)),
            hole: params.get(1).map(|h| Length::mm(*h)),
        },
        Some('R') => Aperture::Rectangle {
            width: Length::mm(params.first().copied().unwrap_or(0.0)),
            height: Length::mm(params.get(1).copied().unwrap_or(0.0)),
            hole: params.get(2).map(|h| Length::mm(*h)),
        },
        Some('O') => Aperture::Obround {
            width: Length::mm(params.first().copied().unwrap_or(0.0)),
            height: Length::mm(params.get(1).copied().unwrap_or(0.0)),
        },
        Some('P') => Aperture::Polygon {
            vertices: params.first().copied().unwrap_or(6.0) as u32,
            diameter: Length::mm(params.get(1).copied().unwrap_or(0.0)),
            rotation: Angle::degrees(params.get(2).copied().unwrap_or(0.0)),
        },
        _ => {
            let name = template
                .trim_start_matches(|c: char| c.is_ascii_uppercase())
                .to_string();
            let name = if name.is_empty() {
                template.to_string()
            } else {
                name
            };
            if !macros.contains_key(&name) {
                return Err(err("AD references unknown macro", body));
            }
            Aperture::Macro {
                name,
                parameters: params,
            }
        }
    };
    Ok((dcode, aperture))
}

fn parse_macro(block: &str) -> Result<(String, Vec<MacroPrimitive>), GerberError> {
    // AMNAME*1,1,1.5,0,0*21,1,2.0X1.0,0,0,0*%
    let mut parts = block.split('*');
    let header = parts.next().unwrap_or_default();
    let name = header.trim_start_matches("AM").to_string();
    if name.is_empty() {
        return Err(err("macro missing name", block));
    }
    let mut prims = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let fields: Vec<&str> = part.split(',').collect();
        if fields.len() >= 2 {
            if let Ok(code) = fields[0].trim().parse::<f64>() {
                let params = fields[1..]
                    .iter()
                    .filter_map(|f| f.trim().parse::<f64>().ok())
                    .collect();
                prims.push(MacroPrimitive {
                    code,
                    parameters: params,
                });
            }
        }
    }
    Ok((name, prims))
}

fn split_operation(cmd: &str) -> Result<(u32, Coords), GerberError> {
    let mut x = None;
    let mut y = None;
    let mut i = None;
    let mut j = None;
    let mut op = 0u32;
    let mut idx = 0;
    let bytes: Vec<char> = cmd.chars().collect();
    while idx < bytes.len() {
        match bytes[idx] {
            'X' | 'Y' | 'I' | 'J' => {
                let key = bytes[idx];
                idx += 1;
                let start = idx;
                while idx < bytes.len()
                    && (bytes[idx].is_ascii_digit() || bytes[idx] == '-' || bytes[idx] == '+')
                {
                    idx += 1;
                }
                let raw: String = bytes[start..idx].iter().collect();
                let v = raw.parse::<i64>().map_err(|_| err("bad coordinate", cmd))?;
                match key {
                    'X' => x = Some(v),
                    'Y' => y = Some(v),
                    'I' => i = Some(v),
                    'J' => j = Some(v),
                    _ => {}
                }
            }
            'D' => {
                let start = idx + 1;
                while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                    idx += 1;
                }
                let raw: String = if idx > start {
                    bytes[start..idx].iter().collect()
                } else {
                    // "D01" consumed the D then digits above
                    idx = start;
                    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                        idx += 1;
                    }
                    bytes[start..idx].iter().collect()
                };
                op = raw.parse::<u32>().map_err(|_| err("bad D op", cmd))?;
            }
            _ => idx += 1,
        }
    }
    Ok((op, Coords { x, y, i, j, op }))
}

/// Fallback center for arcs when I/J offsets are absent: the chord midpoint.
fn arc_center_from(start: Point2, end: Point2) -> Point2 {
    Point2::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0)
}

/// An Excellon drill hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrillHit {
    /// Tool number used.
    pub tool: u32,
    /// X position [m].
    pub x: f64,
    /// Y position [m].
    pub y: f64,
    /// Finished hole diameter [m].
    pub diameter: Length,
}

/// A parsed Excellon drill file.
#[derive(Clone, Debug, Default)]
pub struct ParsedDrill {
    /// Tools with their diameters.
    pub tools: HashMap<u32, Length>,
    /// All hits in file order.
    pub hits: Vec<DrillHit>,
    /// Units declared by the file.
    pub units: Option<Units>,
}

impl GerberParser {
    /// Parses an Excellon drill file (`.drl`, `.xln`, `.txt`).
    pub fn parse_drill(input: &str) -> Result<ParsedDrill, GerberError> {
        let mut result = ParsedDrill::default();
        let mut metric = true;
        let mut current_tool: Option<u32> = None;
        for raw in input.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            let up = line.to_uppercase();
            if up.starts_with("METRIC") || up.contains("FMAT,2") && up.starts_with("METRIC") {
                metric = true;
                result.units = Some(Units::Millimeters);
                continue;
            }
            if up.starts_with("INCH") {
                metric = false;
                result.units = Some(Units::Inches);
                continue;
            }
            if up.starts_with("M48")
                || up.starts_with("%")
                || up.starts_with("M95")
                || up.starts_with("M30")
                || up.starts_with("G90")
                || up.starts_with("G05")
                || up.starts_with("G81")
                || up.starts_with("T0C")
                || up.starts_with("M00")
            {
                continue;
            }
            if up.starts_with('T') && (up.contains('C') || up.contains('F') || up.contains('S')) {
                // Tool definition: T1C0.300F200S80
                let num: String = up[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
                let tool = num
                    .parse::<u32>()
                    .map_err(|_| err("bad tool number", line))?;
                if let Some(ci) = up.find('C') {
                    let dia_str: String = up[ci + 1..]
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                        .collect();
                    let dia_mm = dia_str
                        .parse::<f64>()
                        .map_err(|_| err("bad tool diameter", line))?;
                    let dia = if metric {
                        Length::mm(dia_mm)
                    } else {
                        Length::meters(dia_mm * 0.0254)
                    };
                    result.tools.insert(tool, dia);
                }
                continue;
            }
            if let Some(rest) = up.strip_prefix('T') {
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                current_tool = num.parse::<u32>().ok();
                continue;
            }
            if up.starts_with('X') || up.starts_with('Y') {
                let (x, y) = parse_drill_hit_coords(&up, metric);
                let tool = current_tool.unwrap_or(0);
                let diameter = result.tools.get(&tool).copied().unwrap_or(Length::ZERO);
                result.hits.push(DrillHit {
                    tool,
                    x,
                    y,
                    diameter,
                });
            }
        }
        Ok(result)
    }
}

fn parse_drill_coord(s: &str, metric: bool) -> f64 {
    let v: f64 = s.parse().unwrap_or(0.0);
    if s.contains('.') {
        // explicit decimal: already in file units
        if metric {
            Length::mm(v).as_meters()
        } else {
            v * 0.0254
        }
    } else {
        // implicit decimals: metric 3.3 format (1e-3 mm), inch 2.4
        if metric {
            Length::mm(v / 1000.0).as_meters()
        } else {
            v / 10_000.0 * 0.0254
        }
    }
}

/// Extracts an `X…`/`Y…` drill coordinate pair from a hit line.
fn parse_drill_hit_coords(line: &str, metric: bool) -> (f64, f64) {
    let mut x = 0.0;
    let mut y = 0.0;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'X' || chars[i] == 'Y' {
            let axis = chars[i];
            let start = i + 1;
            let mut j = start;
            while j < chars.len()
                && (chars[j].is_ascii_digit() || chars[j] == '.' || chars[j] == '-')
            {
                j += 1;
            }
            let s: String = chars[start..j].iter().collect();
            let v = parse_drill_coord(&s, metric);
            if axis == 'X' {
                x = v;
            } else {
                y = v;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_geometry::polygon_area;

    const SAMPLE: &str = "\
G04 simple top copper*
%FSLAX36Y36*%
%MOMM*%
%ADD10C,0.250*%
%ADD11R,1.2X0.8*%
%ADD12O,2.0X1.0*%
%ADD13P,6X2.4X30.0*%
D10*
X0Y0D02*
X10000000Y0D01*
D11*
X20000000Y0D03*
D12*
X20000000Y10000000D03*
D13*
X0Y10000000D03*
D10*
X0Y0D02*
G75*
G03X10000000Y10000000I10000000J0D01*
%AMTESTMACRO*1,1,1.5,0,0*21,1,2.0X1.0,0,0,0*%
%ADD20TESTMACRO,0.5*%
D20*
X30000000Y30000000D03*
M02*
";

    const REGION: &str = "\
%FSLAX36Y36*%
%MOMM*%
%LPD*%
G36*
X0Y0D02*
X10000000Y0D01*
X10000000Y10000000D01*
X0Y10000000D01*
X0Y0D01*
G37*
M02*
";

    #[test]
    fn parses_format_units_and_apertures() {
        let p = GerberParser::parse(SAMPLE).unwrap();
        assert_eq!(p.units, Some(Units::Millimeters));
        assert_eq!(p.layer_name, "simple top copper");
        assert_eq!(p.apertures.len(), 5);
        assert_eq!(
            p.apertures.get(&10),
            Some(&Aperture::Circle {
                diameter: Length::mm(0.25),
                hole: None
            })
        );
        assert_eq!(
            p.apertures.get(&12),
            Some(&Aperture::Obround {
                width: Length::mm(2.0),
                height: Length::mm(1.0)
            })
        );
        match p.apertures.get(&13) {
            Some(Aperture::Polygon {
                vertices,
                diameter,
                rotation,
            }) => {
                assert_eq!(*vertices, 6);
                assert!((diameter.as_mm() - 2.4).abs() < 1e-9);
                assert!((rotation.as_degrees() - 30.0).abs() < 1e-9);
            }
            other => panic!("bad polygon aperture {other:?}"),
        }
        // macro aperture registered
        assert!(p.apertures.contains_key(&20));
    }

    #[test]
    fn parses_lines_flashes_and_arc() {
        let p = GerberParser::parse(SAMPLE).unwrap();
        let lines: Vec<_> = p
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Line { .. }))
            .collect();
        assert_eq!(lines.len(), 1);
        if let Primitive::Line { start, end, width } = lines[0] {
            assert!((start.x).abs() < 1e-12 && start.y.abs() < 1e-12);
            assert!((end.x - 10.0e-3).abs() < 1e-12); // 10000000 / 1e6 = 10 mm
            assert!((width.as_mm() - 0.25).abs() < 1e-9);
        }
        let flashes: Vec<_> = p
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Flash { .. }))
            .collect();
        assert_eq!(flashes.len(), 4); // R, O, P, macro
        let arcs: Vec<_> = p
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Arc { .. }))
            .collect();
        assert_eq!(arcs.len(), 1);
        if let Primitive::Arc {
            start,
            end,
            center,
            clockwise,
            ..
        } = arcs[0]
        {
            assert!((center.x - 10.0e-3).abs() < 1e-12);
            assert!(!*clockwise);
            assert!(
                (start.distance_to(end) - (10.0e3_f64).sqrt() * 1e-3 * 10f64.sqrt()).abs() > 0.0
            );
        }
    }

    #[test]
    fn parses_macro_definition() {
        let p = GerberParser::parse(SAMPLE).unwrap();
        // macro was parsed and registered in state → aperture D20 exists
        assert!(
            matches!(p.apertures.get(&20), Some(Aperture::Macro { name, .. }) if name == "TESTMACRO")
        );
    }

    #[test]
    fn parses_region() {
        let p = GerberParser::parse(REGION).unwrap();
        let regions: Vec<_> = p
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Region { .. }))
            .collect();
        assert_eq!(regions.len(), 1);
        if let Primitive::Region { boundary, polarity } = regions[0] {
            assert_eq!(boundary.len(), 5);
            assert_eq!(*polarity, Polarity::Dark);
            let area = polygon_area(boundary);
            assert!((area - 1.0e-4).abs() < 1e-9); // 10mm × 10mm
        }
    }

    #[test]
    fn bounding_box_covers_image() {
        let p = GerberParser::parse(SAMPLE).unwrap();
        let (min, max) = p.bounding_box().unwrap();
        assert!(min.x <= 0.0 && min.y <= 0.0);
        assert!(max.x >= 30.0e-3 && max.y >= 30.0e-3);
    }

    #[test]
    fn error_on_missing_fs() {
        let r = GerberParser::parse("X0Y0D02*\nX10D01*\nM02*");
        assert!(r.is_err());
        let e = r.unwrap_err();
        assert!(e.message.contains("%FS"));
    }

    const DRILL: &str = "\
; EXCELLON V1.0
M48
METRIC
T1C0.300
T2C0.800
%
T1
X10.0Y5.0
X20.0Y5.0
T2
X10.0Y15.0
M30
";

    #[test]
    fn parses_excellon_drill() {
        let d = GerberParser::parse_drill(DRILL).unwrap();
        assert_eq!(d.units, Some(Units::Millimeters));
        assert_eq!(d.tools.len(), 2);
        assert!((d.tools.get(&1).unwrap().as_mm() - 0.3).abs() < 1e-9);
        assert!((d.tools.get(&2).unwrap().as_mm() - 0.8).abs() < 1e-9);
        assert_eq!(d.hits.len(), 3);
        assert_eq!(d.hits[0].tool, 1);
        assert!((d.hits[0].x - 10.0e-3).abs() < 1e-9);
        assert!((d.hits[0].y - 5.0e-3).abs() < 1e-9);
        assert!((d.hits[0].diameter.as_mm() - 0.3).abs() < 1e-9);
        assert_eq!(d.hits[2].tool, 2);
    }

    #[test]
    fn parses_implicit_decimal_drill() {
        let d = GerberParser::parse_drill("M48\nMETRIC\nT1C0.3\n%\nT1\nX1000Y500\nM30\n").unwrap();
        assert!((d.hits[0].x - 1.0e-3).abs() < 1e-12);
        assert!((d.hits[0].y - 0.5e-3).abs() < 1e-12);
    }

    #[test]
    fn split_extended_handles_multiple_blocks() {
        let blocks = split_extended("%FSLAX36Y36*%%MOMM*%");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "FSLAX36Y36*");
    }
}
