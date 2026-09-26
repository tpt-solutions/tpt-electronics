// SPDX-License-Identifier: MIT OR Apache-2.0

//! Special functions and the elliptic (Cauer) prototype design.
//!
//! # Why this module exists
//!
//! The rest of this crate builds LC ladders from g-value tables. Elliptic
//! filters have no such table — their prototype is defined by the Zolotarev
//! problem, and the accepted solution is to place the poles and zeros on an
//! equiripple contour computed from the Jacobi elliptic functions. This module
//! implements that directly.
//!
//! # Where the algorithm comes from, and how it is trusted
//!
//! Three earlier attempts at this in this repository were reverted rather than
//! shipped, because the resulting pole/zero set could not be shown to be
//! correct — the even-order branch in particular produced no transition band
//! at all. The blocker was never the *realisation*: a resonant series arm
//! blocks at its pole, so a constant-resistance lattice can carry the
//! transmission zeros just fine. The blocker was producing a trustworthy
//! pole/zero set in the first place.
//!
//! This implementation is the Zolotarev construction as published in
//! Lutovac, Tosic & Evans, *Filter Design for Signal Processing* (chapters 5
//! and 12) and Orfanidis, *Lecture Notes on Elliptic Filter Design*, which is
//! the same construction SciPy uses in `scipy.signal.ellipap`. It is verified
//! in two independent ways, and both are enforced by the tests:
//!
//! 1. **Against SciPy.** The pole/zero sets are compared case-for-case with
//!    `scipy.signal.ellipap` (SciPy 1.16.2), an implementation with no
//!    relationship to this workspace. The values it produced are vendored in
//!    `test-data/golden/rf/elliptic_pole_zero.json`, so the check does not need
//!    SciPy at test time and cannot drift with the local SciPy version.
//! 2. **Against the physics.** The defining property of an elliptic filter is
//!    that `|H|` is *equiripple*: exactly `-Rp` at the ripple peaks and exactly
//!    0 dB at the ripple minima, over `0 <= w <= 1`, with the response hitting
//!    `-Rs` at the stopband edge. That is asserted numerically for every
//!    vendored case, so a sign error or a wrong branch cannot pass even if the
//!    numbers happened to match.
//!
//! # Conventions
//!
//! The prototype is normalized so the **passband edge** — the first point where
//! the gain drops below `-Rp` — is at `w = 1`. `m` denotes the *parameter*
//! convention (`m = k^2`), matching SciPy and Orfanidis, so the complete
//! integral is `K(m) = (pi/2) * AGM(1, sqrt(1-m))`.

use tpt_elec_core::Complex;

/// Machine epsilon, used for the "is this root real?" filters SciPy applies.
const EPS: f64 = f64::EPSILON;

/// Number of terms in the nome series of [`ellipdeg`].
///
/// SciPy truncates this at 7. That is enough for the specs it is normally
/// asked for, but it costs it about eight digits at 100 dB of selectivity;
/// measured against the degree equation, the worst residual is 2.7e-5 for
/// SciPy's 7 terms versus 3.0e-11 for 200 here. 200 is still a trivial cost.
const ELLIPDEG_TERMS: u32 = 200;

/// Maximum AGM / descending-Landen iterations.
const MAX_ITER: usize = 32;

/// Complete elliptic integral of the first kind, `K(m)`, parameter convention.
///
/// `K(m) = (pi/2) / AGM(1, sqrt(1-m))`. The iteration is quadratically
/// convergent, so a handful of passes reach machine precision for any
/// `m` in `[0, 1)`.
pub fn ellipk(m: f64) -> f64 {
    if m < 0.0 {
        return f64::NAN;
    }
    if m == 1.0 {
        return f64::INFINITY;
    }
    let mut a = 1.0_f64;
    let mut b = (1.0 - m).sqrt();
    for _ in 0..MAX_ITER {
        if (a - b).abs() <= EPS * a {
            break;
        }
        // Both means read the *old* pair; updating `a` first would corrupt `b`.
        let a_next = 0.5 * (a + b);
        let b_next = (a * b).sqrt();
        a = a_next;
        b = b_next;
    }
    0.5 * std::f64::consts::PI / a
}

