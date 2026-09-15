// SPDX-License-Identifier: MIT OR Apache-2.0

//! PCB geometry primitives and voxel meshes.
//!
//! This crate models copper geometry ([`Trace`], [`Pad`], [`CopperArea`]),
//! drills, and — critically for the FEM stack — voxel/hexahedral meshes
//! ([`VoxelGrid`]). Meshing is deliberately voxel-only: hexahedral elements
//! avoid any dependency on GPL tetrahedral meshing libraries (GMSH, Netgen).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use tpt_elec_core::{BoundingBox3, Length, MaterialId, Point2, Point3, TraceId};

/// A routed copper trace.
#[derive(Clone, Debug)]
pub struct Trace {
    /// Trace identifier.
    pub id: TraceId,
    /// Net name.
    pub net: String,
    /// Layer index this trace lives on.
    pub layer: u32,
    /// Trace width [m].
    pub width: Length,
    /// Polyline vertices (meters).
    pub points: Vec<Point2>,
    /// Arc segments attached to the polyline (optional refinement).
    pub arcs: Vec<Arc>,
}

impl Trace {
    /// Total polyline length [m].
    pub fn length(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].distance_to(&w[1]))
            .sum()
    }

    /// Cross-sectional area [m²] (width × thickness, thickness from width
    /// ratio is not used here — caller supplies the stackup thickness).
    pub fn cross_section(&self, thickness: Length) -> f64 {
        self.width.as_meters() * thickness.as_meters()
    }
}

/// A circular arc segment.
#[derive(Clone, Copy, Debug)]
pub struct Arc {
    /// Start point.
    pub start: Point2,
    /// End point.
    pub end: Point2,
    /// Arc center.
    pub center: Point2,
    /// `true` for clockwise sweep (Gerber convention: Y axis up).
    pub clockwise: bool,
}

impl Arc {
    /// Sweep angle [rad].
    pub fn sweep_angle(&self) -> f64 {
        let a1 = (self.start.y - self.center.y).atan2(self.start.x - self.center.x);
        let a2 = (self.end.y - self.center.y).atan2(self.end.x - self.center.x);
        let mut d = a2 - a1;
        if self.clockwise && d > 0.0 {
            d -= std::f64::consts::TAU;
        } else if !self.clockwise && d < 0.0 {
            d += std::f64::consts::TAU;
        }
        d.abs()
    }

    /// Arc radius [m].
    pub fn radius(&self) -> f64 {
        self.start.distance_to(&self.center)
    }
}

/// Pad shapes supported by the geometry model.
#[derive(Clone, Debug, PartialEq)]
pub enum PadShape {
    /// Round pad.
    Circle {
        /// Diameter [m].
        diameter: Length,
    },
    /// Rectangular pad.
    Rectangle {
        /// X extent [m].
        width: Length,
        /// Y extent [m].
        height: Length,
    },
    /// Rounded rectangle.
    RoundedRectangle {
        /// X extent [m].
        width: Length,
        /// Y extent [m].
        height: Length,
        /// Corner radius [m].
        radius: Length,
    },
    /// Oblong (slot-shaped) pad.
    Oblong {
        /// X extent [m].
        width: Length,
        /// Y extent [m].
        height: Length,
    },
    /// Arbitrary polygon outline.
    Custom {
        /// Outline vertices [m].
        polygon: Vec<Point2>,
    },
}

impl PadShape {
    /// Nominal pad area [m²].
    pub fn area(&self) -> f64 {
        match self {
            PadShape::Circle { diameter } => {
                let r = diameter.as_meters() / 2.0;
                std::f64::consts::PI * r * r
            }
            PadShape::Rectangle { width, height } | PadShape::Oblong { width, height } => {
                width.as_meters() * height.as_meters()
            }
            PadShape::RoundedRectangle {
                width,
                height,
                radius,
            } => {
                let w = width.as_meters();
                let h = height.as_meters();
                let r = radius.as_meters();
                w * h - (4.0 - std::f64::consts::PI) * r * r
            }
            PadShape::Custom { polygon } => polygon_area(polygon),
        }
    }
}

