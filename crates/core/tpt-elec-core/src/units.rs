// SPDX-License-Identifier: MIT OR Apache-2.0

//! SI units: lengths, angles, points, vectors, and bounding boxes.
//!
//! Lengths are newtyped meters so that unit mistakes stop at compile time;
//! positions are plain `f64` meters for cheap parser/geometry interop.

use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// A length stored in meters.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Length(f64);

impl Length {
    /// Zero length.
    pub const ZERO: Length = Length(0.0);

    /// Creates a length from meters.
    pub const fn meters(m: f64) -> Self {
        Self(m)
    }

    /// Creates a length from millimeters.
    pub fn mm(mm: f64) -> Self {
        Self(mm * 1e-3)
    }

    /// Creates a length from micrometers.
    pub fn um(um: f64) -> Self {
        Self(um * 1e-6)
    }

    /// Creates a length from nanometers.
    pub fn nm(nm: f64) -> Self {
        Self(nm * 1e-9)
    }

    /// Creates a length from mils (1/1000 inch).
    pub fn mil(mil: f64) -> Self {
        Self(mil * 25.4e-6)
    }

    /// Returns the length in meters.
    pub const fn as_meters(&self) -> f64 {
        self.0
    }

    /// Returns the length in millimeters.
    pub fn as_mm(&self) -> f64 {
        self.0 * 1e3
    }

    /// Returns the length in micrometers.
    pub fn as_um(&self) -> f64 {
        self.0 * 1e6
    }

    /// Returns the length in mils.
    pub fn as_mils(&self) -> f64 {
        self.0 / 25.4e-6
    }

    /// Returns the absolute value.
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    /// Multiplies by a scalar.
    pub fn scaled(&self, factor: f64) -> Self {
        Self(self.0 * factor)
    }
}

impl fmt::Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6} m", self.0)
    }
}

impl Add for Length {
    type Output = Length;
    fn add(self, rhs: Length) -> Length {
        Length(self.0 + rhs.0)
    }
}

impl Sub for Length {
    type Output = Length;
    fn sub(self, rhs: Length) -> Length {
        Length(self.0 - rhs.0)
    }
}

impl Mul<f64> for Length {
    type Output = Length;
    fn mul(self, rhs: f64) -> Length {
        Length(self.0 * rhs)
    }
}

impl Div<f64> for Length {
    type Output = Length;
    fn div(self, rhs: f64) -> Length {
        Length(self.0 / rhs)
    }
}

impl std::iter::Sum for Length {
    fn sum<I: Iterator<Item = Length>>(iter: I) -> Length {
        iter.fold(Length::ZERO, |acc, l| acc + l)
    }
}

impl<'a> std::iter::Sum<&'a Length> for Length {
    fn sum<I: Iterator<Item = &'a Length>>(iter: I) -> Length {
        iter.fold(Length::ZERO, |acc, l| acc + *l)
    }
}

/// An angle stored in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Angle(f64);

impl Angle {
    /// Zero angle.
    pub const ZERO: Angle = Angle(0.0);

    /// Creates an angle from radians.
    pub const fn radians(rad: f64) -> Self {
        Self(rad)
    }

    /// Creates an angle from degrees.
    pub fn degrees(deg: f64) -> Self {
        Self(deg.to_radians())
    }

    /// Returns the angle in radians.
    pub const fn as_radians(&self) -> f64 {
        self.0
    }

    /// Returns the angle in degrees.
    pub fn as_degrees(&self) -> f64 {
        self.0.to_degrees()
    }

    /// Returns the cosine.
    pub fn cos(&self) -> f64 {
        self.0.cos()
    }

    /// Returns the sine.
    pub fn sin(&self) -> f64 {
        self.0.sin()
    }
}

/// A 2D point in meters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2 {
    /// X coordinate [m].
    pub x: f64,
    /// Y coordinate [m].
    pub y: f64,
}

impl Point2 {
    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to another point.
    pub fn distance_to(&self, other: &Point2) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl Add for Point2 {
    type Output = Point2;
    fn add(self, rhs: Point2) -> Point2 {
        Point2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point2 {
    type Output = Point2;
    fn sub(self, rhs: Point2) -> Point2 {
        Point2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// A 3D point in meters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point3 {
    /// X [m].
    pub x: f64,
    /// Y [m].
    pub y: f64,
    /// Z [m].
    pub z: f64,
}

impl Point3 {
    /// Creates a point.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// A 3D vector (e.g. heat flux, in SI units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

impl Vector3 {
    /// Creates a vector.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Zero vector.
    pub const ZERO: Vector3 = Vector3::new(0.0, 0.0, 0.0);

    /// Magnitude.
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Scales the vector.
    pub fn scaled(&self, s: f64) -> Vector3 {
        Vector3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// An axis-aligned 3D bounding box in meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox3 {
    /// Minimum corner.
    pub min: Point3,
    /// Maximum corner.
    pub max: Point3,
}

impl BoundingBox3 {
    /// Creates a bounding box from corners.
    pub const fn new(min: Point3, max: Point3) -> Self {
        Self { min, max }
    }

    /// Extent along each axis.
    pub fn extents(&self) -> Vector3 {
        Vector3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// Total volume [m³].
    pub fn volume(&self) -> f64 {
        let e = self.extents();
        (e.x * e.y * e.z).max(0.0)
    }

    /// Whether a point lies inside (inclusive).
    pub fn contains(&self, p: Point3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_conversions_round_trip() {
        let l = Length::mm(1.6);
        assert!((l.as_meters() - 1.6e-3).abs() < 1e-15);
        assert!((Length::um(35.0).as_mm() - 0.035).abs() < 1e-9);
        assert!((Length::mil(39.37).as_mm() - 0.9999).abs() < 1e-3);
    }

    #[test]
    fn length_arithmetic() {
        let a = Length::mm(1.0) + Length::mm(2.0);
        assert!((a.as_mm() - 3.0).abs() < 1e-12);
        let b = a / 3.0;
        assert!((b.as_mm() - 1.0).abs() < 1e-12);
        assert!((Length::mm(1.0).scaled(2.5).as_mm() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn angle_degrees_radians() {
        let a = Angle::degrees(180.0);
        assert!((a.as_radians() - std::f64::consts::PI).abs() < 1e-12);
        assert!((a.as_degrees() - 180.0).abs() < 1e-9);
        assert!((Angle::degrees(30.0).sin() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn point_distance_and_bbox() {
        let p = Point2::new(0.0, 0.0);
        let q = Point2::new(3.0, 4.0);
        assert!((p.distance_to(&q) - 5.0).abs() < 1e-12);

        let bb = BoundingBox3::new(Point3::new(0., 0., 0.), Point3::new(1., 2., 3.));
        assert!((bb.volume() - 6.0).abs() < 1e-12);
        assert!(bb.contains(Point3::new(0.5, 1.0, 1.5)));
        assert!(!bb.contains(Point3::new(2.0, 1.0, 1.5)));
    }

    #[test]
    fn vector_magnitude() {
        let v = Vector3::new(1.0, 2.0, 2.0);
        assert!((v.magnitude() - 3.0).abs() < 1e-12);
    }
}
