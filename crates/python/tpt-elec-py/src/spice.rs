// SPDX-License-Identifier: MIT OR Apache-2.0

//! SPICE bindings (`tpt-elec-spice-netlist`, `tpt-elec-spice-analysis`).
//!
//! The netlist text is the unit of input rather than a parsed-circuit handle:
//! that keeps the Python side free of opaque handles it has to keep alive,
//! and every entry point is a single call, which is what a notebook sweep
//! wants.

use pyo3::prelude::*;
use tpt_elec_spice_analysis::SpiceAnalyzer;
use tpt_elec_spice_netlist::SpiceNetlistParser;

use crate::new_dict;

/// Parses a netlist and summarises the circuit.
#[pyfunction]
fn parse_netlist<'py>(py: Python<'py>, netlist: &str) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let circuit = SpiceNetlistParser::parse(netlist)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    let nodes: Vec<String> = circuit.nodes().iter().map(|n| n.name.clone()).collect();
    let d = new_dict(py);
    d.set_item("name", circuit.name.clone())?;
    d.set_item("nodes", nodes)?;
    d.set_item("node_count", circuit.node_count())?;
    d.set_item("components", circuit.component_names.clone())?;
    d.set_item("component_count", circuit.components.len())?;
    Ok(d)
}

/// Resolves a node name to the internal index used by the result helpers.
fn node_index(circuit: &tpt_elec_spice_core::Circuit, node: &str) -> PyResult<usize> {
    circuit
        .node_by_name(node)
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("no such node {node:?}")))
}

/// DC operating point.
#[pyfunction]
fn dc_operating_point<'py>(
    py: Python<'py>,
    netlist: &str,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let circuit = SpiceNetlistParser::parse(netlist)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    let op = SpiceAnalyzer::new(circuit)
        .dc_operating_point()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.message))?;
    let d = new_dict(py);
    d.set_item("node_voltages", op.node_voltages)?;
    d.set_item("branch_currents", op.branch_currents)?;
    d.set_item("iterations", op.iterations)?;
    Ok(d)
}

/// Small-signal AC sweep; returns the magnitude/phase of one node.
#[pyfunction]
#[pyo3(signature = (netlist, start_hz, stop_hz, points_per_decade, node="out"))]
fn ac_analysis<'py>(
    py: Python<'py>,
    netlist: &str,
    start_hz: f64,
    stop_hz: f64,
    points_per_decade: u32,
    node: &str,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let circuit = SpiceNetlistParser::parse(netlist)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    let idx = node_index(&circuit, node)?;
    let ac = SpiceAnalyzer::new(circuit)
        .ac_analysis(start_hz, stop_hz, points_per_decade)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.message))?;
    // Derive the node traces before consuming the owned fields below.
    // `magnitude_at` is a linear |V|; dB is derived here so neither name lies.
    let magnitude = ac.magnitude_at(idx);
    let magnitude_db: Vec<f64> = magnitude
        .iter()
        .map(|m| {
            if *m > 0.0 {
                20.0 * m.log10()
            } else {
                f64::NEG_INFINITY
            }
        })
        .collect();
    let phase = ac.phase_at(idx);
    let d = new_dict(py);
    d.set_item("frequencies_hz", ac.frequencies)?;
    d.set_item("magnitude", magnitude)?;
    d.set_item("magnitude_db", magnitude_db)?;
    d.set_item("phase_deg", phase)?;
    d.set_item("node", node)?;
    Ok(d)
}

/// Transient analysis; returns the waveform of one node.
///
/// `t_step_s` is the initial step and `max_step_factor` caps how far the
/// adaptive step may grow. A coarse `t_step_s` is the usual cause of a
/// transient that disagrees with the Rust golden: the buck-converter fixture
/// is reproduced with `t_step_s = 20e-9, max_step_factor = 5.0`.
#[pyfunction]
#[pyo3(signature = (netlist, t_stop_s, t_step_s, node="out", max_step_factor=5.0))]
fn transient<'py>(
    py: Python<'py>,
    netlist: &str,
    t_stop_s: f64,
    t_step_s: f64,
    node: &str,
    max_step_factor: f64,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let circuit = SpiceNetlistParser::parse(netlist)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))?;
    let idx = node_index(&circuit, node)?;
    let tr = SpiceAnalyzer::new(circuit)
        .transient(t_stop_s, t_step_s, max_step_factor)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.message))?;
    // Derive the node trace before consuming the owned fields below.
    let waveform = tr.waveform(idx);
    let d = new_dict(py);
    d.set_item("times_s", tr.times)?;
    d.set_item("voltages", waveform)?;
    d.set_item("node", node)?;
    Ok(d)
}

/// Registers the `spice` submodule on `m`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "spice")?;
    m.add_function(wrap_pyfunction!(parse_netlist, &m)?)?;
    m.add_function(wrap_pyfunction!(dc_operating_point, &m)?)?;
    m.add_function(wrap_pyfunction!(ac_analysis, &m)?)?;
    m.add_function(wrap_pyfunction!(transient, &m)?)?;
    m.add(
        "__doc__",
        "SPICE netlist parsing plus DC/AC/transient analysis.",
    )?;
    parent.add_submodule(&m)?;
    Ok(())
}
