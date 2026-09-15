// SPDX-License-Identifier: MIT OR Apache-2.0

//! SPICE analysis algorithms: DC operating point (Newton-Raphson), AC sweep
//! (small-signal complex MNA per frequency), transient (trapezoidal
//! integration with adaptive stepping), and noise analysis.
//!
//! The engine uses the textbook MNA formulation: the unknown vector is
//! `[V₁..Vₙ, I_branch₁..I_branchₖ]` — node voltages (ground excluded) plus
//! branch currents of voltage sources, inductors, and op-amps. Nonlinear
//! devices contribute linearized companion models re-stamped every Newton
//! iteration.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::{Complex, DenseMatrix};
use tpt_elec_spice_core::{Circuit, CircuitComponent, NodeId};
use tpt_elec_spice_models::{DiodeModel, MosfetModel};
use tpt_elec_spice_noise::{thermal_current_density, NoiseSpot};

/// Newton iteration cap.
const NEWTON_MAX_ITER: usize = 400;
/// Relative Newton tolerance.
const NEWTON_TOL: f64 = 1e-9;
/// Diagonal floor for numerical robustness [S].
const GMIN: f64 = 1e-12;

/// Result of a DC operating point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DcResult {
    /// Node voltages indexed by [`NodeId`] (0 = ground = 0 V) [V].
    pub node_voltages: Vec<f64>,
    /// Branch currents in MNA branch order (voltage sources, inductors,
    /// op-amp outputs, in declaration order) [A].
    pub branch_currents: Vec<f64>,
    /// Newton iterations used.
    pub iterations: usize,
}

/// Result of an AC sweep.
#[derive(Clone, Debug, Default)]
pub struct AcResult {
    /// Frequencies [Hz].
    pub frequencies: Vec<f64>,
    /// Complex node voltages per frequency, indexed by [`NodeId`] [V].
    pub node_voltages: Vec<Vec<Complex>>,
}

impl AcResult {
    /// Magnitude of one node across the sweep.
    pub fn magnitude_at(&self, node: NodeId) -> Vec<f64> {
        self.node_voltages.iter().map(|v| v[node].abs()).collect()
    }

    /// Phase in degrees.
    pub fn phase_at(&self, node: NodeId) -> Vec<f64> {
        self.node_voltages
            .iter()
            .map(|v| v[node].arg().to_degrees())
            .collect()
    }
}

/// Result of a transient run.
#[derive(Clone, Debug, Default)]
pub struct TransientAnalysisResult {
    /// Sample times [s].
    pub times: Vec<f64>,
    /// Node voltages per sample, indexed by [`NodeId`] [V].
    pub node_voltages: Vec<Vec<f64>>,
}

impl TransientAnalysisResult {
    /// Voltage waveform of one node.
    pub fn waveform(&self, node: NodeId) -> Vec<f64> {
        self.node_voltages.iter().map(|v| v[node]).collect()
    }
}

/// Result of a noise analysis.
#[derive(Clone, Debug, Default)]
pub struct NoiseResult {
    /// Output noise density samples [V²/Hz].
    pub spots: Vec<NoiseSpot>,
    /// Integrated RMS output noise [V].
    pub total_rms: f64,
}

/// Analysis errors.
#[derive(Clone, Debug, PartialEq)]
pub struct SpiceError {
    /// Description.
    pub message: String,
}

impl std::fmt::Display for SpiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "spice error: {}", self.message)
    }
}
impl std::error::Error for SpiceError {}

fn err<T>(msg: impl Into<String>) -> Result<T, SpiceError> {
    Err(SpiceError {
        message: msg.into(),
    })
}

/// Node voltage helper (ground = 0 V).
fn node_v(v: &[f64], node: NodeId) -> f64 {
    if node == 0 {
        0.0
    } else {
        v.get(node - 1).copied().unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------
// Complex matrix with LU solve
// ---------------------------------------------------------------------------

/// Dense complex matrix with partial-pivot LU solving (AC analysis).
pub struct ComplexMatrix {
    n: usize,
    data: Vec<Complex>,
}

impl ComplexMatrix {
    /// `n × n` zero matrix.
    pub fn zeros(n: usize) -> Self {
        Self {
            n,
            data: vec![Complex::ZERO; n * n],
        }
    }

    /// Adds to an element.
    pub fn add(&mut self, i: usize, j: usize, v: Complex) {
        self.data[i * self.n + j] += v;
    }

    /// Solves `A·x = b` with partial-pivot LU.
    pub fn solve(&self, b: &[Complex]) -> Option<Vec<Complex>> {
        let n = self.n;
        let mut a = self.data.clone();
        let mut x = b.to_vec();
        for col in 0..n {
            let pivot = (col..n)
                .max_by(|&r1, &r2| a[r1 * n + col].abs().total_cmp(&a[r2 * n + col].abs()))
                .unwrap_or(col);
            if a[pivot * n + col].abs() < 1e-300 {
                return None;
            }
            if pivot != col {
                for j in 0..n {
                    a.swap(col * n + j, pivot * n + j);
                }
                x.swap(col, pivot);
            }
            let d = a[col * n + col];
            for row in (col + 1)..n {
                let f = a[row * n + col] / d;
                if f != Complex::ZERO {
                    for j in col..n {
                        let prod = f * a[col * n + j];
                        a[row * n + j] -= prod;
                    }
                    let prod = f * x[col];
                    x[row] -= prod;
                }
            }
        }
        for row in (0..n).rev() {
            let mut sum = x[row];
            for j in (row + 1)..n {
                sum -= a[row * n + j] * x[j];
            }
            x[row] = sum / a[row * n + row];
        }
        Some(x)
    }
}

// ---------------------------------------------------------------------------
// Stamp abstraction: one device-stamp implementation, two backends
// ---------------------------------------------------------------------------

/// Target for device stamps. `inject` supplies the constant (companion
/// source) part of a linearized device; branch helpers handle MNA branch rows.
trait StampTarget {
    /// Conductance between two nodes.
    fn conductance(&mut self, p: NodeId, n: NodeId, s: f64);
    /// Complex admittance between two nodes (imaginary part only used by
    /// the complex backend; real backend ignores `im`).
    fn admittance(&mut self, p: NodeId, n: NodeId, re: f64, im: f64) {
        let _ = im;
        self.conductance(p, n, re);
    }
    /// Voltage-controlled current source: I(op→on) = g·(v_cp − v_cn).
    fn vccs(&mut self, op: NodeId, on: NodeId, cp: NodeId, cn: NodeId, g: f64);
    /// Companion current injected into node `p` (taken from ground).
    fn inject(&mut self, p: NodeId, i: f64);
}

struct RealStamp<'a> {
    g: &'a mut DenseMatrix,
    b: &'a mut [f64],
}

impl<'a> RealStamp<'a> {
    fn new(g: &'a mut DenseMatrix, b: &'a mut [f64]) -> Self {
        Self { g, b }
    }
}

impl StampTarget for RealStamp<'_> {
    fn conductance(&mut self, p: NodeId, n: NodeId, s: f64) {
        if s == 0.0 {
            return;
        }
        if p > 0 {
            self.g.add(p - 1, p - 1, s);
        }
        if n > 0 {
            self.g.add(n - 1, n - 1, s);
        }
        if p > 0 && n > 0 {
            self.g.add(p - 1, n - 1, -s);
            self.g.add(n - 1, p - 1, -s);
        }
    }

    fn vccs(&mut self, op: NodeId, on: NodeId, cp: NodeId, cn: NodeId, g: f64) {
        if g == 0.0 {
            return;
        }
        if op > 0 && cp > 0 {
            self.g.add(op - 1, cp - 1, g);
        }
        if op > 0 && cn > 0 {
            self.g.add(op - 1, cn - 1, -g);
        }
        if on > 0 && cp > 0 {
            self.g.add(on - 1, cp - 1, -g);
        }
        if on > 0 && cn > 0 {
            self.g.add(on - 1, cn - 1, g);
        }
    }

    fn inject(&mut self, p: NodeId, i: f64) {
        if p > 0 {
            self.b[p - 1] += i;
        }
    }
}

