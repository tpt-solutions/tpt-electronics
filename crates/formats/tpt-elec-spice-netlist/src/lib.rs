// SPDX-License-Identifier: MIT OR Apache-2.0

//! SPICE netlist parser producing [`tpt_elec_spice_core::Circuit`] graphs.
//!
//! Supported syntax (classic SPICE, case-insensitive):
//!
//! ```text
//! R1 in out 10k
//! C1 out 0 100n
//! L1 a b 1u
//! D1 anode cathode DMOD 1
//! M1 drain gate source 0 MMOD w=10u l=1u
//! Q1 collector base emitter QMOD
//! V1 in 0 DC 12
//! Vg gate 0 PULSE(0 5 1n 1n 1n 10n 20n)
//! I1 a 0 1m
//! X1 a b SUB
//! .model DMOD D (IS=1e-14 N=1 RS=0.5 BV=40 IBV=1m)
//! .model MMOD NMOS (LEVEL=1 VTO=0.7 KP=110u LAMBDA=0.01)
//! .model QMOD NPN (IS=1e-15 BF=100)
//! .tran 1n 10u
//! .ac dec 20 1k 10meg
//! .subckt div a b
//!   R1 a mid 1k
//!   R2 mid b 1k
//! .ends
//! .end
//! ```
//!
//! SPICE unit suffixes (`T G MEG K M U/N P F`) and scientific notation are
//! supported; a bare `M` is **milli** per SPICE convention.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fmt;

use tpt_elec_spice_core::{Analysis, Circuit, CircuitComponent, Waveform};
use tpt_elec_spice_models::{
    BjtModel, BjtPolarity, DiodeModel, MosfetLevel, MosfetModel, MosfetParameters, MosfetPolarity,
};

/// Parse errors.
#[derive(Clone, Debug, PartialEq)]
pub struct SpiceParseError {
    /// Description with the offending line.
    pub message: String,
}

impl fmt::Display for SpiceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "netlist error: {}", self.message)
    }
}
impl std::error::Error for SpiceParseError {}

fn err<T>(msg: impl Into<String>) -> Result<T, SpiceParseError> {
    Err(SpiceParseError {
        message: msg.into(),
    })
}

/// The SPICE netlist parser.
#[derive(Default)]
pub struct SpiceNetlistParser;

impl SpiceNetlistParser {
    /// Parses a netlist into a circuit (analyses attached to
    /// [`Circuit::analyses`]).
    pub fn parse(input: &str) -> Result<Circuit, SpiceParseError> {
        // Strip comments, join continuations.
        let mut logical_lines: Vec<String> = Vec::new();
        for raw in input.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('*') || line.starts_with('$') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('+') {
                if let Some(last) = logical_lines.last_mut() {
                    last.push(' ');
                    last.push_str(rest.trim());
                }
                continue;
            }
            logical_lines.push(line.to_string());
        }

