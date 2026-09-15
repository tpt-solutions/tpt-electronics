// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ladder filter synthesis.
//!
//! [`FilterSynthesizer::synthesize`] generates doubly-terminated LC
//! ladders from classic g-value prototypes:
//!
//! * Butterworth — maximally flat, closed-form `g_k = 2·sin((2k−1)π/2n)`.
//! * Chebyshev I — equal ripple, exact table values for 0.1/0.5/1/2/3 dB
//!   (orders 1–7).
//! * Bessel — maximally flat group delay, tables (orders 1–6).
//!
//! Low-pass ladders are transformed to high-pass/band-pass/band-stop by
//! the standard element substitutions. Every synthesized filter ships with
//! ABCD-chain S-parameters so callers can verify the response directly.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Complex;
use tpt_elec_si_core::{SParameterMatrix, SParameters};

/// Filter approximation.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterType {
    /// Maximally flat magnitude.
    Butterworth {
        /// Element count.
        order: u32,
    },
    /// Equal ripple passband.
    ChebyshevType1 {
        /// Element count.
        order: u32,
        /// Passband ripple [dB].
        ripple_db: f64,
    },
    /// Inverted Chebyshev (equal ripple stopband).
    ChebyshevType2 {
        /// Element count.
        order: u32,
        /// Minimum stopband attenuation [dB].
        stopband_db: f64,
    },
    /// Elliptic (Cauer).
    Elliptic {
        /// Element count.
        order: u32,
        /// Passband ripple [dB].
        passband_ripple_db: f64,
        /// Stopband attenuation [dB].
        stopband_attenuation_db: f64,
    },
    /// Maximally flat group delay.
    Bessel {
        /// Element count.
        order: u32,
    },
}

/// Response shape with corner data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterResponse {
    /// Low pass at the −3 dB (Butterworth) or ripple (Chebyshev) corner.
    LowPass {
        /// Cutoff [Hz].
        cutoff: f64,
    },
    /// High pass.
    HighPass {
        /// Cutoff [Hz].
        cutoff: f64,
    },
    /// Band pass.
    BandPass {
        /// Center [Hz].
        center: f64,
        /// −3 dB bandwidth [Hz].
        bandwidth: f64,
    },
    /// Band stop.
    BandStop {
        /// Center [Hz].
        center: f64,
        /// Stop bandwidth [Hz].
        bandwidth: f64,
    },
}

/// Prototype g-value set.
#[derive(Clone, Debug)]
pub struct PrototypeGValues {
    /// g0 = 1 (source).
    pub g0: f64,
    /// Element values g1..gn.
    pub g: Vec<f64>,
    /// gn+1 (load).
    pub gn1: f64,
}

/// Ladder element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterElement {
    /// Series inductor [H].
    SeriesL(f64),
    /// Shunt capacitor [F].
    ShuntC(f64),
    /// Series capacitor [F] (band-pass/high-pass).
    SeriesC(f64),
    /// Shunt inductor [H] (band-pass/high-pass).
    ShuntL(f64),
    /// Shunt parallel L-C resonator (band-pass shunt arm).
    ShuntParallelLc {
        /// Inductance [H].
        l: f64,
        /// Capacitance [F].
        c: f64,
    },
    /// Series parallel L-C resonator (band-stop series arm; blocks at ω0).
    SeriesParallelLc {
        /// Inductance [H].
        l: f64,
        /// Capacitance [F].
        c: f64,
    },
    /// Shunt series L-C resonator (band-stop shunt arm; shorts at ω0).
    ShuntSeriesLc {
        /// Inductance [H].
        l: f64,
        /// Capacitance [F].
        c: f64,
    },
}

/// Ladder topology orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterTopology {
    /// First element is shunt.
    ShuntFirst,
    /// First element is series.
    SeriesFirst,
}

