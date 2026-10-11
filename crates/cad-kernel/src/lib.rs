//! A deliberately narrow, native Rust boundary-representation kernel.
//!
//! Supports rectangular and simple polygon extrusions, including disjoint convex
//! through holes, rigid placements, orthogonal solid Booleans, and bounded convex
//! planar intersection. Polygon caps remain analytic planar faces with explicit
//! inner loops where needed. Orthogonal Boolean faces stay rectangular and share
//! explicit edges and vertices; open pockets, through-holes, empty results, and
//! disconnected solids are supported in that domain.
//! Enclosed cavities, curved surfaces, and nonmanifold contacts are unsupported.
//! General rigid rotations support a single strictly convex solid or a validated
//! right prism with a simple outer boundary and convex disjoint through holes. Exactly represented axis
//! permutations also retain the orthogonal solid domain. Multi-loop prism caps
//! support signed-loop properties; Boolean operations on these prisms are not yet
//! supported.
//! All coordinates, dimensions, and linear tolerances are millimetres; volume is
//! mm³. Results contain analytic lines and planes and explicit oriented topology.
//! Validated mass properties integrate volume, centroid, and central volume
//! inertia (mm⁵) from this boundary; material density is an application concern.
//! There is no external CAD kernel, mesh approximation, or stored volume shortcut.

mod convex_intersection;
mod face_frame;
mod mass_properties;
mod math;
mod orthogonal;
mod polygon;
mod prism;
mod simple_polygon;
mod transform;
mod triangulation;
mod validation;

pub use face_frame::FaceFrame;
pub use mass_properties::MassProperties;
pub use math::Tolerance;
use math::{cross, dot, sub};
pub use orthogonal::{boolean, translate, BooleanOp};
pub use polygon::{extrude_polygon, validate_polygon, MAX_POLYGON_POINTS};
pub use prism::{extrude_polygon_with_holes, validate_polygon_with_holes, MAX_POLYGON_HOLES};
use serde::{Deserialize, Serialize};
pub use simple_polygon::face_is_convex;
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub vertices: [VertexId; 2],
    pub line: Line,
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
    pub plane: Plane,
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
    pub fn validate(&self, tolerance: Tolerance) -> ValidationReport {
        validation::validate(self, tolerance)
    }

    /// Axis-aligned bounds; an empty snapshot has infinite/inverted bounds.
    pub fn bounds(&self) -> Bounds {
        let mut result = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
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

    /// Signed enclosed volume from oriented boundary triangles, in mm³.
    /// Empty models have volume zero. Validate deserialized or edited snapshots
    /// first: invalid references or unsupported integration terms yield NaN.
    pub fn volume(&self) -> f64 {
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
                    line: Line {
                        origin: positions[key.0],
                        direction: delta.map(|v| v / length),
                    },
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
            plane: Plane {
                origin: positions[ring[0]],
                normal: normals[face_index],
            },
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
