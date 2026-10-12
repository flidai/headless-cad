//! Bounded analytic half-space intersection. No tessellation, snapping, or weld.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::BTreeSet;

const MAX_PLANES: usize = 48;
#[derive(Clone)]
struct Support {
    origin: [f64; 3],
    normal: [f64; 3],
    sources: Vec<(usize, FaceId)>,
}
fn unsupported(message: &str) -> KernelError {
    KernelError::Unsupported(message.into())
}
fn empty() -> ModelSnapshot {
    ModelSnapshot {
        vertices: vec![],
        edges: vec![],
        loops: vec![],
        faces: vec![],
        shells: vec![],
        solids: vec![],
    }
}
fn result(model: ModelSnapshot, maps: [BTreeMap<FaceId, Vec<FaceId>>; 2]) -> OperationResult {
    let [left, right] = maps;
    let generated_faces = BTreeMap::from([(
        "boundary".into(),
        (0..model.faces.len()).map(FaceId).collect(),
    )]);
    OperationResult {
        model,
        history: TopologyHistory {
            generated_faces,
            input_faces: BTreeMap::from([("left".into(), left), ("right".into(), right)]),
        },
    }
}

pub(crate) fn intersect(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    let maps = [a, b].map(|m| {
        (0..m.faces.len())
            .map(|i| (FaceId(i), vec![]))
            .collect::<BTreeMap<_, _>>()
    });
    if a.vertices.is_empty() || b.vertices.is_empty() {
        return Ok(result(empty(), maps));
    }
    for model in [a, b] {
        if !validation::geometry::validate_convex(model, tol).is_empty() {
            return Err(unsupported(
                "nonorthogonal intersection requires two single strictly convex solids",
            ));
        }
    }
    if a.faces.len() + b.faces.len() > MAX_PLANES {
        return Err(KernelError::ResourceLimit(
            "convex intersection supports at most 48 input planes".into(),
        ));
    }
    // Work relative to a native vertex to reduce cancellation at large offsets.
    let reference = a.vertices[0].position;
    let scale = a
        .vertices
        .iter()
        .chain(&b.vertices)
        .flat_map(|v| sub(v.position, reference))
        .map(f64::abs)
        .fold(0., f64::max);
    let epsilon = scale * (128. * f64::EPSILON);
    if !epsilon.is_normal() || epsilon > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let mut planes: Vec<Support> = vec![];
    for (operand, model) in [a, b].into_iter().enumerate() {
        for (i, face) in model.faces.iter().enumerate() {
            let origin = sub(face.try_plane().unwrap().origin, reference);
            let normal = face.try_plane().unwrap().normal;
            let mut found = None;
            for (j, plane) in planes.iter().enumerate() {
                if norm(sub(normal, plane.normal)) <= 32. * f64::EPSILON {
                    let gap = dot(normal, sub(origin, plane.origin)).abs();
                    if normal == plane.normal && gap == 0.0 {
                        found = Some(j);
                        break;
                    }
                    if gap <= tol.linear {
                        return Err(unsupported(
                            "supporting planes are within unresolved modeling tolerance",
                        ));
                    }
                }
            }
            if let Some(j) = found {
                planes[j].sources.push((operand, FaceId(i)));
            } else {
                planes.push(Support {
                    origin,
                    normal,
                    sources: vec![(operand, FaceId(i))],
                });
            }
        }
    }
    reconstruct(planes, reference, epsilon, tol, maps, None)
}

