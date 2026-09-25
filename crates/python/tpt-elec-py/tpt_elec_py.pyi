# SPDX-License-Identifier: MIT OR Apache-2.0
"""Type stubs for the tpt-elec-py extension module.

Hand-maintained and checked against the pyo3 signatures in ``src/``. The
submodules are declared as classes so editors and ``mypy`` can type the
members of ``tp.spice`` etc.; ``smoke_test.py`` walks these classes and fails
if a name drifts from the implementation in either direction.

Place next to the built extension module (or in the project root).
"""

from typing import Any, Dict, List, Optional, Tuple

__version__: str

#: Speed of light in vacuum [m/s].
SPEED_OF_LIGHT: float
#: Stefan-Boltzmann constant [W/(m^2 K^4)].
STEFAN_BOLTZMANN: float

class filters:
    @staticmethod
    def filter_types() -> List[str]:
        """Names of the supported filter approximations."""

    @staticmethod
    def synthesize_filter(
        kind: str,
        order: int,
        response: str = "lowpass",
        cutoff_hz: float = 1e9,
        center_hz: Optional[float] = None,
        bandwidth_hz: Optional[float] = None,
        impedance_ohm: float = 50.0,
        ripple_db: float = 0.5,
        stopband_db: float = 40.0,
    ) -> Dict[str, Any]:
        """Synthesise an LC ladder filter.

        ``kind`` is one of ``butterworth``, ``chebyshev1``, ``chebyshev2``,
        ``bessel`` or ``elliptic``; ``response`` is one of ``lowpass``,
        ``highpass``, ``bandpass`` or ``bandstop``.

        Returns ``g_values`` (the prototype, or ``None`` for response-based
        approximations), ``components``, ``frequencies_hz``,
        ``insertion_loss_db`` and ``return_loss_db``.

        :raises ValueError: unknown filter type or response, or a specification
            the solver rejects (including ``elliptic``, not implemented yet).
        """

class impedance:
    @staticmethod
    def microstrip(
        width_mm: float, thickness_mm: float, height_mm: float, er: float
    ) -> Dict[str, Any]:
        """Microstrip impedance via Hammerstad-Jensen. All dimensions in mm.

        Returns z0, z0_odd, z0_even, differential_z0, propagation_delay_s_per_m,
        velocity_m_per_s, loss_tangent and skin_effect.
        """

    @staticmethod
    def stripline(
        width_mm: float,
        thickness_mm: float,
        height_above_mm: float,
        height_below_mm: float,
        er: float,
    ) -> Dict[str, Any]:
        """Symmetric or asymmetric stripline impedance."""

    @staticmethod
    def differential_pair(
        width_mm: float, spacing_mm: float, height_mm: float, er: float
    ) -> Dict[str, Any]:
        """Edge-coupled differential pair impedance (odd/even modes)."""

    @staticmethod
    def suggest_microstrip_width(
        target_ohm: float, thickness_mm: float, height_mm: float, er: float
    ) -> Dict[str, float]:
        """Inverse solve: the trace width [mm] that yields ``target_ohm``."""

    @staticmethod
    def monte_carlo_microstrip(
        width_mm: float,
        thickness_mm: float,
        height_mm: float,
        er: float,
        width_tol: float,
        thickness_tol: float,
        height_tol: float,
        er_tol: float,
        samples: int = 200,
        seed: int = 1,
    ) -> Dict[str, Any]:
        """Monte-Carlo tolerance stack-up. Tolerances are fractional (0.1 = 10 %)."""

    @staticmethod
    def reflection_to_impedance(
        z0_ohm: float, gamma_re: float, gamma_im: float = 0.0
    ) -> Dict[str, float]:
        """Impedance from a reflection coefficient: ``Z = Z0(1+G)/(1-G)``."""

    @staticmethod
    def impedance_to_reflection(
        z0_ohm: float, z_real: float, z_imag: float = 0.0
    ) -> Dict[str, float]:
        """Reflection coefficient from an impedance: ``G = (Z - Z0)/(Z + Z0)``."""

class thermal:
    @staticmethod
    def default_materials() -> Dict[str, Any]:
        """The built-in material library available to the thermal solver."""

    @staticmethod
    def solve_steady_state(
        size_mm: Tuple[float, float, float],
        cell_mm: float,
        heat_sources: List[Tuple[float, float, float, float]],
        ambient_c: float = 25.0,
        convection_h: Optional[float] = None,
        substrate_thickness_mm: Optional[float] = None,
    ) -> Dict[str, Any]:
        """Steady-state conduction on a uniform voxel grid.

        ``heat_sources`` are ``(x_mm, y_mm, z_mm, watts)`` tuples; the power is
        deposited into the nearest cell and ``z`` is accepted for convenience
        (the solver places it on the top layer). ``convection_h`` is a
        convection coefficient [W/(m^2 K)] applied to the surface.

        :raises ValueError: non-positive ``cell_mm`` or ``size_mm``.
        """

class spice:
    @staticmethod
    def parse_netlist(netlist: str) -> Dict[str, Any]:
        """Parse a SPICE netlist; returns name, nodes, node_count, components.

        Node names are normalised to upper case by the parser.

        :raises ValueError: the netlist does not parse.
        """

    @staticmethod
    def dc_operating_point(netlist: str) -> Dict[str, Any]:
        """DC operating point: node voltages, branch currents, iteration count.

        :raises ValueError: the netlist does not parse.
        :raises RuntimeError: the operating point did not converge.
        """

    @staticmethod
    def ac_analysis(
        netlist: str,
        start_hz: float,
        stop_hz: float,
        points_per_decade: int,
        node: str = "out",
    ) -> Dict[str, Any]:
        """Small-signal AC sweep of one node.

        Returns ``frequencies_hz``, ``magnitude`` (linear |V|), ``magnitude_db``
        (20*log10) and ``phase_deg``.

        :raises ValueError: the netlist does not parse, or ``node`` is unknown.
        :raises RuntimeError: the analysis failed to converge.
        """

    @staticmethod
    def transient(
        netlist: str,
        t_stop_s: float,
        t_step_s: float,
        node: str = "out",
        max_step_factor: float = 5.0,
    ) -> Dict[str, Any]:
        """Transient analysis of one node; returns ``times_s`` and ``voltages``.

        ``t_step_s`` is the initial step and ``max_step_factor`` caps adaptive
        step growth. A coarse ``t_step_s`` is the usual cause of a transient
        that disagrees with the Rust golden; the buck-converter fixture is
        reproduced with ``t_step_s = 20e-9, max_step_factor = 5.0``.

        :raises ValueError: the netlist does not parse, or ``node`` is unknown.
        :raises RuntimeError: the analysis failed to converge.
        """

# Re-exported at the top level for convenience.
filter_types: Any
synthesize_filter: Any
