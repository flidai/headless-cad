//! Geometric validity for orthogonal solids and single convex polyhedra. Topology checks run
//! first; all indexed references here have already been checked by the caller.
use super::ValidationIssue;
use crate::*;

const MAX_FACE_PAIRS: usize = 20_000_000;

struct Rectangle {
    axis: usize,
    sign: i32,
    min: [f64; 3],
    max: [f64; 3],
    vertices: Vec<VertexId>,
    edges: Vec<EdgeId>,
}

fn issue(code: &str, message: impl Into<String>) -> Vec<ValidationIssue> {
    vec![ValidationIssue {
        code: code.into(),
        message: message.into(),
    }]
}

pub(super) fn validate(model: &ModelSnapshot, tolerance: Tolerance) -> Vec<ValidationIssue> {
    let orthogonal = model.faces.iter().all(|face| {
        model.loops[face.loops[0].0].edges.len() == 4
            && (0..3).any(|axis| {
                face.plane.normal[axis].abs() == 1.0
                    && (0..3)
                        .filter(|&i| i != axis)
                        .all(|i| face.plane.normal[i] == 0.0)
            })
    }) && model.edges.iter().all(|edge| {
        let a = model.vertices[edge.vertices[0].0].position;
        let b = model.vertices[edge.vertices[1].0].position;
        (0..3).filter(|&i| a[i] != b[i]).count() == 1
    });
    if !orthogonal {
        return validate_convex(model, tolerance);
    }
    validate_orthogonal(model)
}

// Topological closure and per-face convexity have already been verified. Every
// face must be a strict supporting plane of the common convex vertex hull.
// This excludes crossing/folded shells, coincident subdivisions, duplicate
// geometric vertices, and inward surfaces; no generic intersection claim is made.
pub(crate) fn validate_convex(model: &ModelSnapshot, tolerance: Tolerance) -> Vec<ValidationIssue> {
    if model.solids.len() != 1 || model.shells.len() != 1 {
        return issue("unsupported_geometry", "general planar geometry currently requires one convex solid; multiple general solids are unsupported");
    }
    if model
        .faces
        .len()
        .checked_mul(model.vertices.len())
        .is_none_or(|work| work > MAX_FACE_PAIRS)
    {
        return issue(
            "resource_limit",
            "convex supporting-plane checks exceed operation budget",
        );
    }
    for (index, face) in model.faces.iter().enumerate() {
        let vertices: std::collections::BTreeSet<_> = model.loops[face.loops[0].0]
            .edges
            .iter()
            .map(|edge| model.endpoints(*edge).unwrap()[0])
            .collect();
        for (id, vertex) in model.vertices.iter().enumerate() {
            if vertices.contains(&VertexId(id)) {
                continue;
            }
            let delta = crate::math::sub(vertex.position, face.plane.origin);
            let products: [f64; 3] =
                std::array::from_fn(|axis| delta[axis] * face.plane.normal[axis]);
            let distance: f64 = products.into_iter().sum();
            let uncertainty =
                products.into_iter().map(f64::abs).sum::<f64>() * (16.0 * f64::EPSILON);
            if !distance.is_finite()
                || !uncertainty.is_finite()
                || distance >= -tolerance.linear - uncertainty
            {
                return issue(
                    "unsupported_geometry",
                    format!(
                        "Face {index} is not a strict supporting plane of a single convex solid"
                    ),
                );
            }
        }
    }
    Vec::new()
}