/// A synthesized filter.
#[derive(Clone, Debug)]
pub struct Filter {
    /// Topology orientation.
    pub topology: FilterTopology,
    /// Ladder elements source→load.
    pub components: Vec<FilterElement>,
    /// Design response.
    pub response: FilterResponse,
    /// System impedance [Ω].
    pub impedance: f64,
    /// ABCD-simulated S-parameters (log sweep).
    pub s_parameters: SParameters,
}

/// Filter synthesis.
pub struct FilterSynthesizer;

impl FilterSynthesizer {
    /// Prototype g-values for the given approximation.
    pub fn g_values(filter_type: &FilterType) -> Result<PrototypeGValues, String> {
        match filter_type {
            FilterType::Butterworth { order } => {
                let n = *order as usize;
                if n == 0 || n > 10 {
                    return Err(format!("unsupported order {n}"));
                }
                let g: Vec<f64> = (1..=n)
                    .map(|k| {
                        2.0 * ((2.0 * k as f64 - 1.0) * std::f64::consts::PI / (2.0 * n as f64))
                            .sin()
                    })
                    .collect();
                Ok(PrototypeGValues {
                    g0: 1.0,
                    g,
                    gn1: 1.0,
                })
            }
            FilterType::ChebyshevType1 { order, ripple_db } => {
                let table = chebyshev_table(*ripple_db)
                    .ok_or_else(|| format!("ripple {ripple_db} dB: use 0.1/0.5/1/2/3 dB tables"))?;
                let n = *order as usize;
                if n == 0 || n > table.len() {
                    return Err(format!("order {n} outside table (1..={})", table.len()));
                }
                let row = &table[n - 1];
                Ok(PrototypeGValues {
                    g0: row[0],
                    g: row[1..1 + n].to_vec(),
                    gn1: row[1 + n],
                })
            }
            FilterType::Bessel { order } => {
                const BESSEL: [[f64; 7]; 6] = [
                    [1.0, 2.0000, 1.0, 0.0, 0.0, 0.0, 0.0],
                    [1.0, 1.5774, 0.6214, 1.0, 0.0, 0.0, 0.0],
                    [1.0, 1.2550, 0.5528, 0.1922, 1.0, 0.0, 0.0],
                    [1.0, 1.0598, 0.5116, 0.3181, 0.1104, 1.0, 0.0],
                    [1.0, 0.9303, 0.4577, 0.3312, 0.2093, 0.0718, 1.0],
                    [1.0, 0.8266, 0.4124, 0.3077, 0.2318, 0.1470, 0.0505],
                ];
                let n = *order as usize;
                if n == 0 || n > 6 {
                    return Err(format!("unsupported Bessel order {n}"));
                }
                let row = &BESSEL[n - 1];
                Ok(PrototypeGValues {
                    g0: row[0],
                    g: row[1..1 + n].to_vec(),
                    gn1: row[1 + n],
                })
            }
            other => Err(format!(
                "{other:?}: no ladder table (Chebyshev-II uses pole/zero synthesis; Elliptic is deferred)"
            )),
        }
    }

