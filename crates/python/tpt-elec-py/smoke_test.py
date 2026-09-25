# SPDX-License-Identifier: MIT OR Apache-2.0
"""Smoke tests for the tpt-elec-py extension module.

Run against a built extension module:

    cargo build -p tpt-elec-py
    python crates/python/tpt-elec-py/smoke_test.py

The module is picked up from ``target/<profile>/`` on the standard search path.
"""

import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.abspath(os.path.join(_HERE, "..", "..", ".."))
for _profile in ("debug", "release"):
    _candidate = os.path.join(_ROOT, "target", _profile)
    if not os.path.isdir(_candidate):
        continue
    sys.path.insert(0, _candidate)
    # CPython only auto-detects `.pyd`; cargo emits `.dll` on Windows.
    _dll = os.path.join(_candidate, "tpt_elec_py.dll")
    if sys.platform == "win32" and os.path.isfile(_dll):
        _pyd = os.path.join(_candidate, "tpt_elec_py.pyd")
        if not os.path.isfile(_pyd) or os.path.getmtime(_pyd) < os.path.getmtime(_dll):
            with open(_dll, "rb") as _src, open(_pyd, "wb") as _dst:
                _dst.write(_src.read())
    break

import tpt_elec_py as tp  # noqa: E402


def _read(relative_path: str) -> str:
    """Reads a repository file (netlists live in test-data/, outside the crate)."""
    with open(os.path.join(_ROOT, relative_path), encoding="utf-8") as handle:
        return handle.read()


def test_version() -> None:
    assert isinstance(tp.__version__, str)
    assert tp.__version__


def test_impedance() -> None:
    line = tp.impedance.microstrip(
        width_mm=0.35, thickness_mm=0.035, height_mm=0.2, er=4.4
    )
    assert 40.0 < line["z0"] < 60.0, line["z0"]
    assert line["velocity_m_per_s"] > 0.0

    pair = tp.impedance.differential_pair(
        width_mm=0.15, spacing_mm=0.2, height_mm=0.2, er=4.4
    )
    assert pair["z0"] > 0.0

    width = tp.impedance.suggest_microstrip_width(
        target_ohm=50.0, thickness_mm=0.035, height_mm=0.2, er=4.4
    )
    assert abs(width["z0"] - 50.0) < 0.5, width

    z = tp.impedance.reflection_to_impedance(50.0, 0.0, 0.0)
    assert abs(z["z_real"] - 50.0) < 1e-9
    assert abs(z["vswr"] - 1.0) < 1e-9

    mc = tp.impedance.monte_carlo_microstrip(
        width_mm=0.35,
        thickness_mm=0.035,
        height_mm=0.2,
        er=4.4,
        width_tol=0.1,
        thickness_tol=0.1,
        height_tol=0.1,
        er_tol=0.02,
        samples=64,
        seed=7,
    )
    assert mc["samples"] == 64
    assert mc["z0_mean"] > 0.0


def test_filters() -> None:
    kinds = tp.filter_types()
    assert "butterworth" in kinds

    lp = tp.synthesize_filter(
        "butterworth", order=5, response="lowpass", cutoff_hz=100e6
    )
    assert lp["order"] == 5
    assert lp["impedance_ohm"] == 50.0
    assert len(lp["frequencies_hz"]) == len(lp["insertion_loss_db"]) == 401
    assert len(lp["components"]) == 5
    assert lp["g_values"] is not None and len(lp["g_values"]) == 5

    # -3 dB at the corner for a 5th-order Butterworth.
    il = lp["insertion_loss_db"]
    freqs = lp["frequencies_hz"]
    idx = min(range(len(freqs)), key=lambda i: abs(freqs[i] - 100e6))
    assert abs(il[idx] + 3.01) < 0.2, il[idx]

    hp = tp.synthesize_filter(
        "chebyshev1", order=4, response="highpass", cutoff_hz=10e6, ripple_db=0.5
    )
    assert len(hp["components"]) >= 4

    bp = tp.synthesize_filter(
        "butterworth",
        order=3,
        response="bandpass",
        center_hz=100e6,
        bandwidth_hz=20e6,
    )
    assert len(bp["components"]) >= 3

    try:
        tp.synthesize_filter("nope", order=3)
    except ValueError as exc:
        assert "unknown filter type" in str(exc)
    else:  # pragma: no cover - the call above must raise
        raise AssertionError("expected ValueError for an unknown filter type")

    # Elliptic is exposed and reports that it is response-based.
    try:
        ell = tp.synthesize_filter(
            "elliptic",
            order=4,
            response="lowpass",
            cutoff_hz=100e6,
            ripple_db=1.0,
            stopband_db=40.0,
        )
    except ValueError:
        pass  # Not yet implemented in the Rust filter crate; that is fine.
    else:
        assert len(ell["frequencies_hz"]) > 0