        let mut circuit = Circuit::new("netlist");
        let mut subckts: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
        let mut models: HashMap<String, String> = HashMap::new(); // name → TYPE
                                                                  // Title line: the first line, unless it starts with a dot command or
                                                                  // a plausible element name (`letter + digit`, e.g. `R1`, `Vg`).
        if let Some(first_line) = logical_lines.first() {
            let looks_like_element = first_line
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic())
                .unwrap_or(false)
                && first_line
                    .chars()
                    .nth(1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false);
            let is_dot = first_line.starts_with('.');
            if !looks_like_element && !is_dot {
                circuit.name = first_line.to_string();
                logical_lines.remove(0);
            }
        }

        // First pass: register .model types and .subckt bodies.
        for line in &logical_lines {
            let up = line.to_uppercase();
            let tokens = tokens_of(line);
            if tokens.is_empty() {
                continue;
            }
            if up.starts_with(".MODEL") {
                if tokens.len() >= 3 {
                    // Store the full tail: "TYPE (PARAM=VALUE ...)"
                    models.insert(tokens[1].to_uppercase(), tokens[2..].join(" "));
                }
            } else if up.starts_with(".SUBCKT") && tokens.len() >= 3 {
                let name = tokens[1].to_uppercase();
                let ports: Vec<String> = tokens[2..].to_vec();
                let body_start = logical_lines.iter().position(|l| l == line).unwrap_or(0);
                let mut body = Vec::new();
                for l in &logical_lines[body_start + 1..] {
                    let lu = l.to_uppercase();
                    if lu.starts_with(".ENDS") {
                        break;
                    }
                    body.push(l.clone());
                }
                subckts.insert(name, (ports, body));
            }
        }

        // Second pass: instantiate.
        for line in &logical_lines {
            let up = line.to_uppercase();
            if up.starts_with(".MODEL") || up.starts_with(".SUBCKT") || up.starts_with(".ENDS") {
                continue;
            }
            if up.starts_with(".END") {
                break;
            }
            if up.starts_with('.') {
                Self::parse_dot(&mut circuit, &tokens_of(line))?;
                continue;
            }
            let ctx = Context {
                models: &models,
                subckts: &subckts,
            };
            Self::parse_element(&mut circuit, line, &ctx, 0)?;
        }
        Ok(circuit)
    }
}

struct Context<'a> {
    models: &'a HashMap<String, String>,
    subckts: &'a HashMap<String, (Vec<String>, Vec<String>)>,
}