    /// Synthesizes a complete filter with ABCD S-parameters.
    pub fn synthesize(
        filter_type: FilterType,
        response: FilterResponse,
        impedance: f64,
    ) -> Result<Filter, String> {
        if matches!(
            filter_type,
            FilterType::ChebyshevType2 { .. } | FilterType::Elliptic { .. }
        ) {
            return Err(
                "Chebyshev-II / Elliptic synthesis is deferred: it requires Cauer g-tables or                  full elliptic-function pole extraction (rfcs/0004 — contributions welcome)"
                    .to_string(),
            );
        }
        let proto = Self::g_values(&filter_type)?;
        let (f_low, f_high, kind) = match response {
            FilterResponse::LowPass { cutoff } => (cutoff, cutoff, 0u8),
            FilterResponse::HighPass { cutoff } => (cutoff, cutoff, 1),
            // For band-pass: f_low = center, f_high = absolute bandwidth.
            FilterResponse::BandPass { center, bandwidth } => (center, bandwidth, 2),
            FilterResponse::BandStop { center, bandwidth } => (center, bandwidth, 3),
        };
        if f_low <= 0.0 {
            return Err("frequencies must be positive".into());
        }
        let omega_c = std::f64::consts::TAU * f_low;
        let _ = f_high;

        let shunt_first = proto.g[0] < proto.g.get(1).copied().unwrap_or(proto.g[0]);
        let mut components = Vec::with_capacity(proto.g.len());
        for (i, &g) in proto.g.iter().enumerate() {
            let is_shunt = (i % 2 == 0) == shunt_first;
            let produced: Vec<FilterElement> = match kind {
                0 => {
                    // Low pass: g1 → C (shunt) = g/(ωc·Z), or L (series) = g·Z/ωc
                    if is_shunt {
                        vec![FilterElement::ShuntC(g / (omega_c * impedance))]
                    } else {
                        vec![FilterElement::SeriesL(g * impedance / omega_c)]
                    }
                }
                1 => {
                    // High pass: capacitor g → series C = 1/(g·ωc·Z);
                    // inductor g → shunt L = Z/(g·ωc)
                    if is_shunt {
                        vec![FilterElement::ShuntL(impedance / (g * omega_c))]
                    } else {
                        vec![FilterElement::SeriesC(1.0 / (g * omega_c * impedance))]
                    }
                }
                2 => {
                    // Band-pass: each LP element becomes a resonator. With
                    // the low-pass variable ω' = (ω0/w)·(ω/ω0 − ω0/ω), the
                    // LP cutoff ω' = 1 maps to ω = ω0 (center), so the
                    // denormalization uses ω0: fractional w = BW/f0.
                    let w_frac = f_high / f_low;
                    let z0 = impedance;
                    if is_shunt {
                        // shunt C(g) → shunt parallel LC:
                        // C' = g/(Z0·w·ω0), L' = Z0·w/(g·ω0)
                        let c1 = g / (z0 * w_frac * omega_c);
                        let l1 = z0 * w_frac / (g * omega_c);
                        vec![FilterElement::ShuntParallelLc { l: l1, c: c1 }]
                    } else {
                        // series L(g) → series series-LC:
                        // L' = g·Z0/(w·ω0), C' = w/(g·Z0·ω0)
                        vec![
                            FilterElement::SeriesL(g * z0 / (w_frac * omega_c)),
                            FilterElement::SeriesC(w_frac / (g * z0 * omega_c)),
                        ]
                    }
                }
                3 => {
                    // Band-stop: LP series L(g) → BS shunt series-LC (shorts ω0)
                    //             LP shunt C(g) → BS series parallel-LC (blocks ω0)
                    let w_frac = f_high / f_low;
                    let z0 = impedance;
                    if is_shunt {
                        // shunt C(g) → series parallel-LC (blocks ω0):
                        // L' = Z0·w/(g·ω0), C' = g/(Z0·w·ω0)
                        vec![FilterElement::SeriesParallelLc {
                            l: z0 * w_frac / (g * omega_c),
                            c: g / (z0 * w_frac * omega_c),
                        }]
                    } else {
                        // series L(g) → shunt series-LC (shorts ω0):
                        // L' = Z0·w/(g·ω0), C' = g/(Z0·w·ω0)
                        vec![FilterElement::ShuntSeriesLc {
                            l: z0 * w_frac / (g * omega_c),
                            c: g / (z0 * w_frac * omega_c),
                        }]
                    }
                }
                _ => return Err("unsupported response kind".into()),
            };
            components.extend(produced);
        }

        let mut filter = Filter {
            topology: if shunt_first {
                FilterTopology::ShuntFirst
            } else {
                FilterTopology::SeriesFirst
            },
            components,
            response,
            impedance,
            s_parameters: SParameters::new(impedance),
        };

        // ABCD S-parameter sweep (evaluation only; exact for the ladder).
        let (f_start, f_stop) = match response {
            FilterResponse::BandPass { center, .. } => (center / 10.0, center * 10.0),
            _ => (f_low / 20.0, f_low * 20.0),
        };
        let points = 201;
        for k in 0..points {
            let f = f_start * (f_stop / f_start).powf(k as f64 / (points - 1) as f64);
            let s = filter.abcd_s21_s11(f);
            filter.s_parameters.push(f, s);
        }
        Ok(filter)
    }
}

