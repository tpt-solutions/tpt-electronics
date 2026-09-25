// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal complex-arithmetic type used by AC analysis, S-parameters, and RF.

use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A complex number `re + im·i`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl Complex {
    /// The additive identity.
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };
    /// The multiplicative identity.
    pub const ONE: Complex = Complex { re: 1.0, im: 0.0 };

    /// Creates a complex number.
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// Creates a purely real number.
    pub const fn real(re: f64) -> Self {
        Self { re, im: 0.0 }
    }

    /// Creates a purely imaginary number.
    pub const fn imaginary(im: f64) -> Self {
        Self { re: 0.0, im }
    }

    /// Creates from polar coordinates (magnitude, phase).
    pub fn from_polar(mag: f64, phase: f64) -> Self {
        Self {
            re: mag * phase.cos(),
            im: mag * phase.sin(),
        }
    }

    /// Real part.
    pub const fn re(&self) -> f64 {
        self.re
    }

    /// Imaginary part.
    pub const fn im(&self) -> f64 {
        self.im
    }

    /// Conjugate.
    pub fn conj(&self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// Squared magnitude.
    pub fn norm_sqr(&self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// Magnitude.
    pub fn abs(&self) -> f64 {
        self.norm_sqr().sqrt()
    }

    /// Phase angle in radians.
    pub fn arg(&self) -> f64 {
        self.im.atan2(self.re)
    }

    /// Multiplicative inverse.
    pub fn inv(&self) -> Self {
        let d = self.norm_sqr();
        Self {
            re: self.re / d,
            im: -self.im / d,
        }
    }

    /// Complex exponential `e^z`.
    pub fn exp(&self) -> Self {
        let m = self.re.exp();
        Self {
            re: m * self.im.cos(),
            im: m * self.im.sin(),
        }
    }

    /// Principal square root.
    pub fn sqrt(&self) -> Self {
        if self.norm_sqr() == 0.0 {
            return Self::ZERO;
        }
        let m = self.abs().sqrt();
        let a = self.arg() / 2.0;
        Self {
            re: m * a.cos(),
            im: m * a.sin(),
        }
    }

    /// Raises to an integer power.
    pub fn powi(&self, n: i32) -> Self {
        let mut result = Self::ONE;
        let mut base = *self;
        let mut k = n.unsigned_abs();
        while k > 0 {
            if k & 1 == 1 {
                result = result * base;
            }
            base = base * base;
            k >>= 1;
        }
        if n < 0 {
            result.inv()
        } else {
            result
        }
    }

    /// Scales both components by a real factor.
    pub fn scaled(&self, s: f64) -> Self {
        Self {
            re: self.re * s,
            im: self.im * s,
        }
    }
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, rhs: Complex) -> Complex {
        Complex::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl Sub for Complex {
    type Output = Complex;
    fn sub(self, rhs: Complex) -> Complex {
        Complex::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, rhs: Complex) -> Complex {
        Complex::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl Div for Complex {
    type Output = Complex;
    // (a+bi)/(c+di) is expressed via the inverse — clippy's
    // suspicious_arithmetic_impl lint is a false positive here.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, rhs: Complex) -> Complex {
        self * rhs.inv()
    }
}

impl Neg for Complex {
    type Output = Complex;
    fn neg(self) -> Complex {
        Complex::new(-self.re, -self.im)
    }
}

impl AddAssign for Complex {
    fn add_assign(&mut self, rhs: Complex) {
        *self = *self + rhs;
    }
}

impl SubAssign for Complex {
    fn sub_assign(&mut self, rhs: Complex) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Complex {
    type Output = Complex;
    fn mul(self, s: f64) -> Complex {
        self.scaled(s)
    }
}

impl fmt::Display for Complex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.im < 0.0 {
            write!(f, "{}-{}j", self.re, -self.im)
        } else {
            write!(f, "{}+{}j", self.re, self.im)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    #[test]
    fn arithmetic_basics() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, -1.0);
        assert!((a + b - Complex::new(4.0, 1.0)).abs() < EPS);
        assert!((a - b - Complex::new(-2.0, 3.0)).abs() < EPS);
        let m = a * b; // (1+2j)(3-j) = 5 + 5j
        assert!((m - Complex::new(5.0, 5.0)).abs() < EPS);
        let d = a / a;
        assert!((d - Complex::ONE).abs() < EPS);
    }

    #[test]
    fn polar_abs_arg() {
        let z = Complex::from_polar(2.0, std::f64::consts::FRAC_PI_3);
        assert!((z.abs() - 2.0).abs() < EPS);
        assert!((z.arg() - std::f64::consts::FRAC_PI_3).abs() < EPS);
    }

    #[test]
    fn exp_and_sqrt() {
        let z = Complex::new(0.0, std::f64::consts::PI);
        assert!((z.exp() - Complex::new(-1.0, 0.0)).abs() < 1e-12);
        let s = Complex::new(-4.0, 0.0).sqrt();
        assert!((s - Complex::new(0.0, 2.0)).abs() < 1e-12);
    }

    #[test]
    fn powi_and_inverse() {
        let z = Complex::new(1.0, 1.0);
        let z2 = z * z;
        assert!((z.powi(2) - z2).abs() < EPS);
        assert!((z.powi(-1) - z.inv()).abs() < EPS);
    }

    #[test]
    fn display_negative_imaginary() {
        assert_eq!(Complex::new(1.0, -2.0).to_string(), "1-2j");
        assert_eq!(Complex::new(1.0, 2.0).to_string(), "1+2j");
    }
}