fn tokens_of(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for c in line.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

impl SpiceNetlistParser {
    fn parse_dot(circuit: &mut Circuit, tokens: &[String]) -> Result<(), SpiceParseError> {
        match tokens[0].to_uppercase().as_str() {
            ".OP" => circuit.analyses.push(Analysis::DcOperatingPoint),
            ".TRAN" => {
                if tokens.len() >= 3 {
                    let t_step = parse_value(&tokens[1])?;
                    let t_stop = parse_value(&tokens[2])?;
                    circuit.analyses.push(Analysis::Transient {
                        t_start: 0.0,
                        t_stop,
                        t_step,
                    });
                }
            }
            ".AC" if tokens.len() >= 5 => {
                let sweep = tokens[1].to_uppercase();
                let n = tokens[2].parse::<u32>().unwrap_or(10);
                let f_start = parse_value(&tokens[3])?;
                let f_stop = parse_value(&tokens[4])?;
                let _ = sweep;
                circuit.analyses.push(Analysis::AcAnalysis {
                    start_freq: f_start,
                    stop_freq: f_stop,
                    points_per_decade: n,
                });
            }
            _ => {} // .dc, .noise, .include, .lib etc. — scaffolded
        }
        Ok(())
    }

    fn parse_element(
        circuit: &mut Circuit,
        line: &str,
        ctx: &Context,
        depth: usize,
    ) -> Result<(), SpiceParseError> {
        if depth > 8 {
            return err("subckt recursion too deep");
        }
        let tokens = tokens_of(line);
        if tokens.is_empty() {
            return Ok(());
        }
        let name = tokens[0].clone();
        let kind = name.chars().next().unwrap().to_ascii_uppercase();
        let up: Vec<String> = tokens.iter().map(|t| t.to_uppercase()).collect();

        let node = |i: usize| -> String { tokens[i].clone() };

        match kind {
            'R' => {
                if tokens.len() < 4 {
                    return err(format!("R needs `R<name> n1 n2 value`: {line}"));
                }
                let value = parse_value(&tokens[3])?;
                let n1 = circuit.node(&node(1));
                let n2 = circuit.node(&node(2));
                circuit.add_component(
                    &name,
                    CircuitComponent::Resistor {
                        nodes: [n1, n2],
                        value,
                    },
                );
            }
            'C' => {
                if tokens.len() < 4 {
                    return err(format!("C needs `C<name> n1 n2 value`: {line}"));
                }
                let value = parse_value(&tokens[3])?;
                let n1 = circuit.node(&node(1));
                let n2 = circuit.node(&node(2));
                circuit.add_component(
                    &name,
                    CircuitComponent::Capacitor {
                        nodes: [n1, n2],
                        value,
                    },
                );
            }
            'L' => {
                if tokens.len() < 4 {
                    return err(format!("L needs `L<name> n1 n2 value`: {line}"));
                }
                let value = parse_value(&tokens[3])?;
                let n1 = circuit.node(&node(1));
                let n2 = circuit.node(&node(2));
                circuit.add_component(
                    &name,
                    CircuitComponent::Inductor {
                        nodes: [n1, n2],
                        value,
                    },
                );
            }
            'D' => {
                if tokens.len() < 4 {
                    return err(format!("D needs `D<name> a c model`: {line}"));
                }
                let area = tokens
                    .get(4)
                    .map(|t| parse_value(t))
                    .transpose()?
                    .unwrap_or(1.0);
                let model = diode_model(&up[3], ctx.models)?;
                let a = circuit.node(&node(1));
                let c = circuit.node(&node(2));
                circuit.add_component(
                    &name,
                    CircuitComponent::Diode {
                        nodes: [a, c],
                        model,
                        area,
                    },
                );
            }
            'M' => {
                if tokens.len() < 6 {
                    return err(format!("M needs `M<name> d g s b model`: {line}"));
                }
                let mut model = mosfet_model(&up[5], ctx.models)?;
                // Instance W/L overrides
                for tok in &tokens[6..] {
                    if let Some((k, v)) = tok.split_once('=') {
                        let v = parse_value(v)?;
                        match k.to_uppercase().as_str() {
                            "W" => model.parameters.w = tpt_elec_core::Length::meters(v),
                            "L" => model.parameters.l = tpt_elec_core::Length::meters(v),
                            _ => {}
                        }
                    }
                }
                let d = circuit.node(&node(1));
                let g = circuit.node(&node(2));
                let s = circuit.node(&node(3));
                let b = circuit.node(&node(4));
                circuit.add_component(
                    &name,
                    CircuitComponent::Mosfet {
                        nodes: [d, g, s, b],
                        model,
                    },
                );
            }
            'Q' => {
                if tokens.len() < 5 {
                    return err(format!("Q needs `Q<name> c b e model`: {line}"));
                }
                let model = bjt_model(&up[4], ctx.models)?;
                let c = circuit.node(&node(1));
                let b = circuit.node(&node(2));
                let e = circuit.node(&node(3));
                circuit.add_component(
                    &name,
                    CircuitComponent::Bjt {
                        nodes: [c, b, e],
                        model,
                    },
                );
            }
            'V' | 'I' => {
                if tokens.len() < 4 {
                    return err(format!("source needs `V<name> n+ n- spec`: {line}"));
                }
                let p = circuit.node(&node(1));
                let m = circuit.node(&node(2));
                let (waveform, ac_mag) = parse_source_spec(&tokens[3..])?;
                if kind == 'V' {
                    circuit.add_component(
                        &name,
                        CircuitComponent::VoltageSource {
                            nodes: [p, m],
                            waveform,
                            ac_magnitude: ac_mag,
                        },
                    );
                } else {
                    circuit.add_component(
                        &name,
                        CircuitComponent::CurrentSource {
                            nodes: [p, m],
                            waveform,
                        },
                    );
                }
            }
            'X' => {
                if tokens.len() < 3 {
                    return err(format!("X needs `X<name> nodes... subname`: {line}"));
                }
                let sub_name = up.last().unwrap().clone();
                let (ports, body) = ctx
                    .subckts
                    .get(&sub_name)
                    .ok_or_else(|| SpiceParseError {
                        message: format!(
                            "unknown subckt '{sub_name}' (defined: {:?})",
                            ctx.subckts.keys().collect::<Vec<_>>()
                        ),
                    })?
                    .clone();
                let external = &tokens[1..tokens.len() - 1];
                if external.len() != ports.len() {
                    return err(format!(
                        "subckt {sub_name} expects {} ports, got {} in: {line}",
                        ports.len(),
                        external.len()
                    ));
                }
                // Rename definition ports to the instance's global nodes,
                // then prefix element names for uniqueness.
                for sub_line in &body {
                    let sub_tokens = tokens_of(sub_line);
                    if sub_tokens.is_empty() {
                        continue;
                    }
                    let mut new_tokens = vec![format!(
                        "{}__{}",
                        sub_tokens[0],
                        name.trim_start_matches(|c: char| !c.is_ascii_alphabetic())
                    )];
                    for tok in &sub_tokens[1..] {
                        let mut mapped = tok.clone();
                        for (di, dn) in ports.iter().enumerate() {
                            if tok.eq_ignore_ascii_case(dn) {
                                mapped = external[di].clone();
                                break;
                            }
                        }
                        new_tokens.push(mapped);
                    }
                    let inner_ctx = Context {
                        models: ctx.models,
                        subckts: ctx.subckts,
                    };
                    Self::parse_element(circuit, &new_tokens.join(" "), &inner_ctx, depth + 1)?;
                }
            }
            other => {
                return err(format!("unsupported element `{other}` in line: {line}"));
            }
        }
        Ok(())
    }
}

fn diode_model(
    model_ref: &str,
    models: &HashMap<String, String>,
) -> Result<DiodeModel, SpiceParseError> {
    let mut m = DiodeModel::default();
    // Declared .model parameters first, then any inline overrides.
    let declared = models.get(model_ref).cloned().unwrap_or_default();
    let params = raw_params(&format!("{model_ref} {declared}"));
    if let Some(kv) = params {
        for (k, v) in kv {
            let v = parse_value(&v)?;
            match k.as_str() {
                "IS" => m.is = v,
                "N" => m.n = v,
                "RS" => m.rs = v,
                "CJO" => m.cjo = v,
                "VJ" => m.vj = v,
                "M" => m.m = v,
                "BV" => m.bv = v,
                "IBV" => m.ibv = v,
                _ => {}
            }
        }
    }
    Ok(m)
}

fn mosfet_model(
    model_ref: &str,
    models: &HashMap<String, String>,
) -> Result<MosfetModel, SpiceParseError> {
    let mut p = MosfetParameters::default();
    let mut polarity = MosfetPolarity::N;
    let level = MosfetLevel::Level1;
    let declared = models.get(model_ref).cloned().unwrap_or_default();
    if declared.contains("PMOS") {
        polarity = MosfetPolarity::P;
    }
    // Inline params on the instance line, plus declared .model params.
    let params = raw_params(&format!("{model_ref} {declared}"));
    if let Some(kv) = params {
        for (k, v) in kv {
            let f = parse_value(&v).unwrap_or(0.0);
            match k.as_str() {
                "VTO" => p.vth0 = f,
                "KP" => p.kp = f,
                "LAMBDA" => p.lambda = f,
                "GAMMA" => p.gamma = f,
                "PHI" => p.phi = f,
                "TOX" => {
                    if f > 0.0 {
                        p.cox = 3.9 * 8.854e-12 / f
                    }
                }
                "W" => p.w = tpt_elec_core::Length::meters(f),
                "L" => p.l = tpt_elec_core::Length::meters(f),
                _ => {}
            }
        }
    }
    Ok(MosfetModel {
        level,
        polarity,
        parameters: p,
    })
}

fn bjt_model(
    model_ref: &str,
    models: &HashMap<String, String>,
) -> Result<BjtModel, SpiceParseError> {
    let mut m = BjtModel::default();
    let declared = models.get(model_ref).cloned().unwrap_or_default();
    if declared.contains("PNP") {
        m.polarity = BjtPolarity::Pnp;
    }
    if let Some(kv) = raw_params(&format!("{model_ref} {declared}")) {
        for (k, v) in kv {
            let v = parse_value(&v).unwrap_or(0.0);
            match k.as_str() {
                "IS" => m.is = v,
                "BF" => m.bf = v,
                "BR" => m.br = v,
                "VA" => m.va = v,
                _ => {}
            }
        }
    }
    Ok(m)
}

/// Extracts KEY=VALUE pairs from a model reference like
/// `MMOD (LEVEL=1 VTO=0.7 KP=110u)` or `DMOD (IS=1e-14)` — the parameters
/// may be attached to the token that names the model.
fn raw_params(token: &str) -> Option<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    let cleaned = token.trim_start_matches('(').trim_end_matches(')');
    for part in cleaned.split_whitespace() {
        if let Some((k, v)) = part.split_once('=') {
            pairs.push((k.to_uppercase(), v.to_string()));
        }
    }
    // Also support parenthesized groups inside the token.
    if let Some(open) = token.find('(') {
        if let Some(close) = token[open..].find(')').map(|c| c + open) {
            for part in token[open + 1..close].split_whitespace() {
                if let Some((k, v)) = part.split_once('=') {
                    let k = k.to_uppercase();
                    if !pairs.iter().any(|(ek, _)| *ek == k) {
                        pairs.push((k, v.to_string()));
                    }
                }
            }
        }
    }
    if pairs.is_empty() {
        None
    } else {
        Some(pairs)
    }
}