struct ComplexStamp<'a> {
    a: &'a mut ComplexMatrix,
    /// Kept for interface parity with the real backend; AC stamps never
    /// inject companion currents.
    #[allow(dead_code)]
    rhs: &'a mut Vec<Complex>,
}

impl StampTarget for ComplexStamp<'_> {
    fn admittance(&mut self, p: NodeId, n: NodeId, re: f64, im: f64) {
        let y = Complex::new(re, im);
        if p > 0 {
            self.a.add(p - 1, p - 1, y);
        }
        if n > 0 {
            self.a.add(n - 1, n - 1, y);
        }
        if p > 0 && n > 0 {
            self.a.add(p - 1, n - 1, -y);
            self.a.add(n - 1, p - 1, -y);
        }
    }

    fn conductance(&mut self, p: NodeId, n: NodeId, s: f64) {
        let s = Complex::real(s);
        if p > 0 {
            self.a.add(p - 1, p - 1, s);
        }
        if n > 0 {
            self.a.add(n - 1, n - 1, s);
        }
        if p > 0 && n > 0 {
            self.a.add(p - 1, n - 1, -s);
            self.a.add(n - 1, p - 1, -s);
        }
    }

    fn vccs(&mut self, op: NodeId, on: NodeId, cp: NodeId, cn: NodeId, g: f64) {
        let g = Complex::real(g);
        if op > 0 && cp > 0 {
            self.a.add(op - 1, cp - 1, g);
        }
        if op > 0 && cn > 0 {
            self.a.add(op - 1, cn - 1, -g);
        }
        if on > 0 && cp > 0 {
            self.a.add(on - 1, cp - 1, -g);
        }
        if on > 0 && cn > 0 {
            self.a.add(on - 1, cn - 1, g);
        }
    }

    fn inject(&mut self, _p: NodeId, _i: f64) {
        // AC small-signal stamps have no constant part.
    }
}

/// SPICE-style junction limiting (pnjlim): the voltage a diode is
/// linearized at may move at most a couple of thermal voltages per Newton
/// iteration, forcing intermediate stamps through the exponential's
/// transition band instead of toggling between on and off.
fn limit_junction(v_requested: f64, v_prev_stamp: f64, vt: f64) -> f64 {
    let diff = v_requested - v_prev_stamp;
    let max_step = 4.0 * vt;
    if diff.abs() <= max_step {
        return v_requested;
    }
    // Far from the last stamp: geometric approach (25% of the distance)
    // so distant operating points converge in tens, not hundreds, of steps.
    let step = diff.signum() * (diff.abs() * 0.25).max(max_step);
    v_prev_stamp + step
}

/// Linearized diode stamp (works for real and complex backends).
/// `stamp_v`/`next_stamp_v` carry the per-diode limiting state.
fn stamp_diode(
    st: &mut dyn StampTarget,
    a: NodeId,
    c: NodeId,
    model: &DiodeModel,
    v_junction: f64,
    stamp_v: &mut f64,
    area: f64,
) {
    let vt = model.n * tpt_elec_spice_models::thermal_voltage(300.0);
    let vd = limit_junction(v_junction, *stamp_v, vt);
    *stamp_v = vd;
    let i_d = model.current(vd, 300.0) * area;
    let g_d = model.conductance(vd, 300.0) * area;
    st.conductance(a, c, g_d);
    // I(a→c) = g·v + J with J = i − g·v: J leaves the anode and returns
    // at the cathode (both legs must be stamped for KCL).
    let j = i_d - g_d * vd;
    st.inject(a, -j);
    st.inject(c, j);
}

/// Linearized MOSFET stamp.
fn stamp_mosfet(
    st: &mut dyn StampTarget,
    v: &[f64],
    d: NodeId,
    g: NodeId,
    s: NodeId,
    b: NodeId,
    model: &MosfetModel,
) {
    let vgs = node_v(v, g) - node_v(v, s);
    let vds = node_v(v, d) - node_v(v, s);
    let vbs = node_v(v, b) - node_v(v, s);
    let op = model.operating_point(vgs, vds, vbs);
    // gds: conductance d→s
    st.conductance(d, s, op.gds);
    // gm: I(d→s) = gm·vgs
    st.vccs(d, s, g, s, op.gm);
    // gmb: I(d→s) = gmb·vbs
    st.vccs(d, s, b, s, op.gmb);
    // Companion source (ids flows d→s internally; the return leg must be
    // stamped at the source for KCL when the source is not ground).
    let lin = op.gm * vgs + op.gmb * vbs + op.gds * vds;
    let j = op.ids - lin;
    st.inject(d, -j);
    st.inject(s, j);
}