/// A pad (component land or via land).
#[derive(Clone, Debug)]
pub struct Pad {
    /// Pad identifier.
    pub id: u64,
    /// Outline.
    pub shape: PadShape,
    /// Center position [m].
    pub position: Point2,
    /// Layer index.
    pub layer: u32,
    /// Associated drill (through-hole pads and vias).
    pub drill: Option<DrillHole>,
}

/// A drilled hole.
#[derive(Clone, Debug, PartialEq)]
pub struct DrillHole {
    /// Finished hole diameter [m].
    pub diameter: Length,
    /// Copper plating thickness on the barrel [m].
    pub plating_thickness: Length,
    /// Whether this hole is a via (vs. a component hole).
    pub is_via: bool,
    /// Via classification.
    pub via_type: ViaType,
}

/// Via classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViaType {
    /// Spans the full board.
    ThroughHole,
    /// Outer layer to inner layer.
    Blind,
    /// Inner layer to inner layer.
    Buried,
    /// Laser-drilled small via.
    MicroVia,
    /// Deliberate thermal via array member.
    ThermalVia,
}

/// A filled copper region (plane or pour).
#[derive(Clone, Debug)]
pub struct CopperArea {
    /// Layer index.
    pub layer: u32,
    /// Outline polygon.
    pub polygon: Vec<Point2>,
    /// Outline area [m²] (filled).
    pub area: f64,
    /// Fraction of the outline actually covered by copper (after clearances).
    pub copper_coverage: f64,
}

impl CopperArea {
    /// Creates a copper area, computing the outline area automatically.
    pub fn from_polygon(layer: u32, polygon: Vec<Point2>, coverage: f64) -> Self {
        Self {
            layer,
            area: polygon_area(&polygon),
            polygon,
            copper_coverage: coverage.clamp(0.0, 1.0),
        }
    }

    /// Effective copper area [m²].
    pub fn effective_area(&self) -> f64 {
        self.area * self.copper_coverage
    }
}

/// Shoelace (surveyor's) polygon area [m²]; sign is positive for CCW order.
pub fn polygon_area(polygon: &[Point2]) -> f64 {
    let n = polygon.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    (sum / 2.0).abs()
}

/// Point-in-polygon test (ray casting, boundary included approximately).
pub fn point_in_polygon(p: Point2, polygon: &[Point2]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > p.y) != (pj.y > p.y))
            && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y + 1e-300) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Voxel edge lengths [m].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelResolution {
    /// Cell size along X.
    pub dx: Length,
    /// Cell size along Y.
    pub dy: Length,
    /// Cell size along Z.
    pub dz: Length,
}

/// A single voxel (hexahedral cell).
#[derive(Clone, Debug)]
pub struct Voxel {
    /// Cell index along X.
    pub x: u32,
    /// Cell index along Y.
    pub y: u32,
    /// Cell index along Z.
    pub z: u32,
    /// Material filling the cell.
    pub material: MaterialId,
    /// Fraction of the cell actually inside the part [0..1].
    pub volume_fraction: f64,
}

/// A structured voxel grid (the engine's hexahedral FEM mesh).
#[derive(Clone, Debug)]
pub struct VoxelGrid {
    /// Cell sizes.
    pub resolution: VoxelResolution,
    /// Domain bounds.
    pub bounds: BoundingBox3,
    nx: u32,
    ny: u32,
    nz: u32,
    /// Voxels in x-major, then y, then z order (`i = x + nx*(y + ny*z)`).
    voxels: Vec<Voxel>,
}