/// Parses a source specification: `DC 5`, `5`, `AC 1 DC 5`, `PULSE(...)`,
/// `SIN(...)`, `PWL(t1 v1 t2 v2 ...)`.
fn parse_source_spec(tokens: &[String]) -> Result<(Waveform, f64), SpiceParseError> {
    // An `AC <mag>` clause may appear before or after the time-domain spec.
    let mut ac_mag = 0.0f64;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].to_uppercase() == "AC" {
            ac_mag = tokens
                .get(i + 1)
                .map(|t| parse_value(t).unwrap_or(1.0))
                .unwrap_or(1.0);
            i += 2;
        } else {
            rest.push(tokens[i].clone());
            i += 1;
        }
    }
    if rest.is_empty() {
        return Ok((Waveform::Dc(0.0), ac_mag));
    }
    let first_up = rest[0].to_uppercase();
    let joined = rest.join(" ");
    let extract_paren = |name: &str| -> Option<String> {
        let up = joined.to_uppercase();
        let start = up.find(name)? + name.len();
        let open = joined[start..].find('(')? + start;
        let close = joined[open..].find(')')? + open;
        Some(joined[open + 1..close].to_string())
    };
    let waveform = if first_up == "DC" {
        Waveform::Dc(parse_value(rest.get(1).unwrap_or(&"0".to_string()))?)
    } else if first_up.starts_with("PULSE") || extract_paren("PULSE").is_some() {
        let args: Vec<String> = extract_paren("PULSE")
            .unwrap_or_default()
            .split_whitespace()
            .map(String::from)
            .collect();
        let v = |k: usize| {
            args.get(k)
                .map(|s| parse_value(s).unwrap_or(0.0))
                .unwrap_or(0.0)
        };
        Waveform::Pulse {
            v1: v(0),
            v2: v(1),
            delay: v(2),
            rise: v(3),
            fall: v(4),
            width: v(5),
            period: v(6),
        }
    } else if first_up.starts_with("SIN") || extract_paren("SIN").is_some() {
        let args: Vec<String> = extract_paren("SIN")
            .unwrap_or_default()
            .split_whitespace()
            .map(String::from)
            .collect();
        let v = |k: usize| {
            args.get(k)
                .map(|s| parse_value(s).unwrap_or(0.0))
                .unwrap_or(0.0)
        };
        Waveform::Sine {
            offset: v(0),
            amplitude: v(1),
            freq: v(2),
            delay: v(3),
        }
    } else if first_up.starts_with("PWL") || extract_paren("PWL").is_some() {
        let args: Vec<String> = extract_paren("PWL")
            .unwrap_or_default()
            .split_whitespace()
            .map(String::from)
            .collect();
        let pts: Vec<(f64, f64)> = args
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some((
                        parse_value(&c[0]).unwrap_or(0.0),
                        parse_value(&c[1]).unwrap_or(0.0),
                    ))
                } else {
                    None
                }
            })
            .collect();
        Waveform::Pwl(pts)
    } else {
        Waveform::Dc(parse_value(&rest[0])?)
    };
    Ok((waveform, ac_mag))
}