fn validate_orthogonal(model: &ModelSnapshot) -> Vec<ValidationIssue> {
    let mut rectangles = Vec::with_capacity(model.faces.len());
    for (index, face) in model.faces.iter().enumerate() {
        let boundary = &model.loops[face.loops[0].0];
        let axis = (0..3).find(|&axis| {
            face.plane.normal[axis].abs() == 1.0
                && (0..3)
                    .filter(|&i| i != axis)
                    .all(|i| face.plane.normal[i] == 0.0)
        });
        let Some(axis) = axis else {
            return issue(
                "unsupported_geometry",
                format!("Face {index}: only exact axis-aligned rectangular geometry is supported"),
            );
        };
        if boundary.edges.len() != 4 {
            return issue(
                "unsupported_geometry",
                format!("Face {index}: only rectangular quadrilateral faces are supported"),
            );
        }
        let vertices: Vec<_> = boundary
            .edges
            .iter()
            .map(|edge_use| model.endpoints(*edge_use).unwrap()[0])
            .collect();
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for &vertex in &vertices {
            for (axis, coordinate) in model.vertices[vertex.0].position.into_iter().enumerate() {
                min[axis] = min[axis].min(coordinate);
                max[axis] = max[axis].max(coordinate);
            }
        }
        if min[axis] != max[axis] || min[axis] != face.plane.origin[axis] {
            return issue(
                "unsupported_geometry",
                format!(
                    "Face {index}: vertices must lie exactly on the supporting coordinate plane"
                ),
            );
        }
        for edge_use in &boundary.edges {
            let edge = &model.edges[edge_use.edge.0];
            let a = model.vertices[edge.vertices[0].0].position;
            let b = model.vertices[edge.vertices[1].0].position;
            let changing: Vec<_> = (0..3).filter(|&i| a[i] != b[i]).collect();
            if changing.len() != 1 || changing[0] == axis {
                return issue(
                    "unsupported_geometry",
                    format!("Face {index}: edges must be axis-aligned rectangle sides"),
                );
            }
            let direction_axis = changing[0];
            if edge.line.direction[direction_axis].abs() != 1.0
                || (0..3)
                    .filter(|&i| i != direction_axis)
                    .any(|i| edge.line.direction[i] != 0.0 || edge.line.origin[i] != a[i])
            {
                return issue("unsupported_geometry", format!("Edge {}: supporting line must coincide exactly with its axis-aligned endpoints", edge_use.edge.0));
            }
        }
        rectangles.push(Rectangle {
            axis,
            sign: if face.plane.normal[axis] > 0.0 { 1 } else { -1 },
            min,
            max,
            vertices,
            edges: boundary
                .edges
                .iter()
                .map(|edge_use| edge_use.edge)
                .collect(),
        });
    }

    let pairs = rectangles
        .len()
        .checked_mul(rectangles.len().saturating_sub(1))
        .and_then(|n| n.checked_div(2));
    if pairs.is_none_or(|pairs| pairs > MAX_FACE_PAIRS) {
        return issue(
            "resource_limit",
            format!("Geometric validation exceeds {MAX_FACE_PAIRS} face-pair checks"),
        );
    }
    for (i, a) in rectangles.iter().enumerate() {
        for (j, b) in rectangles.iter().enumerate().skip(i + 1) {
            let low: [f64; 3] = std::array::from_fn(|axis| a.min[axis].max(b.min[axis]));
            let high: [f64; 3] = std::array::from_fn(|axis| a.max[axis].min(b.max[axis]));
            if (0..3).any(|axis| low[axis] > high[axis]) {
                continue;
            }
            let dimension = (0..3).filter(|&axis| low[axis] < high[axis]).count();
            let legitimate = match dimension {
                // A rectangular face pair may meet only at a full shared edge
                // or shared vertex. Partial edges are unsupported T-junctions.
                0 => a.vertices.iter().any(|vertex| {
                    b.vertices.contains(vertex) && model.vertices[vertex.0].position == low
                }),
                1 => a.edges.iter().any(|edge_id| {
                    if !b.edges.contains(edge_id) {
                        return false;
                    }
                    let edge = &model.edges[edge_id.0];
                    let p = model.vertices[edge.vertices[0].0].position;
                    let q = model.vertices[edge.vertices[1].0].position;
                    (p == low && q == high) || (p == high && q == low)
                }),
                _ => false,
            };
            if !legitimate {
                return issue("geometric_intersection", format!("Faces {i} and {j} overlap, cross, or touch without a matching full topological edge/vertex"));
            }
        }
    }

    // Boundary intersections cannot detect one complete solid inside another.
    // Use a half-open projected rectangle convention so a ray along a tessellation
    // seam counts exactly one face. Boundary contacts were rejected above.
    for (i, solid) in model.solids.iter().enumerate() {
        let shell = &model.shells[solid.shells[0].0];
        let rectangle = &rectangles[shell.faces[0].0];
        let sample = model.vertices[rectangle.vertices[0].0].position;
        for (j, other) in model.solids.iter().enumerate() {
            if i == j {
                continue;
            }
            let other_shell = &model.shells[other.shells[0].0];
            let winding: i32 = other_shell
                .faces
                .iter()
                .map(|id| &rectangles[id.0])
                .filter(|face| {
                    face.axis == 0
                        && face.min[0] > sample[0]
                        && face.min[1] <= sample[1]
                        && sample[1] < face.max[1]
                        && face.min[2] <= sample[2]
                        && sample[2] < face.max[2]
                })
                .map(|face| face.sign)
                .sum();
            if winding != 0 {
                return issue("overlapping_solids", format!("Solid {i} lies inside solid {j}; separate solids must have disjoint interiors"));
            }
        }
    }
    Vec::new()
}
