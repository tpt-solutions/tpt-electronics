// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core domain types and numeric kernels for `tpt-electronics`.
//!
//! This crate is the foundation of the electronics simulation engine. It is
//! intentionally dependency-free: the linear algebra, complex arithmetic, and
//! unit types every other crate relies on are implemented here in pure,
//! `unsafe`-free Rust.
//!
//! # Contents
//!
//! * [`ids`] — strongly-typed identifiers (`BoardId`, `NetId`, …)
//! * [`units`] — SI lengths, angles, points, vectors, bounding boxes
//! * [`stackup`] — PCB layer stackup model
//! * [`component`] — component placement and thermal/electrical models
//! * [`linalg`] — dense and sparse (CSR) matrices, Conjugate Gradient solver
//! * [`complex`] — complex arithmetic for AC / S-parameter work
//!
//! # Units
//!
//! All lengths are stored internally in **meters** as `f64` ([`Length`]),
//! temperatures in **degrees Celsius** unless a formula explicitly requires
//! Kelvin (documented at the call site), and angles in **radians**.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod complex;
pub mod component;
pub mod ids;
pub mod linalg;
pub mod stackup;
pub mod units;

pub use complex::Complex;
pub use component::{
    BoardSide, Component, ElectricalModel, PackageType, ThermalComponentModel, ThermalPad, ViaArray,
};
pub use ids::{BoardId, ComponentId, LayerId, MaterialId, NetId, PadId, TraceId, ViaId};
pub use linalg::{DenseMatrix, SparseBuilder, SparseMatrix};
pub use stackup::{CopperWeight, Layer, LayerType, Stackup};
pub use units::{Angle, BoundingBox3, Length, Point2, Point3, Vector3};

/// Speed of light in vacuum [m/s].
pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;