/// Parses a SPICE value: `10k`, `1.5meg`, `100n`, `4.7p`, `1e-9`, `2.2K`.
pub fn parse_value(s: &str) -> Result<f64, SpiceParseError> {
    let s = s.trim().trim_end_matches(',').trim();
    let bytes: Vec<char> = s.chars().collect();
    let mut num_end = 0usize;
    let mut seen_dot = false;
    let mut seen_exp = false;
    while num_end < bytes.len() {
        let c = bytes[num_end];
        if c.is_ascii_digit() {
            num_end += 1;
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            num_end += 1;
        } else if (c == 'e' || c == 'E') && !seen_exp && num_end > 0 {
            // could be scientific or the farad suffix
            if num_end + 1 < bytes.len()
                && (bytes[num_end + 1].is_ascii_digit()
                    || bytes[num_end + 1] == '+'
                    || bytes[num_end + 1] == '-')
            {
                seen_exp = true;
                num_end += 1;
            } else {
                break;
            }
        } else if (c == '+' || c == '-') && num_end > 0 && seen_exp {
            num_end += 1;
        } else {
            break;
        }
    }
    if num_end == 0 {
        return err(format!("bad numeric value {s:?}"));
    }
    let num: String = bytes[..num_end].iter().collect();
    let mut value: f64 = num.parse().map_err(|_| SpiceParseError {
        message: format!("bad numeric value {s:?}"),
    })?;
    let suffix: String = bytes[num_end..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_uppercase();
    let mult = match suffix.as_str() {
        "T" => 1e12,
        "G" => 1e9,
        "MEG" => 1e6,
        "K" => 1e3,
        "M" | "MIL" => 1e-3,
        "U" | "N" => {
            if suffix == "U" {
                1e-6
            } else {
                1e-9
            }
        }
        "P" => 1e-12,
        "F" => 1e-15,
        "" => 1.0,
        // Unknown letters after the value (e.g. "5Hz") — ignore
        _ => 1.0,
    };
    value *= mult;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spice_suffixes() {
        assert!((parse_value("10k").unwrap() - 1e4).abs() < 1e-9);
        assert!((parse_value("1.5meg").unwrap() - 1.5e6).abs() < 1e-3);
        assert!((parse_value("100n").unwrap() - 1e-7).abs() < 1e-15);
        assert!((parse_value("4.7p").unwrap() - 4.7e-12).abs() < 1e-20);
        assert!((parse_value("1u").unwrap() - 1e-6).abs() < 1e-15);
        assert!((parse_value("5m").unwrap() - 5e-3).abs() < 1e-12); // milli!
        assert!((parse_value("2G").unwrap() - 2e9).abs() < 1.0);
        assert!((parse_value("1e-9").unwrap() - 1e-9).abs() < 1e-20);
        assert!((parse_value("42").unwrap() - 42.0).abs() < 1e-12);
        assert!(parse_value("abc").is_err());
    }

    const NETLIST: &str = "\
RC low-pass demo
V1 in 0 DC 1 AC 1
R1 in out 1k
C1 out 0 1n
.tran 1n 10u
.ac dec 20 1k 10meg
.end
";

    #[test]
    fn parses_rc_netlist() {
        let c = SpiceNetlistParser::parse(NETLIST).unwrap();
        assert_eq!(c.name, "RC low-pass demo");
        assert_eq!(c.node_count(), 2); // in, out
        assert_eq!(c.components.len(), 3);
        assert_eq!(c.analyses.len(), 2);
        match &c.analyses[0] {
            Analysis::Transient { t_stop, t_step, .. } => {
                assert!((*t_stop - 10e-6).abs() < 1e-15);
                assert!((*t_step - 1e-9).abs() < 1e-15);
            }
            other => panic!("expected transient, got {other:?}"),
        }
        match &c.analyses[1] {
            Analysis::AcAnalysis {
                start_freq,
                stop_freq,
                points_per_decade,
            } => {
                assert!((*start_freq - 1e3).abs() < 1.0);
                assert!((*stop_freq - 1e7).abs() < 1.0);
                assert_eq!(*points_per_decade, 20);
            }
            other => panic!("expected AC, got {other:?}"),
        }
        // AC magnitude parsed from the source line
        match &c.components[0] {
            CircuitComponent::VoltageSource { ac_magnitude, .. } => {
                assert!((*ac_magnitude - 1.0).abs() < 1e-12);
            }
            other => panic!("expected V1, got {other:?}"),
        }
    }

    const MOSFET_NET: &str = "\
* mosfet + diode test
.model NMOS NMOS (LEVEL=1 VTO=0.7 KP=110u LAMBDA=0.01)
.model DW D (IS=1e-12 RS=0.1)
V1 in 0 12
Vg g 0 PULSE(0 10 1n 1n 1n 9u 10u)
M1 sw g 0 0 NMOS w=1000u l=1u
D1 0 sw DW
RL sw 0 10
.tran 1n 20u
.end
";

    #[test]
    fn parses_mosfet_and_diode() {
        let c = SpiceNetlistParser::parse(MOSFET_NET).unwrap();
        assert_eq!(c.components.len(), 5);
        match &c.components[2] {
            CircuitComponent::Mosfet { nodes, model, .. } => {
                assert_eq!(nodes[0], c.node_by_name("SW").unwrap());
                assert!((model.parameters.vth0 - 0.7).abs() < 1e-12);
                assert!((model.parameters.kp - 110e-6).abs() < 1e-12);
                assert!((model.parameters.w.as_meters() - 1000e-6).abs() < 1e-12);
            }
            other => panic!("expected MOSFET, got {other:?}"),
        }
        match &c.components[3] {
            CircuitComponent::Diode { model, .. } => {
                assert!((model.rs - 0.1).abs() < 1e-12);
                assert!((model.is - 1e-12).abs() < 1e-24);
            }
            other => panic!("expected diode, got {other:?}"),
        }
        // PULSE waveform parsed
        match &c.components[1] {
            CircuitComponent::VoltageSource { waveform, .. } => match waveform {
                Waveform::Pulse { v2, period, .. } => {
                    assert!((*v2 - 10.0).abs() < 1e-12);
                    assert!((*period - 10e-6).abs() < 1e-12);
                }
                other => panic!("expected pulse, got {other:?}"),
            },
            _ => panic!("expected Vg"),
        }
    }

    const SUBCKT_NET: &str = "\
.subckt divider a b
R1 a mid 1k
R2 mid b 2k
.ends
V1 p 0 10
X1 p 0 divider
.op
.end
";

    #[test]
    fn flattens_subckt_instances() {
        let c = SpiceNetlistParser::parse(SUBCKT_NET).unwrap();
        // Two resistors from the subckt + the source
        assert!(c.components.len() >= 3);
        // Mid node exists (internal, unique to the instance)
        assert!(c.node_by_name("MID").is_some());
    }

    #[test]
    fn current_source_parses() {
        let c = SpiceNetlistParser::parse("I1 a 0 1m\nR1 a 0 1k\n.op\n.end").unwrap();
        match &c.components[0] {
            CircuitComponent::CurrentSource { waveform, .. } => {
                assert!((waveform.value(0.0) - 1e-3).abs() < 1e-12);
            }
            other => panic!("expected current source, got {other:?}"),
        }
    }

    #[test]
    fn comments_and_continuations() {
        let c = SpiceNetlistParser::parse("* comment\nR1 a\n+ b 5k\n$ another\n.op\n.end").unwrap();
        assert_eq!(c.components.len(), 1);
        match &c.components[0] {
            CircuitComponent::Resistor { value, .. } => {
                assert!((value - 5e3).abs() < 1e-9);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn rejects_unknown_elements() {
        assert!(SpiceNetlistParser::parse("Z1 a b 1\n.end").is_err());
        assert!(SpiceNetlistParser::parse("R1 a 1k\n.end").is_err());
    }
}