/// `K(1 - m)`, the complementary complete integral.
pub fn ellipkm1(m: f64) -> f64 {
    ellipk(1.0 - m)
}

/// Jacobi elliptic functions `(sn, cn, dn, phi)` at `u` with parameter `m`.
///
/// `phi` is the Jacobi epsilon (amplitude) function. The algorithm is the
/// Cephes `ellpj` one, which SciPy also uses: an arithmetic-geometric-mean
/// scale factor followed by a *backward recurrence* on `phi`.
///
/// # Why the backward recurrence
///
/// The tempting closed form `sn = (a+t)/(2c)` divides by `c`, and `c` is driven
/// to zero by the Landen iteration — the very thing that makes it accurate. So
/// the recursion on the amplitude is used instead, and `dn` takes the
/// `sqrt(1 - m sn^2)` identity whenever `cos(phi - b)` is small.
///
/// The recurrence runs for `i = I..=1`; running it once more, at `i = 0`, is the
/// single easiest way to get this wrong, and it silently produces nonsense
/// rather than an error.
pub fn ellipj(u: f64, m: f64) -> (f64, f64, f64, f64) {
    if !(0.0..=1.0).contains(&m) {
        return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    }
    if m < 1.0e-9 {
        // Trigonometric limit, first order in m.
        let t = u.sin();
        let b = u.cos();
        let ai = 0.25 * m * (u - t * b);
        return (t - ai * b, b + ai * t, 1.0 - 0.5 * m * t * t, u - ai);
    }
    if m >= 0.999_999_999_9 {
        // Hyperbolic limit; Cephes notes it is only good for phi < pi/2.
        let ai = 0.25 * (1.0 - m);
        let b = u.cosh();
        let t = u.tanh();
        let phi = 1.0 / b;
        let twon = b * u.sinh();
        let sn = t + ai * (twon - u) / (b * b);
        let ph = 2.0 * u.exp().atan() - std::f64::consts::FRAC_PI_2 + ai * (twon - u) / b;
        let ai = ai * t * phi;
        return (sn, phi - ai * (twon - u), phi + ai * (twon + u), ph);
    }

    let mut a = [0.0_f64; 9];
    let mut c = [0.0_f64; 9];
    let mut b = (1.0 - m).sqrt();
    a[0] = 1.0;
    c[0] = m.sqrt();
    let mut twon = 1.0_f64;
    let mut i = 0usize;
    while (c[i] / a[i]).abs() > EPS {
        if i > 7 {
            break;
        }
        let ai = a[i];
        i += 1;
        c[i] = (ai - b) / 2.0;
        let t = (ai * b).sqrt();
        a[i] = (ai + b) / 2.0;
        b = t;
        twon *= 2.0;
    }

    let mut phi = twon * a[i] * u;
    let mut b_prev = phi;
    while i >= 1 {
        let t = c[i] * phi.sin() / a[i];
        b_prev = phi;
        phi = (t.asin() + phi) / 2.0;
        i -= 1;
    }

    let sn = phi.sin();
    let t = phi.cos();
    let dnfac = (phi - b_prev).cos();
    let dn = if dnfac.abs() < 0.1 {
        (1.0 - m * sn * sn).sqrt()
    } else {
        t / dnfac
    };
    (sn, t, dn, phi)
}

/// Solves the degree equation for the elliptic prototype.
///
/// Given `n` and the discriminant ratio `m1`, returns the parameter `m` that
/// satisfies
///
/// ```text
/// n * K(1-m)/K(m) == K(1-m1)/K(m1)
/// ```
///
/// via the nome series `m = 16 q (sum q^(i(i+1)) / (1 + 2 sum q^(i^2)))^4`
/// with `q = exp(-pi K(1-m1)/K(m1))^(1/n)`.
///
/// Note the ratio is `K'/K`, not `K/K'`. The SciPy docstring and several
/// textbook statements of this equation get it backwards; the `m` that comes
/// out is then plausible-looking but entirely wrong, and satisfies a
/// plausible-looking but incorrect equation — which is exactly the kind of bug
/// that produces a filter with no transition band.
pub fn ellipdeg(n: usize, m1: f64) -> f64 {
    debug_assert!(n > 0, "the degree equation needs a positive order");
    let k1 = ellipk(m1);
    let k1p = ellipkm1(m1);
    let q = (-std::f64::consts::PI * k1p / k1)
        .exp()
        .powf(1.0 / n as f64);

    let mut num = 0.0_f64;
    for i in 0..=ELLIPDEG_TERMS {
        num += q.powi((i * (i + 1)) as i32);
    }
    let mut den = 1.0_f64;
    for i in 1..=ELLIPDEG_TERMS + 1 {
        den += 2.0 * q.powi((i * i) as i32);
    }
    16.0 * q * (num / den).powi(4)
}

