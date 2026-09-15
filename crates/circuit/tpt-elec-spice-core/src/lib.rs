// SPDX-License-Identifier: MIT OR Apache-2.0

//! Circuit graph, Modified Nodal Analysis (MNA) formulation, and analysis
//! kinds for the SPICE engine.
//!
//! Node 0 is always ground. Each device stamps itself into the
//! [`MnaMatrix`] (`G`, `C`, `B`) — the analysis crate drives the stamps for
//! DC/AC/transient; this crate owns the data model and the linear stamps for
//! passive devices and sources.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::DenseMatrix;
use tpt_elec_spice_models::{BjtModel, DiodeModel, MosfetModel, OpAmpModel};

/// Node index; `0` is the always-present ground node.
pub type NodeId = usize;

/// A circuit node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Net name (`"0"` for ground).
    pub name: String,
    /// Whether this is ground.
    pub is_ground: bool,
}

/// Source waveform.
#[derive(Clone, Debug, PartialEq)]
pub enum Waveform {
    /// Constant value.
    Dc(f64),
    /// Rectangular pulse: `v1 → v2` with rise/fall/width/period [s].
    Pulse {
        /// Initial value.
        v1: f64,
        /// Pulsed value.
        v2: f64,
        /// Delay before first edge [s].
        delay: f64,
        /// Rise time [s].
        rise: f64,
        /// Fall time [s].
        fall: f64,
        /// Pulse width [s].
        width: f64,
        /// Period [s].
        period: f64,
    },
    /// Damped sine: `offset + amplitude·sin(2πf(t−delay))`.
    Sine {
        /// Offset.
        offset: f64,
        /// Amplitude.
        amplitude: f64,
        /// Frequency [Hz].
        freq: f64,
        /// Delay [s].
        delay: f64,
    },
    /// Piecewise-linear breakpoints `(time, value)`.
    Pwl(Vec<(f64, f64)>),
}

impl Waveform {
    /// Value at time `t` [s].
    pub fn value(&self, t: f64) -> f64 {
        match self {
            Waveform::Dc(v) => *v,
            Waveform::Pulse {
                v1,
                v2,
                delay,
                rise,
                fall,
                width,
                period,
            } => {
                if *period <= 0.0 {
                    return *v1;
                }
                let tc = (t - delay).max(0.0) % period;
                let (rise, fall) = (rise.max(1e-18), fall.max(1e-18));
                if tc < rise {
                    v1 + (v2 - v1) * (tc / rise)
                } else if tc < rise + width {
                    *v2
                } else if tc < rise + width + fall {
                    v2 - (v2 - v1) * ((tc - rise - width) / fall)
                } else {
                    *v1
                }
            }
            Waveform::Sine {
                offset,
                amplitude,
                freq,
                delay,
            } => {
                if t < *delay {
                    *offset
                } else {
                    offset + amplitude * (std::f64::consts::TAU * freq * (t - delay)).sin()
                }
            }
            Waveform::Pwl(points) => {
                if points.is_empty() {
                    return 0.0;
                }
                if t <= points[0].0 {
                    return points[0].1;
                }
                for w in points.windows(2) {
                    if t <= w[1].0 {
                        let f = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-300);
                        return w[0].1 + f * (w[1].1 - w[0].1);
                    }
                }
                points[points.len() - 1].1
            }
        }
    }
}

/// A circuit element.
#[derive(Clone, Debug)]
pub enum CircuitComponent {
    /// Resistor.
    Resistor {
        /// Nodes (positive, negative).
        nodes: [NodeId; 2],
        /// Resistance [Ω].
        value: f64,
    },
    /// Capacitor.
    Capacitor {
        /// Nodes.
        nodes: [NodeId; 2],
        /// Capacitance [F].
        value: f64,
    },
    /// Inductor.
    Inductor {
        /// Nodes.
        nodes: [NodeId; 2],
        /// Inductance [H].
        value: f64,
    },
    /// Diode.
    Diode {
        /// Anode, cathode.
        nodes: [NodeId; 2],
        /// Model.
        model: DiodeModel,
        /// Area multiplier.
        area: f64,
    },
    /// MOSFET (4-terminal).
    Mosfet {
        /// Drain, gate, source, bulk.
        nodes: [NodeId; 4],
        /// Model.
        model: MosfetModel,
    },
    /// Bipolar junction transistor.
    Bjt {
        /// Collector, base, emitter.
        nodes: [NodeId; 3],
        /// Model.
        model: BjtModel,
    },
    /// Ideal op-amp (VCCS driving `out` through `rout` internally).
    OpAmp {
        /// Inverting input.
        inv: NodeId,
        /// Non-inverting input.
        noninv: NodeId,
        /// Output.
        out: NodeId,
        /// Model.
        model: OpAmpModel,
    },
    /// Independent voltage source (with optional AC magnitude for `.ac`).
    VoltageSource {
        /// Positive, negative.
        nodes: [NodeId; 2],
        /// Time-domain waveform.
        waveform: Waveform,
        /// AC magnitude used by AC analysis [V].
        ac_magnitude: f64,
    },
    /// Independent current source (flows from `nodes[0]` to `nodes[1]`).
    CurrentSource {
        /// Positive, negative.
        nodes: [NodeId; 2],
        /// Time-domain waveform.
        waveform: Waveform,
    },
}

