//! A deliberately narrow, native Rust boundary-representation kernel.
//!
//! Supports rectangular and simple polygon extrusions, including disjoint simple
//! through holes, rigid placements, orthogonal solid Booleans, and bounded convex
//! planar intersection. Polygon caps remain analytic planar faces with explicit
//! inner loops where needed. Orthogonal Boolean faces stay rectangular and share
//! explicit edges and vertices; open pockets, through-holes, empty results, and
//! disconnected solids are supported in that domain.
//! Convex CCW line/arc paths additionally extrude to native trimmed circular edges
//! and cylindrical wall patches, with optional circular through-holes. Arc sweeps
//! are bounded to at most a half-turn; tangent joins retain source vertices.
//! Circular extrusions additionally retain exact circles, planar caps, a cylindrical
//! wall, and an explicit periodic seam. Full-turn piecewise-linear meridian revolutions
//! additionally support connected stepped shafts and coaxial counterbores with
//! exact planar shoulders, cylindrical walls, conical frusta, and conical poles.
//! Enclosed cavities, general curved Booleans, and nonmanifold contacts remain
//! unsupported.
//! General rigid rotations support a single strictly convex solid or a validated
//! right prism with a simple outer boundary and simple disjoint through holes. Exactly represented axis
//! permutations also retain the orthogonal solid domain. Multi-loop prism caps
//! support signed-loop properties; Boolean operations on these prisms are not yet
//! supported.
//! All coordinates, dimensions, and linear tolerances are millimetres; volume is
//! mm³. Results contain tagged analytic curves/surfaces and explicit oriented topology.
//! Validated mass properties integrate volume, centroid, and central volume
//! inertia (mm⁵) from this boundary; material density is an application concern.
//! Authoritative solids use neither an external CAD kernel nor mesh approximation.
//! Display and STL meshes are derived with an explicit chordal-error policy;
//! native physical properties never integrate those approximations.

mod analytic;
mod arc_prism;
mod bored_prism;
mod circular_boolean;
mod convex_clearance;
mod hub_keyway;
mod keyseat;
mod rotation_clearance;
mod solid_clearance;
pub use rotation_clearance::{
    RotationClearance, RotationInterval, RotationSample, MAX_ROTATION_DEPTH, MAX_ROTATION_QUERIES,
};
mod translation_clearance;
pub use solid_clearance::{ClearanceMethod, SolidClearance};
pub use translation_clearance::{
    TranslationClearance, TranslationInterval, TranslationSample, MAX_TRANSLATION_DEPTH,
    MAX_TRANSLATION_QUERIES,
};
mod convex_intersection;
pub use convex_clearance::{
    ConvexClearance, MAX_CLEARANCE_CANDIDATES, MAX_CLEARANCE_EDGES, MAX_CLEARANCE_FACES,
    MAX_CLEARANCE_ITERATIONS, MAX_CLEARANCE_VERTICES,
};
pub use convex_intersection::chamfer;
mod face_frame;
mod mass_properties;
mod math;
mod orthogonal;
mod path_serde;
mod polygon;
mod prism;
mod profile_prism;
mod revolve;
mod section;
mod simple_polygon;
pub use section::{PlanarSection, SectionCurve, SectionLoop, SectionPlane, SectionSegment};
mod stepped_bore;
pub use stepped_bore::MAX_STEPPED_BORE_SEGMENTS;
mod round_stock;
mod tessellation;
mod through_bore;
mod transform;
mod triangulation;
mod validation;

pub use analytic::{
    extrude_annulus, extrude_circle, validate_annulus, validate_circle, CylinderGeometry,
};
pub use arc_prism::{extrude_path, validate_path, ArcPrismGeometry, PathSegment, PlanarPath};
pub use bored_prism::{BoredPrismGeometry, PrismBore};
pub use face_frame::FaceFrame;
pub use mass_properties::MassProperties;
pub use math::Tolerance;
use math::{cross, dot, sub};
pub use orthogonal::{boolean, translate, BooleanOp};
pub use polygon::{extrude_polygon, validate_polygon, MAX_POLYGON_POINTS};
pub use prism::{extrude_polygon_with_holes, validate_polygon_with_holes, MAX_POLYGON_HOLES};
pub use profile_prism::{
    extrude_polygon_with_circular_holes, validate_polygon_with_circular_holes, CircleHole,
    ProfilePrismGeometry,
};
pub use revolve::{revolve_polygon, validate_revolve_profile, RevolveGeometry, RevolveSegment};
use serde::{Deserialize, Serialize};
pub use simple_polygon::face_is_convex;
use std::collections::BTreeMap;
pub use tessellation::{
    tessellate_analytic, tessellate_cylinder, CylinderMesh, MeshPatch, TessellationPolicy,
};
use thiserror::Error;
pub use transform::{transform, validate_transform};
pub use triangulation::triangulate_face;
pub use validation::{ValidationIssue, ValidationReport};

pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Deterministic source, dependency, compiler, target, and feature fingerprint.
/// Changes independently of semver during development; contains no checkout path.
pub const KERNEL_BUILD_ID: &str = env!("CAD_KERNEL_BUILD_ID");

macro_rules! entity_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub usize);
    )+};
}
entity_id!(VertexId, EdgeId, LoopId, FaceId, ShellId, SolidId);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

/// Oriented periodic circle. Parameter zero is origin + radius*x_direction;
/// increasing parameter follows axis cross x_direction. A full circle edge
/// has the same vertex at both endpoints; it is not a zero-length line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circle {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub radius: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "geometry",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Curve {
    Line(Line),
    Circle(Circle),
    CircularArc(CircularArc),
}
/// Trimmed circle whose start radial direction is the support X direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircularArc {
    pub circle: Circle,
    pub sweep_radians: f64,
}
fn is_false(value: &bool) -> bool {
    !value
}
/// An oriented analytic cylinder. Axial bounds belong to topology,
/// not to the supporting infinite surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cylinder {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub radius: f64,
    /// Reverse the support normal for an interior cylindrical wall.
    #[serde(default, skip_serializing_if = "is_false")]
    pub inward: bool,
}
/// Conical support, optionally trimmed at an apex. Radius at axial coordinate z
/// is `radius + slope*z`; origin lies at the lower trimming circle center or pole.
/// The outward support normal follows radial - slope*axis (then inward reversal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cone {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub radius: f64,
    pub slope: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inward: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "geometry",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Surface {
    Plane(Plane),
    Cylinder(Cylinder),
    Cone(Cone),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub vertices: [VertexId; 2],
    pub curve: Curve,
}

