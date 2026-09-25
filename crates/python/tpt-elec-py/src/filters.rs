// SPDX-License-Identifier: MIT OR Apache-2.0

//! LC ladder filter synthesis bindings (`tpt-elec-rf-filters`).

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use tpt_elec_rf_filters::{Filter, FilterElement, FilterResponse, FilterSynthesizer, FilterType};

use crate::new_dict;

/// Builds a [`FilterType`] from a name plus its option values.
pub fn parse_filter_type(
    kind: &str,
    order: u32,
    ripple_db: f64,
    stopband_db: f64,
) -> PyResult<FilterType> {
    let lowered = kind.to_ascii_lowercase();
    let t = match lowered.as_str() {
        "butterworth" | "butter" => FilterType::Butterworth { order },
        "chebyshev1" | "chebyshev-i" => FilterType::ChebyshevType1 { order, ripple_db },
        "chebyshev2" | "chebyshev-ii" => FilterType::ChebyshevType2 { order, stopband_db },
        "bessel" | "thomson" => FilterType::Bessel { order },
        "elliptic" | "cauer" => FilterType::Elliptic {
            order,
            passband_ripple_db: ripple_db,
            stopband_attenuation_db: stopband_db,
        },
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown filter type {other:?}; expected butterworth, chebyshev1, chebyshev2, \
                 bessel or elliptic"
            )))
        }
    };
    Ok(t)
}

/// Builds a [`FilterResponse`] from a shape name.
pub fn parse_response(
    shape: &str,
    cutoff_hz: f64,
    center_hz: f64,
    bandwidth_hz: f64,
) -> PyResult<FilterResponse> {
    Ok(match shape.to_ascii_lowercase().as_str() {
        "lowpass" | "lp" => FilterResponse::LowPass { cutoff: cutoff_hz },
        "highpass" | "hp" => FilterResponse::HighPass { cutoff: cutoff_hz },
        "bandpass" | "bp" => FilterResponse::BandPass {
            center: center_hz,
            bandwidth: bandwidth_hz,
        },
        "bandstop" | "bs" => FilterResponse::BandStop {
            center: center_hz,
            bandwidth: bandwidth_hz,
        },
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "unknown response {other:?}; expected lowpass, highpass, bandpass or bandstop"
            )))
        }
    })
}

/// Names of the supported filter approximations.
#[pyfunction]
pub(crate) fn filter_types(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    PyList::new(
        py,
        [
            "butterworth",
            "chebyshev1",
            "chebyshev2",
            "bessel",
            "elliptic",
        ],
    )
}

/// Index of the swept frequency closest to `f`.
fn nearest_index(filter: &Filter, f: f64) -> usize {
    filter
        .s_parameters
        .frequencies
        .iter()
        .enumerate()
        .min_by(|a, b| ((a.1 - f).abs()).total_cmp(&((b.1 - f).abs())))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Describes each ladder element as a `(kind, value)` pair.
fn element_summary(filter: &Filter) -> Vec<(String, f64)> {
    filter
        .components
        .iter()
        .map(|el| match *el {
            FilterElement::SeriesL(l) => ("series_l_h".to_string(), l),
            FilterElement::ShuntC(c) => ("shunt_c_f".to_string(), c),
            FilterElement::SeriesC(c) => ("series_c_f".to_string(), c),
            FilterElement::ShuntL(l) => ("shunt_l_h".to_string(), l),
            FilterElement::ShuntParallelLc { l, .. } => ("shunt_parallel_lc".to_string(), l),
            FilterElement::SeriesParallelLc { l, .. } => ("series_parallel_lc".to_string(), l),
            FilterElement::ShuntSeriesLc { l, .. } => ("shunt_series_lc".to_string(), l),
        })
        .collect()
}

/// Synthesises a filter and returns its prototype, elements and swept response.
///
/// `cutoff_hz` is the corner frequency for low/high-pass and the centre
/// frequency for band shapes. `bandwidth_hz` applies to band shapes only.
#[pyfunction]
#[pyo3(signature = (kind, order, response="lowpass", cutoff_hz=1e9, center_hz=None, bandwidth_hz=None, impedance_ohm=50.0, ripple_db=0.5, stopband_db=40.0))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize_filter<'py>(
    py: Python<'py>,
    kind: &str,
    order: u32,
    response: &str,
    cutoff_hz: f64,
    center_hz: Option<f64>,
    bandwidth_hz: Option<f64>,
    impedance_ohm: f64,
    ripple_db: f64,
    stopband_db: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let ftype = parse_filter_type(kind, order, ripple_db, stopband_db)?;
    let center = center_hz.unwrap_or(cutoff_hz);
    let bw = bandwidth_hz.unwrap_or(cutoff_hz);
    let resp = parse_response(response, cutoff_hz, center, bw)?;
    let filter = FilterSynthesizer::synthesize(ftype.clone(), resp, impedance_ohm)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    // Prototype g-values exist only for the table-based approximations.
    let g_values = FilterSynthesizer::g_values(&ftype).ok().map(|p| p.g);

    // Sweep the response so callers can plot or optimise against it.
    let points = 401usize;
    let (f_start, f_stop) = match resp {
        FilterResponse::BandPass { center, .. } | FilterResponse::BandStop { center, .. } => {
            (center / 10.0, center * 10.0)
        }
        _ => (cutoff_hz / 20.0, cutoff_hz * 20.0),
    };
    let mut freqs = Vec::with_capacity(points);
    let mut il = Vec::with_capacity(points);
    let mut rl = Vec::with_capacity(points);
    let ratio = (f_stop / f_start).max(1.0 + f64::EPSILON);
    for k in 0..points {
        let t = k as f64 / (points - 1) as f64;
        freqs.push(f_start * ratio.powf(t));
    }
    for f in &freqs {
        let idx = nearest_index(&filter, *f);
        let s = &filter.s_parameters.data[idx];
        il.push(s.insertion_loss_db());
        rl.push(s.return_loss_db());
    }

    let d = new_dict(py);
    d.set_item("kind", kind)?;
    d.set_item("order", order)?;
    d.set_item("impedance_ohm", impedance_ohm)?;
    d.set_item("g_values", g_values)?;
    d.set_item("components", element_summary(&filter))?;
    d.set_item("frequencies_hz", freqs)?;
    d.set_item("insertion_loss_db", il)?;
    d.set_item("return_loss_db", rl)?;
    Ok(d)
}

/// Registers the `filters` submodule on `m`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "filters")?;
    m.add_function(wrap_pyfunction!(filter_types, &m)?)?;
    m.add_function(wrap_pyfunction!(synthesize_filter, &m)?)?;
    m.add(
        "__doc__",
        "LC ladder filter synthesis (Butterworth, Chebyshev, Bessel).",
    )?;
    parent.add_submodule(&m)?;
    Ok(())
}
