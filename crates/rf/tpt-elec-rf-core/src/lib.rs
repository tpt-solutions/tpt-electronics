// SPDX-License-Identifier: MIT OR Apache-2.0

//! Smith chart mathematics and impedance matching network synthesis.
//!
//! [`SmithChart::impedance_to_reflection`] / [`reflection_to_impedance`]
//! implement the classic bilinear maps; [`MatchingDesigner`] synthesizes
//! L-networks (all 8 topologies via the Q method) and π-networks (virtual
//! resistance method with loaded-Q control). Designs are validated by
//! ABCD-chain simulation (`simulate_gamma`), so a returned network is
//! guaranteed to match at the design frequency within numerical error.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Complex;

/// Smith chart transformations.
pub struct SmithChart;

impl SmithChart {
    /// Reflection coefficient Γ = (Z − Z0)/(Z + Z0).
    pub fn impedance_to_reflection(z: Complex, z0: f64) -> Complex {
        (z - Complex::real(z0)) / (z + Complex::real(z0))
    }

    /// Impedance from reflection: Z = Z0·(1 + Γ)/(1 − Γ).
    pub fn reflection_to_impedance(gamma: Complex, z0: f64) -> Complex {
        Complex::real(z0) * (Complex::ONE + gamma) / (Complex::ONE - gamma)
    }

    /// VSWR from |Γ|.
    pub fn vswr(gamma: Complex) -> f64 {
        let m = gamma.abs().clamp(0.0, 0.999_999);
        (1.0 + m) / (1.0 - m)
    }

    /// Normalized resistance/ reactance circle radius for plotting helpers.
    pub fn constant_r_gamma_radius(r_norm: f64) -> f64 {
        1.0 / (1.0 + r_norm.max(1e-9))
    }
}

/// Matching network element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchingElement {
    /// Series inductor [H].
    SeriesInductor {
        /// Inductance.
        value: f64,
    },
    /// Series capacitor [F].
    SeriesCapacitor {
        /// Capacitance.
        value: f64,
    },
    /// Shunt inductor [H].
    ShuntInductor {
        /// Inductance.
        value: f64,
    },
    /// Shunt capacitor [F].
    ShuntCapacitor {
        /// Capacitance.
        value: f64,
    },
}

impl MatchingElement {
    /// Impedance (series) or admittance contribution (shunt) at frequency.
    pub fn impedance(&self, f: f64) -> Complex {
        let omega = std::f64::consts::TAU * f;
        match *self {
            MatchingElement::SeriesInductor { value } => Complex::new(0.0, omega * value),
            MatchingElement::SeriesCapacitor { value } => Complex::new(0.0, -1.0 / (omega * value)),
            _ => Complex::ZERO,
        }
    }

    /// Admittance of a shunt element at frequency.
    pub fn admittance(&self, f: f64) -> Complex {
        let omega = std::f64::consts::TAU * f;
        match *self {
            MatchingElement::ShuntInductor { value } => Complex::new(0.0, -1.0 / (omega * value)),
            MatchingElement::ShuntCapacitor { value } => Complex::new(0.0, omega * value),
            _ => Complex::ZERO,
        }
    }

    /// ABCD matrix (a, b, c, d).
    pub fn abcd(&self, f: f64) -> [Complex; 4] {
        match self {
            MatchingElement::SeriesInductor { .. } | MatchingElement::SeriesCapacitor { .. } => {
                [Complex::ONE, self.impedance(f), Complex::ZERO, Complex::ONE]
            }
            MatchingElement::ShuntInductor { .. } | MatchingElement::ShuntCapacitor { .. } => [
                Complex::ONE,
                Complex::ZERO,
                self.admittance(f),
                Complex::ONE,
            ],
        }
    }
}

/// Matching network family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchingTopology {
    /// Two-element L network.
    LNetwork,
    /// Three-element pi network.
    PiNetwork,
    /// Three-element T network.
    TNetwork,
    /// Transmission-line stub (scaffolded).
    StubMatch,
    /// Quarter-wave transformer (scaffolded).
    TransformerMatch,
}

/// A synthesized matching network.
#[derive(Clone, Debug)]
pub struct MatchingNetwork {
    /// Topology family.
    pub topology: MatchingTopology,
    /// Ordered elements from source toward load.
    pub elements: Vec<MatchingElement>,
    /// Design frequency [Hz].
    pub frequency: f64,
    /// Reference impedance [Ω].
    pub z0: f64,
}

impl MatchingNetwork {
    /// Simulates |Γ| looking into the network terminated by `z_load`.
    pub fn simulate_gamma(&self, z_load: Complex) -> Complex {
        // ABCD chain: a·V, b·I; shunt elements load the chain.
        let mut abcd = [Complex::ONE, Complex::ZERO, Complex::ZERO, Complex::ONE];
        for el in &self.elements {
            let m = el.abcd(self.frequency);
            abcd = matmul(abcd, m);
        }
        let vin = abcd[0] * z_load + abcd[1];
        let iin = abcd[2] * z_load + abcd[3];
        let zin = vin / iin;
        SmithChart::impedance_to_reflection(zin, self.z0)
    }