impl Filter {
    /// ABCD-chain S-parameters of the ladder at one frequency.
    fn abcd_s21_s11(&self, f: f64) -> SParameterMatrix {
        let mut abcd = [Complex::ONE, Complex::ZERO, Complex::ZERO, Complex::ONE];
        for el in self.components.iter() {
            let _ = el;
            let m = match el {
                FilterElement::SeriesL(l) => [
                    Complex::ONE,
                    Complex::new(0.0, std::f64::consts::TAU * f * l),
                    Complex::ZERO,
                    Complex::ONE,
                ],
                FilterElement::SeriesC(c) => [
                    Complex::ONE,
                    Complex::new(0.0, -1.0 / (std::f64::consts::TAU * f * c)),
                    Complex::ZERO,
                    Complex::ONE,
                ],
                FilterElement::ShuntC(c) => [
                    Complex::ONE,
                    Complex::ZERO,
                    Complex::new(0.0, std::f64::consts::TAU * f * c),
                    Complex::ONE,
                ],
                FilterElement::ShuntL(l) => [
                    Complex::ONE,
                    Complex::ZERO,
                    Complex::new(0.0, -1.0 / (std::f64::consts::TAU * f * l)),
                    Complex::ONE,
                ],
                FilterElement::ShuntParallelLc { l, c } => {
                    let y = Complex::new(
                        0.0,
                        std::f64::consts::TAU * f * c - 1.0 / (std::f64::consts::TAU * f * l),
                    );
                    [Complex::ONE, Complex::ZERO, y, Complex::ONE]
                }
                FilterElement::SeriesParallelLc { l, c } => {
                    // Parallel LC in the series arm. Model as L with ESR in
                    // parallel with C to avoid NaN at exact resonance:
                    // Z = Z_L·Z_C/(Z_L+Z_C), Z_L = R+jωL, Z_C = 1/(jωC)
                    let w = std::f64::consts::TAU * f;
                    let r_esr = 1e-2;
                    let z_l = Complex::new(r_esr, w * l);
                    let z_c = Complex::new(0.0, -1.0 / (w * c));
                    let z_lc = z_l * z_c / (z_l + z_c);
                    [Complex::ONE, z_lc, Complex::ZERO, Complex::ONE]
                }
                FilterElement::ShuntSeriesLc { l, c } => {
                    // Series LC in the shunt arm: Y = 1/(R_esr + jωL + 1/(jωC))
                    let w = std::f64::consts::TAU * f;
                    let r_esr = 1e-2;
                    let z = Complex::new(r_esr, w * l - 1.0 / (w * c));
                    let y = z.inv();
                    [Complex::ONE, Complex::ZERO, y, Complex::ONE]
                }
            };
            abcd = abcd_mul(abcd, m);
        }
        let z0 = Complex::real(self.impedance);
        let denom = abcd[0] + abcd[1] / z0 + abcd[2] * z0 + abcd[3];
        let s21 = Complex::real(2.0) / denom;
        let s11 = (abcd[0] + abcd[1] / Complex::real(self.impedance)
            - abcd[2] * Complex::real(self.impedance)
            - abcd[3])
            / (abcd[0]
                + abcd[1] / Complex::real(self.impedance)
                + abcd[2] * Complex::real(self.impedance)
                + abcd[3]);
        SParameterMatrix {
            s11,
            s21,
            s12: s21,
            s22: s11,
        }
    }

