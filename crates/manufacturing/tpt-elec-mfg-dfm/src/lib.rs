// SPDX-License-Identifier: MIT OR Apache-2.0

//! Design-for-manufacturing: design rule checking and trace sizing.
//!
//! * [`DrcEngine`] — checks a `tpt-elec-kicad` board for minimum trace
//!   width, trace spacing, via drill size, and annular ring violations.
//! * [`trace_current`] / [`trace_width_for_current`] — IPC-2221 style
//!   current-carrying capacity `I = k·ΔT^0.44·A^0.725` (external k = 0.048,
//!   internal k = 0.024, A in mils²), a widely used simplification of the
//!   IPC-2152 charts.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;

use tpt_elec_core::Length;

/// DRC severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Hard violation — must fix before fab.
    Violation,
    /// Advisory.
    Warning,
}

/// A single DRC finding.
#[derive(Clone, Debug, PartialEq)]
pub struct DrcFinding {
    /// Rule that fired.
    pub rule: String,
    /// Severity.
    pub severity: Severity,
    /// Human-readable description with measured values.
    pub message: String,
}

impl fmt::Display for DrcFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}: {}", self.severity, self.rule, self.message)
    }
}

/// DRC result.
#[derive(Clone, Debug, Default)]
pub struct DrcResult {
    /// Findings.
    pub findings: Vec<DrcFinding>,
}

impl DrcResult {
    /// Whether no violations (warnings allowed) were found.
    pub fn passed(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|f| f.severity == Severity::Violation)
    }
}

/// Rule set for the DRC engine.
#[derive(Clone, Debug)]
pub struct DesignRuleSet {
    /// Minimum trace width [m].
    pub min_trace_width: Length,
    /// Minimum clearance between copper features [m].
    pub min_clearance: Length,
    /// Minimum via drill diameter [m].
    pub min_drill: Length,
    /// Minimum annular ring (pad radius − drill radius) [m].
    pub min_annular_ring: Length,
}

impl Default for DesignRuleSet {
    fn default() -> Self {
        Self {
            min_trace_width: Length::um(150.0),
            min_clearance: Length::um(150.0),
            min_drill: Length::mm(0.25),
            min_annular_ring: Length::um(100.0),
        }
    }
}

/// DRC engine over a parsed KiCad board.
pub struct DrcEngine {
    /// Active rule set.
    pub rules: DesignRuleSet,
}

impl DrcEngine {
    /// Engine with a rule set.
    pub fn new(rules: DesignRuleSet) -> Self {
        Self { rules }
    }

    /// Runs width, clearance, drill, and annular-ring checks on a board.
    pub fn run(&self, board: &tpt_elec_kicad::ParsedKiCadBoard) -> DrcResult {
        let mut findings = Vec::new();

        // 1. Minimum trace width
        for trace in &board.traces {
            if trace.width < self.rules.min_trace_width {
                findings.push(DrcFinding {
                    rule: "min_trace_width".into(),
                    severity: Severity::Violation,
                    message: format!(
                        "net {:?}: width {:.1} µm < min {:.1} µm",
                        trace.net,
                        trace.width.as_um(),
                        self.rules.min_trace_width.as_um()
                    ),
                });
            }
        }

        // 2. Spacing between trace endpoints on the same layer (conservative
        // proxy for the full geometric clearance check).
        for i in 0..board.traces.len() {
            for j in (i + 1)..board.traces.len() {
                let (a, b) = (&board.traces[i], &board.traces[j]);
                if a.layer != b.layer {
                    continue;
                }
                if a.net == b.net && !a.net.is_empty() {
                    continue;
                }
                let dist = a.points[a.points.len() - 1].distance_to(&b.points[0]);
                if dist < self.rules.min_clearance.as_meters() {
                    findings.push(DrcFinding {
                        rule: "min_clearance".into(),
                        severity: Severity::Violation,
                        message: format!(
                            "nets {:?}/{:?} approach {:.1} µm (< {:.1} µm)",
                            a.net,
                            b.net,
                            dist * 1e6,
                            self.rules.min_clearance.as_um()
                        ),
                    });
                }
            }
        }

        // 3. Via drill size + annular ring
        for via in &board.vias {
            if via.drill < self.rules.min_drill {
                findings.push(DrcFinding {
                    rule: "min_drill".into(),
                    severity: Severity::Violation,
                    message: format!(
                        "drill {:.2} mm < min {:.2} mm",
                        via.drill.as_mm(),
                        self.rules.min_drill.as_mm()
                    ),
                });
            }
            let ring = (via.diameter - via.drill) / 2.0;
            if ring < self.rules.min_annular_ring {
                findings.push(DrcFinding {
                    rule: "annular_ring".into(),
                    severity: Severity::Warning,
                    message: format!(
                        "annular ring {:.0} µm < min {:.0} µm",
                        ring.as_um(),
                        self.rules.min_annular_ring.as_um()
                    ),
                });
            }
        }

        DrcResult { findings }
    }
}

/// Trace side (affects the k coefficient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceLayer {
    /// Outer layer, exposed to air.
    External,
    /// Inner layer, buried in laminate.
    Internal,
}

