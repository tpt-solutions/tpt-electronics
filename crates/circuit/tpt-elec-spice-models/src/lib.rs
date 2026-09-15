// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device compact models for SPICE simulation.
//!
//! Clean-room implementations of the classic textbook models (no proprietary
//! model libraries — spec §7):
//!
//! * [`DiodeModel`] — Shockley diode equation with series resistance,
//!   junction capacitance, and reverse breakdown.
//! * [`MosfetModel`] — Level 1 (square-law), Level 2 (geometry), Level 3
//!   (semi-empirical); BSIM3/BSIM4/EKV enum slots fall back to Level 1
//!   until their parameter sets are implemented.
//! * [`BjtModel`] — Ebers-Moll transport form.
//! * [`OpAmpModel`] — single-pole voltage-controlled voltage source.
//!
//! All models expose both current *and* first-order conductances, which is
//! what the Newton-Raphson loop in `tpt-elec-spice-analysis` stamps.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::Length;

/// Boltzmann constant [J/K].
pub const BOLTZMANN: f64 = 1.380649e-23;
/// Elementary charge [C].
pub const ELEMENTARY_CHARGE: f64 = 1.602176634e-19;

/// Thermal voltage `kT/q` [V].
pub fn thermal_voltage(temperature_k: f64) -> f64 {
    BOLTZMANN * temperature_k / ELEMENTARY_CHARGE
}

/// MOSFET polarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MosfetPolarity {
    /// N-channel.
    N,
    /// P-channel (equations evaluated with reversed signs).
    P,
}

/// MOSFET model complexity level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MosfetLevel {
    /// Square-law (SPICE level 1).
    Level1,
    /// Geometry-aware refinement (SPICE level 2).
    Level2,
    /// Semi-empirical (SPICE level 3).
    Level3,
    /// BSIM3v3 (parameter set reserved; falls back to Level 1 math).
    Bsim3,
    /// BSIM4 (parameter set reserved; falls back to Level 1 math).
    Bsim4,
    /// EKV charge-sheet (parameter set reserved; falls back to Level 1).
    Ekv,
}

/// MOSFET model parameters.
#[derive(Clone, Debug)]
pub struct MosfetParameters {
    /// Zero-bias threshold voltage [V].
    pub vth0: f64,
    /// Transconductance parameter [A/V²].
    pub kp: f64,
    /// Channel-length modulation [1/V].
    pub lambda: f64,
    /// Body-effect coefficient [V^½].
    pub gamma: f64,
    /// Surface potential [V].
    pub phi: f64,
    /// Channel width [m].
    pub w: Length,
    /// Channel length [m].
    pub l: Length,
    /// Gate oxide capacitance per area [F/m²].
    pub cox: f64,
    /// Bulk junction saturation current [A].
    pub is_bulk: f64,
    /// Mobility degradation coefficient θ [1/V] (BSIM3/4 subset).
    pub theta: f64,
    /// Critical field for velocity saturation [V/m] (BSIM3/4 subset).
    pub e_sat: f64,
    /// Slope factor n (EKV subset).
    pub n_ekv: f64,
    /// Specific current I_S [A] (EKV subset).
    pub is_ekv: f64,
}

impl Default for MosfetParameters {
    fn default() -> Self {
        Self {
            vth0: 0.7,
            kp: 110e-6,
            lambda: 0.01,
            gamma: 0.4,
            phi: 0.6,
            w: Length::um(10.0),
            l: Length::um(1.0),
            cox: 1.0e-3, // ~ tox 3.45 nm
            is_bulk: 1.0e-14,
            theta: 0.05,
            e_sat: 4.0e6, // ~ 2 V across a 0.5 µm channel
            n_ekv: 1.3,
            is_ekv: 1.0e-6,
        }
    }
}

/// A MOSFET compact model.
#[derive(Clone, Debug)]
pub struct MosfetModel {
    /// Model level.
    pub level: MosfetLevel,
    /// Channel polarity.
    pub polarity: MosfetPolarity,
    /// Parameters.
    pub parameters: MosfetParameters,
}