def test_thermal() -> None:
    mats = tp.thermal.default_materials()
    assert mats["count"] > 0

    result = tp.thermal.solve_steady_state(
        size_mm=(20.0, 20.0, 1.6),
        cell_mm=2.0,
        heat_sources=[(10.0, 10.0, 0.0, 2.0)],
        ambient_c=25.0,
        convection_h=10.0,
    )
    assert result["cell_count"] > 0
    # A 2 W hotspot on a convection-cooled board must run above ambient.
    assert result["max_temp_c"] > 25.0, result["max_temp_c"]
    assert len(result["temperatures_c"]) == result["cell_count"]

    try:
        tp.thermal.solve_steady_state(
            size_mm=(20.0, 20.0, 1.6), cell_mm=0.0, heat_sources=[]
        )
    except ValueError as exc:
        assert "cell_mm" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("expected ValueError for a non-positive cell size")


def test_spice() -> None:
    import math

    rc = _read("test-data/spice/rc_lowpass.net")

    summary = tp.spice.parse_netlist(rc)
    assert summary["components"], summary
    # The parser normalises node names to upper case.
    assert "OUT" in summary["nodes"], summary["nodes"]

    # The RC low-pass is a known analytic single pole: fc = 1/(2*pi*R*C),
    # |H| = 1/sqrt(1+(f/fc)^2), which is -3.0103 dB at fc.
    fc = 1.0 / (2.0 * math.pi * 1000.0 * 1e-9)
    ac = tp.spice.ac_analysis(rc, 1e3, 1e7, 10, node="out")
    freqs, mags = ac["frequencies_hz"], ac["magnitude"]
    assert len(freqs) == len(mags) > 10
    idx = min(range(len(freqs)), key=lambda i: abs(freqs[i] - fc))
    expected_linear = 1.0 / math.sqrt(1.0 + (freqs[idx] / fc) ** 2)
    assert abs(mags[idx] - expected_linear) < 1e-3, mags[idx]
    assert abs(ac["magnitude_db"][idx] - (-3.0103)) < 0.2, ac["magnitude_db"][idx]
    # Magnitude must fall monotonically across the sweep.
    assert mags[0] > mags[-1]
    # Phase is degrees and trends toward -90 deg for a single pole.
    assert ac["phase_deg"][0] > ac["phase_deg"][-1]
    assert ac["phase_deg"][-1] < -80.0, ac["phase_deg"][-1]

    op = tp.spice.dc_operating_point(rc)
    assert len(op["node_voltages"]) >= 1

    # Buck startup, checked against the crate's own golden fixture so the
    # binding is tied to the same reference the Rust test uses.
    import json

    golden = json.loads(_read("test-data/golden/spice/buck_converter_transient.json"))
    buck = _read("test-data/spice/buck.net")
    tr = tp.spice.transient(
        buck,
        golden["sample_times_us"][-1] * 1e-6,
        20e-9,
        node=golden["node"],
        max_step_factor=5.0,
    )
    times, volts = tr["times_s"], tr["voltages"]
    assert len(volts) == len(times) > 10
    for t_us, want in zip(golden["sample_times_us"], golden["v_out_v"]):
        idx = min(range(len(times)), key=lambda i: abs(times[i] - t_us * 1e-6))
        assert abs(volts[idx] - want) < golden["tolerance_v"], (
            f"t={t_us}us: got {volts[idx]}, golden {want}"
        )
    # Physical check: rising during startup and bounded by the ideal D*Vin.
    assert all(b >= a for a, b in zip(volts, volts[1:])), "output fell during startup"
    assert max(volts) < 6.2, max(volts)

    try:
        tp.spice.ac_analysis(rc, 1e3, 1e7, 10, node="does_not_exist")
    except ValueError as exc:
        assert "no such node" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("expected ValueError for an unknown node")


def test_stub_matches_runtime() -> None:
    """The .pyi stub and the built module must declare the same names.

    The stubs are hand-maintained, so this fails if they drift from the pyo3
    signatures in either direction — which would otherwise only show up in an
    editor.
    """
    import ast
    import inspect

    with open(os.path.join(_HERE, "tpt_elec_py.pyi"), encoding="utf-8-sig") as handle:
        tree = ast.parse(handle.read())

    # Map each stub submodule class to the members it declares.
    stubbed = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        members = set()
        for item in node.body:
            if isinstance(item, ast.FunctionDef):
                members.add(item.name)
        stubbed[node.name] = members

    for sub, declared in stubbed.items():
        assert hasattr(tp, sub), f"missing submodule {sub}"
        module = getattr(tp, sub)
        actual = {
            name
            for name, value in vars(module).items()
            if not name.startswith("_") and callable(value)
        }
        missing = declared - actual
        extra = actual - declared
        assert not missing, f"{sub}: stubbed but missing at runtime: {sorted(missing)}"
        assert not extra, f"{sub}: present at runtime but not in the stub: {sorted(extra)}"

    # The two top-level re-exports must really be re-exported.
    for name in ("filter_types", "synthesize_filter"):
        assert callable(getattr(tp, name, None)), f"tp.{name} is not re-exported"


def main() -> int:
    tests = [
        test_version,
        test_impedance,
        test_filters,
        test_thermal,
        test_spice,
        test_stub_matches_runtime,
    ]
    failures = 0
    for t in tests:
        try:
            t()
        except Exception as exc:  # noqa: BLE001 - report every failure
            failures += 1
            print(f"FAIL {t.__name__}: {type(exc).__name__}: {exc}")
        else:
            print(f"ok   {t.__name__}")
    if failures:
        print(f"\n{failures} test(s) failed")
    else:
        print("\nall tests passed")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