impl CircuitComponent {
    /// Device name (for diagnostics).
    pub fn kind(&self) -> &'static str {
        match self {
            CircuitComponent::Resistor { .. } => "resistor",
            CircuitComponent::Capacitor { .. } => "capacitor",
            CircuitComponent::Inductor { .. } => "inductor",
            CircuitComponent::Diode { .. } => "diode",
            CircuitComponent::Mosfet { .. } => "mosfet",
            CircuitComponent::Bjt { .. } => "bjt",
            CircuitComponent::OpAmp { .. } => "opamp",
            CircuitComponent::VoltageSource { .. } => "vsource",
            CircuitComponent::CurrentSource { .. } => "isource",
        }
    }
}

/// Modified Nodal Analysis matrices: `[G]{x} + [C]{dx/dt} = {b}`.
#[derive(Clone, Debug, Default)]
pub struct MnaMatrix {
    /// Conductance (resistive) matrix, size `(n+m)²` including voltage-source
    /// branch rows.
    pub g: DenseMatrix,
    /// Capacitance/inductance matrix (same shape).
    pub c: DenseMatrix,
    /// Source vector.
    pub b: Vec<f64>,
}

/// Requested analysis type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Analysis {
    /// `.op` — DC operating point.
    DcOperatingPoint,
    /// `.dc src start stop step`.
    DcSweep {
        /// Source index in the circuit's component list.
        source: usize,
        /// Sweep start.
        start: f64,
        /// Sweep stop.
        stop: f64,
        /// Sweep step.
        step: f64,
    },
    /// `.ac` — frequency sweep.
    AcAnalysis {
        /// Start frequency [Hz].
        start_freq: f64,
        /// Stop frequency [Hz].
        stop_freq: f64,
        /// Points per decade.
        points_per_decade: u32,
    },
    /// `.tran`.
    Transient {
        /// Start time (output after this) [s].
        t_start: f64,
        /// Stop time [s].
        t_stop: f64,
        /// Nominal time step [s].
        t_step: f64,
    },
    /// Noise analysis across a band.
    NoiseAnalysis {
        /// Input (source) reference.
        input_source: usize,
        /// Output node.
        output_node: NodeId,
        /// Band edges [Hz].
        band: (f64, f64),
    },
}

/// A SPICE circuit.
#[derive(Clone, Debug, Default)]
pub struct Circuit {
    /// Circuit title line.
    pub name: String,
    nodes: Vec<Node>,
    name_to_id: std::collections::HashMap<String, NodeId>,
    /// Elements in declaration order.
    pub components: Vec<CircuitComponent>,
    /// Element names, parallel to `components`.
    pub component_names: Vec<String>,
    /// Parsed analyses (may be several).
    pub analyses: Vec<Analysis>,
}