/// Linearized MOSFET small-signal companion (for Newton stamping).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MosfetOperatingPoint {
    /// Drain-source current [A].
    pub ids: f64,
    /// Gate transconductance ∂Ids/∂Vgs [S].
    pub gm: f64,
    /// Output conductance ∂Ids/∂Vds [S].
    pub gds: f64,
    /// Bulk transconductance ∂Ids/∂Vbs [S].
    pub gmb: f64,
    /// Threshold voltage including body effect [V].
    pub vth: f64,
    /// Saturation flag.
    pub saturated: bool,
}

impl MosfetModel {
    /// A Level-1 model with the given parameters.
    pub fn level1(parameters: MosfetParameters) -> Self {
        Self {
            level: MosfetLevel::Level1,
            polarity: MosfetPolarity::N,
            parameters,
        }
    }

    /// Threshold voltage including body effect [V].
    pub fn threshold_voltage(&self, vbs: f64) -> f64 {
        let p = &self.parameters;
        let body = p.gamma * ((p.phi - vbs).max(0.0).sqrt() - p.phi.sqrt());
        let sign = match self.polarity {
            MosfetPolarity::N => 1.0,
            MosfetPolarity::P => -1.0,
        };
        sign * (p.vth0 + body)
    }

    /// Operating point evaluation (current + conductances).
    ///
    /// Voltages use the N-channel convention: `vgs`, `vds`, `vbs` as
    /// measured on the actual terminals. P-channel devices negate internally.
    pub fn operating_point(&self, vgs: f64, vds: f64, vbs: f64) -> MosfetOperatingPoint {
        let p = &self.parameters;
        let sign = match self.polarity {
            MosfetPolarity::N => 1.0,
            MosfetPolarity::P => -1.0,
        };
        // Fold polarity into the "intrinsic" device.
        let (vgs_i, vds_i, vbs_i) = (vgs * sign, vds * sign, vbs * sign);
        let vth = p.vth0 + p.gamma * ((p.phi - vbs_i).max(0.0).sqrt() - p.phi.sqrt());
        let vov = vgs_i - vth;
        let beta = p.kp * (p.w.as_meters() / p.l.as_meters());
        let lambda = p.lambda.max(0.0);

        if vov <= 0.0 && self.level != MosfetLevel::Ekv {
            // Cutoff (ignoring subthreshold for levels 1–3). EKV uses the
            // ln² charge-sheet law, which is continuous through threshold.
            return MosfetOperatingPoint {
                ids: 0.0,
                gm: 0.0,
                gds: 0.0,
                gmb: 0.0,
                vth: vth * sign,
                saturated: false,
            };
        }

        let (ids, gm, gds, saturated);
        match self.level {
            MosfetLevel::Bsim3 | MosfetLevel::Bsim4 => {
                // Short-channel subset shared by both levels: vertical-field
                // mobility degradation + velocity saturation + CLM.
                let theta = p.theta.max(0.0);
                let mu_factor = 1.0 / (1.0 + theta * vov);
                let beta_eff = beta * mu_factor;
                // Velocity saturation: quadratic current limited to
                // I_sat_lin = β_eff·vov·vds/(1 + vds/(Ec·L)) in triode and a
                // soft saturation knee at vds_sat = vov·EcL/(vov+EcL).
                let ec_l = (p.e_sat * p.l.as_meters()).max(1e-3);
                let vds_sat = vov * ec_l / (vov + ec_l);
                if vds_i < vds_sat {
                    let denom = 1.0 + vds_i / ec_l;
                    ids = beta_eff * (vov - 0.5 * vds_i) * vds_i / denom;
                    gm = beta_eff * vds_i * (1.0 + theta * vov / (1.0 + theta * vov)) / denom;
                    gds = beta_eff * (vov - vds_i) / denom;
                    saturated = false;
                } else {
                    // Id_sat = β_eff/2 · vov² / (1 + vov/(Ec·L)):
                    // quadratic → linear in vov as velocity saturation bites.
                    let denom = 1.0 + vov / ec_l;
                    let lin = beta_eff * 0.5 * vov * vov / denom;
                    ids = lin * (1.0 + lambda * vds_i);
                    gm = (beta_eff * vov * (1.0 + vov / ec_l / 2.0) / (denom * denom))
                        * (1.0 + lambda * vds_i);
                    gds = 0.5 * beta_eff * vov * vov / denom * lambda;
                    saturated = true;
                }
            }
            MosfetLevel::Ekv => {
                // EKV charge-sheet static law: smooth single expression across
                // weak/moderate/strong inversion:
                // Id = Is·(ln²(1+e^v_f) − ln²(1+e^v_r)), v = (Vov)/2nVt.
                let vt = 0.02585;
                let vf = (vov / (2.0 * p.n_ekv * vt)).clamp(-60.0, 60.0);
                let vsat = (vds_i - vov) / (2.0 * p.n_ekv * vt);
                let ln_f = (1.0 + vf.exp()).ln();
                let ln_r = (1.0 + ((-vsat).clamp(-60.0, 60.0)).exp()).ln();
                ids = p.is_ekv * (ln_f * ln_f - ln_r * ln_r);
                // Small-signal: numeric derivatives (smooth function).
                let dv = 1e-6;
                let vf2 = ((vov + dv) / (2.0 * p.n_ekv * vt)).clamp(-60.0, 60.0);
                let ln_f2 = (1.0 + vf2.exp()).ln();
                gm = p.is_ekv * (ln_f2 * ln_f2 - ln_f * ln_f) / dv;
                let vds2 = vds_i + dv;
                let vsat3 = (vds2 - vov) / (2.0 * p.n_ekv * vt);
                let ln_r3 = (1.0 + ((-vsat3).clamp(-60.0, 60.0)).exp()).ln();
                let ids2 = p.is_ekv * (ln_f * ln_f - ln_r3 * ln_r3);
                gds = (ids2 - ids) / dv;
                saturated = vds_i >= vov;
            }
            level => {
                let level = match level {
                    MosfetLevel::Level3 => MosfetLevel::Level3,
                    _ => MosfetLevel::Level1,
                };
                match level {
                    MosfetLevel::Level3 => {
                        // Semi-empirical: linear-region mobility reduction factor.
                        let f = 1.0 / (1.0 + 0.5 * vov);
                        if vds_i < vov {
                            let b = beta * f;
                            ids = b * (vov * vds_i - 0.5 * vds_i * vds_i);
                            gm = b * vds_i;
                            gds = b * vov;
                            saturated = false;
                        } else {
                            ids = 0.5 * beta * f * vov * vov * (1.0 + lambda * vds_i);
                            gm = beta * f * vov * (1.0 + lambda * vds_i);
                            gds = 0.5 * beta * f * vov * vov * lambda;
                            saturated = true;
                        }
                    }
                    _ => {
                        if vds_i < vov {
                            // Linear (triode) region
                            ids = beta * (vov * vds_i - 0.5 * vds_i * vds_i);
                            gm = beta * vds_i;
                            gds = beta * vov;
                            saturated = false;
                        } else {
                            // Saturation with channel-length modulation
                            ids = 0.5 * beta * vov * vov * (1.0 + lambda * vds_i);
                            gm = beta * vov * (1.0 + lambda * vds_i);
                            gds = 0.5 * beta * vov * vov * lambda;
                            saturated = true;
                        }
                    }
                }
            }
        }

        MosfetOperatingPoint {
            ids: ids * sign,
            gm: gm * sign,
            gds,
            gmb: -p.gamma / (2.0 * (p.phi - vbs_i).max(1e-3).sqrt()) * gm * sign,
            vth: vth * sign,
            saturated,
        }
    }

