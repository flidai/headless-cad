//! Bounded analytic half-space intersection. No tessellation, snapping, or weld.
use crate::math::{cross, dot, norm, sub};
use crate::*;

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
    let mut maps = [a, b].map(|m| {
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
                if det.abs() <= 128. * f64::EPSILON {
                    return Err(unsupported("near-dependent supporting planes exceed reliable intersection conditioning"));
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
                let c23 = cross(n2, n3);
                let c31 = cross(n3, n1);
                let c12 = cross(n1, n2);
                let p = std::array::from_fn(|axis| {
                    (offsets[i] * c23[axis] + offsets[j] * c31[axis] + offsets[k] * c12[axis]) / det
                });
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
                        return Err(unsupported("intersection vertex has unresolved incidence with another supporting plane"));
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

#[cfg(test)]
mod tests {
    use super::*;
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
