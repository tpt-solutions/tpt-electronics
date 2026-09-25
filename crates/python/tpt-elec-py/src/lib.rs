// SPDX-License-Identifier: MIT OR Apache-2.0

//! Python bindings for `tpt-electronics`.
//!
//! Exposes the thermal solver, controlled-impedance calculators and LC ladder
//! filter synthesis to Python so parametric sweeps and optimisation can be
//! scripted from notebooks.
//!
//! # Safety
//!
//! This crate contains no hand-written `unsafe`. The `unsafe_code = "allow"`
//! lint entry in `Cargo.toml` exists solely because pyo3's generated FFI glue
//! requires it; the workspace default is `forbid` everywhere else.
//!
//! # Example
//!
//! ```python
//! import tpt_elec_py as tp
//!
//! # Filter synthesis: prototype g-values, ladder elements and a swept response.
//! lp = tp.synthesize_filter("butterworth", order=5, response="lowpass",
//!                           cutoff_hz=100e6)
//! print(lp["g_values"])
//! print(lp["insertion_loss_db"][200])
//!
//! # Controlled impedance.
//! trace = tp.impedance.microstrip(width_mm=0.35, thickness_mm=0.035,
//!                                 height_mm=0.2, er=4.4)
//! print(trace["z0"])
//!
//! # Steady-state thermal.
//! r = tp.thermal.solve_steady_state(
//!     size_mm=(50.0, 50.0, 1.6), cell_mm=2.5,
//!     heat_sources=[(25.0, 25.0, 0.0, 3.0)],
//!     ambient_c=25.0, convection_h=10.0)
//! print(r["max_temp_c"])
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;

mod filters;
mod impedance;
mod spice;
mod thermal;

/// The package version reported to Python.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The top-level `tpt_elec_py` module.
#[pymodule]
fn tpt_elec_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", VERSION)?;

    // Submodules.
    filters::register(m)?;
    impedance::register(m)?;
    thermal::register(m)?;
    spice::register(m)?;

    // Convenience re-exports so the common entry points are one call away.
    m.add_function(wrap_pyfunction!(filters::synthesize_filter, m)?)?;
    m.add_function(wrap_pyfunction!(filters::filter_types, m)?)?;

    // Constants mirroring `tpt_elec_core`.
    m.add("SPEED_OF_LIGHT", tpt_elec_core::SPEED_OF_LIGHT)?;
    m.add("STEFAN_BOLTZMANN", tpt_elec_thermal::STEFAN_BOLTZMANN)?;

    Ok(())
}

/// Creates an empty Python dict.
pub(crate) fn new_dict<'py>(py: Python<'py>) -> Bound<'py, PyDict> {
    PyDict::new(py)
}
