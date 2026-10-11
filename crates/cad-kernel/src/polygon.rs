//! Native planar convex polygon profiles and authoritative polygonal cap faces.
use crate::math::{norm, sub};
use crate::*;

pub const MAX_POLYGON_POINTS: usize = 256;

/// Validate a simple strictly convex counterclockwise XY polygon in millimetres.
/// Repeated/collinear points and uncertain turns at the modeling tolerance are
/// rejected; the profile is never silently reordered or repaired.
pub fn validate_polygon(points: &[[f64; 2]], tolerance: Tolerance) -> Result<(), KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !(3..=MAX_POLYGON_POINTS).contains(&points.len()) {
        return Err(KernelError::Unsupported(format!(
            "polygon requires 3..={MAX_POLYGON_POINTS} vertices"
        )));
    }
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(KernelError::NumericRange);
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let edge = [b[0] - a[0], b[1] - a[1]];
        let length = edge[0].hypot(edge[1]);
        if !length.is_normal() || length <= tolerance.linear {
            return Err(KernelError::Unsupported(
                "polygon edges must exceed modeling tolerance".into(),
            ));
        }
        for (j, p) in points.iter().enumerate() {
            if j == i || j == (i + 1) % points.len() {
                continue;
            }
            let delta = [p[0] - a[0], p[1] - a[1]];
            let left = edge[0] * delta[1];
            let right = edge[1] * delta[0];
            let determinant = left - right;
            let uncertainty = (left.abs() + right.abs()) * (16.0 * f64::EPSILON);
            if !determinant.is_normal() || !uncertainty.is_finite() {
                return Err(KernelError::NumericRange);
            }
            if determinant <= tolerance.linear * length + uncertainty {
                return Err(KernelError::Unsupported("polygon must be simple, strictly convex, counterclockwise, and separated from collinearity by tolerance".into()));
            }
        }
    }
    Ok(())
}

/// Extrude a strictly convex CCW polygon along +Z, with polygonal cap topology.
/// Coordinates and height are millimetres. Supports translation/export; the
/// Boolean union/difference still reject nonorthogonal input explicitly;
/// intersection also supports bounded single convex planar solids.
pub fn extrude_polygon(
    points: &[[f64; 2]],
    height: f64,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_polygon(points, tolerance)?;
    if !height.is_finite() || height <= tolerance.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "height",
        });
    }
    let n = points.len();
    let positions: Vec<_> = [0.0, height]
        .into_iter()
        .flat_map(|z| points.iter().map(move |p| [p[0], p[1], z]))
        .collect();
    let mut model = ModelSnapshot {
        vertices: positions
            .iter()
            .map(|&position| Vertex { position })
            .collect(),
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    };
    let mut rings = vec![(0..n).rev().collect::<Vec<_>>(), (n..2 * n).collect()];
    let mut normals = vec![[0.0, 0.0, -1.0], [0.0, 0.0, 1.0]];
    for i in 0..n {
        let j = (i + 1) % n;
        rings.push(vec![i, j, j + n, i + n]);
        let delta = sub(positions[j], positions[i]);
        let length = norm(delta);
        normals.push([delta[1] / length, -delta[0] / length, 0.0]);
    }
    let mut edges = BTreeMap::new();
    for (ring, normal) in rings.iter().zip(normals) {
        let mut uses = Vec::new();
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            let key = (a.min(b), a.max(b));
            let edge = *edges.entry(key).or_insert_with(|| {
                let id = EdgeId(model.edges.len());
                let delta = sub(positions[key.1], positions[key.0]);
                let length = norm(delta);
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
                edge,
                reversed: a != key.0,
            });
        }
        let loop_id = LoopId(model.loops.len());
        model.loops.push(Loop { edges: uses });
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: positions[ring[0]],
                normal,
            }),
            loops: vec![loop_id],
        });
    }
    model.shells.push(Shell {
        faces: (0..n + 2).map(FaceId).collect(),
    });
    model.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let mut generated_faces = BTreeMap::from([
        ("start_cap".into(), vec![FaceId(0)]),
        ("end_cap".into(), vec![FaceId(1)]),
        ("walls".into(), (2..n + 2).map(FaceId).collect()),
    ]);
    for i in 0..n {
        generated_faces.insert(format!("wall_{i}"), vec![FaceId(i + 2)]);
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces,
            input_faces: BTreeMap::new(),
        },
    })
}