fn reconstruct(
    mut planes: Vec<Support>,
    reference: [f64; 3],
    epsilon: f64,
    tol: Tolerance,
    mut maps: [BTreeMap<FaceId, Vec<FaceId>>; 2],
    bounds: Option<[[f64; 3]; 2]>,
) -> Result<OperationResult, KernelError> {
    if planes.len() > MAX_PLANES {
        return Err(KernelError::ResourceLimit(
            "convex reconstruction supports at most 48 planes".into(),
        ));
    }
    // Canonical geometric ordering makes reconstruction independent of operand order.
    planes.sort_by(|a, b| {
        a.normal
            .into_iter()
            .chain(a.origin)
            .zip(b.normal.into_iter().chain(b.origin))
            .map(|(x, y)| x.total_cmp(&y))
            .find(|o| !o.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let offsets: Vec<_> = planes.iter().map(|p| dot(p.normal, p.origin)).collect();
    let mut candidates: BTreeMap<Vec<usize>, [f64; 3]> = BTreeMap::new();
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            for k in j + 1..planes.len() {
                let [n1, n2, n3] = [planes[i].normal, planes[j].normal, planes[k].normal];
                // Exact equal/opposite unit normals prove dependence without
                // evaluating a cancellation-prone scalar triple product.
                if [(n1, n2), (n1, n3), (n2, n3)]
                    .into_iter()
                    .any(|(a, b)| a == b || a == b.map(|v| -v))
                {
                    continue;
                }
                let det = dot(n1, cross(n2, n3));
                if det == 0. {
                    continue;
                }
                let c23 = cross(n2, n3);
                let c31 = cross(n3, n1);
                let c12 = cross(n1, n2);
                let numerators: [f64; 3] = std::array::from_fn(|axis| {
                    offsets[i] * c23[axis] + offsets[j] * c31[axis] + offsets[k] * c12[axis]
                });
                // A chamfer is contained in its input bounds. Cramer's numerator
                // must equal det*x for any feasible point. Exclude inconsistent
                // triples without dividing by a near-zero determinant. The wide
                // arithmetic envelope covers unit-normal products, local plane
                // offsets and determinant error over the bounded coordinates.
                // This does not declare nearly dependent planes coincident.
                if bounds.is_some_and(|[lo, hi]| {
                    (0..3).any(|axis| {
                        let a = det * (lo[axis] - tol.linear);
                        let b = det * (hi[axis] + tol.linear);
                        numerators[axis] < a.min(b) - epsilon * 64.
                            || numerators[axis] > a.max(b) + epsilon * 64.
                    })
                }) {
                    continue;
                }
                if det.abs() <= 128. * f64::EPSILON {
                    return Err(unsupported(
                        "near-dependent supporting planes exceed reliable intersection conditioning",
                    ));
                }
                // Cramer's rule amplifies coefficient and arithmetic error by
                // inverse determinant. Leave a wide margin below model tolerance;
                // residual checks alone cannot detect an ill-conditioned solve.
                let solve_error = epsilon * 8.0 / det.abs();
                if !solve_error.is_normal() || solve_error > tol.linear / 16.0 {
                    return Err(unsupported(
                        "plane intersection conditioning exceeds modeling tolerance",
                    ));
                }
                let p = numerators.map(|value| value / det);
                if !p.into_iter().all(f64::is_finite) {
                    return Err(KernelError::NumericRange);
                }
                let distances: Vec<_> = planes
                    .iter()
                    .map(|plane| dot(plane.normal, sub(p, plane.origin)))
                    .collect();
                if distances.iter().any(|&d| d > tol.linear + solve_error) {
                    continue;
                }
                if distances.iter().any(|d| !d.is_finite()) {
                    return Err(KernelError::NumericRange);
                }
                let mut active = Vec::new();
                for (index, &distance) in distances.iter().enumerate() {
                    if [i, j, k].contains(&index) {
                        if distance.abs() > solve_error {
                            return Err(KernelError::NumericRange);
                        }
                        active.push(index);
                    } else if distance == 0.0 {
                        active.push(index);
                    } else if distance.abs() <= tol.linear + solve_error {
                        // Only the generating triplet has known incidence. A
                        // nearby fourth plane is not welded into that vertex.
                        return Err(unsupported(
                            "intersection vertex has unresolved incidence with another supporting plane",
                        ));
                    }
                }
                if let Some(old) = candidates.get(&active) {
                    if norm(sub(*old, p)) > epsilon * 4. {
                        return Err(KernelError::NumericRange);
                    }
                } else {
                    candidates.insert(active, p);
                }
            }
        }
    }
    if candidates.is_empty() {
        return Ok(result(empty(), maps));
    }
    if candidates.len() < 4 {
        return Err(unsupported("intersection is tangent or lower-dimensional"));
    }
    let entries: Vec<_> = candidates.into_iter().collect();
    let mut model = empty();
    for (_, p) in &entries {
        let world = std::array::from_fn(|i| p[i] + reference[i]);
        if !world.into_iter().all(f64::is_finite)
            || norm(sub(sub(world, reference), *p)) > tol.linear
        {
            return Err(KernelError::NumericRange);
        }
        model.vertices.push(Vertex { position: world });
    }
    let mut edges = BTreeMap::new();
    for (plane_index, plane) in planes.iter().enumerate() {
        let mut ring: Vec<_> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, (active, _))| active.contains(&plane_index).then_some(i))
            .collect();
        if ring.len() < 3 {
            continue;
        }
        if ring.len() > MAX_POLYGON_POINTS {
            return Err(KernelError::ResourceLimit(
                "intersection face vertex limit".into(),
            ));
        }
        let center: [f64; 3] = std::array::from_fn(|axis| {
            ring.iter()
                .map(|&i| entries[i].1[axis] / ring.len() as f64)
                .sum()
        });
        let axis = (0..3)
            .min_by(|&i, &j| plane.normal[i].abs().total_cmp(&plane.normal[j].abs()))
            .unwrap();
        let mut unit = [0.; 3];
        unit[axis] = 1.;
        let u = cross(unit, plane.normal);
        let v = cross(plane.normal, u);
        ring.sort_by(|&a, &b| {
            let a = sub(entries[a].1, center);
            let b = sub(entries[b].1, center);
            dot(a, v)
                .atan2(dot(a, u))
                .total_cmp(&dot(b, v).atan2(dot(b, u)))
        });
        let mut uses = vec![];
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let key = (a.min(b), a.max(b));
            let id = *edges.entry(key).or_insert_with(|| {
                let id = EdgeId(model.edges.len());
                let origin = model.vertices[key.0].position;
                let delta = sub(model.vertices[key.1].position, origin);
                let length = norm(delta);
                model.edges.push(Edge {
                    vertices: [VertexId(key.0), VertexId(key.1)],
                    curve: Curve::Line(Line {
                        origin,
                        direction: delta.map(|x| x / length),
                    }),
                });
                id
            });
            uses.push(EdgeUse {
                edge: id,
                reversed: a != key.0,
            });
        }
        let face = FaceId(model.faces.len());
        let loop_id = LoopId(model.loops.len());
        model.loops.push(Loop { edges: uses });
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: model.vertices[ring[0]].position,
                normal: plane.normal,
            }),
            loops: vec![loop_id],
        });
        for &(operand, source) in &plane.sources {
            maps[operand].get_mut(&source).unwrap().push(face);
        }
    }
    if model.faces.len() < 4 {
        return Err(unsupported("intersection is tangent or lower-dimensional"));
    }
    model.shells.push(Shell {
        faces: (0..model.faces.len()).map(FaceId).collect(),
    });
    model.solids.push(Solid {
        shells: vec![ShellId(0)],
    });
    let report = model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    Ok(result(model, maps))
}