/// Linearized BJT stamp (Ebers-Moll transport).
fn stamp_bjt(
    st: &mut dyn StampTarget,
    v: &[f64],
    c: NodeId,
    base: NodeId,
    e: NodeId,
    model: &tpt_elec_spice_models::BjtModel,
) {
    use tpt_elec_spice_models::thermal_voltage;
    let sign = match model.polarity {
        tpt_elec_spice_models::BjtPolarity::Npn => 1.0,
        tpt_elec_spice_models::BjtPolarity::Pnp => -1.0,
    };
    let vbe = (node_v(v, base) - node_v(v, e)) * sign;
    let vbc = (node_v(v, base) - node_v(v, c)) * sign;
    let op = model.operating_point(vbe * sign, vbc * sign, 300.0);
    let vt = thermal_voltage(300.0);

    // gpi conductance base→emitter
    let gpi = (op.ib * sign).max(0.0) / vt.max(1e-6);
    st.conductance(base, e, gpi.max(1e-15));
    // gm: I(c→e) = gm·vbe
    st.vccs(c, e, base, e, op.gm.abs() * sign);
    // Early-effect output conductance: I(c→e) += gmu·vbc
    st.vccs(c, e, base, c, op.gmu.abs());
    // Companion currents (collector and base currents both return at the
    // emitter — stamp both legs for KCL).
    let lin_ic = op.gm * vbe;
    let jc = op.ic * sign - lin_ic;
    st.inject(c, -jc * sign);
    st.inject(e, jc * sign);
    let lin_ib = gpi * vbe;
    let jb = op.ib * sign - lin_ib;
    st.inject(base, -jb * sign);
    st.inject(e, jb * sign);
}

// ---------------------------------------------------------------------------
// The analyzer
// ---------------------------------------------------------------------------

/// The SPICE analyzer.
pub struct SpiceAnalyzer {
    circuit: Circuit,
}

impl SpiceAnalyzer {
    /// Binds an analyzer to a circuit.
    pub fn new(circuit: Circuit) -> Self {
        Self { circuit }
    }

    /// Borrows the circuit.
    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    /// Branch rows: voltage sources, inductors, and op-amps, in order.
    fn branch_count(&self) -> usize {
        self.circuit
            .components
            .iter()
            .filter(|c| self.needs_branch(c))
            .count()
    }

    /// Per-diode linearization voltages, seeded from the current operating
    /// point (junction limiting walks from here).
    fn diode_init_v(&self, v: &[f64]) -> Vec<f64> {
        self.circuit
            .components
            .iter()
            .map(|c| match c {
                CircuitComponent::Diode { nodes, .. } => node_v(v, nodes[0]) - node_v(v, nodes[1]),
                _ => 0.0,
            })
            .collect()
    }

    fn needs_branch(&self, c: &CircuitComponent) -> bool {
        matches!(
            c,
            CircuitComponent::VoltageSource { .. }
                | CircuitComponent::Inductor { .. }
                | CircuitComponent::OpAmp { .. }
        )
    }

    fn branch_row(&self, component_index: usize) -> Option<usize> {
        let mut branch = self.circuit.node_count();
        for (i, c) in self.circuit.components.iter().enumerate() {
            if self.needs_branch(c) {
                if i == component_index {
                    return Some(branch);
                }
                branch += 1;
            }
        }
        None
    }

    fn system_size(&self) -> usize {
        self.circuit.node_count() + self.branch_count()
    }

    // -- DC --------------------------------------------------------------