impl VoxelGrid {
    /// Creates a uniform grid filled with `fill` material.
    pub fn new(bounds: BoundingBox3, resolution: VoxelResolution, fill: MaterialId) -> Self {
        let e = bounds.extents();
        let nx = (e.x / resolution.dx.as_meters()).round().max(1.0) as u32;
        let ny = (e.y / resolution.dy.as_meters()).round().max(1.0) as u32;
        let nz = (e.z / resolution.dz.as_meters()).round().max(1.0) as u32;
        let count = (nx as usize) * (ny as usize) * (nz as usize);
        let voxels = (0..count)
            .map(|i| {
                let (x, y, z) = Self::unflatten(nx, ny, i as u32);
                Voxel {
                    x,
                    y,
                    z,
                    material: fill.clone(),
                    volume_fraction: 1.0,
                }
            })
            .collect();
        Self {
            resolution,
            bounds,
            nx,
            ny,
            nz,
            voxels,
        }
    }

    /// Number of cells along X.
    pub fn nx(&self) -> u32 {
        self.nx
    }

    /// Number of cells along Y.
    pub fn ny(&self) -> u32 {
        self.ny
    }

    /// Number of cells along Z.
    pub fn nz(&self) -> u32 {
        self.nz
    }

    /// Total voxel count.
    pub fn len(&self) -> usize {
        self.voxels.len()
    }

    /// Whether the grid has no voxels.
    pub fn is_empty(&self) -> bool {
        self.voxels.is_empty()
    }

    /// Borrows all voxels in storage order.
    pub fn voxels(&self) -> &[Voxel] {
        &self.voxels
    }

    /// Mutable borrow of all voxels in storage order.
    pub fn voxels_mut(&mut self) -> &mut [Voxel] {
        &mut self.voxels
    }

    /// Flattened storage index for cell coordinates.
    pub fn index(&self, x: u32, y: u32, z: u32) -> Option<usize> {
        if x < self.nx && y < self.ny && z < self.nz {
            Some(x as usize + self.nx as usize * (y as usize + self.ny as usize * z as usize))
        } else {
            None
        }
    }

    /// Voxel at cell coordinates.
    pub fn get(&self, x: u32, y: u32, z: u32) -> Option<&Voxel> {
        self.index(x, y, z).map(|i| &self.voxels[i])
    }

    /// Mutable voxel at cell coordinates.
    pub fn get_mut(&mut self, x: u32, y: u32, z: u32) -> Option<&mut Voxel> {
        let idx = self.index(x, y, z)?;
        self.voxels.get_mut(idx)
    }

    /// Center point of a cell [m].
    pub fn center_of(&self, x: u32, y: u32, z: u32) -> Point3 {
        let r = &self.resolution;
        Point3::new(
            self.bounds.min.x + (x as f64 + 0.5) * r.dx.as_meters(),
            self.bounds.min.y + (y as f64 + 0.5) * r.dy.as_meters(),
            self.bounds.min.z + (z as f64 + 0.5) * r.dz.as_meters(),
        )
    }

    /// Cell volume [m³].
    pub fn cell_volume(&self) -> f64 {
        let r = &self.resolution;
        r.dx.as_meters() * r.dy.as_meters() * r.dz.as_meters()
    }

