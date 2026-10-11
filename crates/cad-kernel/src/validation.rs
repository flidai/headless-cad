//! Validation for closed orientable orthogonal B-reps with rectangular faces.
//! Nonconvex solids, handles, and disconnected solids are supported. Each solid
//! currently requires one connected shell; enclosed cavities are unsupported.
//! Geometric intersection and containment checks follow the topology checks.
use crate::math::{cross, dot, finite, norm, sub};
use crate::{ModelSnapshot, Tolerance};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[path = "geometric_validation.rs"]
mod geometry;

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
        let direction_norm = norm(edge.line.direction);
        if !finite(edge.line.origin)
            || !finite(edge.line.direction)
            || (direction_norm - 1.0).abs() > 1.0e-12
        {
            issue(
                "invalid_line",
                format!("Edge {index} needs a finite line with a unit direction"),
            );
        }
        for vertex in [a, b] {
            let distance = norm(cross(
                sub(vertex.position, edge.line.origin),
                edge.line.direction,
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
        if face.loops.len() != 1 {
            issue(
                "unsupported_face",
                format!("Face {index}: only one convex outer loop is supported"),
            );
        }
        let normal_norm = norm(face.plane.normal);
        if !finite(face.plane.origin)
            || !finite(face.plane.normal)
            || (normal_norm - 1.0).abs() > 1.0e-12
        {
            issue(
                "invalid_plane",
                format!("Face {index} needs a finite plane with a unit normal"),
            );
        }
        for &loop_id in &face.loops {
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
                let distance = dot(sub(point, face.plane.origin), face.plane.normal).abs();
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
            // For a convex face, every other boundary vertex must lie on the
            // interior side of every oriented edge. This also rejects star rings.
            for i in 0..points.len() {
                let a = points[i];
                let direction = sub(points[(i + 1) % points.len()], a);
                let length = norm(direction);
                let inward = cross(face.plane.normal, direction.map(|v| v / length));
                for (j, &point) in points.iter().enumerate() {
                    if j == i || j == (i + 1) % points.len() {
                        continue;
                    }
                    let side = dot(sub(point, a), inward);
                    if !side.is_finite() || side <= 0.0 {
                        issue(
                            "face_orientation_or_degeneracy",
                            format!("Face {index} is not strictly convex and outward-oriented"),
                        );
                        break;
                    }
                }
            }
            let mut area_twice = 0.0;
            for triangle in points[1..].windows(2) {
                let triangle_area_twice = dot(
                    cross(sub(triangle[0], points[0]), sub(triangle[1], points[0])),
                    face.plane.normal,
                );
                if !(triangle_area_twice / 2.0).is_normal() {
                    issue(
                        "numeric_range",
                        format!("Face {index} triangle area is outside the normal f64 range"),
                    );
                }
                area_twice += triangle_area_twice;
            }
            if !(area_twice / 2.0).is_normal() || area_twice <= 0.0 {
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
        let characteristic = vertices.len() as i128 - edges.len() as i128 + members.len() as i128;
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
        issues.extend(geometry::validate(model));
    }
    ValidationReport {
        valid: issues.is_empty(),
        issues,
    }
}
