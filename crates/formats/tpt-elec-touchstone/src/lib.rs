// SPDX-License-Identifier: MIT OR Apache-2.0

//! Touchstone S-parameter file parser.
//!
//! Supports the Touchstone 1.x keyword header (`# <unit> <type> <format>
//! R <ref>`), frequency in Hz/kHz/MHz/GHz, S/Y/Z parameter types in
//! magnitude-angle (MA), dB-angle (DB), or real-imaginary (RI) form, and
//! both one-port (`.s1p`) and two-port (`.s2p`) files. Two-port data may be
//! in the 3-lines-per-point (split) layout as well as the classic 9-column
//! row. Comments (`!`) and blank lines are ignored.
//!
//! Data is normalized to complex S-parameters in [`SParameterMatrix`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;

use tpt_elec_core::Complex;
use tpt_elec_si_core::{SParameterMatrix, SParameters};

/// Parse errors.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchstoneError {
    /// Description with line context.
    pub message: String,
}

impl fmt::Display for TouchstoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "touchstone error: {}", self.message)
    }
}
impl std::error::Error for TouchstoneError {}

fn err<T>(msg: impl Into<String>) -> Result<T, TouchstoneError> {
    Err(TouchstoneError {
        message: msg.into(),
    })
}

/// Frequency unit of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrequencyUnit {
    /// Hertz.
    Hz,
    /// Kilohertz.
    KHz,
    /// Megahertz.
    MHz,
    /// Gigahertz.
    GHz,
}

/// Data format: magnitude-angle, dB-angle, or real-imaginary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataFormat {
    /// Magnitude / angle in degrees.
    Ma,
    /// dB / angle in degrees.
    Db,
    /// Real / imaginary.
    Ri,
}

/// The Touchstone parser.
#[derive(Default)]
pub struct TouchstoneParser;

impl TouchstoneParser {
    /// Parses a `.s1p`/`.s2p` document into two-port S-parameters
    /// (one-port data occupies S11 only).
    pub fn parse(input: &str) -> Result<SParameters, TouchstoneError> {
        let mut unit = FrequencyUnit::GHz;
        let mut format = DataFormat::Ma;
        let mut reference = 50.0;
        let mut ports = 2usize;
        let mut seen_header = false;
        // Pending split-layout second row for two-port data.
        let mut pending: Option<(f64, [f64; 4])> = None;

        let mut out = SParameters::new(reference);

        for (lineno, raw) in input.lines().enumerate() {
            let line = match raw.find('!') {
                Some(pos) => &raw[..pos],
                None => raw,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix('#') {
                parse_header(header, &mut unit, &mut format, &mut reference)?;
                out.reference_impedance = reference;
                seen_header = true;
                continue;
            }
            if !seen_header {
                // Files may omit the header; defaults apply (GHz S MA R 50).
                seen_header = true;
                out.reference_impedance = reference;
            }
            let tokens: Vec<f64> = line
                .split_whitespace()
                .map(|t| t.parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| TouchstoneError {
                    message: format!("line {}: bad number in {:?}", lineno + 1, line),
                })?;
            if tokens.is_empty() {
                continue;
            }

            // Two-port split layout: `freq S11 S21` on one line (5 tokens),
            // then `S12 S22` on the next (4 tokens).
            if ports == 2 && tokens.len() == 5 {
                let f_hz = tokens[0] * freq_scale(unit);
                let (s11, s21) = decode_pair(format, &tokens[1..3], &tokens[3..5])?;
                pending = Some((f_hz, [s11.re, s11.im, s21.re, s21.im]));
                continue;
            }
            if ports == 2 && tokens.len() == 4 && pending.is_some() {
                let (f0, first) = pending.take().unwrap();
                let (s11, s21) = (
                    Complex::new(first[0], first[1]),
                    Complex::new(first[2], first[3]),
                );
                let (s12, s22) = decode_pair(format, &tokens[0..2], &tokens[2..4])?;
                out.push(f0, SParameterMatrix { s11, s21, s12, s22 });
                continue;
            }
            let f_hz = tokens[0] * freq_scale(unit);
            let values = &tokens[1..];

            if ports == 2 && values.len() == 8 {
                let (s11, s21) = decode_pair(format, &values[0..2], &values[2..4])?;
                let (s12, s22) = decode_pair(format, &values[4..6], &values[6..8])?;
                out.push(f_hz, SParameterMatrix { s11, s21, s12, s22 });
                continue;
            }
            if ports == 2 && values.len() == 4 {
                return err(format!(
                    "line {}: split data row without a preceding frequency row",
                    lineno + 1
                ));
            }
            if values.len() == 2 {
                // One-port (or first one-port row): S11 only.
                let (s11, _) = decode_pair(format, &values[0..2], &values[0..2])?;
                out.push(
                    f_hz,
                    SParameterMatrix {
                        s11,
                        s21: Complex::ZERO,
                        s12: Complex::ZERO,
                        s22: Complex::ZERO,
                    },
                );
                ports = 1;
                continue;
            }
            return err(format!(
                "line {}: unsupported token count {}",
                lineno + 1,
                values.len()
            ));
        }
        if out.is_empty() {
            return err("no data rows found");
        }
        Ok(out)
    }
}

fn freq_scale(unit: FrequencyUnit) -> f64 {
    match unit {
        FrequencyUnit::Hz => 1.0,
        FrequencyUnit::KHz => 1e3,
        FrequencyUnit::MHz => 1e6,
        FrequencyUnit::GHz => 1e9,
    }
}

fn parse_header(
    header: &str,
    unit: &mut FrequencyUnit,
    format: &mut DataFormat,
    reference: &mut f64,
) -> Result<(), TouchstoneError> {
    let tokens: Vec<String> = header
        .to_uppercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "HZ" => *unit = FrequencyUnit::Hz,
            "KHZ" => *unit = FrequencyUnit::KHz,
            "MHZ" => *unit = FrequencyUnit::MHz,
            "GHZ" => *unit = FrequencyUnit::GHz,
            "S" | "Y" | "Z" => { /* parameter type: S handled; Y/Z scaffolded */ }
            "MA" => *format = DataFormat::Ma,
            "DB" => *format = DataFormat::Db,
            "RI" => *format = DataFormat::Ri,
            "R" => {
                i += 1;
                *reference =
                    tokens
                        .get(i)
                        .and_then(|t| t.parse().ok())
                        .ok_or_else(|| TouchstoneError {
                            message: "R needs a reference resistance".into(),
                        })?;
            }
            other => {
                return err(format!("unknown header keyword {other:?}"));
            }
        }
        i += 1;
    }
    Ok(())
}