/// Equal face-distance chamfer of one straight edge of a strictly convex solid.
/// Supplied directions are outward unit normals in the input model frame.
pub fn chamfer(
    body: &ModelSnapshot,
    face_normals: [[f64; 3]; 2],
    distance: f64,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !distance.is_finite() || distance <= tol.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "chamfer distance",
        });
    }
    let validation = body.validate(tol);
    if !validation.valid {
        return Err(KernelError::InvalidInput(validation));
    }
    if body.vertices.is_empty() || !validation::geometry::validate_convex(body, tol).is_empty() {
        return Err(unsupported(
            "chamfer requires one strictly convex planar solid",
        ));
    }
    if body.faces.len() + 1 > MAX_PLANES {
        return Err(KernelError::ResourceLimit(
            "chamfer supports at most 48 planes including new face".into(),
        ));
    }
    let mut selected = vec![];
    for normal in face_normals {
        if normal.iter().any(|v| !v.is_finite()) || (norm(normal) - 1.).abs() > 128. * f64::EPSILON
        {
            return Err(unsupported(
                "chamfer face directions must be finite unit outward normals",
            ));
        }
        let matches: Vec<_> = body
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.try_plane()
                    .is_some_and(|p| norm(sub(p.normal, normal)) <= 128. * f64::EPSILON)
            })
            .map(|(i, _)| FaceId(i))
            .collect();
        if matches.len() != 1 {
            return Err(unsupported(
                "chamfer direction must resolve exactly one planar face",
            ));
        }
        selected.push(matches[0]);
    }
    if selected[0] == selected[1] {
        return Err(unsupported("chamfer requires two distinct adjacent faces"));
    }
    let edges = |face: FaceId| {
        body.faces[face.0]
            .loops
            .iter()
            .flat_map(|l| body.loops[l.0].edges.iter().map(|e| e.edge))
            .collect::<BTreeSet<_>>()
    };
    let shared: Vec<_> = edges(selected[0])
        .intersection(&edges(selected[1]))
        .copied()
        .collect();
    if shared.len() != 1 {
        return Err(unsupported(
            "chamfer faces must share exactly one straight edge",
        ));
    }
    let edge = &body.edges[shared[0].0];
    if !matches!(edge.curve, Curve::Line(_)) {
        return Err(unsupported("chamfer requires a straight edge"));
    }
    let normals = selected
        .iter()
        .map(|f| body.faces[f.0].try_plane().unwrap().normal)
        .collect::<Vec<_>>();
    let cosine = dot(normals[0], normals[1]);
    let sum = std::array::from_fn(|i| normals[0][i] + normals[1][i]);
    let length = norm(sum);
    let sine_half = ((1. - cosine) / 2.).sqrt();
    if !sine_half.is_finite() || sine_half <= 1e-6 || length <= 1e-6 {
        return Err(unsupported(
            "chamfer dihedral is unresolved or ill-conditioned",
        ));
    }
    let normal = sum.map(|v| v / length);
    let setback = distance * sine_half;
    let reference = body.vertices[edge.vertices[0].0].position;
    let scale = body
        .vertices
        .iter()
        .flat_map(|v| sub(v.position, reference))
        .map(f64::abs)
        .fold(distance, f64::max);
    // Public snapshots may use distant tangential plane origins. Include their
    // local coordinate magnitude in the offset dot-product error budget; vertex
    // bounds alone do not bound cancellation in dot(normal, plane.origin).
    // Reject unresolved representations rather than silently reanchoring them.
    let scale = body
        .faces
        .iter()
        .flat_map(|face| sub(face.try_plane().unwrap().origin, reference))
        .map(f64::abs)
        .fold(scale, f64::max);
    let epsilon = scale * 128. * f64::EPSILON;
    let world_scale = body
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .map(f64::abs)
        .fold(scale, f64::max);
    let error = (epsilon + world_scale * 128. * f64::EPSILON) / length.min(sine_half);
    if !epsilon.is_normal()
        || !error.is_normal()
        || error > tol.linear / 16.
        || !setback.is_normal()
    {
        return Err(KernelError::NumericRange);
    }
    for (id, vertex) in body.vertices.iter().enumerate() {
        let signed = dot(normal, sub(vertex.position, reference)) + setback;
        if !signed.is_finite() {
            return Err(KernelError::NumericRange);
        }
        if edge.vertices.contains(&VertexId(id)) {
            if signed <= tol.linear + error {
                return Err(unsupported("chamfer setback is unresolved"));
            }
        } else if signed >= -tol.linear - error {
            return Err(unsupported(
                "chamfer reaches another vertex or topology event",
            ));
        }
    }
    let mut planes: Vec<_> = body
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let p = f.try_plane().unwrap();
            Support {
                origin: sub(p.origin, reference),
                normal: p.normal,
                sources: vec![(0, FaceId(i))],
            }
        })
        .collect();
    planes.push(Support {
        origin: normal.map(|v| -setback * v),
        normal,
        sources: vec![(1, FaceId(0))],
    });
    let maps = [
        (0..body.faces.len()).map(|i| (FaceId(i), vec![])).collect(),
        BTreeMap::from([(FaceId(0), vec![])]),
    ];
    let lo = std::array::from_fn(|axis| {
        body.vertices
            .iter()
            .map(|v| v.position[axis] - reference[axis])
            .fold(f64::INFINITY, f64::min)
    });
    let hi = std::array::from_fn(|axis| {
        body.vertices
            .iter()
            .map(|v| v.position[axis] - reference[axis])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let mut result = reconstruct(planes, reference, epsilon, tol, maps, Some([lo, hi]))?;
    let retained = result.history.input_faces.remove("left").unwrap();
    let generated = result
        .history
        .input_faces
        .remove("right")
        .unwrap()
        .remove(&FaceId(0))
        .unwrap();
    if retained.values().any(|v| v.len() != 1)
        || generated.len() != 1
        || result.model.faces.len() != body.faces.len() + 1
    {
        return Err(unsupported("chamfer failed local-edge topology contract"));
    }
    result.history.input_faces.insert("body".into(), retained);
    result
        .history
        .generated_faces
        .insert("chamfer".into(), generated);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chamfer_box_volume_lineage_and_rigid_covariance() {
        let t = Tolerance::default();
        let b = extrude_rectangle(10., 12., 8., t).unwrap().model;
        let out = chamfer(&b, [[-1., 0., 0.], [0., -1., 0.]], 2., t).unwrap();
        assert!((out.model.mass_properties(t).unwrap().volume - 944.).abs() < 1e-9);
        assert_eq!(out.history.generated_faces["chamfer"].len(), 1);
        assert_eq!(out.history.input_faces["body"].len(), 6);
        assert!(out.history.input_faces["body"]
            .values()
            .all(|v| v.len() == 1));
        assert_eq!(
            out.model,
            chamfer(&b, [[0., -1., 0.], [-1., 0., 0.]], 2., t)
                .unwrap()
                .model
        );
        let moved = transform(&b, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
            .unwrap()
            .model;
        let rotate = |v: [f64; 3]| {
            [
                (v[0] + v[1]) / 2. + 0.5_f64.sqrt() * v[2],
                (v[0] + v[1]) / 2. - 0.5_f64.sqrt() * v[2],
                0.5_f64.sqrt() * (v[1] - v[0]),
            ]
        };
        let rotated = chamfer(
            &moved,
            [rotate([-1., 0., 0.]), rotate([0., -1., 0.])],
            2.,
            t,
        )
        .unwrap();
        assert!((rotated.model.mass_properties(t).unwrap().volume - 944.).abs() < 1e-8);
        let angle = 0.41_f64;
        let axis = [1. / 14_f64.sqrt(), 2. / 14_f64.sqrt(), 3. / 14_f64.sqrt()];
        let quaternion = [
            axis[0] * angle.sin(),
            axis[1] * angle.sin(),
            axis[2] * angle.sin(),
            angle.cos(),
        ];
        let general = transform(&b, [3., -5., 9.], quaternion, t).unwrap().model;
        let original_ids = [
            b.faces
                .iter()
                .position(|f| f.try_plane().unwrap().normal == [-1., 0., 0.])
                .unwrap(),
            b.faces
                .iter()
                .position(|f| f.try_plane().unwrap().normal == [0., -1., 0.])
                .unwrap(),
        ];
        let normal = original_ids.map(|i| general.faces[i].try_plane().unwrap().normal);
        assert!((chamfer(&general, normal, 2., t).unwrap().model.volume() - 944.).abs() < 1e-8);
        let next = chamfer(&out.model, [[1., 0., 0.], [0., 1., 0.]], 1., t).unwrap();
        assert!((next.model.volume() - 940.).abs() < 1e-8);
        let nextrotated = chamfer(
            &rotated.model,
            [rotate([1., 0., 0.]), rotate([0., 1., 0.])],
            1.,
            t,
        )
        .unwrap();
        assert!((nextrotated.model.volume() - 940.).abs() < 1e-8);
    }
    #[test]
    fn chamfer_nonright_setback_and_six_sequential_edges() {
        let t = Tolerance::default();
        for x in [5., -5.] {
            let y = 5. * 3_f64.sqrt();
            let b = extrude_polygon(&[[0., 0.], [20., 0.], [x, y]], 8., t)
                .unwrap()
                .model;
            let n = [-y / (x * x + y * y).sqrt(), x / (x * x + y * y).sqrt(), 0.];
            let cut = chamfer(&b, [[0., -1., 0.], n], 2., t).unwrap();
            let expected = b.volume() - 8. * 2. * 2. * y / (x * x + y * y).sqrt() / 2.;
            assert!((cut.model.volume() - expected).abs() < 1e-8);
            // Each new point on the bottom cap is exactly the specified setback
            // from the original corner, on one of the adjacent side faces.
            let new_points: Vec<_> = cut
                .model
                .vertices
                .iter()
                .filter(|v| v.position[2].abs() < 1e-9 && v.position[0].hypot(v.position[1]) < 3.)
                .collect();
            assert_eq!(new_points.len(), 2);
            for v in new_points {
                assert!((v.position[0].hypot(v.position[1]) - 2.).abs() < 1e-9);
            }
        }
        for d in [3., 4., 5.] {
            for e in [0.75, 1.5, 2.25] {
                let mut b = extrude_rectangle(60., 40., 15., t).unwrap().model;
                for sx in [-1., 1.] {
                    for sy in [-1., 1.] {
                        b = chamfer(&b, [[sx, 0., 0.], [0., sy, 0.]], d, t)
                            .unwrap()
                            .model;
                    }
                }
                for sy in [-1., 1.] {
                    b = chamfer(&b, [[0., sy, 0.], [0., 0., 1.]], e, t)
                        .unwrap()
                        .model;
                }
                let expected =
                    (2400. - 2. * d * d) * 15. - (60. - 2. * d) * e * e - 2. * e * e * e / 3.;
                assert!((b.volume() - expected).abs() < 1e-7);
                assert!(b.validate(t).valid);
            }
        }
    }
    #[test]
    fn chamfer_rejects_unresolved_and_nonlocal_inputs() {
        let t = Tolerance::default();
        let b = extrude_rectangle(10., 12., 8., t).unwrap().model;
        let good = [[-1., 0., 0.], [0., -1., 0.]];
        for distance in [0., -1., f64::NAN, f64::INFINITY, t.linear, 10., 11.] {
            assert!(chamfer(&b, good, distance, t).is_err());
        }
        for normals in [
            [[-2., 0., 0.], [0., -1., 0.]],
            [[f64::NAN, 0., 0.], [0., -1., 0.]],
            [[-1., 0., 0.], [1., 0., 0.]],
            [[-1., 0., 0.], [-1., 0., 0.]],
            [[0.6, 0.8, 0.], [0., -1., 0.]],
        ] {
            assert!(chamfer(&b, normals, 1., t).is_err());
        }
        let concave = extrude_polygon_with_holes(
            &[
                [0., 0.],
                [10., 0.],
                [10., 3.],
                [3., 3.],
                [3., 10.],
                [0., 10.],
            ],
            &[],
            8.,
            t,
        )
        .unwrap()
        .model;
        assert!(chamfer(&concave, good, 1., t).is_err());
        let curved = extrude_circle(5., 8., t).unwrap().model;
        assert!(chamfer(&curved, good, 1., t).is_err());
    }
    #[test]
    fn chamfer_rejects_unresolved_distant_tangential_plane_origins() {
        let t = Tolerance::default();
        let mut body = extrude_rectangle(10., 12., 8., t).unwrap().model;
        for face in &mut body.faces {
            let Surface::Plane(plane) = &mut face.surface else {
                unreachable!()
            };
            if plane.normal == [-1., 0., 0.] {
                plane.origin[1] = 1e16;
            }
        }
        // This is the same exact axis-aligned support and valid topology, but
        // its representation exceeds the conservative chamfer arithmetic domain.
        assert!(body.validate(t).valid);
        assert!(matches!(
            chamfer(&body, [[-1., 0., 0.], [0., -1., 0.]], 2., t),
            Err(KernelError::NumericRange)
        ));
    }
    fn diamond(t: Tolerance) -> ModelSnapshot {
        let square = extrude_rectangle(10. * 2_f64.sqrt(), 10. * 2_f64.sqrt(), 8., t)
            .unwrap()
            .model;
        let angle = std::f64::consts::FRAC_PI_8;
        transform(
            &square,
            [10., 0., 0.],
            [0., 0., angle.sin(), angle.cos()],
            t,
        )
        .unwrap()
        .model
    }
    #[test]
    fn oblique_octagon_analytic_volume_bounds_and_commutativity() {
        let t = Tolerance::default();
        let a = diamond(t);
        let b = translate(
            &extrude_rectangle(12., 14., 6., t).unwrap().model,
            [4., 3., 1.],
            t,
        )
        .unwrap()
        .model;
        for (left, right) in [(&a, &b), (&b, &a)] {
            let out = boolean(left, right, BooleanOp::Intersection, t).unwrap();
            assert!((out.model.volume() - 900.).abs() < 1e-9);
            assert_eq!(out.model.faces.len(), 10);
            for (value, expected) in out
                .model
                .bounds()
                .min
                .into_iter()
                .chain(out.model.bounds().max)
                .zip([4., 3., 1., 16., 17., 7.])
            {
                assert!((value - expected).abs() < 1e-10);
            }
            assert_eq!(out.history.input_faces["left"].len(), left.faces.len());
            assert_eq!(out.history.input_faces["right"].len(), right.faces.len());
        }
    }
    #[test]
    fn containment_disjoint_and_identical_planes_have_explicit_lineage() {
        let t = Tolerance::default();
        let a = diamond(t);
        let small = translate(
            &extrude_rectangle(2., 2., 2., t).unwrap().model,
            [9., 9., 2.],
            t,
        )
        .unwrap()
        .model;
        let out = boolean(&a, &small, BooleanOp::Intersection, t).unwrap();
        assert!((out.model.volume() - 8.).abs() < 1e-10);
        assert!(out.history.input_faces["left"].values().all(Vec::is_empty));
        assert!(out.history.input_faces["right"]
            .values()
            .all(|v| v.len() == 1));
        let same = boolean(&a, &a, BooleanOp::Intersection, t).unwrap();
        assert!((same.model.volume() - 1600.).abs() < 1e-8);
        assert_eq!(
            same.history.input_faces["left"],
            same.history.input_faces["right"]
        );
        let far = translate(&small, [100., 0., 0.], t).unwrap().model;
        let none = boolean(&a, &far, BooleanOp::Intersection, t).unwrap();
        assert!(none.model.vertices.is_empty());
        assert!(none
            .history
            .input_faces
            .values()
            .all(|m| m.values().all(Vec::is_empty)));
    }
    #[test]
    fn tangent_nearby_planes_and_complexity_rejected() {
        let t = Tolerance::default();
        let a = diamond(t);
        let contact = translate(
            &extrude_rectangle(2., 2., 2., t).unwrap().model,
            [9., 9., 8.],
            t,
        )
        .unwrap()
        .model;
        assert!(boolean(&a, &contact, BooleanOp::Intersection, t).is_err());
        let near = translate(&a, [0., 0., t.linear / 2.], t).unwrap().model;
        assert!(boolean(&a, &near, BooleanOp::Intersection, t).is_err());
        let tiny_gap = translate(&a, [0., 0., 1e-14], t).unwrap().model;
        assert!(matches!(
            boolean(&a, &tiny_gap, BooleanOp::Intersection, t),
            Err(KernelError::Unsupported(_))
        ));
        let angle = 1e-8_f64;
        let near_parallel = transform(&a, [1., 1., 1.], [0., 0., angle.sin(), angle.cos()], t)
            .unwrap()
            .model;
        assert!(matches!(
            boolean(&a, &near_parallel, BooleanOp::Intersection, t),
            Err(KernelError::Unsupported(_))
        ));
        let poly: Vec<_> = (0..48)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 48.;
                [10. * angle.cos(), 10. * angle.sin()]
            })
            .collect();
        let many = extrude_polygon(&poly, 2., t).unwrap().model;
        assert!(matches!(
            boolean(&a, &many, BooleanOp::Intersection, t),
            Err(KernelError::ResourceLimit(_))
        ));
        assert!(matches!(
            boolean(&a, &contact, BooleanOp::Union, t),
            Err(KernelError::Unsupported(_))
        ));
    }

    #[test]
    fn independently_oriented_boxes_containment_and_commutativity() {
        let t = Tolerance::default();
        let cube = translate(
            &extrude_rectangle(4., 4., 4., t).unwrap().model,
            [-2.; 3],
            t,
        )
        .unwrap()
        .model;
        // Ry(0.6)*Rx(0.4) and Rz(0.5)*Ry(-0.3): noncommuting compositions.
        let (a, b) = (0.2_f64, 0.3_f64);
        let q1 = [
            b.cos() * a.sin(),
            b.sin() * a.cos(),
            -b.sin() * a.sin(),
            b.cos() * a.cos(),
        ];
        let (c, d) = (0.25_f64, -0.15_f64);
        let q2 = [
            -c.sin() * d.sin(),
            c.cos() * d.sin(),
            c.sin() * d.cos(),
            c.cos() * d.cos(),
        ];
        let left = transform(&cube, [0.; 3], q1, t).unwrap().model;
        let right = transform(&cube, [1., 0.5, 0.3], q2, t).unwrap().model;
        let forward = boolean(&left, &right, BooleanOp::Intersection, t)
            .unwrap()
            .model;
        let backward = boolean(&right, &left, BooleanOp::Intersection, t)
            .unwrap()
            .model;
        assert!(forward.volume() > 0. && forward.volume() < 64.);
        assert!((forward.volume() - backward.volume()).abs() < 1e-9);
        for (a, b) in forward
            .bounds()
            .min
            .into_iter()
            .chain(forward.bounds().max)
            .zip(
                backward
                    .bounds()
                    .min
                    .into_iter()
                    .chain(backward.bounds().max),
            )
        {
            assert!((a - b).abs() < 1e-10);
        }
        let large = translate(
            &extrude_rectangle(20., 20., 20., t).unwrap().model,
            [-10.; 3],
            t,
        )
        .unwrap()
        .model;
        let large = transform(&large, [0.; 3], q1, t).unwrap().model;
        let contained = boolean(&large, &right, BooleanOp::Intersection, t)
            .unwrap()
            .model;
        assert!((contained.volume() - 64.).abs() < 1e-9);
    }
}