    /// Solves the DC operating point with Newton-Raphson.
    pub fn dc_operating_point(&self) -> Result<DcResult, SpiceError> {
        if self.system_size() == 0 {
            return Ok(DcResult::default());
        }
        let mut v = vec![0.0f64; self.system_size()];
        for (k, comp) in self.circuit.components.iter().enumerate() {
            if let CircuitComponent::VoltageSource { waveform, .. } = comp {
                if let Some(br) = self.branch_row(k) {
                    v[br] = waveform.value(0.0);
                }
            }
        }

        let mut iterations = 0usize;
        for iter in 0..NEWTON_MAX_ITER {
            iterations = iterations.max(iter + 1);
            let x = self.solve_dc_once(&v)?;
            let x_damped = damp(&x, &v, 0.25);
            let delta = x_damped
                .iter()
                .zip(&v)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f64, f64::max);
            v = x_damped;
            if delta < NEWTON_TOL {
                return Ok(self.collect_dc(v, iterations));
            }
        }
        err("Newton-Raphson did not converge for the DC operating point")
    }

    fn solve_dc_once(&self, v: &[f64]) -> Result<Vec<f64>, SpiceError> {
        let size = self.system_size();
        let mut g = DenseMatrix::zeros(size, size);
        let mut b = vec![0.0f64; size];
        for i in 0..self.circuit.node_count() {
            g.add(i, i, GMIN);
        }
        let mut diode_v = self.diode_init_v(v);
        {
            let mut st = RealStamp::new(&mut g, &mut b);
            self.stamp_devices(v, &mut st, 0.0, None, &mut diode_v);
        }
        self.stamp_branches(v, &mut g, &mut b, 0.0, None, 0.0)?;
        g.solve(&b).ok_or_else(|| SpiceError {
            message: "singular MNA matrix".into(),
        })
    }

    /// Stamps all devices (linear + linearized companions) into a target.
    fn stamp_devices(
        &self,
        v: &[f64],
        st: &mut dyn StampTarget,
        omega: f64,
        cap_admittance: Option<f64>,
        diode_stamp_v: &mut [f64],
    ) {
        // cap_admittance carries ω when set (AC); None for DC/transient.
        let _ = omega;
        for (idx, comp) in self.circuit.components.iter().enumerate() {
            match comp {
                CircuitComponent::Resistor { nodes, value } => {
                    st.conductance(nodes[0], nodes[1], 1.0 / value.max(1e-18));
                }
                CircuitComponent::Capacitor { nodes, value } => {
                    if let Some(omega) = cap_admittance {
                        st.admittance(nodes[0], nodes[1], 0.0, omega * value);
                    }
                    // DC / transient use their own companion handling
                }
                CircuitComponent::Diode { nodes, model, area } => {
                    let vd = node_v(v, nodes[0]) - node_v(v, nodes[1]);
                    stamp_diode(
                        st,
                        nodes[0],
                        nodes[1],
                        model,
                        vd,
                        &mut diode_stamp_v[idx],
                        *area,
                    );
                }
                CircuitComponent::Mosfet {
                    nodes: [d, g, s, bl],
                    model,
                } => stamp_mosfet(st, v, *d, *g, *s, *bl, model),
                CircuitComponent::Bjt {
                    nodes: [c, base, e],
                    model,
                } => stamp_bjt(st, v, *c, *base, *e, model),
                _ => {}
            }
        }
    }

    /// Stamps branch elements (voltage sources, inductors, op-amps).
    fn stamp_branches(
        &self,
        _v: &[f64],
        g: &mut DenseMatrix,
        b: &mut [f64],
        t: f64,
        ind_hist: Option<&[(usize, f64, f64)]>, // (comp_idx, v_prev, i_prev)
        dt_l_scale: f64,                        // 0 for DC; dt/(2L) factor use 2L/dt here
    ) -> Result<(), SpiceError> {
        let n_nodes = self.circuit.node_count();
        let mut branch = n_nodes;
        for (idx, comp) in self.circuit.components.iter().enumerate() {
            match comp {
                CircuitComponent::VoltageSource {
                    nodes, waveform, ..
                } => {
                    stamp_branch_row(g, nodes[0], nodes[1], branch, 1.0);
                    b[branch] = waveform.value(t);
                    branch += 1;
                }
                CircuitComponent::Inductor { nodes, .. } => {
                    stamp_branch_row(g, nodes[0], nodes[1], branch, 1.0);
                    let coef = 1.0;
                    let _ = coef;
                    if let Some(hist) = ind_hist.and_then(|h| h.iter().find(|h| h.0 == idx)) {
                        // v = (2L/dt)(i_n − i_prev) − v_prev
                        let two_l_over_dt = dt_l_scale;
                        // row already has v_p − v_n; add −(2L/dt)·i on branch
                        g.add(branch, branch, -two_l_over_dt);
                        b[branch] = -(two_l_over_dt * hist.2 + hist.1);
                    } else {
                        // DC: v = 0
                        b[branch] = 0.0;
                    }
                    branch += 1;
                }
                CircuitComponent::OpAmp {
                    inv,
                    noninv,
                    out,
                    model,
                } => {
                    // v_out = A·(v+ − v−)
                    let row = branch;
                    if *out > 0 {
                        g.add(row, out - 1, 1.0);
                        g.add(out - 1, row, 1.0);
                    }
                    if *noninv > 0 {
                        g.add(row, noninv - 1, -model.gain);
                    }
                    if *inv > 0 {
                        g.add(row, inv - 1, model.gain);
                    }
                    b[row] = 0.0;
                    branch += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_dc(&self, x: Vec<f64>, iterations: usize) -> DcResult {
        let n_nodes = self.circuit.nodes().len();
        let mut node_voltages = vec![0.0; n_nodes];
        for (id, slot) in node_voltages.iter_mut().enumerate().skip(1) {
            *slot = x[id - 1];
        }
        let branch_currents = x[n_nodes - 1..].to_vec();
        DcResult {
            node_voltages,
            branch_currents,
            iterations,
        }
    }

    fn dc_to_mna_vector(&self, op: &DcResult) -> Vec<f64> {
        let mut v = vec![0.0; self.system_size()];
        for (id, &volt) in op.node_voltages.iter().enumerate().skip(1) {
            v[id - 1] = volt;
        }
        let n_nodes = self.circuit.node_count();
        for (i, cur) in op.branch_currents.iter().enumerate() {
            v[n_nodes + i] = *cur;
        }
        v
    }

    // -- AC --------------------------------------------------------------

    /// Small-signal AC sweep. Devices are linearized around the DC operating
    /// point; capacitors/inductors contribute complex admittances.
    pub fn ac_analysis(
        &self,
        start_freq: f64,
        stop_freq: f64,
        points_per_decade: u32,
    ) -> Result<AcResult, SpiceError> {
        if start_freq <= 0.0 || stop_freq < start_freq {
            return err("AC sweep needs 0 < start ≤ stop frequency");
        }
        let op = self.dc_operating_point()?;
        let op_v = self.dc_to_mna_vector(&op);
        let n_nodes = self.circuit.nodes().len();
        let _j = Complex::new(0.0, 1.0);
        let ppd = points_per_decade.max(1) as f64;
        let decades = (stop_freq.log10() - start_freq.log10()).max(0.0);
        let steps = (decades * ppd).ceil() as usize;

        let mut frequencies = Vec::new();
        let mut node_voltages = Vec::new();
        for k in 0..=steps {
            let f = (start_freq * 10f64.powf(k as f64 / ppd)).min(stop_freq);
            let omega = std::f64::consts::TAU * f;
            let size = self.system_size();
            let mut a = ComplexMatrix::zeros(size);
            let mut rhs = vec![Complex::ZERO; size];
            for i in 0..self.circuit.node_count() {
                a.add(i, i, Complex::real(GMIN));
            }
            {
                let mut st = ComplexStamp {
                    a: &mut a,
                    rhs: &mut rhs,
                };
                let mut diode_v = self.diode_init_v(&op_v);
                self.stamp_devices(&op_v, &mut st, omega, Some(omega), &mut diode_v);
            }
            // Branches with complex coefficient for inductors.
            self.stamp_branches_ac(&op_v, &mut a, &mut rhs, omega)?;
            let x = a.solve(&rhs).ok_or_else(|| SpiceError {
                message: format!("singular AC matrix at {f} Hz"),
            })?;
            let mut row = vec![Complex::ZERO; n_nodes];
            for (id, slot) in row.iter_mut().enumerate().skip(1) {
                *slot = x[id - 1];
            }
            frequencies.push(f);
            node_voltages.push(row);
            if f >= stop_freq {
                break;
            }
        }
        Ok(AcResult {
            frequencies,
            node_voltages,
        })
    }

    fn stamp_branches_ac(
        &self,
        _op_v: &[f64],
        a: &mut ComplexMatrix,
        rhs: &mut [Complex],
        omega: f64,
    ) -> Result<(), SpiceError> {
        let n_nodes = self.circuit.node_count();
        let j = Complex::new(0.0, 1.0);
        let mut branch = n_nodes;
        for comp in &self.circuit.components {
            match comp {
                CircuitComponent::VoltageSource {
                    nodes,
                    ac_magnitude,
                    ..
                } => {
                    stamp_branch_row_c(a, nodes[0], nodes[1], branch);
                    rhs[branch] = Complex::real(*ac_magnitude);
                    branch += 1;
                }
                CircuitComponent::Inductor { nodes, value } => {
                    stamp_branch_row_c(a, nodes[0], nodes[1], branch);
                    a.add(branch, branch, -(j * (omega * value.max(1e-15))));
                    rhs[branch] = Complex::ZERO;
                    branch += 1;
                }
                CircuitComponent::OpAmp {
                    inv,
                    noninv,
                    out,
                    model,
                } => {
                    let fp = model.pole_hz.max(1e-3);
                    let gain =
                        Complex::real(model.gain) / (Complex::real(1.0) + j * (f_hz(omega) / fp));
                    let row = branch;
                    if *out > 0 {
                        a.add(row, out - 1, Complex::ONE);
                        a.add(out - 1, row, Complex::ONE);
                    }
                    if *noninv > 0 {
                        a.add(row, noninv - 1, -gain);
                    }
                    if *inv > 0 {
                        a.add(row, inv - 1, gain);
                    }
                    rhs[row] = Complex::ZERO;
                    branch += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    // -- Transient ---------------------------------------------------------

    /// Trapezoidal transient with adaptive stepping: Δt halves when Newton
    /// fails, grows modestly after repeated easy steps.
    pub fn transient(
        &self,
        t_stop: f64,
        t_step: f64,
        max_step_factor: f64,
    ) -> Result<TransientAnalysisResult, SpiceError> {
        if t_step <= 0.0 || t_stop <= 0.0 {
            return err("transient needs positive t_step and t_stop");
        }
        let n_nodes = self.circuit.nodes().len();
        let op = self.dc_operating_point()?;
        let mut v_prev = self.dc_to_mna_vector(&op);
        let mut times = vec![0.0];
        let mut out_rows = vec![plain_voltages(&v_prev, n_nodes)];

        #[derive(Clone, Copy, Default)]
        struct Hist {
            v_prev: f64,
            i_prev: f64,
        }
        // Histories initialize from the DC operating point (SPICE convention):
        // caps carry their DC voltage with zero current, inductors carry
        // their DC branch current with zero voltage.
        let mut cap_hist = vec![Hist::default(); self.circuit.components.len()];
        let mut ind_hist = vec![Hist::default(); self.circuit.components.len()];
        for (idx, comp) in self.circuit.components.iter().enumerate() {
            match comp {
                CircuitComponent::Capacitor { nodes, .. } => {
                    cap_hist[idx] = Hist {
                        v_prev: node_v(&v_prev, nodes[0]) - node_v(&v_prev, nodes[1]),
                        i_prev: 0.0,
                    };
                }
                CircuitComponent::Inductor { .. } => {
                    let br = self.branch_row(idx).ok_or_else(|| SpiceError {
                        message: "inductor branch missing".into(),
                    })?;
                    ind_hist[idx] = Hist {
                        v_prev: 0.0,
                        i_prev: v_prev.get(br).copied().unwrap_or(0.0),
                    };
                }
                _ => {}
            }
        }

        let mut t = 0.0;
        let mut dt = t_step;
        let dt_min = t_step / 64.0;
        let dt_max = t_step * max_step_factor.max(1.0);
        let mut easy_streak = 0usize;

        while t < t_stop - 1e-18 {
            let dt_eff = dt.min(t_stop - t);
            let t_next = t + dt_eff;
            let mut iter_v = v_prev.clone();
            let mut converged = false;
            let mut iters_used = NEWTON_MAX_ITER;
            let mut damping = 0.25f64;
            let mut stall = 0usize;
            let mut prev_delta = f64::INFINITY;
            // Diode junction-limiting state persists across Newton
            // iterations within a step; seeded from the last converged point.
            let mut diode_stamp_v = self.diode_init_v(&v_prev);

            for iter in 0..NEWTON_MAX_ITER {
                iters_used = iter + 1;
                let size = self.system_size();
                let mut g = DenseMatrix::zeros(size, size);
                let mut b = vec![0.0f64; size];
                for i in 0..self.circuit.node_count() {
                    g.add(i, i, GMIN);
                }
                {
                    let mut st = RealStamp::new(&mut g, &mut b);
                    // Devices (nonlinear companion stamps at iter_v)
                    self.stamp_devices(&iter_v, &mut st, 0.0, None, &mut diode_stamp_v);
                    // Capacitor companions: i = g_eq·v + J
                    for (idx, comp) in self.circuit.components.iter().enumerate() {
                        if let CircuitComponent::Capacitor { nodes, value } = comp {
                            // Trapezoid: i_n = g_eq·v_n − (g_eq·v_{n−1} + i_{n−1})
                            // so the companion is g_eq in parallel with the
                            // current source J = −(g_eq·v_{n−1} + i_{n−1}).
                            let g_eq = 2.0 * value / dt_eff;
                            st.conductance(nodes[0], nodes[1], g_eq);
                            let h = cap_hist[idx];
                            let j = -(g_eq * h.v_prev + h.i_prev);
                            st.inject(nodes[0], -j);
                            st.inject(nodes[1], j);
                        }
                    }
                }
                // Inductor branch histories
                let hist_list: Vec<(usize, f64, f64)> = self
                    .circuit
                    .components
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| matches!(c, CircuitComponent::Inductor { .. }))
                    .map(|(i, _)| (i, ind_hist[i].v_prev, ind_hist[i].i_prev))
                    .collect();
                let two_l = |l: f64| 2.0 * l.max(1e-15) / dt_eff;
                self.stamp_branches(&iter_v, &mut g, &mut b, t_next, Some(&hist_list), 0.0)?;
                // In-law: the generic branch helper takes a single scale; we
                // patch per-inductor coefficients here instead.
                {
                    let mut branch = self.circuit.node_count();
                    for (idx, comp) in self.circuit.components.iter().enumerate() {
                        match comp {
                            CircuitComponent::VoltageSource { .. }
                            | CircuitComponent::OpAmp { .. } => {
                                branch += 1;
                            }
                            CircuitComponent::Inductor { nodes, value } => {
                                let coef = two_l(*value);
                                g.add(branch, branch, -coef);
                                b[branch] = -(coef * ind_hist[idx].i_prev + ind_hist[idx].v_prev);
                                let _ = nodes;
                                branch += 1;
                            }
                            _ => {}
                        }
                    }
                }

                let x = g.solve(&b).ok_or_else(|| SpiceError {
                    message: "singular transient matrix".into(),
                })?;
                let raw_delta = x
                    .iter()
                    .zip(&iter_v)
                    .map(|(a2, b2)| (a2 - b2).abs())
                    .fold(0.0f64, f64::max);
                // Oscillation/stagnation guard: shrink the damping window
                // while the raw residual stalls (device handoff cycling).
                // After a few stalled iterations force one FULL Newton step:
                // two-cycles of the damped map are broken by undamped
                // Newton, which converges for exponential devices.
                if raw_delta < prev_delta * 0.7 {
                    damping = (damping * 2.0).min(1.0);
                    stall = 0;
                } else {
                    stall += 1;
                    if stall >= 8 {
                        damping = 1.0;
                        stall = 0;
                    } else {
                        damping = (damping * 0.5).max(0.002);
                    }
                }
                let x_damped = damp(&x, &iter_v, damping);
                let delta = x_damped
                    .iter()
                    .zip(&iter_v)
                    .map(|(a2, b2)| (a2 - b2).abs())
                    .fold(0.0f64, f64::max);
                prev_delta = raw_delta;
                iter_v = x_damped;
                if delta < NEWTON_TOL * 100.0 {
                    converged = true;
                    break;
                }
            }

            if !converged {
                if dt_eff <= dt_min {
                    return err(format!(
                        "transient Newton failed even at minimum step (t={t:.3e})"
                    ));
                }
                dt /= 2.0;
                easy_streak = 0;
                continue;
            }

            // Update energy histories from the converged solution.
            for (idx, comp) in self.circuit.components.iter().enumerate() {
                match comp {
                    CircuitComponent::Capacitor { nodes, value } => {
                        let vdev = node_v(&iter_v, nodes[0]) - node_v(&iter_v, nodes[1]);
                        let g_eq = 2.0 * value / dt_eff;
                        let i_prev = g_eq * (vdev - cap_hist[idx].v_prev) - cap_hist[idx].i_prev;
                        cap_hist[idx] = Hist {
                            v_prev: vdev,
                            i_prev,
                        };
                    }
                    CircuitComponent::Inductor { nodes, value } => {
                        let vdev = node_v(&iter_v, nodes[0]) - node_v(&iter_v, nodes[1]);
                        let g_eq = dt_eff / (2.0 * value.max(1e-15));
                        let i_prev = ind_hist[idx].i_prev + g_eq * (vdev + ind_hist[idx].v_prev);
                        ind_hist[idx] = Hist {
                            v_prev: vdev,
                            i_prev,
                        };
                    }
                    _ => {}
                }
            }

            v_prev = iter_v;
            t = t_next;
            times.push(t);
            out_rows.push(plain_voltages(&v_prev, n_nodes));

            if iters_used <= 3 {
                easy_streak += 1;
                if easy_streak >= 10 {
                    dt = (dt * 1.3).min(dt_max);
                    easy_streak = 0;
                }
            } else {
                easy_streak = 0;
            }
        }

        Ok(TransientAnalysisResult {
            times,
            node_voltages: out_rows,
        })
    }

    // -- Noise -------------------------------------------------------------

    /// Output noise over a band driven by resistor thermal noise: each
    /// resistor's noise current is transferred to the output through the
    /// small-signal network (contributions add in power).
    pub fn noise_analysis(
        &self,
        output_node: NodeId,
        band: (f64, f64),
        points: usize,
    ) -> Result<NoiseResult, SpiceError> {
        if points < 2 {
            return err("noise analysis needs ≥2 frequency points");
        }
        let op = self.dc_operating_point()?;
        let op_v = self.dc_to_mna_vector(&op);
        let mut spots = Vec::with_capacity(points);
        for k in 0..points {
            let f = band.0 * (band.1 / band.0).powf(k as f64 / (points - 1) as f64);
            let omega = std::f64::consts::TAU * f;
            let mut density_v2 = 0.0;
            for comp in &self.circuit.components {
                if let CircuitComponent::Resistor { nodes, value } = comp {
                    let density_i2 = thermal_current_density(*value, 300.0);
                    if density_i2 <= 0.0 {
                        continue;
                    }
                    let size = self.system_size();
                    let mut a = ComplexMatrix::zeros(size);
                    let mut rhs = vec![Complex::ZERO; size];
                    for i in 0..self.circuit.node_count() {
                        a.add(i, i, Complex::real(GMIN));
                    }
                    {
                        let mut st = ComplexStamp {
                            a: &mut a,
                            rhs: &mut rhs,
                        };
                        let mut diode_v = self.diode_init_v(&op_v);
                        self.stamp_devices(&op_v, &mut st, omega, Some(omega), &mut diode_v);
                    }
                    self.stamp_branches_ac(&op_v, &mut a, &mut rhs, omega)?;
                    inject_current_ac(&mut rhs, nodes[0], nodes[1], Complex::ONE);
                    let x = a.solve(&rhs).ok_or_else(|| SpiceError {
                        message: "singular noise matrix".into(),
                    })?;
                    let transfer = if output_node > 0 {
                        x.get(output_node - 1).copied().unwrap_or(Complex::ZERO)
                    } else {
                        Complex::ZERO
                    };
                    density_v2 += density_i2 * transfer.norm_sqr();
                }
            }
            spots.push(NoiseSpot {
                frequency: f,
                density: density_v2,
            });
        }
        let total_rms = tpt_elec_spice_noise::integrate_band(&spots);
        Ok(NoiseResult { spots, total_rms })
    }
}

fn f_hz(omega: f64) -> f64 {
    omega / std::f64::consts::TAU
}

/// Newton damping: limits per-iteration node-voltage updates so switching
/// events cannot oscillate the iteration between extreme operating points.
fn damp(new: &[f64], old: &[f64], max_frac: f64) -> Vec<f64> {
    let scale = old.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let allowed = max_frac * scale + 0.75;
    new.iter()
        .zip(old)
        .map(|(&n, &o)| {
            let d = (n - o).clamp(-allowed, allowed);
            o + d
        })
        .collect()
}

/// Node voltages extracted from an MNA solution vector.
fn plain_voltages(x: &[f64], n_nodes: usize) -> Vec<f64> {
    let mut out = vec![0.0; n_nodes];
    for (id, slot) in out.iter_mut().enumerate().skip(1) {
        *slot = x[id - 1];
    }
    out
}

/// Real branch row stamp: `v_p − v_n = coef·i + rhs`.
fn stamp_branch_row(g: &mut DenseMatrix, p: NodeId, n: NodeId, branch: usize, coef: f64) {
    if p > 0 {
        g.add(branch, p - 1, 1.0);
        g.add(p - 1, branch, 1.0);
    }
    if n > 0 {
        g.add(branch, n - 1, -1.0);
        g.add(n - 1, branch, -1.0);
    }
    let _ = coef;
}

fn stamp_branch_row_c(a: &mut ComplexMatrix, p: NodeId, n: NodeId, branch: usize) {
    if p > 0 {
        a.add(branch, p - 1, Complex::ONE);
        a.add(p - 1, branch, Complex::ONE);
    }
    if n > 0 {
        a.add(branch, n - 1, -Complex::ONE);
        a.add(n - 1, branch, -Complex::ONE);
    }
}

/// Unit current injection between two nodes (for noise transfer).
fn inject_current_ac(rhs: &mut [Complex], p: NodeId, n: NodeId, i: Complex) {
    if p > 0 {
        rhs[p - 1] += i;
    }
    if n > 0 {
        rhs[n - 1] -= i;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_spice_core::{CircuitComponent, Waveform};

    fn rc_lowpass() -> (Circuit, NodeId, NodeId) {
        let mut c = Circuit::new("rc");
        let inp = c.node("in");
        let out = c.node("out");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [inp, 0],
                waveform: Waveform::Dc(1.0),
                ac_magnitude: 1.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [inp, out],
                value: 1e3,
            },
        );
        c.add_component(
            "C1",
            CircuitComponent::Capacitor {
                nodes: [out, 0],
                value: 1e-9,
            },
        );
        (c, inp, out)
    }

    #[test]
    fn dc_resistive_divider() {
        let mut c = Circuit::new("divider");
        let a = c.node("a");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [a, 0],
                waveform: Waveform::Dc(10.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [a, 0],
                value: 1e3,
            },
        );
        let r = SpiceAnalyzer::new(c).dc_operating_point().unwrap();
        assert!((r.node_voltages[1] - 10.0).abs() < 1e-6);
        assert!((r.branch_currents[0] + 0.01).abs() < 1e-6); // 10 mA out of source
    }

    #[test]
    fn dc_voltage_divider_two_resistors() {
        let mut c = Circuit::new("div2");
        let a = c.node("a");
        let mid = c.node("mid");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [a, 0],
                waveform: Waveform::Dc(12.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [a, mid],
                value: 1e3,
            },
        );
        c.add_component(
            "R2",
            CircuitComponent::Resistor {
                nodes: [mid, 0],
                value: 2e3,
            },
        );
        let r = SpiceAnalyzer::new(c).dc_operating_point().unwrap();
        assert!((r.node_voltages[2] - 8.0).abs() < 1e-6);
    }

    #[test]
    fn dc_diode_forward_bias() {
        let mut c = Circuit::new("diode");
        let a = c.node("a");
        let mid = c.node("mid");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [a, 0],
                waveform: Waveform::Dc(5.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [a, mid],
                value: 1e3,
            },
        );
        c.add_component(
            "D1",
            CircuitComponent::Diode {
                nodes: [mid, 0],
                model: DiodeModel::default(),
                area: 1.0,
            },
        );
        let r = SpiceAnalyzer::new(c).dc_operating_point().unwrap();
        let vd = r.node_voltages[2];
        assert!((0.6..0.8).contains(&vd), "diode drop {vd}");
        // Current through R equals diode current
        let i_r = (5.0 - vd) / 1e3;
        let d = DiodeModel::default();
        assert!((d.current(vd, 300.0) - i_r).abs() / i_r < 1e-3);
    }

    #[test]
    fn ac_rc_lowpass_matches_analytic() {
        let (c, _inp, _out) = rc_lowpass();
        let ac = SpiceAnalyzer::new(c).ac_analysis(1e3, 1e8, 10).unwrap();
        // fc = 1/(2πRC) ≈ 159.15 kHz — wait: R=1k, C=1n → fc = 159 kHz
        for (i, &f) in ac.frequencies.iter().enumerate() {
            let expected_mag = 1.0 / (1.0 + (f / 159_154.943).powi(2)).sqrt();
            let mag = ac.node_voltages[i][2].abs();
            assert!(
                (mag - expected_mag).abs() / expected_mag < 1e-4,
                "f={f} mag={mag} expected={expected_mag}"
            );
            // Phase: −atan(f/fc)
            let expected_ph = -(f / 159_154.943).atan().to_degrees();
            let ph = ac.phase_at(2)[i];
            assert!((ph - expected_ph).abs() < 0.01, "ph {ph} vs {expected_ph}");
        }
    }

    #[test]
    fn transient_rc_charges_exponentially() {
        // 0 → 1 V step at t = 0 through the RC low-pass.
        let mut c = Circuit::new("rc_step");
        let inp = c.node("in");
        let out = c.node("out");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [inp, 0],
                waveform: Waveform::Pwl(vec![(0.0, 0.0), (1.0e-12, 1.0), (10.0, 1.0)]),
                ac_magnitude: 1.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [inp, out],
                value: 1e3,
            },
        );
        c.add_component(
            "C1",
            CircuitComponent::Capacitor {
                nodes: [out, 0],
                value: 1e-9,
            },
        );
        let tr = SpiceAnalyzer::new(c).transient(5e-6, 1e-9, 10.0).unwrap();
        let tau = 1e3 * 1e-9;
        let wf = tr.waveform(2);
        // v(5τ) = 1·(1 − e⁻⁵) ≈ 0.9933
        let expected = 1.0f64 * (1.0 - (-(5.0e-6f64) / tau).exp());
        let last = *wf.last().unwrap();
        assert!(
            (last - expected).abs() < 0.005,
            "v(5τ) = {last}, expected {expected}"
        );
        // Starts near zero (DC op of the step source is 0 V).
        assert!(wf[0] < 0.05, "v(0) = {}", wf[0]);
        // Mid-run sample: v(1τ) ≈ 0.632
        let idx = tr.times.iter().position(|&t| t >= tau).unwrap();
        assert!((wf[idx] - 0.632).abs() < 0.02, "v(τ) = {}", wf[idx]);
    }

    #[test]
    fn transient_pulse_through_rc() {
        let mut c = Circuit::new("pulse_rc");
        let inp = c.node("in");
        let out = c.node("out");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [inp, 0],
                waveform: Waveform::Pulse {
                    v1: 0.0,
                    v2: 3.3,
                    delay: 0.0,
                    rise: 1e-9,
                    fall: 1e-9,
                    width: 1e-6,
                    period: 2e-6,
                },
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "R1",
            CircuitComponent::Resistor {
                nodes: [inp, out],
                value: 10e3,
            },
        );
        c.add_component(
            "C1",
            CircuitComponent::Capacitor {
                nodes: [out, 0],
                value: 10e-12,
            },
        );
        let tr = SpiceAnalyzer::new(c).transient(2e-6, 2e-9, 5.0).unwrap();
        let wf = tr.waveform(2);
        let tau = 10e3 * 10e-12; // 100 ns
        assert!((wf[0] - 0.0).abs() < 0.1);
        // Mid-pulse (t = 0.5 µs = 5τ): fully charged
        let mid_idx = tr.times.iter().position(|&t| t >= 0.5e-6).unwrap();
        let expected = 3.3f64 * (1.0 - (-(0.5e-6f64) / tau).exp());
        assert!(
            (wf[mid_idx] - expected).abs() < 0.05,
            "v(0.5µs) = {}, expected {expected}",
            wf[mid_idx]
        );
        // After the pulse (t = 1.5 µs = 5τ past the fall): fully discharged
        let late_idx = tr.times.iter().position(|&t| t >= 1.5e-6).unwrap();
        assert!(wf[late_idx] < 0.05, "v(1.5µs) = {}", wf[late_idx]);
    }

    #[test]
    fn mosfet_dc_amplifier_operates() {
        let mut c = Circuit::new("nmos");
        let d = c.node("drain");
        let g = c.node("gate");
        c.add_component(
            "VDD",
            CircuitComponent::VoltageSource {
                nodes: [d, 0],
                waveform: Waveform::Dc(5.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "VGS",
            CircuitComponent::VoltageSource {
                nodes: [g, 0],
                waveform: Waveform::Dc(2.0),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "RD",
            CircuitComponent::Resistor {
                nodes: [d, 0],
                value: 1e3,
            },
        );
        c.add_component(
            "M1",
            CircuitComponent::Mosfet {
                nodes: [0, g, 0, 0], // drain to ground? No — see below
                model: tpt_elec_spice_models::MosfetModel::level1(
                    tpt_elec_spice_models::MosfetParameters::default(),
                ),
            },
        );
        // Fix topology: M1 drain at `d`, source at 0 — replace the component.
        c.components.pop();
        c.component_names.pop();
        c.add_component(
            "M1",
            CircuitComponent::Mosfet {
                nodes: [d, g, 0, 0],
                model: tpt_elec_spice_models::MosfetModel::level1(
                    tpt_elec_spice_models::MosfetParameters::default(),
                ),
            },
        );
        let r = SpiceAnalyzer::new(c).dc_operating_point().unwrap();
        // Id ≈ 0.5·110µ·10·(1.3)² ≈ 9.3 mA → V(d) = 5 − 9.3·1 = negative → triode!
        // In triode the solver still must find a consistent solution:
        let vd = r.node_voltages[1];
        assert!(vd.is_finite() && (-1e-6..=5.0).contains(&vd), "vd = {vd}");
    }

    #[test]
    fn noise_of_rc_lowpass() {
        let (c, _inp, _out) = rc_lowpass();
        let noise = SpiceAnalyzer::new(c)
            .noise_analysis(2, (1e3, 1e6), 20)
            .unwrap();
        // At DC the full 4kTR of R1 appears across the (open) cap; density
        // at low f ≈ 4kT·R ≈ 1.66e-17 V²/Hz transferred ≈ 1.66e-17
        let first = noise.spots[0].density;
        let expected = 4.0 * 1.380649e-23 * 300.0 * 1e3;
        assert!(
            (first - expected).abs() / expected < 0.2,
            "{first} vs {expected}"
        );
        assert!(noise.total_rms > 0.0);
    }

    #[test]
    fn golden_rc_lowpass_ac() {
        // Analytic single-pole reference: test-data/golden/spice/rc_lowpass_ac.json
        #[derive(serde::Deserialize)]
        struct Golden {
            frequencies_hz: Vec<f64>,
            r_ohm: f64,
            c_farad: f64,
            magnitude_tolerance_rel: f64,
        }
        let raw = include_str!("../../../../test-data/golden/spice/rc_lowpass_ac.json");
        let golden: Golden = serde_json::from_str(raw).unwrap();
        let fc = 1.0 / (std::f64::consts::TAU * golden.r_ohm * golden.c_farad);

        let netlist = include_str!("../../../../test-data/spice/rc_lowpass.net");
        let circuit = tpt_elec_spice_netlist::SpiceNetlistParser::parse(netlist).unwrap();
        let ac = SpiceAnalyzer::new(circuit)
            .ac_analysis(
                golden.frequencies_hz[0],
                *golden.frequencies_hz.last().unwrap(),
                10,
            )
            .unwrap();
        for &f in golden.frequencies_hz.iter() {
            let pos = ac
                .frequencies
                .iter()
                .enumerate()
                .min_by(|a, b| ((a.1 - f).abs()).total_cmp(&(b.1 - f).abs()))
                .map(|(k, _)| k)
                .unwrap();
            let expected = 1.0 / (1.0 + (f / fc).powi(2)).sqrt();
            let got = ac.node_voltages[pos][2].abs();
            assert!(
                (got - expected).abs() / expected < golden.magnitude_tolerance_rel,
                "f={f}: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn golden_buck_converter_transient() {
        // Phase 2 milestone: buck converter, netlist → MNA → transient.
        #[derive(serde::Deserialize)]
        struct Golden {
            sample_times_us: Vec<f64>,
            v_out_v: Vec<f64>,
            tolerance_v: f64,
        }
        let raw = include_str!("../../../../test-data/golden/spice/buck_converter_transient.json");
        let golden: Golden = serde_json::from_str(raw).unwrap();

        let netlist = include_str!("../../../../test-data/spice/buck.net");
        let circuit = tpt_elec_spice_netlist::SpiceNetlistParser::parse(netlist).unwrap();
        let tr = SpiceAnalyzer::new(circuit)
            .transient(50e-6, 20e-9, 5.0)
            .unwrap();
        // node 4 = "out" (in=1, g=2, sw=3, out=4)
        let mut prev = f64::NEG_INFINITY;
        for (i, &t_us) in golden.sample_times_us.iter().enumerate() {
            let idx = tr
                .times
                .iter()
                .enumerate()
                .min_by(|a, b| ((a.1 - t_us * 1e-6).abs()).total_cmp(&((b.1 - t_us * 1e-6).abs())))
                .map(|(k, _)| k)
                .unwrap();
            let v = tr.node_voltages[idx][4];
            assert!(
                (v - golden.v_out_v[i]).abs() < golden.tolerance_v,
                "t={t_us}µs: v={v}, golden={}",
                golden.v_out_v[i]
            );
            assert!(
                v > prev && v < 6.0,
                "physical check: monotonic rise below 6 V, got {v} after {prev}"
            );
            prev = v;
        }
    }

    #[test]
    fn opamp_gain_shows_in_dc() {
        let mut c = Circuit::new("opamp");
        let inp = c.node("in");
        let out = c.node("out");
        c.add_component(
            "V1",
            CircuitComponent::VoltageSource {
                nodes: [inp, 0],
                waveform: Waveform::Dc(0.1),
                ac_magnitude: 0.0,
            },
        );
        c.add_component(
            "X1",
            CircuitComponent::OpAmp {
                inv: 0,
                noninv: inp,
                out,
                model: tpt_elec_spice_models::OpAmpModel {
                    gain: 100.0,
                    ..Default::default()
                },
            },
        );
        let r = SpiceAnalyzer::new(c).dc_operating_point().unwrap();
        assert!((r.node_voltages[2] - 10.0).abs() < 1e-6);
    }
}