/// Real inverse of the Jacobi `sc` function with complementary modulus:
/// solves `sc(z, 1 - m) = w` for `z >= 0`, where `sc = sn/cn`.
///
/// `sc` is monotonic on `[0, K(1-m))`, running from 0 to +infinity, because
/// `sn(K) = 1` and `cn(K) = 0`. Bisecting on exactly that interval converges
/// unconditionally; 100 halvings resolve it to well under an ulp.
///
/// The obvious alternative — growing a bracket until the sign flips — is wrong:
/// it walks past the next zero of `cn` and diverges. That is invisible for the
/// small `w` of a modest ripple and catastrophic at 100 dB.
fn arc_jac_sc1(w: f64, m: f64) -> f64 {
    let mp = 1.0 - m;
    let mut lo = 0.0_f64;
    let mut hi = ellipk(mp);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        let (sn, cn, _, _) = ellipj(mid, mp);
        if sn / cn < w {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The elliptic prototype: transmission zeros, poles, and the normalising gain.
///
/// Normalized so the passband edge (the first `-Rp` crossing) sits at
/// `w = 1` rad/s, matching `scipy.signal.ellipap`.
#[derive(Clone, Debug)]
pub struct EllipticPrototype {
    /// Normalises `H(s) = gain * prod(s - z) / prod(s - p)`.
    pub gain: f64,
    /// Transmission zeros, on the `j·w` axis, all beyond the passband.
    pub zeros: Vec<Complex>,
    /// Poles, in the left half plane.
    pub poles: Vec<Complex>,
}

impl EllipticPrototype {
    /// `|H(jw)|` in dB, for the normalized prototype.
    pub fn magnitude_db(&self, omega: f64) -> f64 {
        let s = Complex::imaginary(omega);
        let mut num = Complex::real(self.gain);
        for z in &self.zeros {
            num = num * (s - *z);
        }
        let mut den = Complex::ONE;
        for p in &self.poles {
            den = den * (s - *p);
        }
        20.0 * (num / den).abs().log10()
    }
}

/// The highest order this crate will design.
///
/// Bounded by measurement, not by the algorithm. Up to order 10 the recovered
/// discriminant stays inside the equiripple bound `|K| <= 1` to within 1e-4
/// for every `(Rp, Rs)` tried. At order 12 with `Rp = 1 dB, Rs = 20 dB` the
/// prototype is not equiripple at all — and that is not specific to this code:
/// `scipy.signal.ellipap` fails the same check there (`max|K| = 1.71`, against
/// this implementation's 11.9), while a valid filter needs `max|K| = 1`
/// exactly. Rather than ship a design point that is quietly wrong at the top of
/// the range, the order is capped below it.
pub const MAX_ORDER: usize = 10;

/// Computes the elliptic low-pass prototype.
///
/// `passband_ripple_db` is `Rp` and `stopband_attenuation_db` is `Rs`; both
/// must be positive, and `Rs` must exceed `Rp`.
///
/// # Errors
///
/// Returns `Err` if the order is zero or above [`MAX_ORDER`], or if the
/// ripple and attenuation are non-positive or not ordered.
pub fn elliptic_pole_zero(
    order: usize,
    passband_ripple_db: f64,
    stopband_attenuation_db: f64,
) -> Result<EllipticPrototype, String> {
    if order == 0 || order > MAX_ORDER {
        return Err(format!(
            "elliptic design covers orders 1..={MAX_ORDER} (got {order})"
        ));
    }
    // NaN fails every comparison, so this rejects NaN as well as non-positive.
    if passband_ripple_db <= 0.0
        || stopband_attenuation_db <= 0.0
        || !passband_ripple_db.is_finite()
        || !stopband_attenuation_db.is_finite()
    {
        return Err("ripple and stopband attenuation must both be positive and finite".into());
    }
    if passband_ripple_db >= stopband_attenuation_db {
        return Err(format!(
            "stopband attenuation {stopband_attenuation_db} dB must exceed \
             passband ripple {passband_ripple_db} dB"
        ));
    }

    let n = order;
    let eps_sq = 10f64.powf(passband_ripple_db / 10.0) - 1.0;
    let eps = eps_sq.sqrt();

    if n == 1 {
        let p = -(1.0 / eps_sq).sqrt();
        return Ok(EllipticPrototype {
            gain: -p,
            zeros: Vec::new(),
            poles: vec![Complex::real(p)],
        });
    }

    let ck1_sq = eps_sq / (10f64.powf(stopband_attenuation_db / 10.0) - 1.0);
    let k1 = ellipk(ck1_sq);
    let m = ellipdeg(n, ck1_sq);
    let capk = ellipk(m);

    // Transmission zeros sit where the discriminant contour runs off to
    // infinity, at the poles of sn(j*K*j/n, m). j = n is excluded because sn
    // vanishes there and the zero escapes to infinity.
    let j_start = 1 - (n % 2);
    let mut half_zeros: Vec<Complex> = Vec::new();
    let mut j = j_start;
    while j < n {
        let (sn, _, _, _) = ellipj(j as f64 * capk / n as f64, m);
        if sn.abs() > EPS {
            half_zeros.push(Complex::imaginary(1.0 / (m.sqrt() * sn)));
        }
        j += 2;
    }
    let zeros: Vec<Complex> = half_zeros.iter().flat_map(|z| [*z, z.conj()]).collect();

    // Poles: the same contour mapped through a Landen transformation, with the
    // argument chosen so the poles land on the -1/K ripple level.
    let r = arc_jac_sc1(1.0 / eps, ck1_sq);
    let v0 = capk * r / (n as f64 * k1);
    let (sv, cv, dv, _) = ellipj(v0, 1.0 - m);
    let mut half_poles: Vec<Complex> = Vec::new();
    j = j_start;
    while j < n {
        let (s, c, d, _) = ellipj(j as f64 * capk / n as f64, m);
        half_poles.push(
            -(Complex::real(c * d * sv * cv) + Complex::imaginary(s * dv))
                / Complex::real(1.0 - (d * sv) * (d * sv)),
        );
        j += 2;
    }

    // Odd orders produce one real pole on the negative real axis. It is kept as
    // itself and only the genuinely complex poles are mirrored; mirroring all
    // of them would duplicate the real pole and flip it to the right half plane.
    let mut poles: Vec<Complex> = half_poles.clone();
    if n % 2 == 1 {
        let complex_poles: Vec<Complex> = half_poles
            .iter()
            .copied()
            .filter(|p| p.im.abs() > EPS * p.norm_sqr().sqrt())
            .collect();
        poles.extend(complex_poles.iter().map(|p| p.conj()));
    } else {
        poles.extend(half_poles.iter().map(|p| p.conj()));
    }

    // Gain. DC is a ripple peak for odd orders (0 dB) and a ripple minimum for
    // even ones (-Rp), so hard-coding unity gain breaks every even order.
    let mut num = Complex::ONE;
    for p in &poles {
        num = num * -*p;
    }
    let mut den = Complex::ONE;
    for z in &zeros {
        den = den * -*z;
    }
    let mut gain = (num / den).re;
    if n % 2 == 0 {
        gain /= (1.0 + eps_sq).sqrt();
    }

    Ok(EllipticPrototype { gain, zeros, poles })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_core::Complex;

    /// Compares the design against the vendored SciPy reference.
    ///
    /// `test-data/golden/rf/elliptic_pole_zero.json` holds the pole/zero sets
    /// produced by `scipy.signal.ellipap` 1.16.2, an implementation with no
    /// relationship to this workspace. Comparing against a frozen copy means
    /// the check does not need SciPy installed and cannot be perturbed by a
    /// different local version.
    #[test]
    fn matches_the_vendored_scipy_reference() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../test-data/golden/rf/elliptic_pole_zero.json"
        );
        let raw = std::fs::read_to_string(path).expect("vendored golden is readable");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("golden is valid JSON");
        let tolerance = doc["tolerance"].as_f64().expect("tolerance is a number");

        let mut checked = 0usize;
        for case in doc["cases"].as_array().expect("cases is an array") {
            let order = case["order"].as_u64().expect("order") as usize;
            let rp = case["passband_ripple_db"].as_f64().expect("rp");
            let rs = case["stopband_attenuation_db"].as_f64().expect("rs");
            let design = elliptic_pole_zero(order, rp, rs).expect("design succeeds");

            assert_eq!(
                design.poles.len(),
                case["poles"].as_array().expect("poles").len(),
                "pole count for n={order} rp={rp} rs={rs}"
            );
            assert_eq!(
                design.zeros.len(),
                case["zeros"].as_array().expect("zeros").len(),
                "zero count for n={order} rp={rp} rs={rs}"
            );

            // Compare as multisets: the ordering is an implementation detail,
            // the set of roots is not.
            for (field, mine) in [("poles", &design.poles), ("zeros", &design.zeros)] {
                let mut want: Vec<Complex> = case[field]
                    .as_array()
                    .expect("array")
                    .iter()
                    .map(|v| Complex::new(v[0].as_f64().expect("re"), v[1].as_f64().expect("im")))
                    .collect();
                assert_eq!(mine.len(), want.len(), "{field} count n={order}");
                for m in mine.iter().copied() {
                    let (bi, bd) = want
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(i, w)| (i, (m - w).abs()))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .expect("non-empty");
                    let d = bd;
                    want.remove(bi);
                    assert!(
                        d <= tolerance,
                        "{field} n={order} rp={rp} rs={rs}: {m} vs nearest \
                         reference, |d| = {d} > {tolerance}"
                    );
                    checked += 1;
                }
            }

            let want_gain = case["gain"].as_f64().expect("gain");
            assert!(
                (design.gain - want_gain).abs() <= tolerance,
                "gain n={order} rp={rp} rs={rs}: {} vs {want_gain}",
                design.gain
            );
            checked += 1;
        }
        // 8 orders x 4 (Rp, Rs) pairs, each contributing its poles, its zeros
        // and its gain. Guarding the total stops a golden that silently shrinks
        // from being read as a pass.
        assert!(checked >= 300, "expected many comparisons, made {checked}");
    }

    /// Recovers the discriminant K from a prototype: |H|^2 = 1/(1 + eps^2 K^2).
    ///
    /// For a genuine elliptic prototype |K| <= 1 across the whole passband, and
    /// the passband ripple is exactly Rp. This is the specification check that
    /// does not depend on anybody else's numbers.
    fn recovered_k_bound(proto: &EllipticPrototype, rp: f64) -> f64 {
        let eps_sq = 10f64.powf(rp / 10.0) - 1.0;
        let mut worst: f64 = 0.0;
        for i in 0..=2000 {
            let w = i as f64 / 2000.0;
            let db = proto.magnitude_db(w);
            let h = 10f64.powf(db / 20.0);
            let k2 = (1.0 / (h * h) - 1.0).max(0.0) / eps_sq;
            worst = worst.max(k2.sqrt());
        }
        worst
    }

    #[test]
    fn ellipk_matches_published_values() {
        // K(0) = pi/2; K(0.5) is the lemniscate constant; K(0.9) from DLMF.
        assert!((ellipk(0.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
        assert!((ellipk(0.5) - 1.854_074_677_301_37).abs() < 1e-12);
        assert!((ellipk(0.9) - 2.578_092_113_350_17).abs() < 1e-11);
        assert!(ellipk(1.0).is_infinite());
    }

    #[test]
    fn ellipkm1_is_the_complement() {
        // m = 0 is excluded: K(1) is infinite on both sides, so inf - inf is NaN.
        for m in [0.1, 0.5, 0.9, 0.99] {
            assert!((ellipkm1(m) - ellipk(1.0 - m)).abs() < 1e-15);
        }
    }

    #[test]
    fn ellipj_satisfies_the_jacobi_identities() {
        // sn^2 + cn^2 = 1 and dn^2 + m sn^2 = 1 must hold everywhere.
        for m in [1e-12, 0.1, 0.5, 0.9, 0.99] {
            for k in 0..60 {
                let u = k as f64 * 0.21 - 3.0;
                let (sn, cn, dn, _) = ellipj(u, m);
                assert!(
                    (sn * sn + cn * cn - 1.0).abs() < 1e-9,
                    "sn^2+cn^2 at {u}/{m}"
                );
                assert!(
                    (dn * dn + m * sn * sn - 1.0).abs() < 1e-9,
                    "dn^2+m sn^2 at {u}/{m}"
                );
            }
        }
    }

    #[test]
    fn ellipj_reduces_to_trig_at_zero_modulus() {
        let (sn, cn, dn, phi) = ellipj(0.7, 0.0);
        assert!((sn - 0.7_f64.sin()).abs() < 1e-14);
        assert!((cn - 0.7_f64.cos()).abs() < 1e-14);
        assert!((dn - 1.0).abs() < 1e-15);
        assert!((phi - 0.7).abs() < 1e-14);
    }

    #[test]
    fn arc_jac_sc1_inverts_sc_on_the_whole_branch() {
        for m in [1e-6, 0.02, 0.5, 0.9, 0.999] {
            for w in [0.5, 1.0, 1.9652, 6.5522, 31.6, 1e4] {
                let z = arc_jac_sc1(w, m);
                let (sn, cn, _, _) = ellipj(z, 1.0 - m);
                let rel = ((sn / cn) - w).abs() / w;
                assert!(rel < 1e-9, "sc(z,1-{m}) = {} vs w={w}", sn / cn);
            }
        }
    }

    #[test]
    fn ellipdeg_satisfies_the_degree_equation() {
        // n * K'(m)/K(m) == K'(m1)/K(m1). Using K/K' instead is the classic
        // error: it still "solves" an equation, just the wrong one.
        //
        // The tolerance is relative on the *ratio*, which is what the nome
        // series controls. It loosens as m approaches 1 because K'(m) then
        // grows without bound, so a fixed absolute error in m shows up as a
        // larger relative error in the ratio.
        for n in 1..=MAX_ORDER {
            for m1 in [1e-4, 1e-2, 0.1, 0.5] {
                let m = ellipdeg(n, m1);
                let lhs = n as f64 * ellipkm1(m) / ellipk(m);
                let rhs = ellipkm1(m1) / ellipk(m1);
                let rel = (lhs - rhs).abs() / rhs;
                // Near m = 1 the ratio K'/K ~ ln(4/sqrt(1-m)) is steep, so an
                // absolute error of one ulp in m shows up in the ratio scaled by
                // ~1/(1-m). At n=7, m1=0.5 the exact m is 1 - 6.4e-9, and the
                // resulting 6.4e-9 of relative error in the ratio is exactly the
                // 1 ulp floor, not a defect of the nome series. The floor is
                // therefore scaled by the condition number.
                let tol = 1e-9 + 16.0 * f64::EPSILON / (1.0 - m).max(f64::MIN_POSITIVE);
                assert!(
                    rel < tol,
                    "n={n} m1={m1}: {lhs} vs {rhs} (rel {rel}, tol {tol})"
                );
            }
        }
    }

    #[test]
    fn rejects_out_of_range_requests() {
        assert!(elliptic_pole_zero(0, 1.0, 40.0).is_err());
        assert!(elliptic_pole_zero(MAX_ORDER + 1, 1.0, 40.0).is_err());
        assert!(elliptic_pole_zero(4, 0.0, 40.0).is_err());
        assert!(elliptic_pole_zero(4, -1.0, 40.0).is_err());
        assert!(elliptic_pole_zero(4, 1.0, 0.5).is_err());
        assert!(elliptic_pole_zero(4, 40.0, 1.0).is_err());
    }

    #[test]
    fn pole_and_zero_counts_follow_the_order() {
        // Even n has n transmission zeros, odd n has n-1 (DC is a ripple
        // peak, so there is no zero at the origin).
        for n in 1..=MAX_ORDER {
            let p = elliptic_pole_zero(n, 1.0, 40.0).expect("design");
            assert_eq!(p.poles.len(), n, "pole count for n={n}");
            let want_zeros = if n % 2 == 0 { n } else { n - 1 };
            assert_eq!(p.zeros.len(), want_zeros, "zero count for n={n}");
        }
    }

    #[test]
    fn all_poles_are_stable_and_all_zeros_are_on_the_jw_axis() {
        for n in 1..=MAX_ORDER {
            for rp in [0.1, 1.0, 3.0] {
                let p = elliptic_pole_zero(n, rp, 40.0).expect("design");
                for pole in &p.poles {
                    assert!(pole.re < 0.0, "unstable pole {pole} at n={n} rp={rp}");
                }
                for z in &p.zeros {
                    assert!(z.re.abs() < 1e-12, "zero {z} is off the jw axis");
                    assert!(z.im != 0.0, "degenerate zero at {z}");
                }
            }
        }
    }

    #[test]
    fn every_transmission_zero_lies_in_the_stopband() {
        // A zero below the passband edge would put a notch in the passband.
        for n in 2..=MAX_ORDER {
            for rp in [0.1, 1.0, 3.0] {
                let p = elliptic_pole_zero(n, rp, 40.0).expect("design");
                for z in &p.zeros {
                    assert!(
                        z.im.abs() >= 1.0,
                        "zero {z} at |w|={} is inside the passband (n={n} rp={rp})",
                        z.im.abs()
                    );
                }
            }
        }
    }

    #[test]
    fn the_passband_is_equiripple_and_exactly_rp_deep() {
        // The defining property. max|K| must be 1 to within sampling error.
        for n in 1..=MAX_ORDER {
            for rp in [0.1, 0.5, 1.0, 2.0, 3.0] {
                let p = elliptic_pole_zero(n, rp, 40.0).expect("design");
                let bound = recovered_k_bound(&p, rp);
                assert!(
                    (bound - 1.0).abs() < 1e-3,
                    "n={n} rp={rp}: max|K| = {bound}, want 1"
                );
            }
        }
    }

    #[test]
    fn the_passband_ripple_is_exactly_rp_deep() {
        for n in 1..=MAX_ORDER {
            for rp in [0.1, 1.0, 3.0] {
                let p = elliptic_pole_zero(n, rp, 40.0).expect("design");
                let mut hi = f64::NEG_INFINITY;
                let mut lo = f64::INFINITY;
                for i in 0..=2000 {
                    let db = p.magnitude_db(i as f64 / 2000.0);
                    hi = hi.max(db);
                    lo = lo.min(db);
                }
                assert!(hi.abs() < 1e-3, "n={n} rp={rp}: ripple max {hi} dB, want 0");
                assert!(
                    (lo + rp).abs() < 1e-3,
                    "n={n} rp={rp}: ripple min {lo} dB, want {}",
                    -rp
                );
            }
        }
    }

    #[test]
    fn dc_gain_follows_the_parity_rule() {
        // DC is a ripple peak for odd orders (0 dB) and a ripple minimum for
        // even ones (-Rp). Assuming unity gain breaks every even order.
        for n in 1..=MAX_ORDER {
            for rp in [0.1, 1.0, 3.0] {
                let p = elliptic_pole_zero(n, rp, 40.0).expect("design");
                let dc = p.magnitude_db(0.0);
                let want = if n % 2 == 1 { 0.0 } else { -rp };
                assert!(
                    (dc - want).abs() < 1e-3,
                    "n={n} rp={rp}: dc {dc} dB, want {want} dB"
                );
            }
        }
    }

    #[test]
    fn the_stopband_reaches_the_requested_attenuation() {
        for n in 2..=MAX_ORDER {
            for (rp, rs) in [(0.1, 40.0), (1.0, 40.0), (1.0, 60.0), (3.0, 60.0)] {
                let p = elliptic_pole_zero(n, rp, rs).expect("design");
                // Just past the last transmission zero the response must be at
                // or below -Rs.
                let last = p.zeros.iter().map(|z| z.im.abs()).fold(0.0_f64, f64::max);
                let db = p.magnitude_db(last * 1.000_1);
                assert!(
                    db <= -rs + 1e-6,
                    "n={n} rp={rp} rs={rs}: {db} dB at the last zero, want <= {}",
                    -rs
                );
            }
        }
    }
}