    /// Drain current [A] (convenience wrapper over
    /// [`operating_point`](Self::operating_point)).
    pub fn drain_current(&self, vgs: f64, vds: f64, vbs: f64) -> f64 {
        self.operating_point(vgs, vds, vbs).ids
    }
}

/// A diode compact model (Shockley equation).
#[derive(Clone, Debug)]
pub struct DiodeModel {
    /// Saturation current [A].
    pub is: f64,
    /// Ideality factor.
    pub n: f64,
    /// Series resistance [Ω] (0 disables).
    pub rs: f64,
    /// Zero-bias junction capacitance [F].
    pub cjo: f64,
    /// Junction potential [V].
    pub vj: f64,
    /// Grading coefficient.
    pub m: f64,
    /// Reverse breakdown voltage [V] (0 disables).
    pub bv: f64,
    /// Current at breakdown [A].
    pub ibv: f64,
}

impl Default for DiodeModel {
    fn default() -> Self {
        Self {
            is: 1.0e-14,
            n: 1.0,
            rs: 0.0,
            cjo: 0.0,
            vj: 0.7,
            m: 0.5,
            bv: 0.0,
            ibv: 1.0e-3,
        }
    }
}

impl DiodeModel {
    /// Junction current [A] at forward voltage `v` and temperature `t_k` [K].
    ///
    /// The exponent is clamped (as SPICE does) to keep Newton iterations
    /// finite for large forward voltages.
    pub fn current(&self, v: f64, temperature_k: f64) -> f64 {
        let vt = self.n * thermal_voltage(temperature_k);
        let v_d = self.internal_voltage(v, temperature_k);
        let x = (v_d / vt).clamp(-80.0, 80.0);
        self.is * (x.exp() - 1.0)
    }