    fn unflatten(nx: u32, ny: u32, i: u32) -> (u32, u32, u32) {
        let x = i % nx;
        let rest = i / nx;
        let y = rest % ny;
        let z = rest / ny;
        (x, y, z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_elec_core::Length;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point2> {
        vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ]
    }

    #[test]
    fn polygon_area_shoelace() {
        // 10 mm × 20 mm rectangle
        let a = polygon_area(&rect(0.0, 0.0, 0.01, 0.02));
        assert!((a - 2.0e-4).abs() < 1e-12);
        // L-shape: 3×3 square minus 2×1 notch = 7
        let l = vec![
            Point2::new(0.0, 0.0),
            Point2::new(3.0, 0.0),
            Point2::new(3.0, 2.0),
            Point2::new(1.0, 2.0),
            Point2::new(1.0, 3.0),
            Point2::new(0.0, 3.0),
        ];
        assert!((polygon_area(&l) - 7.0).abs() < 1e-12);
        assert!(polygon_area(&[Point2::new(0., 0.), Point2::new(1., 1.)]) == 0.0);
    }

    #[test]
    fn point_in_polygon_works() {
        let square = rect(0.0, 0.0, 1.0, 1.0);
        assert!(point_in_polygon(Point2::new(0.5, 0.5), &square));
        assert!(!point_in_polygon(Point2::new(1.5, 0.5), &square));
        assert!(!point_in_polygon(Point2::new(-0.1, 0.5), &square));
    }

    #[test]
    fn pad_shape_areas() {
        let circle = PadShape::Circle {
            diameter: Length::mm(1.0),
        };
        assert!((circle.area() - std::f64::consts::PI * (0.5e-3_f64).powi(2)).abs() < 1e-14);
        let r = PadShape::Rectangle {
            width: Length::mm(2.0),
            height: Length::mm(1.0),
        };
        assert!((r.area() - 2.0e-6).abs() < 1e-12);
        let rr = PadShape::RoundedRectangle {
            width: Length::mm(2.0),
            height: Length::mm(1.0),
            radius: Length::mm(0.5),
        };
        // That is an oblong: rect + full semicircle ends = rect minus corners + circle
        assert!(rr.area() < r.area() && rr.area() > 0.0);
        let custom = PadShape::Custom {
            polygon: rect(0., 0., 3e-3, 2e-3),
        };
        assert!((custom.area() - 6.0e-6).abs() < 1e-12);
    }

    #[test]
    fn trace_length_and_arc_sweep() {
        let t = Trace {
            id: TraceId::new(1),
            net: "CLK".into(),
            layer: 0,
            width: Length::mm(0.2),
            points: vec![Point2::new(0., 0.), Point2::new(3e-3, 4e-3)],
            arcs: vec![],
        };
        assert!((t.length() - 5e-3).abs() < 1e-15);

        let arc = Arc {
            start: Point2::new(1.0, 0.0),
            end: Point2::new(0.0, 1.0),
            center: Point2::new(0.0, 0.0),
            clockwise: false,
        };
        assert!((arc.sweep_angle() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((arc.radius() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn voxel_grid_indexing_and_centers() {
        let bounds = BoundingBox3::new(Point3::new(0., 0., 0.), Point3::new(4e-3, 2e-3, 1e-3));
        let grid = VoxelGrid::new(
            bounds,
            VoxelResolution {
                dx: Length::mm(1.0),
                dy: Length::mm(1.0),
                dz: Length::mm(0.5),
            },
            MaterialId::new("fr4"),
        );
        assert_eq!((grid.nx(), grid.ny(), grid.nz()), (4, 2, 2));
        assert_eq!(grid.len(), 16);
        let c = grid.center_of(0, 0, 0);
        assert!((c.x - 0.5e-3).abs() < 1e-15);
        assert!((c.z - 0.25e-3).abs() < 1e-15);
        assert!(grid.get(4, 0, 0).is_none());
        assert_eq!(grid.get(3, 1, 1).unwrap().material, MaterialId::new("fr4"));
    }

    #[test]
    fn voxel_material_assignment() {
        let bounds = BoundingBox3::new(Point3::new(0., 0., 0.), Point3::new(2e-3, 1e-3, 1e-3));
        let mut grid = VoxelGrid::new(
            bounds,
            VoxelResolution {
                dx: Length::mm(1.0),
                dy: Length::mm(1.0),
                dz: Length::mm(1.0),
            },
            MaterialId::new("fr4"),
        );
        grid.get_mut(1, 0, 0).unwrap().material = MaterialId::new("copper");
        assert_eq!(
            grid.get(1, 0, 0).unwrap().material,
            MaterialId::new("copper")
        );
        assert_eq!(grid.get(0, 0, 0).unwrap().material, MaterialId::new("fr4"));
        assert!((grid.cell_volume() - 1e-9).abs() < 1e-18);
    }

    #[test]
    fn copper_area_coverage() {
        let area = CopperArea::from_polygon(1, rect(0., 0., 0.02, 0.01), 0.75);
        assert!((area.area - 2.0e-4).abs() < 1e-12);
        assert!((area.effective_area() - 1.5e-4).abs() < 1e-12);
    }
}
