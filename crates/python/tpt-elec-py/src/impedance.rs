// SPDX-License-Identifier: MIT OR Apache-2.0

//! Controlled-impedance bindings (`tpt-elec-si-impedance`).

use pyo3::prelude::*;
use tpt_elec_core::Length;
use tpt_elec_rf_core::SmithChart;
use tpt_elec_si_impedance::ImpedanceCalculator;

use crate::new_dict;

/// Converts a `LineParameters` into a Python dict.
fn line_to_dict<'py>(
    py: Python<'py>,
    line: &tpt_elec_si_core::LineParameters,
) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
    let d = new_dict(py);
    d.set_item("z0", line.z0)?;
    d.set_item("z0_odd", line.z0_odd)?;
    d.set_item("z0_even", line.z0_even)?;
    d.set_item("differential_z0", line.differential_impedance())?;
    d.set_item("propagation_delay_s_per_m", line.propagation_delay)?;
    d.set_item(
        "velocity_m_per_s",
        1.0 / line.propagation_delay.max(f64::MIN_POSITIVE),
    )?;
    d.set_item("loss_tangent", line.loss_tangent)?;
    d.set_item("skin_effect", line.skin_effect)?;
    Ok(d)
}

/// Microstrip characteristic impedance via Hammerstad-Jensen.
///
/// All dimensions are in millimetres.
#[pyfunction]
#[pyo3(signature = (width_mm, thickness_mm, height_mm, er))]
fn microstrip(
    py: Python<'_>,
    width_mm: f64,
    thickness_mm: f64,
    height_mm: f64,
    er: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let line = ImpedanceCalculator::microstrip(
        Length::mm(width_mm),
        Length::mm(thickness_mm),
        Length::mm(height_mm),
        er,
    );
    line_to_dict(py, &line)
}

/// Symmetric or asymmetric stripline impedance.
#[pyfunction]
#[pyo3(signature = (width_mm, thickness_mm, height_above_mm, height_below_mm, er))]
fn stripline(
    py: Python<'_>,
    width_mm: f64,
    thickness_mm: f64,
    height_above_mm: f64,
    height_below_mm: f64,
    er: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let line = ImpedanceCalculator::stripline(
        Length::mm(width_mm),
        Length::mm(thickness_mm),
        Length::mm(height_above_mm),
        Length::mm(height_below_mm),
        er,
    );
    line_to_dict(py, &line)
}

/// Edge-coupled differential pair impedance (odd/even modes).
#[pyfunction]
#[pyo3(signature = (width_mm, spacing_mm, height_mm, er))]
fn differential_pair(
    py: Python<'_>,
    width_mm: f64,
    spacing_mm: f64,
    height_mm: f64,
    er: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let line = ImpedanceCalculator::differential_pair(
        Length::mm(width_mm),
        Length::mm(spacing_mm),
        Length::mm(height_mm),
        er,
    );
    line_to_dict(py, &line)
}

/// Solves the inverse problem: trace width [mm] achieving `target_ohm`.
#[pyfunction]
#[pyo3(signature = (target_ohm, thickness_mm, height_mm, er))]
fn suggest_microstrip_width(
    py: Python<'_>,
    target_ohm: f64,
    thickness_mm: f64,
    height_mm: f64,
    er: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let w = ImpedanceCalculator::suggest_microstrip(
        target_ohm,
        Length::mm(thickness_mm),
        Length::mm(height_mm),
        er,
    );
    let z =
        ImpedanceCalculator::microstrip(w, Length::mm(thickness_mm), Length::mm(height_mm), er).z0;
    let d = new_dict(py);
    d.set_item("width_mm", w.as_meters() * 1e3)?;
    d.set_item("z0", z)?;
    Ok(d)
}

/// Monte-Carlo tolerance stack-up for microstrip Z0.
///
/// `tolerances` are fractional (e.g. `0.1` for ±10 %).
#[pyfunction]
#[pyo3(signature = (width_mm, thickness_mm, height_mm, er, width_tol, thickness_tol, height_tol, er_tol, samples=200, seed=1))]
#[allow(clippy::too_many_arguments)]
fn monte_carlo_microstrip(
    py: Python<'_>,
    width_mm: f64,
    thickness_mm: f64,
    height_mm: f64,
    er: f64,
    width_tol: f64,
    thickness_tol: f64,
    height_tol: f64,
    er_tol: f64,
    samples: u32,
    seed: u64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let mc = ImpedanceCalculator::monte_carlo_microstrip(
        Length::mm(width_mm),
        Length::mm(thickness_mm),
        height_mm * 1e-3,
        er,
        width_tol,
        height_tol,
        thickness_tol,
        er_tol,
        samples,
        seed,
    );
    let d = new_dict(py);
    d.set_item("samples", mc.samples)?;
    d.set_item("z0_mean", mc.mean)?;
    d.set_item("z0_std", mc.std)?;
    d.set_item("z0_p05", mc.p05)?;
    d.set_item("z0_p50", mc.p50)?;
    d.set_item("z0_p95", mc.p95)?;
    Ok(d)
}

/// Impedance from a reflection coefficient: `Z = Z0(1+G)/(1-G)`.
#[pyfunction]
#[pyo3(signature = (z0_ohm, gamma_re, gamma_im=0.0))]
fn reflection_to_impedance(
    py: Python<'_>,
    z0_ohm: f64,
    gamma_re: f64,
    gamma_im: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let gamma = tpt_elec_core::Complex::new(gamma_re, gamma_im);
    let z = SmithChart::reflection_to_impedance(gamma, z0_ohm);
    let vswr = SmithChart::vswr(gamma);
    let d = new_dict(py);
    d.set_item("z_real", z.re)?;
    d.set_item("z_imag", z.im)?;
    d.set_item("vswr", vswr)?;
    Ok(d)
}

/// Reflection coefficient from an impedance: `G = (Z - Z0)/(Z + Z0)`.
#[pyfunction]
#[pyo3(signature = (z0_ohm, z_real, z_imag=0.0))]
fn impedance_to_reflection(
    py: Python<'_>,
    z0_ohm: f64,
    z_real: f64,
    z_imag: f64,
) -> PyResult<Bound<'_, pyo3::types::PyDict>> {
    let z = tpt_elec_core::Complex::new(z_real, z_imag);
    let g = SmithChart::impedance_to_reflection(z, z0_ohm);
    let d = new_dict(py);
    d.set_item("gamma_re", g.re)?;
    d.set_item("gamma_imag", g.im)?;
    Ok(d)
}

/// Registers the `impedance` submodule on `m`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "impedance")?;
    m.add_function(wrap_pyfunction!(microstrip, &m)?)?;
    m.add_function(wrap_pyfunction!(stripline, &m)?)?;
    m.add_function(wrap_pyfunction!(differential_pair, &m)?)?;
    m.add_function(wrap_pyfunction!(suggest_microstrip_width, &m)?)?;
    m.add_function(wrap_pyfunction!(monte_carlo_microstrip, &m)?)?;
    m.add_function(wrap_pyfunction!(reflection_to_impedance, &m)?)?;
    m.add_function(wrap_pyfunction!(impedance_to_reflection, &m)?)?;
    m.add(
        "__doc__",
        "Controlled-impedance calculators (Hammerstad-Jensen, IPC-2141).",
    )?;
    parent.add_submodule(&m)?;
    Ok(())
}