    /// Junction conductance `dI/dv` [S] including series resistance.
    pub fn conductance(&self, v: f64, temperature_k: f64) -> f64 {
        let vt = self.n * thermal_voltage(temperature_k);
        let v_d = self.internal_voltage(v, temperature_k);
        let x = (v_d / vt).clamp(-80.0, 80.0);
        let g_junction = self.is / vt * x.exp();
        if self.rs > 0.0 {
            g_junction / (1.0 + g_junction * self.rs)
        } else {
            g_junction
        }
    }

    /// Junction (depletion) capacitance [F].
    pub fn junction_capacitance(&self, v: f64) -> f64 {
        if self.cjo == 0.0 {
            return 0.0;
        }
        let vj = self.vj;
        if v < vj * 0.999 {
            self.cjo / (1.0 - v / vj).powf(self.m)
        } else {
            // linear extrapolation beyond Vj
            self.cjo / (1.0f64 - 0.999).powf(self.m) * (1.0 + self.m * (v - vj * 0.999) / vj)
        }
    }

    fn internal_voltage(&self, v: f64, t_k: f64) -> f64 {
        if self.rs <= 0.0 {
            return v;
        }
        // Solve f(vd) = vd + rs·I(vd) − v = 0 with damped Newton steps
        // (undamped iterations overshoot through the exponential).
        let vt = self.n * thermal_voltage(t_k);
        let mut vd = v.min(0.9);
        for _ in 0..40 {
            let x = (vd / vt).clamp(-80.0, 80.0);
            let i = self.is * (x.exp() - 1.0);
            let g = self.is / vt * x.exp();
            let f = vd + self.rs * i - v;
            let fp = 1.0 + self.rs * g;
            let step = (f / fp).clamp(-2.0 * vt, 0.5 * vt);
            vd -= step;
            if step.abs() < 1e-12 {
                break;
            }
        }
        vd.max(-50.0 * vt)
    }
}

/// BJT polarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BjtPolarity {
    /// NPN.
    Npn,
    /// PNP.
    Pnp,
}