impl Circuit {
    /// An empty circuit with only ground.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: vec![Node {
                name: "0".into(),
                is_ground: true,
            }],
            name_to_id: [("0".to_string(), 0 as NodeId)].into_iter().collect(),
            components: Vec::new(),
            component_names: Vec::new(),
            analyses: Vec::new(),
        }
    }

    /// Adds (or reuses) a node by name; `"0"` / `"gnd"` map to ground.
    pub fn node(&mut self, name: &str) -> NodeId {
        let key = name.trim_matches('"').to_uppercase();
        let is_ground = key == "0" || key == "GND" || key == "GROUND";
        if is_ground {
            return 0;
        }
        if let Some(&id) = self.name_to_id.get(&key) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: key.clone(),
            is_ground: false,
        });
        self.name_to_id.insert(key, id);
        id
    }

    /// Looks up a node by name.
    pub fn node_by_name(&self, name: &str) -> Option<NodeId> {
        self.name_to_id
            .get(&name.trim_matches('"').to_uppercase())
            .copied()
    }

    /// All nodes; index 0 is ground.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Number of non-ground nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len() - 1
    }

    /// Number of voltage sources (MNA branch rows).
    pub fn vsource_count(&self) -> usize {
        self.components
            .iter()
            .filter(|c| matches!(c, CircuitComponent::VoltageSource { .. }))
            .count()
    }

    /// Total MNA system size: nodes + voltage-source branches.
    pub fn system_size(&self) -> usize {
        self.node_count() + self.vsource_count()
    }

    /// Adds a named element.
    pub fn add_component(&mut self, name: &str, component: CircuitComponent) {
        self.components.push(component);
        self.component_names.push(name.to_string());
    }

    /// Index of the k-th voltage source in `components` order (branch id).
    pub fn vsource_branch(&self, k: usize) -> Option<usize> {
        self.components
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, CircuitComponent::VoltageSource { .. }))
            .nth(k)
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_dc_and_pulse() {
        let pulse = Waveform::Pulse {
            v1: 0.0,
            v2: 5.0,
            delay: 1e-6,
            rise: 1e-9,
            fall: 1e-9,
            width: 9e-6,
            period: 20e-6,
        };
        assert!((pulse.value(0.0) - 0.0).abs() < 1e-12); // before delay
        assert!(pulse.value(2e-6) - 5.0 < 1e-6); // high phase
        assert!(pulse.value(1e-6 + 0.5e-9) - 2.5 < 0.1); // mid rise
        assert!((pulse.value(15e-6) - 0.0).abs() < 1e-6); // after fall
                                                          // Periods run from the delay: 21 µs is a period boundary (low), 23 µs is high.
        assert!(pulse.value(21e-6).abs() < 1e-6);
        assert!(pulse.value(23e-6) > 4.9); // second period, high phase
    }

    #[test]
    fn waveform_sine_and_pwl() {
        let sine = Waveform::Sine {
            offset: 1.0,
            amplitude: 0.5,
            freq: 1e3,
            delay: 0.0,
        };
        // Quarter period (90°): offset + amplitude
        assert!((sine.value(2.5e-4) - 1.5).abs() < 1e-9);
        let pwl = Waveform::Pwl(vec![(0.0, 0.0), (1.0, 10.0), (2.0, 0.0)]);
        assert!((pwl.value(0.5) - 5.0).abs() < 1e-12);
        assert!((pwl.value(1.5) - 5.0).abs() < 1e-12);
        assert!((pwl.value(5.0) - 0.0).abs() < 1e-12);
        assert!((pwl.value(-1.0) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn circuit_nodes_and_ground() {
        let mut c = Circuit::new("test");
        assert_eq!(c.node("0"), 0);
        let a = c.node("in");
        let b = c.node("IN"); // case-insensitive reuse
        assert_eq!(a, b);
        let g = c.node("gnd");
        assert_eq!(g, 0);
        assert_eq!(c.node_count(), 1);
        assert!(c.nodes()[0].is_ground);
    }

    #[test]
    fn system_size_counts_vsources() {
        let mut c = Circuit::new("test");
        let a = c.node("a");
        let b = c.node("b");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [a, b],
                waveform: Waveform::Dc(5.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [a, b],
                value: 1e3,
            },
        );
        assert_eq!(c.vsource_count(), 1);
        assert_eq!(c.system_size(), 2 + 1);
        assert_eq!(c.vsource_branch(0), Some(0));
    }

    #[test]
    fn mna_matrix_stamps_resistor() {
        // Grounded divider: 1 Ω from source node to node a, 1 Ω from a to gnd.
        let mut g = DenseMatrix::zeros(2, 2);
        g.add(0, 0, 2.0);
        g.add(1, 1, 1.0);
        g.add(0, 1, -1.0);
        g.add(1, 0, -1.0);
        let x = g.solve(&[1.0, 0.0]).unwrap();
        assert!((x[0] - x[1] - 0.0).abs() < 1e-12);
        assert!((x[1] - 1.0).abs() < 1e-12); // 1 A through the grounded 1 Ohm
    }

    #[test]
    fn analysis_enum_roundtrip() {
        let a = Analysis::Transient {
            t_start: 0.0,
            t_stop: 1e-3,
            t_step: 1e-6,
        };
        assert_ne!(a, Analysis::DcOperatingPoint);
    }
}