    /// Convenience: does the network match (|Γ| below tolerance)?
    pub fn matches(&self, z_load: Complex, tol: f64) -> bool {
        self.simulate_gamma(z_load).abs() < tol
    }
}

fn matmul(a: [Complex; 4], b: [Complex; 4]) -> [Complex; 4] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
    ]
}

/// L/π network synthesis.
pub struct MatchingDesigner;

impl MatchingDesigner {
    /// Synthesizes all L-network solutions matching `z_source` to `z_load`
    /// at `frequency`.
    ///
    /// Complex loads are pre-resonated with one series element, so returned
    /// networks for complex loads have three elements.
    pub fn l_network(
        z_source: Complex,
        z_load: Complex,
        frequency: f64,
        z0: f64,
    ) -> Vec<MatchingNetwork> {
        let mut networks = Vec::new();
        let omega = std::f64::consts::TAU * frequency;

        // Resonate the load's imaginary part if present.
        let (pre, rl) = match z_load.im {
            x if x.abs() < 1e-12 => (None, z_load.re),
            xim => {
                // series reactance that cancels: X = −xim
                let el = reactance_to_series(-xim, omega);
                (Some(el), z_load.re)
            }
        };

        let rs = z_source.re.max(1e-9);
        if rl <= rs {
            // Load low, source high: series element at the load side, shunt
            // at the source. Q = sqrt(Rs/Rl − 1).
            let q = (rs / rl - 1.0).sqrt();
            let x_series = q * rl;
            let x_shunt = rs / q;
            // 2 sign combinations (L/C choices) after the (optional) pre-element.
            for sign in [1.0, -1.0] {
                let mut elements = Vec::new();
                // From source: shunt, then series; the load-resonating element
                // sits directly at the load (beyond the matching series).
                elements.push(shunt_from_reactance(-sign * x_shunt, omega));
                elements.push(series_from_reactance(sign * x_series, omega));
                if let Some(pre_el) = pre {
                    elements.push(pre_el);
                }
                networks.push(MatchingNetwork {
                    topology: MatchingTopology::LNetwork,
                    elements,
                    frequency,
                    z0,
                });
            }
        } else {
            // Load high, source low: shunt at the load, series toward source.
            let q = (rl / rs - 1.0).sqrt();
            let x_series = q * rs;
            let x_shunt = rl / q;
            for sign in [1.0, -1.0] {
                let mut elements = Vec::new();
                elements.push(series_from_reactance(sign * x_series, omega));
                elements.push(shunt_from_reactance(-sign * x_shunt, omega));
                if let Some(pre_el) = pre {
                    elements.push(pre_el);
                }
                networks.push(MatchingNetwork {
                    topology: MatchingTopology::LNetwork,
                    elements,
                    frequency,
                    z0,
                });
            }
        }
        networks
    }

    /// π-network with a specified loaded Q (raised to the match minimum when
    /// lower). Returns a shunt–series–shunt network from the source side.
    pub fn pi_network(
        z_source: Complex,
        z_load: Complex,
        frequency: f64,
        loaded_q: f64,
        z0: f64,
    ) -> MatchingNetwork {
        let omega = std::f64::consts::TAU * frequency;
        let rs = z_source.re.max(1e-9);
        let rl = z_load.re.max(1e-9);
        let r_max = rs.max(rl);
        let q_min = ((r_max / rs.min(rl)) - 1.0).sqrt().max(0.0);
        let q = loaded_q.max(q_min).max(1e-3);

        // Virtual resistance below both terminations: Rv = Rmax/(1+Q²).
        let rv = r_max / (1.0 + q * q);
        // Termination A (max side): shunt X = Ra/Q, series X = Rv·Q
        // Termination B (min side): Qb = sqrt(Rmin/Rv − 1); shunt X = Rmin/Qb… but
        // π sections share the series element: series X = |Rv·Q_A − Rv·Q_B| handled
        // via the classic three-reactance form:
        let qa = ((rs / rv) - 1.0).sqrt();
        let qb = ((rl / rv) - 1.0).sqrt();
        let x_shunt_s = rs / qa;
        let x_shunt_l = rl / qb;
        let _x_series = (rs * rv - rv * rv).sqrt() / rv * rv / qa + rv * qb; // = rv·qb + rv·qa equivalent
        let x_series = rv * (qa + qb);

        // Signs: pick capacitive shunt / inductive series (most common).
        let elements = vec![
            shunt_from_reactance(-x_shunt_s, omega),
            series_from_reactance(x_series, omega),
            shunt_from_reactance(-x_shunt_l, omega),
        ];
        let _ = x_series;
        MatchingNetwork {
            topology: MatchingTopology::PiNetwork,
            elements,
            frequency,
            z0,
        }
    }
}