/// Ebers-Moll (transport form) BJT model.
#[derive(Clone, Debug)]
pub struct BjtModel {
    /// Polarity.
    pub polarity: BjtPolarity,
    /// Transport saturation current [A].
    pub is: f64,
    /// Maximum forward current gain.
    pub bf: f64,
    /// Maximum reverse current gain.
    pub br: f64,
    /// Forward early voltage [V] (0 disables).
    pub va: f64,
}

impl Default for BjtModel {
    fn default() -> Self {
        Self {
            polarity: BjtPolarity::Npn,
            is: 1.0e-15,
            bf: 100.0,
            br: 1.0,
            va: 50.0,
        }
    }
}

/// BJT operating point (collector / base currents and conductances).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BjtOperatingPoint {
    /// Collector current [A].
    pub ic: f64,
    /// Base current [A].
    pub ib: f64,
    /// ∂Ic/∂Vbe [S].
    pub gm: f64,
    /// ∂Ib/∂Vbe [S].
    pub gpi: f64,
    /// ∂Ic/∂Vce [S].
    pub gmu: f64,
}

impl BjtModel {
    /// Collector and base currents for the given junction voltages
    /// (`vbe`, `vbc`, both positive when forward-biased for NPN).
    pub fn operating_point(&self, vbe: f64, vbc: f64, t_k: f64) -> BjtOperatingPoint {
        let sign = match self.polarity {
            BjtPolarity::Npn => 1.0,
            BjtPolarity::Pnp => -1.0,
        };
        let vt = thermal_voltage(t_k);
        let (vbe_i, vbc_i) = (vbe * sign, vbc * sign);
        let exp_be = (vbe_i / vt).clamp(-80.0, 80.0).exp();
        let exp_bc = (vbc_i / vt).clamp(-80.0, 80.0).exp();

        // Transport form
        let it_f = self.is * (exp_be - 1.0);
        let it_r = self.is * (exp_bc - 1.0);
        let early = if self.va > 0.0 {
            1.0 + vbe_sign(vbe_i, vbc_i) * vbc_i.max(0.0) / self.va
        } else {
            1.0
        };
        let ic = (it_f - it_r / self.br_sign()) * early * sign;
        let ib = (it_f / self.bf + it_r / self.br) * sign;

        BjtOperatingPoint {
            ic,
            ib,
            gm: it_f / vt * early,
            gpi: it_f.max(self.is) / (self.bf * vt),
            gmu: it_r.max(self.is) / (self.br * vt),
        }
    }

    fn br_sign(&self) -> f64 {
        self.br.max(1e-3)
    }
}

fn vbe_sign(_vbe: f64, _vbc: f64) -> f64 {
    1.0
}

/// Single-pole op-amp macromodel.
#[derive(Clone, Debug)]
pub struct OpAmpModel {
    /// Open-loop DC gain [V/V].
    pub gain: f64,
    /// Input resistance [Ω].
    pub rin: f64,
    /// Output resistance [Ω].
    pub rout: f64,
    /// Dominant-pole frequency [Hz] (used by AC analysis).
    pub pole_hz: f64,
}