impl Edge {
    pub fn try_line(&self) -> Option<&Line> {
        match &self.curve {
            Curve::Line(v) => Some(v),
            _ => None,
        }
    }
    pub fn try_line_mut(&mut self) -> Option<&mut Line> {
        match &mut self.curve {
            Curve::Line(v) => Some(v),
            _ => None,
        }
    }
}
impl Face {
    pub fn try_plane(&self) -> Option<&Plane> {
        match &self.surface {
            Surface::Plane(v) => Some(v),
            _ => None,
        }
    }
    pub fn try_plane_mut(&mut self) -> Option<&mut Plane> {
        match &mut self.surface {
            Surface::Plane(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeUse {
    pub edge: EdgeId,
    pub reversed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loop {
    pub edges: Vec<EdgeUse>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub surface: Surface,
    pub loops: Vec<LoopId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shell {
    pub faces: Vec<FaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Solid {
    pub shells: Vec<ShellId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSnapshot {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
    pub loops: Vec<Loop>,
    pub faces: Vec<Face>,
    pub shells: Vec<Shell>,
    pub solids: Vec<Solid>,
}

impl ModelSnapshot {
    pub fn has_curved_geometry(&self) -> bool {
        self.edges
            .iter()
            .any(|e| !matches!(e.curve, Curve::Line(_)))
            || self
                .faces
                .iter()
                .any(|f| !matches!(f.surface, Surface::Plane(_)))
    }

    pub fn validate(&self, tolerance: Tolerance) -> ValidationReport {
        validation::validate(self, tolerance)
    }

    /// Axis-aligned bounds; an empty snapshot has infinite/inverted bounds.
    pub fn bounds(&self) -> Bounds {
        if let Ok(b) = hub_keyway::bounds(self) {
            return b;
        }
        if let Ok(bounds) = keyseat::bounds(self) {
            return bounds;
        }
        let mut result = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        // Periodic circles carry analytic extrema absent from seam vertices.
        for edge in &self.edges {
            if let Curve::CircularArc(arc) = &edge.curve {
                let c = &arc.circle;
                let y = cross(c.axis, c.x_direction);
                for (axis, y_component) in y.iter().enumerate() {
                    let maximum = y_component.atan2(c.x_direction[axis]);
                    for angle in [maximum, maximum + std::f64::consts::PI] {
                        let progress =
                            (angle * arc.sweep_radians.signum()).rem_euclid(std::f64::consts::TAU);
                        if progress <= arc.sweep_radians.abs() + 64. * f64::EPSILON {
                            let value = c.origin[axis]
                                + c.radius
                                    * (c.x_direction[axis] * angle.cos() + y[axis] * angle.sin());
                            result.min[axis] = result.min[axis].min(value);
                            result.max[axis] = result.max[axis].max(value);
                        }
                    }
                }
            }
            if let Curve::Circle(circle) = &edge.curve {
                let y = cross(circle.axis, circle.x_direction);
                for (axis, y_component) in y.iter().enumerate() {
                    let extent = circle.radius * circle.x_direction[axis].hypot(*y_component);
                    result.min[axis] = result.min[axis].min(circle.origin[axis] - extent);
                    result.max[axis] = result.max[axis].max(circle.origin[axis] + extent);
                }
            }
        }
        for vertex in &self.vertices {
            for axis in 0..3 {
                result.min[axis] = result.min[axis].min(vertex.position[axis]);
                result.max[axis] = result.max[axis].max(vertex.position[axis]);
            }
        }
        result
    }

    pub(crate) fn endpoints(&self, edge_use: EdgeUse) -> Option<[VertexId; 2]> {
        let edge = self.edges.get(edge_use.edge.0)?;
        let [a, b] = edge.vertices;
        Some(if edge_use.reversed { [b, a] } else { [a, b] })
    }

    pub(crate) fn loop_points(&self, id: LoopId) -> Option<Vec<[f64; 3]>> {
        self.loops
            .get(id.0)?
            .edges
            .iter()
            .map(|edge_use| {
                let vertex = self.endpoints(*edge_use)?[0];
                Some(self.vertices.get(vertex.0)?.position)
            })
            .collect()
    }

    /// Enclosed volume from the analytic boundary, in mm³. Planar models use
    /// signed boundary integration; validated circular extrusions use exact
    /// circular cross-section integration.
    /// Empty models have volume zero. Validate deserialized or edited snapshots
    /// first: invalid references or unsupported integration terms yield NaN.
    pub fn volume(&self) -> f64 {
        if let Ok(v) = hub_keyway::volume(self) {
            return v;
        }
        if let Ok(v) = keyseat::volume(self) {
            return v;
        }
        if let Ok(v) = stepped_bore::volume(self) {
            return v;
        }
        if let Ok(v) = bored_prism::volume(self) {
            return v;
        }
        if self
            .edges
            .iter()
            .any(|e| matches!(e.curve, Curve::CircularArc(_)))
        {
            return arc_prism::volume(self).unwrap_or(f64::NAN);
        }
        if self.has_curved_geometry() {
            return analytic::recognize(self, None)
                .map(|g| g.volume())
                .or_else(|_| profile_prism::volume(self))
                .or_else(|_| revolve::volume(self))
                .unwrap_or(f64::NAN);
        }
        self.shells
            .iter()
            .map(|shell| self.faces_volume(&shell.faces))
            .sum()
    }

    pub(crate) fn faces_volume(&self, faces: &[FaceId]) -> f64 {
        let Some(reference) = faces
            .first()
            .and_then(|id| self.faces.get(id.0))
            .and_then(|face| face.loops.first())
            .and_then(|id| self.loop_points(*id))
            .and_then(|points| points.first().copied())
        else {
            return f64::NAN;
        };
        let mut volume = 0.0;
        let mut compensation = 0.0;
        for face_id in faces {
            let Some(face) = self.faces.get(face_id.0) else {
                return f64::NAN;
            };
            for &loop_id in &face.loops {
                let Some(points) = self.loop_points(loop_id) else {
                    return f64::NAN;
                };
                if points.len() < 3 {
                    return f64::NAN;
                }
                let a = sub(points[0], reference);
                for triangle in points[1..].windows(2) {
                    let determinant = dot(
                        a,
                        cross(sub(triangle[0], reference), sub(triangle[1], reference)),
                    );
                    let contribution = determinant / 6.0;
                    if determinant != 0.0 && (!determinant.is_normal() || !contribution.is_normal())
                    {
                        return f64::NAN;
                    }
                    let corrected = contribution - compensation;
                    let next = volume + corrected;
                    compensation = (next - volume) - corrected;
                    volume = next;
                }
            }
        }
        volume
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyHistory {
    /// Semantic roles generated by this operation, local to this result.
    pub generated_faces: BTreeMap<String, Vec<FaceId>>,
    /// Exact face provenance, indexed by operation operand and operand-local ID.
    /// Translate and transform use `input`; Booleans use `left` and `right`. Every input face
    /// has an entry: an empty list means deleted, multiple IDs mean split. Lists
    /// are sorted and contain no duplicates. Coincident contributing input
    /// surfaces may both map to the same output face. Difference reverses the
    /// right operand's orientation. These are geometric witnesses, not a claim
    /// that a semantic selection is uniquely resolved after a parameter edit.
    /// An absent operand means no provenance supplied (including older JSON).
    #[serde(default)]
    pub input_faces: BTreeMap<String, BTreeMap<FaceId, Vec<FaceId>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationResult {
    pub model: ModelSnapshot,
    pub history: TopologyHistory,
}

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("invalid planar face frame: {0}")]
    InvalidFaceFrame(String),
    #[error("invalid rigid transform: {0}")]
    InvalidTransform(String),
    #[error("linear tolerance must be finite and positive")]
    InvalidTolerance,
    #[error("unsupported geometry: {0}")]
    Unsupported(String),
    #[error("operation exceeds resource limit: {0}")]
    ResourceLimit(String),
    #[error("input model failed validation: {0:?}")]
    InvalidInput(ValidationReport),
    #[error("{dimension} must be finite and greater than the linear tolerance (millimetres)")]
    InvalidDimension { dimension: &'static str },
    #[error("derived geometry is outside the supported normal floating-point range")]
    NumericRange,
    #[error("generated model failed validation: {0:?}")]
    InvalidResult(ValidationReport),
}

/// Extrude [0,width] × [0,depth] from Z=0 to Z=height, in millimetres.
/// Dimensions must exceed the positive linear tolerance. No input is mutated.
/// Face areas, triangle areas, volume, and nonzero tetrahedral contributions
/// must stay in the normal f64 range; subnormal geometry is unsupported.
pub fn extrude_rectangle(
    width: f64,
    depth: f64,
    height: f64,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    for (dimension, value) in [("width", width), ("depth", depth), ("height", height)] {
        if !value.is_finite() || value <= tolerance.linear {
            return Err(KernelError::InvalidDimension { dimension });
        }
    }
    // Reject subnormal arithmetic as well as overflow: merely being nonzero is
    // insufficient for reliable relative precision. Include /2 face triangles
    // and /6 tetrahedral volume contributions in the supported envelope.
    for quantity in [
        width * depth,
        width * height,
        depth * height,
        width * depth * height,
        width * depth / 2.0,
        width * height / 2.0,
        depth * height / 2.0,
        width * depth * height / 6.0,
    ] {
        if !quantity.is_normal() || quantity <= 0.0 || !(quantity * 12.0).is_finite() {
            return Err(KernelError::NumericRange);
        }
    }
    let positions = [
        [0.0, 0.0, 0.0],
        [width, 0.0, 0.0],
        [width, depth, 0.0],
        [0.0, depth, 0.0],
        [0.0, 0.0, height],
        [width, 0.0, height],
        [width, depth, height],
        [0.0, depth, height],
    ];
    let mut model = ModelSnapshot {
        vertices: positions.map(|position| Vertex { position }).to_vec(),
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    // Outward-oriented rings: bottom, top, y-min, x-max, y-max, x-min.
    let rings = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    let normals = [
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
    ];
    let mut edge_map = BTreeMap::new();
    for (face_index, ring) in rings.iter().enumerate() {
        let mut uses = Vec::new();
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            let key = (a.min(b), a.max(b));
            let edge_id = *edge_map.entry(key).or_insert_with(|| {
                let id = EdgeId(model.edges.len());
                let delta = sub(positions[key.1], positions[key.0]);
                let length = math::norm(delta);
                model.edges.push(Edge {
                    vertices: [VertexId(key.0), VertexId(key.1)],
                    curve: Curve::Line(Line {
                        origin: positions[key.0],
                        direction: delta.map(|v| v / length),
                    }),
                });
                id
            });
            uses.push(EdgeUse {
                edge: edge_id,
                reversed: a != key.0,
            });
        }
        let loop_id = LoopId(model.loops.len());
        model.loops.push(Loop { edges: uses });
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: positions[ring[0]],
                normal: normals[face_index],
            }),
            loops: vec![loop_id],
        });
    }
    model.shells.push(Shell {
        faces: (0..6).map(FaceId).collect(),
    });
    model.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let generated_faces = [
        ("start_cap", vec![FaceId(0)]),
        ("end_cap", vec![FaceId(1)]),
        ("walls", (2..6).map(FaceId).collect()),
        ("wall_y_min", vec![FaceId(2)]),
        ("wall_x_max", vec![FaceId(3)]),
        ("wall_y_max", vec![FaceId(4)]),
        ("wall_x_min", vec![FaceId(5)]),
    ]
    .into_iter()
    .map(|(role, faces)| (role.to_owned(), faces))
    .collect();
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces,
            input_faces: BTreeMap::new(),
        },
    })
}

#[cfg(test)]
mod tests;