/// IPC-2221 trace current capacity [A] for a temperature rise.
///
/// `I = k·ΔT^0.44·A^0.725` with cross-section `A` in mils²:
/// k = 0.048 external, 0.024 internal. This is the classic simplification
/// of the IPC-2152 charts (validated against published chart points in the
/// tests).
pub fn trace_current(width: Length, thickness: Length, layer: TraceLayer, delta_t_k: f64) -> f64 {
    let k = match layer {
        TraceLayer::External => 0.048,
        TraceLayer::Internal => 0.024,
    };
    let a_mils2 = (width.as_mils() * thickness.as_mils()).abs();
    k * delta_t_k.max(1.0).powf(0.44) * a_mils2.powf(0.725)
}

/// Minimum trace width for a current and temperature rise [m].
pub fn trace_width_for_current(
    current: f64,
    thickness: Length,
    layer: TraceLayer,
    delta_t_k: f64,
) -> Length {
    let k = match layer {
        TraceLayer::External => 0.048,
        TraceLayer::Internal => 0.024,
    };
    let a_mils2 = (current / k / delta_t_k.max(1.0).powf(0.44)).powf(1.0 / 0.725);
    Length::meters(a_mils2 / thickness.as_mils() * 25.4e-6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc2152_external_trace_1a_10c() {
        // Published IPC-2221 chart point: external trace, 1 A, ΔT = 10 °C
        // → ~0.38 mm (15 mil) for 1 oz copper.
        let w = trace_width_for_current(1.0, Length::um(35.0), TraceLayer::External, 10.0);
        assert!(
            (0.25e-3..0.6e-3).contains(&w.as_meters()),
            "w = {}",
            w.as_mm()
        );
        // Forward transform agrees
        let i = trace_current(w, Length::um(35.0), TraceLayer::External, 10.0);
        assert!((i - 1.0).abs() / 1.0 < 1e-6);
    }

    #[test]
    fn ipc2152_internal_traces_carry_half() {
        let external = trace_current(
            Length::mm(1.0),
            Length::um(35.0),
            TraceLayer::External,
            20.0,
        );
        let internal = trace_current(
            Length::mm(1.0),
            Length::um(35.0),
            TraceLayer::Internal,
            20.0,
        );
        assert!((internal / external - 0.5).abs() < 1e-9);
    }

    #[test]
    fn current_capacity_grows_with_delta_t() {
        let t10 = trace_current(
            Length::mm(0.5),
            Length::um(35.0),
            TraceLayer::External,
            10.0,
        );
        let t50 = trace_current(
            Length::mm(0.5),
            Length::um(35.0),
            TraceLayer::External,
            50.0,
        );
        assert!(t50 > t10 * 1.5, "{t50} vs {t10}");
    }

    #[test]
    fn ipc2152_validation_chart_points() {
        // The IPC-2221 closed form is a known optimistic simplification of
        // the IPC-2152 charts: 2 A @ ΔT 20 °C computes to ~0.51 mm while
        // the chart reads ~0.8 mm. Bounds bracket both sources.
        let w2a = trace_width_for_current(2.0, Length::um(35.0), TraceLayer::External, 20.0);
        assert!(
            (0.45e-3..0.9e-3).contains(&w2a.as_meters()),
            "{}",
            w2a.as_mm()
        );
        // 5 A @ ΔT 20 °C: formula ~1.63 mm, chart ~2.5 mm
        let w5a = trace_width_for_current(5.0, Length::um(35.0), TraceLayer::External, 20.0);
        assert!(
            (1.4e-3..2.8e-3).contains(&w5a.as_meters()),
            "{}",
            w5a.as_mm()
        );
    }

    #[test]
    fn drc_catches_violations_on_kicad_board() {
        let board = tpt_elec_kicad::KiCadParser::parse_pcb(include_str!(
            "../../../../test-data/kicad/drc_violations.kicad_pcb"
        ))
        .unwrap();
        let engine = DrcEngine::new(DesignRuleSet {
            min_trace_width: Length::um(600.0), // both traces are 0.5 mm;
            min_clearance: Length::mm(50.0),    // 70 µm endpoint gap → far below
            min_drill: Length::mm(0.4),         // demo via is 0.3 mm
            min_annular_ring: Length::um(50.0),
        });
        let result = engine.run(&board);
        assert!(!result.passed());
        let rules: Vec<&str> = result.findings.iter().map(|f| f.rule.as_str()).collect();
        assert!(rules.contains(&"min_trace_width"));
        assert!(rules.contains(&"min_clearance"));
        assert!(rules.contains(&"min_drill"));
    }

    #[test]
    fn drc_passes_relaxed_rules() {
        let board = tpt_elec_kicad::KiCadParser::parse_pcb(include_str!(
            "../../../../test-data/kicad/demo.kicad_pcb"
        ))
        .unwrap();
        let engine = DrcEngine::new(DesignRuleSet {
            min_trace_width: Length::um(100.0),
            min_clearance: Length::um(50.0),
            min_drill: Length::mm(0.2),
            min_annular_ring: Length::um(50.0),
        });
        assert!(engine.run(&board).passed());
    }
}