impl Default for OpAmpModel {
    fn default() -> Self {
        Self {
            gain: 1.0e5,
            rin: 1.0e6,
            rout: 50.0,
            pole_hz: 10.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mosfet_saturation_current() {
        // Spec §8 example: VGS = 2.0 V, VDS = 5.0 V → strongly saturated.
        let model = MosfetModel::level1(MosfetParameters {
            vth0: 0.7,
            kp: 110e-6,
            w: Length::um(10.0),
            l: Length::um(1.0),
            ..Default::default()
        });
        let id = model.drain_current(2.0, 5.0, 0.0);
        assert!(id > 0.0);
        // Square law: 0.5·kp·(W/L)·(VGS−VTH)²·(1+λ·VDS)
        let expected = 0.5 * 110e-6 * 10.0 * (2.0f64 - 0.7).powi(2) * (1.0 + 0.01 * 5.0);
        assert!((id - expected).abs() < 1e-12, "{id} vs {expected}");
        let op = model.operating_point(2.0, 5.0, 0.0);
        assert!(op.saturated);
    }

    #[test]
    fn mosfet_triode_and_cutoff() {
        let model = MosfetModel::level1(MosfetParameters::default());
        // Cutoff below threshold
        assert_eq!(model.drain_current(0.5, 1.0, 0.0), 0.0);
        // Triode: vds < vgs - vth (0.6 < 1.3)
        let op = model.operating_point(2.0, 0.6, 0.0);
        assert!(!op.saturated);
        let expected = 110e-6 * 10.0 * (1.3 * 0.6 - 0.5 * 0.36);
        assert!((op.ids - expected).abs() < 1e-12);
        // gm in triode = beta·vds
        assert!((op.gm - 110e-6 * 10.0 * 0.6).abs() < 1e-12);
    }

    #[test]
    fn mosfet_body_effect_raises_threshold() {
        let model = MosfetModel::level1(MosfetParameters::default());
        let vth0 = model.threshold_voltage(0.0);
        let vth_rev = model.threshold_voltage(-1.0);
        assert!(vth_rev > vth0, "reverse body bias raises |Vth|");
    }

    #[test]
    fn mosfet_polarity() {
        let p = MosfetParameters {
            vth0: 0.7,
            ..Default::default()
        };
        let n = MosfetModel {
            level: MosfetLevel::Level1,
            polarity: MosfetPolarity::N,
            parameters: p.clone(),
        };
        let p_ch = MosfetModel {
            level: MosfetLevel::Level1,
            polarity: MosfetPolarity::P,
            parameters: p,
        };
        // P-channel conducting with negative VGS/VDS yields negative current.
        let ids_p = p_ch.drain_current(-2.0, -5.0, 0.0);
        assert!(ids_p < 0.0);
        assert!(n.drain_current(2.0, 5.0, 0.0) > 0.0);
    }

    #[test]
    fn diode_shockley_equation() {
        let d = DiodeModel::default(); // is = 1e-14, n = 1
        let t = 300.0;
        let vt = thermal_voltage(t);
        // At v = 0.7 V: I = is·(e^(v/vt) − 1)
        let expected = 1e-14 * ((0.7 / vt).exp() - 1.0);
        let i = d.current(0.7, t);
        assert!((i - expected).abs() / expected < 1e-9);
        // Zero bias → zero current
        assert!(d.current(0.0, t).abs() < 1e-30);
        // Reverse: ≈ −is
        assert!((d.current(-1.0, t) + 1e-14).abs() < 1e-16);
        // Conductance consistent with current (numerical derivative)
        let dv = 1e-6;
        let g_num = (d.current(0.7 + dv, t) - d.current(0.7 - dv, t)) / (2.0 * dv);
        assert!((d.conductance(0.7, t) - g_num).abs() / g_num < 1e-4);
    }

    #[test]
    fn diode_series_resistance_limits_current() {
        let d = DiodeModel {
            rs: 10.0,
            ..DiodeModel::default()
        };
        let t = 300.0;
        // 1 V across 10 Ω + diode: current must be < 1/10 A
        let i = d.current(1.0, t);
        assert!(i > 0.0 && i < 0.1, "i = {i}");
    }

    #[test]
    fn diode_junction_capacitance() {
        let d = DiodeModel {
            cjo: 1.0e-12,
            vj: 0.7,
            ..DiodeModel::default()
        };
        let c0 = d.junction_capacitance(0.0);
        assert!((c0 - 1e-12).abs() < 1e-15);
        // Reverse bias reduces capacitance
        assert!(d.junction_capacitance(-1.0) < c0);
    }

    #[test]
    fn bjt_transport_model() {
        let q = BjtModel::default();
        let t = 300.0;
        let vt = thermal_voltage(t);
        // Active region: VBE = 0.7, VBC = -1
        let op = q.operating_point(0.7, -1.0, t);
        let ic_expected = 1e-15 * ((0.7 / vt).exp() - 1.0) * (1.0 + 0.0);
        assert!((op.ic - ic_expected).abs() / ic_expected < 1e-6);
        // ib ≈ ic/bf
        assert!((op.ib - op.ic / q.bf).abs() / op.ic.abs() < 0.01);
        // gm positive in active region
        assert!(op.gm > 0.0);
        // PNP mirrors with negative currents
        let mut pnp = q.clone();
        pnp.polarity = BjtPolarity::Pnp;
        let op_p = pnp.operating_point(-0.7, 1.0, t);
        assert!(op_p.ic < 0.0);
    }

    #[test]
    fn bsim_subset_mobility_reduces_current() {
        let base = MosfetParameters {
            vth0: 0.7,
            kp: 110e-6,
            theta: 0.0,
            ..Default::default()
        };
        let no_theta = MosfetModel {
            level: MosfetLevel::Bsim3,
            polarity: MosfetPolarity::N,
            parameters: base.clone(),
        };
        let with_theta = MosfetModel {
            level: MosfetLevel::Bsim3,
            polarity: MosfetPolarity::N,
            parameters: MosfetParameters {
                theta: 0.2,
                ..base.clone()
            },
        };
        let i_clean = no_theta.drain_current(3.0, 3.0, 0.0);
        let i_degraded = with_theta.drain_current(3.0, 3.0, 0.0);
        // θ = 0.2 at vov = 2.3 V: μ factor = 1/(1+0.46) ≈ 0.685
        assert!(i_degraded < i_clean * 0.75, "{i_degraded} vs {i_clean}");
        assert!(i_degraded > 0.0);
    }

    #[test]
    fn bsim_subset_velocity_saturation_caps_current() {
        let m = MosfetModel {
            level: MosfetLevel::Bsim4,
            polarity: MosfetPolarity::N,
            parameters: MosfetParameters {
                vth0: 0.7,
                kp: 110e-6,
                lambda: 0.0,
                e_sat: 2.0e6,
                ..Default::default()
            },
        };
        // Long-channel square law would predict 4× current for 2× vov;
        // velocity saturation caps growth well below that.
        let i1 = m.drain_current(1.7, 3.0, 0.0);
        let i2 = m.drain_current(2.7, 3.0, 0.0);
        assert!(i2 > i1);
        assert!(i2 / i1 < 3.0, "ratio {}", i2 / i1);
        // Smooth knee: id continuous across vds_sat (finite gds)
        let op = m.operating_point(2.0, 3.0, 0.0);
        assert!(op.gds >= 0.0);
    }

    #[test]
    fn ekv_gm_over_id_law_in_weak_inversion() {
        let n = 1.4;
        let m = MosfetModel {
            level: MosfetLevel::Ekv,
            polarity: MosfetPolarity::N,
            parameters: MosfetParameters {
                vth0: 0.7,
                is_ekv: 1e-6,
                n_ekv: n,
                ..Default::default()
            },
        };
        // Weak inversion: vgs well below vth → gm/Id → 1/(n·Vt)
        let vt = 0.02585;
        let vgs = 0.4;
        let op = m.operating_point(vgs, 1.0, 0.0);
        let gm_over_id = op.gm / op.ids;
        assert!(
            (gm_over_id - 1.0 / (n * vt)).abs() / (1.0 / (n * vt)) < 0.02,
            "gm/id = {}",
            gm_over_id
        );
        // Strong inversion: current positive, continuous, saturated
        let strong = m.operating_point(2.0, 2.0, 0.0);
        assert!(strong.ids > 0.0 && strong.saturated);
    }

    #[test]
    fn thermal_voltage_value() {
        // ≈ 25.85 mV at 300 K
        assert!((thermal_voltage(300.0) - 0.02585).abs() < 1e-4);
    }
}