fn decode_pair(
    format: DataFormat,
    a: &[f64],
    b: &[f64],
) -> Result<(Complex, Complex), TouchstoneError> {
    if a.len() < 2 || b.len() < 2 {
        return err("pair needs two numbers");
    }
    let ca = match format {
        DataFormat::Ma => Complex::from_polar(a[0], a[1].to_radians()),
        DataFormat::Db => Complex::from_polar(10f64.powf(a[0] / 20.0), a[1].to_radians()),
        DataFormat::Ri => Complex::new(a[0], a[1]),
    };
    let cb = match format {
        DataFormat::Ma => Complex::from_polar(b[0], b[1].to_radians()),
        DataFormat::Db => Complex::from_polar(10f64.powf(b[0] / 20.0), b[1].to_radians()),
        DataFormat::Ri => Complex::new(b[0], b[1]),
    };
    Ok((ca, cb))
}

#[cfg(test)]
mod tests {
    use super::*;

    const S2P: &str = "\
# GHZ S MA R 50
! 50 ohm line
1.0  0.1 -30   0.95 -5   0.95 -5   0.1 -30
2.0  0.15 -45  0.90 -10  0.90 -10  0.15 -45
";

    const S1P: &str = "\
# MHZ S RI R 50
1   0.044723 -0.089443
10  0.2121 -0.2121
";

    const SPLIT: &str = "\
# HZ S DB R 75
2.0e9 -20 0 -1 10
-1 10 -20 0
2.5e9 -18 5 -2 12
-2 12 -18 5
";

    #[test]
    fn parses_two_port_ma() {
        let sp = TouchstoneParser::parse(S2P).unwrap();
        assert_eq!(sp.reference_impedance, 50.0);
        assert_eq!(sp.len(), 2);
        assert!((sp.frequencies[0] - 1e9).abs() < 1.0);
        let m = &sp.data[0];
        // 0.1 at −30°
        assert!((m.s11.abs() - 0.1).abs() < 1e-9);
        assert!((m.s11.arg().to_degrees() + 30.0).abs() < 1e-9);
        assert!((m.s21.abs() - 0.95).abs() < 1e-9);
        assert!((m.s12.abs() - 0.95).abs() < 1e-9);
        assert!((m.s22.abs() - 0.1).abs() < 1e-9);
        assert!((m.insertion_loss_db() + 0.4455).abs() < 1e-3);
        assert!(m.vswr() > 1.0 && m.vswr() < 2.5);
    }

    #[test]
    fn parses_one_port_ri() {
        let sp = TouchstoneParser::parse(S1P).unwrap();
        assert_eq!(sp.reference_impedance, 50.0);
        assert!((sp.frequencies[0] - 1e6).abs() < 1.0);
        // RI 0.05, −9.9 → magnitude ≈ 0.1 at ≈ −63°
        let m = &sp.data[0];
        assert!((m.s11.abs() - 0.1).abs() < 1e-6);
        assert!((m.s11.arg().to_degrees() + 63.435).abs() < 0.01);
        // Ports marked one-port: s21 stays zero
        assert_eq!(m.s21, Complex::ZERO);
    }

    #[test]
    fn parses_split_layout_db() {
        let sp = TouchstoneParser::parse(SPLIT).unwrap();
        assert_eq!(sp.reference_impedance, 75.0);
        assert_eq!(sp.len(), 2);
        let m = &sp.data[0];
        assert!((m.s11.abs() - 0.1).abs() < 1e-9); // −20 dB
        assert!((m.s21.abs() - 0.891).abs() < 1e-2); // −1 dB
        assert!((m.s12.abs() - 0.891).abs() < 1e-2);
        assert!((m.s22.abs() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn comment_and_blank_lines_ignored() {
        let sp = TouchstoneParser::parse("!\n! only comments\n").unwrap_err();
        assert!(sp.message.contains("no data"));
    }

    #[test]
    fn rejects_bad_tokens() {
        let e = TouchstoneParser::parse("# GHZ S MA R 50\n1.0 x y\n").unwrap_err();
        assert!(e.message.contains("bad number"));
        let e = TouchstoneParser::parse("# QQQ S MA R 50\n1 0 1 2\n").unwrap_err();
        assert!(e.message.contains("unknown header"));
    }
}