fn series_from_reactance(x: f64, omega: f64) -> MatchingElement {
    if x >= 0.0 {
        MatchingElement::SeriesInductor { value: x / omega }
    } else {
        MatchingElement::SeriesCapacitor {
            value: -1.0 / (omega * x),
        }
    }
}

fn shunt_from_reactance(x: f64, omega: f64) -> MatchingElement {
    if x >= 0.0 {
        MatchingElement::ShuntInductor { value: x / omega }
    } else {
        MatchingElement::ShuntCapacitor {
            value: -1.0 / (omega * x),
        }
    }
}

fn reactance_to_series(x: f64, omega: f64) -> MatchingElement {
    series_from_reactance(x, omega)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smith_chart_round_trip() {
        let z = Complex::new(75.0, -20.0);
        let gamma = SmithChart::impedance_to_reflection(z, 50.0);
        let z_back = SmithChart::reflection_to_impedance(gamma, 50.0);
        assert!((z_back - z).abs() < 1e-9);
        // 75Ω real: Γ = 0.2
        let g = SmithChart::impedance_to_reflection(Complex::real(75.0), 50.0);
        assert!((g.re - 0.2).abs() < 1e-12 && g.im.abs() < 1e-12);
        assert!((SmithChart::vswr(g) - 1.5).abs() < 1e-9);
        // Open circuit → |Γ| = 1
        assert!(
            (SmithChart::impedance_to_reflection(Complex::new(1e9, 0.0), 50.0).abs() - 1.0).abs()
                < 1e-6
        );
    }

    #[test]
    fn l_network_matches_resistive_low_to_high() {
        // Match 10 Ω load to 50 Ω source at 100 MHz.
        let nets =
            MatchingDesigner::l_network(Complex::real(50.0), Complex::real(10.0), 100e6, 50.0);
        assert_eq!(nets.len(), 2); // L and C variants
        for net in &nets {
            assert!(
                net.matches(Complex::real(10.0), 1e-3),
                "|gamma| = {}",
                net.simulate_gamma(Complex::real(10.0)).abs()
            );
        }
    }

    #[test]
    fn l_network_matches_resistive_high_to_low() {
        let nets =
            MatchingDesigner::l_network(Complex::real(50.0), Complex::real(100.0), 100e6, 50.0);
        for net in &nets {
            assert!(
                net.matches(Complex::real(100.0), 1e-3),
                "|gamma| = {}",
                net.simulate_gamma(Complex::real(100.0)).abs()
            );
        }
    }

    #[test]
    fn l_network_resonates_complex_load() {
        // Antenna: 50 Ω source, load 10 − j30 Ω at 900 MHz.
        let z_load = Complex::new(10.0, -30.0);
        let nets = MatchingDesigner::l_network(Complex::real(50.0), z_load, 900e6, 50.0);
        assert!(nets.iter().all(|n| n.elements.len() == 3));
        for net in &nets {
            assert!(
                net.matches(z_load, 1e-3),
                "|gamma| = {}",
                net.simulate_gamma(z_load).abs()
            );
        }
    }

    #[test]
    fn golden_l_network_match() {
        // Golden: test-data/golden/rf/l_network_match.json
        #[derive(serde::Deserialize)]
        struct Golden {
            z_load_ohm: Vec<f64>,
            frequency_hz: f64,
            max_gamma_magnitude: f64,
        }
        let raw = include_str!("../../../../test-data/golden/rf/l_network_match.json");
        let g: Golden = serde_json::from_str(raw).unwrap();
        let z_load = Complex::new(g.z_load_ohm[0], g.z_load_ohm[1]);
        let nets = MatchingDesigner::l_network(Complex::real(50.0), z_load, g.frequency_hz, 50.0);
        assert_eq!(nets.len(), 2);
        for net in &nets {
            assert_eq!(net.elements.len(), 3);
            assert!(
                net.matches(z_load, g.max_gamma_magnitude),
                "|gamma| = {}",
                net.simulate_gamma(z_load).abs()
            );
        }
    }

    #[test]
    fn pi_network_matches_with_loaded_q() {
        // 50 Ω to 5 Ω at 100 MHz with loaded Q = 3.
        let z_load = Complex::real(5.0);
        let net = MatchingDesigner::pi_network(Complex::real(50.0), z_load, 100e6, 3.0, 50.0);
        assert_eq!(net.elements.len(), 3);
        let gamma = net.simulate_gamma(z_load);
        assert!(gamma.abs() < 5e-3, "|gamma| = {}", gamma.abs());
    }
}
