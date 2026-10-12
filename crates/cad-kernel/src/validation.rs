//! Validation for closed orientable planar B-reps. In the orthogonal domain,
//! nonconvex solids, handles, and disconnected solids are supported. General
//! planar geometry is restricted to a single strictly convex solid or a validated
//! right prism with a simple outer profile and disjoint simple through holes.
//! Each solid currently requires one connected shell; enclosed cavities are
//! unsupported.
//! Geometric intersection and containment checks follow the topology checks.
use crate::math::{cross, dot, finite, norm, sub};
use crate::{ModelSnapshot, Tolerance};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[path = "geometric_validation.rs"]
pub(crate) mod geometry;

// Bound aggregate quadratic face-orientation work before following topology.
// A per-face vertex cap alone does not bound repeated/malformed face references.
const MAX_FACE_CONVEXITY_WORK: usize = 20_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

pub(crate) fn validate(model: &ModelSnapshot, tolerance: Tolerance) -> ValidationReport {
    if model
        .edges
        .iter()
        .any(|e| matches!(e.curve, crate::Curve::CircularArc(_)))
    {
        let result = crate::arc_prism::recognize(model, tolerance)
            .map(|_| ())
            .or_else(|_| crate::bored_prism::recognize(model, tolerance).map(|_| ()));
        return ValidationReport {
            valid: result.is_ok(),
            issues: result
                .err()
                .map(|e| ValidationIssue {
                    code: "invalid_arc_prism".into(),
                    message: e.to_string(),
                })
                .into_iter()
                .collect(),
        };
    }
    if model.has_curved_geometry() {
        let result = if tolerance.is_valid() {
            crate::analytic::recognize(model, Some(tolerance))
                .map(|_| ())
                .or_else(|_| crate::profile_prism::recognize(model, tolerance).map(|_| ()))
                .or_else(|_| crate::revolve::recognize(model, tolerance).map(|_| ()))
                .or_else(|_| crate::bored_prism::recognize(model, tolerance).map(|_| ()))
        } else {
            Err(crate::KernelError::InvalidTolerance)
        };
        return ValidationReport {
            valid: result.is_ok(),
            issues: result
                .err()
                .map(|error| ValidationIssue {
                    code: "invalid_analytic_geometry".into(),
                    message: error.to_string(),
                })
                .into_iter()
                .collect(),
        };
    }
    let mut issues = Vec::new();
    let mut issue = |code: &str, message: String| {
        issues.push(ValidationIssue {
            code: code.into(),
            message,
        });
    };
    if !tolerance.is_valid() {
        issue(
            "invalid_tolerance",
            "Linear tolerance must be finite and positive".into(),
        );
        return ValidationReport {
            valid: false,
            issues,
        };
    }
    if model.vertices.is_empty()
        && model.edges.is_empty()
        && model.loops.is_empty()
        && model.faces.is_empty()
        && model.shells.is_empty()
        && model.solids.is_empty()
    {
        return ValidationReport {
            valid: true,
            issues,
        };
    }
    if model.solids.is_empty()
        || model.shells.is_empty()
        || model.faces.len() < 4
        || model.vertices.len() < 4
    {
        issue(
            "unsupported_topology",
            "Expected a complete closed solid or a completely empty model".into(),
        );
    }
    let mut face_work = 0usize;
    for face in &model.faces {
        for loop_id in &face.loops {
            let Some(boundary) = model.loops.get(loop_id.0) else {
                continue;
            };
            let vertices = boundary.edges.len();
            let work = vertices
                .checked_mul(vertices)
                .and_then(|work| face_work.checked_add(work));
            if vertices > crate::MAX_POLYGON_POINTS
                || work.is_none_or(|work| work > MAX_FACE_CONVEXITY_WORK)
            {
                issue(
                    "resource_limit",
                    "Convex face checks exceed the per-face vertex or aggregate operation budget"
                        .into(),
                );
                return ValidationReport {
                    valid: false,
                    issues,
                };
            }
            face_work = work.expect("bounded work checked above");
        }
    }
    let mut vertex_owners = vec![0usize; model.vertices.len()];
    for (index, vertex) in model.vertices.iter().enumerate() {
        if !finite(vertex.position) {
            issue(
                "nonfinite_vertex",
                format!("Vertex {index} contains a nonfinite coordinate"),
            );
        }
    }
    for (index, edge) in model.edges.iter().enumerate() {
        let [a, b] = edge.vertices;
        for id in [a, b] {
            if let Some(count) = vertex_owners.get_mut(id.0) {
                *count += 1;
            }
        }
        let (Some(a), Some(b)) = (model.vertices.get(a.0), model.vertices.get(b.0)) else {
            issue(
                "invalid_vertex_reference",
                format!("Edge {index} references a missing vertex"),
            );
            continue;
        };
        let length = norm(sub(b.position, a.position));
        if !length.is_finite() || length <= tolerance.linear {
            issue(
                "degenerate_edge",
                format!("Edge {index} is nonfinite or shorter than tolerance"),
            );
        }
        let direction_norm = norm(edge.try_line().unwrap().direction);
        if !finite(edge.try_line().unwrap().origin)
            || !finite(edge.try_line().unwrap().direction)
            || (direction_norm - 1.0).abs() > 1.0e-12
        {
            issue(
                "invalid_line",
                format!("Edge {index} needs a finite line with a unit direction"),
            );
        }
        for vertex in [a, b] {
            let distance = norm(cross(
                sub(vertex.position, edge.try_line().unwrap().origin),
                edge.try_line().unwrap().direction,
            ));
            if !distance.is_finite() || distance > tolerance.linear {
                issue(
                    "edge_geometry_mismatch",
                    format!("Edge {index} endpoint is not on its supporting line"),
                );
            }
        }
    }
    if vertex_owners.contains(&0) {
        issue(
            "unused_vertex",
            "Every vertex must belong to an edge".into(),
        );
    }
    let mut edge_uses: Vec<Vec<(usize, bool)>> = vec![Vec::new(); model.edges.len()];
    for (index, boundary) in model.loops.iter().enumerate() {
        if boundary.edges.len() < 3 {
            issue(
                "degenerate_loop",
                format!("Loop {index} needs at least three edges"),
            );
        }
        let mut distinct_edges = BTreeSet::new();
        let mut distinct_vertices = BTreeSet::new();
        for (position, edge_use) in boundary.edges.iter().enumerate() {
            if !distinct_edges.insert(edge_use.edge) {
                issue("repeated_edge", format!("Loop {index} repeats an edge"));
            }
            if let Some(uses) = edge_uses.get_mut(edge_use.edge.0) {
                uses.push((index, edge_use.reversed));
            }
            let Some([start, end]) = model.endpoints(*edge_use) else {
                issue(
                    "invalid_edge_reference",
                    format!("Loop {index} references a missing edge"),
                );
                continue;
            };
            if !distinct_vertices.insert(start) {
                issue("repeated_vertex", format!("Loop {index} repeats a vertex"));
            }
            let next = boundary.edges[(position + 1) % boundary.edges.len()];
            if let Some([next_start, _]) = model.endpoints(next) {
                if end != next_start {
                    issue(
                        "open_loop",
                        format!("Loop {index} has disconnected consecutive edges"),
                    );
                }
            }
        }
    }
    for (index, uses) in edge_uses.iter().enumerate() {
        if uses.len() != 2 {
            issue(
                "edge_incidence",
                format!("Edge {index} has {} uses; expected two", uses.len()),
            );
        } else if uses[0].1 == uses[1].1 {
            issue(
                "edge_orientation",
                format!("Edge {index} uses must have opposite orientations"),
            );
        }
    }
    let mut loop_owners = vec![Vec::new(); model.loops.len()];
    for (index, face) in model.faces.iter().enumerate() {
        if face.loops.is_empty() || face.loops.len() > crate::MAX_POLYGON_HOLES + 1 {
            issue(
                "unsupported_face",
                format!(
                    "Face {index}: one simple outer loop and at most 32 convex inner loops are supported"
                ),
            );
        }
        let normal_norm = norm(face.try_plane().unwrap().normal);
        if !finite(face.try_plane().unwrap().origin)
            || !finite(face.try_plane().unwrap().normal)
            || (normal_norm - 1.0).abs() > 1.0e-12
        {
            issue(
                "invalid_plane",
                format!("Face {index} needs a finite plane with a unit normal"),
            );
        }
        for (boundary_index, &loop_id) in face.loops.iter().enumerate() {
            let orientation = if boundary_index == 0 { 1.0 } else { -1.0 };
            let Some(owners) = loop_owners.get_mut(loop_id.0) else {
                issue(
                    "invalid_loop_reference",
                    format!("Face {index} references a missing loop"),
                );
                continue;
            };
            owners.push(index);
            let Some(points) = model.loop_points(loop_id) else {
                continue;
            };
            for &point in &points {
                let distance = dot(
                    sub(point, face.try_plane().unwrap().origin),
                    face.try_plane().unwrap().normal,
                )
                .abs();
                if !distance.is_finite() || distance > tolerance.linear {
                    issue(
                        "face_geometry_mismatch",
                        format!("Face {index} boundary is not on its plane"),
                    );
                    break;
                }
            }
            if points.len() < 3 {
                continue;
            }
            if points.len() > crate::MAX_POLYGON_POINTS {
                issue(
                    "resource_limit",
                    format!("Face {index} exceeds the polygon vertex limit"),
                );
                continue;
            }
            let convex = crate::simple_polygon::convex_ring(
                &points,
                face.try_plane().unwrap().normal,
                orientation,
            );
            if !convex {
                let direction = sub(points[1], points[0]);
                let x = direction.map(|v| v / norm(direction));
                let y = cross(face.try_plane().unwrap().normal, x);
                let mut projected: Vec<_> = points
                    .iter()
                    .map(|p| {
                        let d = sub(*p, points[0]);
                        [dot(d, x), dot(d, y)]
                    })
                    .collect();
                if boundary_index > 0 {
                    projected.reverse();
                }
                if let Err(error) = crate::simple_polygon::validate_ring(&projected, tolerance) {
                    issue(
                        "face_orientation_or_degeneracy",
                        format!("Face {index}: {error}"),
                    );
                }
            }
            let mut area_twice = 0.0;
            for triangle in points[1..].windows(2) {
                let triangle_area_twice = dot(
                    cross(sub(triangle[0], points[0]), sub(triangle[1], points[0])),
                    face.try_plane().unwrap().normal,
                );
                if triangle_area_twice != 0.0 && !(triangle_area_twice / 2.0).is_normal() {
                    issue(
                        "numeric_range",
                        format!("Face {index} triangle area is outside the normal f64 range"),
                    );
                }
                area_twice += triangle_area_twice;
            }
            if !(area_twice / 2.0).is_normal() || area_twice * orientation <= 0.0 {
                issue(
                    "degenerate_face",
                    format!("Face {index} needs positive oriented area in the normal f64 range"),
                );
            }
        }
    }
    for (index, owners) in loop_owners.iter().enumerate() {
        if owners.len() != 1 {
            issue(
                "loop_ownership",
                format!("Loop {index} must belong to exactly one face"),
            );
        }
    }
    let mut face_owners = vec![0usize; model.faces.len()];
    for (index, shell) in model.shells.iter().enumerate() {
        for face in &shell.faces {
            if let Some(count) = face_owners.get_mut(face.0) {
                *count += 1;
            } else {
                issue(
                    "invalid_face_reference",
                    format!("Shell {index} references a missing face"),
                );
            }
        }
    }
    if face_owners.iter().any(|&count| count != 1) {
        issue(
            "face_ownership",
            "Every face must belong to exactly one shell".into(),
        );
    }
    let mut shell_owners = vec![0usize; model.shells.len()];
    for (index, solid) in model.solids.iter().enumerate() {
        if solid.shells.len() != 1 {
            issue(
                "unsupported_solid",
                format!("Solid {index} requires exactly one shell"),
            );
        }
        for shell in &solid.shells {
            if let Some(count) = shell_owners.get_mut(shell.0) {
                *count += 1;
            } else {
                issue(
                    "invalid_shell_reference",
                    format!("Solid {index} references a missing shell"),
                );
            }
        }
    }
    if shell_owners.iter().any(|&count| count != 1) {
        issue(
            "shell_ownership",
            "Every shell must belong to exactly one solid".into(),
        );
    }
    // The incidence graph is meaningful only after references, ownership and
    // local geometry pass. In particular, a reused loop could otherwise expand
    // one edge's incident-face list quadratically despite already being invalid.
    if !issues.is_empty() {
        return ValidationReport {
            valid: false,
            issues,
        };
    }
    let mut issue = |code: &str, message: String| {
        issues.push(ValidationIssue {
            code: code.into(),
            message,
        });
    };
    let mut adjacency = vec![BTreeSet::new(); model.faces.len()];
    let mut vertex_faces = vec![BTreeSet::new(); model.vertices.len()];
    let mut vertex_links = vec![Vec::new(); model.vertices.len()];
    for (edge_index, uses) in edge_uses.iter().enumerate() {
        let incident: Vec<usize> = uses
            .iter()
            .flat_map(|(loop_id, _)| loop_owners[*loop_id].iter().copied())
            .collect();
        for &a in &incident {
            for &b in &incident {
                if a != b {
                    adjacency[a].insert(b);
                }
            }
        }
        for endpoint in model.edges[edge_index].vertices {
            if let Some(faces) = vertex_faces.get_mut(endpoint.0) {
                faces.extend(incident.iter().copied());
                if incident.len() == 2 {
                    vertex_links[endpoint.0].push((incident[0], incident[1]));
                }
            }
        }
    }
    // A closed edge-manifold can still pinch at a vertex. Its incident faces
    // must form one connected circular link, never two shells touching at a point.
    for (vertex, faces) in vertex_faces.iter().enumerate() {
        let mut visited = BTreeSet::new();
        let mut pending: Vec<usize> = faces.first().copied().into_iter().collect();
        while let Some(face) = pending.pop() {
            if !visited.insert(face) {
                continue;
            }
            for &(a, b) in &vertex_links[vertex] {
                if a == face {
                    pending.push(b);
                }
                if b == face {
                    pending.push(a);
                }
            }
        }
        if visited.len() != faces.len() {
            issue(
                "nonmanifold_vertex",
                format!("Vertex {vertex} has disconnected incident face fans"),
            );
        }
    }
    for (shell_index, shell) in model.shells.iter().enumerate() {
        let members: BTreeSet<usize> = shell.faces.iter().map(|id| id.0).collect();
        let mut visited = BTreeSet::new();
        let mut pending: Vec<usize> = members.first().copied().into_iter().collect();
        while let Some(face) = pending.pop() {
            if !members.contains(&face) || !visited.insert(face) {
                continue;
            }
            if let Some(neighbors) = adjacency.get(face) {
                pending.extend(neighbors);
            }
        }
        if visited.len() != members.len() || members.is_empty() {
            issue(
                "disconnected_shell",
                format!("Shell {shell_index} faces must be connected through shared edges"),
            );
        }
        let mut edges = BTreeSet::new();
        let mut vertices = BTreeSet::new();
        for face_id in &shell.faces {
            let Some(face) = model.faces.get(face_id.0) else {
                continue;
            };
            for loop_id in &face.loops {
                let Some(boundary) = model.loops.get(loop_id.0) else {
                    continue;
                };
                for edge_use in &boundary.edges {
                    edges.insert(edge_use.edge);
                    if let Some(edge) = model.edges.get(edge_use.edge.0) {
                        vertices.extend(edge.vertices);
                    }
                    if let Some(uses) = edge_uses.get(edge_use.edge.0) {
                        for (other_loop, _) in uses {
                            for owner in &loop_owners[*other_loop] {
                                if !members.contains(owner) {
                                    issue(
                                        "shell_incidence",
                                        format!(
                                            "Shell {shell_index} shares an edge with another shell"
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        let face_characteristic: i128 = members
            .iter()
            .map(|&id| 2 - model.faces[id].loops.len() as i128)
            .sum();
        let characteristic = vertices.len() as i128 - edges.len() as i128 + face_characteristic;
        if characteristic > 2 || characteristic % 2 != 0 {
            issue(
                "euler_characteristic",
                format!(
                    "Shell {shell_index} must have orientable closed-surface characteristic 2 - 2g"
                ),
            );
        }
        let volume = model.faces_volume(&shell.faces);
        if !volume.is_normal() || volume <= 0.0 {
            issue(
                "invalid_volume",
                format!("Shell {shell_index} must enclose positive volume in the normal f64 range"),
            );
        }
    }
    let volume = model.volume();
    if !volume.is_normal() || volume <= 0.0 {
        issue(
            "invalid_volume",
            "Oriented boundary must enclose positive volume in the normal f64 range".into(),
        );
    }
    if issues.is_empty() {
        issues.extend(geometry::validate(model, tolerance));
    }
    ValidationReport {
        valid: issues.is_empty(),
        issues,
    }
}