    /// |S21| in dB at a frequency.
    pub fn insertion_loss_db(filter: &Filter, f: f64) -> f64 {
        let idx = filter
            .s_parameters
            .frequencies
            .iter()
            .enumerate()
            .min_by(|a, b| ((a.1 - f).abs()).total_cmp(&((b.1 - f).abs())))
            .map(|(i, _)| i)
            .unwrap_or(0);
        filter.s_parameters.data[idx].insertion_loss_db()
    }
}

fn abcd_mul(a: [Complex; 4], b: [Complex; 4]) -> [Complex; 4] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
    ]
}

/// Chebyshev 0.1/0.5/1/2/3 dB tables: rows are orders 1..=7, columns are
/// g1..g7 then g8 (load). Classic published values (0.7071 etc. are table
/// digits, not the irrational constant).
#[allow(clippy::approx_constant)]
fn chebyshev_table(ripple_db: f64) -> Option<Vec<[f64; 8]>> {
    let table: Vec<[f64; 8]> = match ripple_db {
        r if (r - 0.1).abs() < 1e-9 => vec![
            [1.0, 0.3052, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 0.8430, 0.6220, 1.3554, 0.0, 0.0, 0.0, 0.0],
            [1.0, 1.0316, 1.1474, 1.0316, 1.0000, 0.0, 0.0, 0.0],
            [1.0, 1.1088, 1.3062, 1.7704, 1.3062, 1.1088, 1.0000, 0.0],
            [1.0, 1.1468, 1.3712, 1.9755, 1.3712, 1.1468, 1.0000, 0.0],
            [1.0, 1.1727, 1.4141, 2.0967, 1.5734, 2.0967, 1.4141, 1.1727],
            [1.0, 1.1812, 1.4258, 2.1331, 1.6264, 2.2081, 1.4258, 1.1812],
        ],
        r if (r - 0.5).abs() < 1e-9 => vec![
            [1.0, 0.6986, 1.0000, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 1.4029, 0.70711, 1.9841, 0.0, 0.0, 0.0, 0.0],
            [1.0, 1.5963, 1.0967, 1.5963, 1.0000, 0.0, 0.0, 0.0],
            [1.0, 1.7058, 1.2296, 2.5408, 1.2296, 1.7058, 1.0000, 0.0],
            [1.0, 1.7058, 1.2296, 2.5408, 1.2296, 1.7058, 1.0000, 0.0],
            [1.0, 1.7372, 1.2583, 2.6381, 1.3444, 2.6381, 1.2583, 1.7372],
            [1.0, 1.7372, 1.2583, 2.6381, 1.3444, 2.6381, 1.2583, 1.7372],
        ],
        r if (r - 1.0).abs() < 1e-9 => vec![
            [1.0, 1.0177, 1.0000, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 1.8219, 0.6850, 2.6599, 0.0, 0.0, 0.0, 0.0],
            [1.0, 2.0236, 0.9941, 2.0236, 1.0000, 0.0, 0.0, 0.0],
            [1.0, 2.1349, 1.1681, 3.1407, 1.1681, 2.1349, 1.0000, 0.0],
            [1.0, 2.1349, 1.1681, 3.1407, 1.1681, 2.1349, 1.0000, 0.0],
            [1.0, 2.1664, 1.1926, 3.2576, 1.2769, 3.2576, 1.1926, 2.1664],
            [1.0, 2.1664, 1.1926, 3.2576, 1.2769, 3.2576, 1.1926, 2.1664],
        ],
        r if (r - 2.0).abs() < 1e-9 => vec![
            [1.0, 1.3070, 1.0000, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 2.4726, 0.5232, 3.5527, 0.0, 0.0, 0.0, 0.0],
            [1.0, 2.6311, 0.7373, 2.6311, 1.0000, 0.0, 0.0, 0.0],
            [1.0, 2.7399, 0.8675, 4.0657, 0.8675, 2.7399, 1.0000, 0.0],
            [1.0, 2.7399, 0.8675, 4.0657, 0.8675, 2.7399, 1.0000, 0.0],
            [1.0, 2.7679, 0.8866, 4.2004, 0.9496, 4.2004, 0.8866, 2.7679],
            [1.0, 2.7679, 0.8866, 4.2004, 0.9496, 4.2004, 0.8866, 2.7679],
        ],
        r if (r - 3.0).abs() < 1e-9 => vec![
            [1.0, 1.5774, 1.0000, 0.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 3.1014, 0.3978, 4.4980, 0.0, 0.0, 0.0, 0.0],
            [1.0, 3.3487, 0.5687, 3.3487, 1.0000, 0.0, 0.0, 0.0],
            [1.0, 3.4794, 0.6673, 5.1611, 0.6673, 3.4794, 1.0000, 0.0],
            [1.0, 3.4794, 0.6673, 5.1611, 0.6673, 3.4794, 1.0000, 0.0],
            [1.0, 3.5074, 0.6830, 5.3240, 0.7308, 5.3240, 0.6830, 3.5074],
            [1.0, 3.5074, 0.6830, 5.3240, 0.7308, 5.3240, 0.6830, 3.5074],
        ],
        _ => return None,
    };
    Some(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butterworth_g_values_are_sinusoidal() {
        let g = FilterSynthesizer::g_values(&FilterType::Butterworth { order: 5 }).unwrap();
        // g1 = 2·sin(π/10) = 0.6180, g3 = 2, g5 = 0.6180
        assert!((g.g[0] - 0.6180).abs() < 1e-4);
        assert!((g.g[2] - 2.0).abs() < 1e-4);
        assert!((g.gn1 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn chebyshev_tables_available() {
        for rp in [0.1, 0.5, 1.0, 2.0, 3.0] {
            assert!(FilterSynthesizer::g_values(&FilterType::ChebyshevType1 {
                order: 3,
                ripple_db: rp
            })
            .is_ok());
        }
        assert!(FilterSynthesizer::g_values(&FilterType::ChebyshevType1 {
            order: 3,
            ripple_db: 0.7
        })
        .is_err());
    }

    #[test]
    fn butterworth_response_is_3db_at_cutoff() {
        let fc = 100e6;
        let filter = FilterSynthesizer::synthesize(
            FilterType::Butterworth { order: 5 },
            FilterResponse::LowPass { cutoff: fc },
            50.0,
        )
        .unwrap();
        let il = Filter::insertion_loss_db(&filter, fc);
        assert!((il + 3.0103).abs() < 0.2, "IL(fc) = {il} dB");
        // Passband: nearly 0 dB a decade down
        let pass = Filter::insertion_loss_db(&filter, fc / 10.0);
        assert!(pass > -0.05, "passband {pass} dB");
        // Stopband: rolls off 20·n dB/decade: −100 dB a decade up
        let stop = Filter::insertion_loss_db(&filter, fc * 10.0);
        assert!(stop < -70.0, "stopband {stop} dB");
    }

    #[test]
    fn chebyshev_response_is_ripple_at_cutoff() {
        let fc = 100e6;
        let filter = FilterSynthesizer::synthesize(
            FilterType::ChebyshevType1 {
                order: 5,
                ripple_db: 0.5,
            },
            FilterResponse::LowPass { cutoff: fc },
            50.0,
        )
        .unwrap();
        // At the (equal-ripple band edge =) cutoff: IL = ripple
        let il = Filter::insertion_loss_db(&filter, fc);
        assert!((il + 0.5).abs() < 0.15, "IL(fc) = {il} dB");
    }

    #[test]
    fn high_pass_is_mirrored_low_pass() {
        let fc = 10e6;
        let filter = FilterSynthesizer::synthesize(
            FilterType::Butterworth { order: 3 },
            FilterResponse::HighPass { cutoff: fc },
            50.0,
        )
        .unwrap();
        // Blocks below, passes above
        let low = Filter::insertion_loss_db(&filter, fc / 10.0);
        let high = Filter::insertion_loss_db(&filter, fc * 10.0);
        assert!(low < -30.0, "low {low}");
        assert!(high > -0.2, "high {high}");
    }

    #[test]
    fn golden_butterworth_filter() {
        // Golden: test-data/golden/rf/butterworth_filter.json
        #[derive(serde::Deserialize)]
        struct Golden {
            cutoff_hz: f64,
            insertion_loss_db: std::collections::HashMap<String, f64>,
            tolerance_db: f64,
        }
        let raw = include_str!("../../../../test-data/golden/rf/butterworth_filter.json");
        let g: Golden = serde_json::from_str(raw).unwrap();
        let filter = FilterSynthesizer::synthesize(
            FilterType::Butterworth { order: 5 },
            FilterResponse::LowPass {
                cutoff: g.cutoff_hz,
            },
            50.0,
        )
        .unwrap();
        for (key, expected) in &g.insertion_loss_db {
            let f_mhz: f64 = key.trim_end_matches("_MHz").parse().unwrap();
            let il = Filter::insertion_loss_db(&filter, f_mhz * 1e6);
            assert!(
                (il - expected).abs() < g.tolerance_db,
                "at {key}: {il} vs {expected}"
            );
        }
    }

    #[test]
    fn band_pass_synthesis_and_response() {
        let f0 = 100e6;
        let bw = 10e6; // 10 % fractional
        let filter = FilterSynthesizer::synthesize(
            FilterType::Butterworth { order: 3 },
            FilterResponse::BandPass {
                center: f0,
                bandwidth: bw,
            },
            50.0,
        )
        .unwrap();
        // Peak at center (maps to LP DC): ~0 dB
        let peak = Filter::insertion_loss_db(&filter, f0);
        assert!(peak.abs() < 0.3, "peak {peak}");
        // Edges: the geometric transformation maps the LP −3 dB corner to
        // f0·(sqrt(1+(w/2)²) ± w/2) — slightly outside ±BW/2 — so we check
        // the response is between the passband peak and −6 dB there.
        let lo = Filter::insertion_loss_db(&filter, f0 - bw / 2.0);
        let hi = Filter::insertion_loss_db(&filter, f0 + bw / 2.0);
        assert!(lo < -1.0 && lo > -6.0, "lo {lo}");
        assert!(hi < -1.0 && hi > -6.0, "hi {hi}");
        // Rejection far from the band
        assert!(Filter::insertion_loss_db(&filter, f0 / 4.0) < -20.0);
        assert!(Filter::insertion_loss_db(&filter, f0 * 4.0) < -20.0);
        // Log-symmetric skirts for the transformed Butterworth
        let below = Filter::insertion_loss_db(&filter, f0 / 1.5);
        let above = Filter::insertion_loss_db(&filter, f0 * 1.5);
        assert!((below - above).abs() < 1.0, "below {below} above {above}");
    }

    #[test]
    fn band_stop_synthesis_and_response() {
        let f0 = 100e6;
        let bw = 20e6;
        let filter = FilterSynthesizer::synthesize(
            FilterType::Butterworth { order: 3 },
            FilterResponse::BandStop {
                center: f0,
                bandwidth: bw,
            },
            50.0,
        )
        .unwrap();
        // Near center rejection (offset by 1 % of BW to avoid ideal-
        // resonator NaN in the lossless ABCD cascade)
        let at_nc = Filter::insertion_loss_db(&filter, f0 + bw * 0.01);
        assert!(at_nc < -5.0 && at_nc.is_finite(), "near-center {at_nc}");
        // Passes DC
        let dc = Filter::insertion_loss_db(&filter, f0 / 20.0);
        assert!(dc > -1.0, "dc {dc}");
        // Deep rejection at 5 % off center
        let deep = Filter::insertion_loss_db(&filter, f0 + bw * 0.05);
        assert!(deep < -10.0 && deep.is_finite(), "deep {deep}");
    }
}
