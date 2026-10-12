//! Exact full-height outer corner rounds on canonical right prisms.
use crate::math::norm;
use crate::*;
use std::collections::BTreeSet;
fn bad() -> KernelError {
    KernelError::Unsupported("longitudinal fillet requires a convex line/arc right prism, two adjacent straight outer walls, and a radius strictly before every topology event".into())
}
fn end(s: &PathSegment) -> [f64; 2] {
    match s {
        PathSegment::LineTo { end } | PathSegment::ArcTo { end, .. } => *end,
    }
}
fn minus(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn unit(a: [f64; 2]) -> [f64; 2] {
    let n = length(a);
    a.map(|x| x / n)
}
fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn edges(m: &ModelSnapshot, f: FaceId) -> BTreeSet<EdgeId> {
    m.faces[f.0]
        .loops
        .iter()
        .flat_map(|l| m.loops[l.0].edges.iter().map(|c| c.edge))
        .collect()
}
/// Round one full-height convex line–line outer edge with a tangent cylindrical
/// wall. Existing outer arcs and circular holes survive; cap edges, polygonal
/// holes, reflex corners and radius-induced topology changes are unsupported.
/// Face handles are snapshot-local; authoring layers must resolve semantic intent.
pub fn fillet_longitudinal(
    body: &ModelSnapshot,
    faces: [FaceId; 2],
    radius: f64,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !radius.is_finite() || radius <= tol.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "fillet radius",
        });
    }
    if body.faces.len() > MAX_POLYGON_POINTS + MAX_POLYGON_HOLES + 2
        || body.edges.len() > 3 * (MAX_POLYGON_POINTS + MAX_POLYGON_HOLES)
        || body.vertices.len() > 2 * (MAX_POLYGON_POINTS + MAX_POLYGON_HOLES)
        || body.loops.len() > MAX_POLYGON_POINTS + 3 * MAX_POLYGON_HOLES + 2
        || body.shells.len() != 1
        || body.solids.len() != 1
        || body
            .loops
            .iter()
            .any(|l| l.edges.len() > MAX_POLYGON_POINTS)
        || body
            .faces
            .iter()
            .any(|f| f.loops.len() > MAX_POLYGON_HOLES + 1)
        || body
            .shells
            .iter()
            .any(|s| s.faces.len() > MAX_POLYGON_POINTS + MAX_POLYGON_HOLES + 2)
        || body.solids.iter().any(|s| s.shells.len() != 1)
    {
        return Err(KernelError::ResourceLimit(
            "fillet native boundary budget exceeded".into(),
        ));
    }
    let report = body.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    if faces[0] == faces[1]
        || faces
            .iter()
            .any(|f| f.0 >= body.faces.len() || body.faces[f.0].try_plane().is_none())
    {
        return Err(bad());
    }
    let stock = crate::through_bore::stock(body, tol)?;
    let (mut path, circles) = match stock.profile {
        crate::through_bore::Profile::Arc(g) => (g.path, g.circular_holes),
        crate::through_bore::Profile::Polygon { rings, circles } => {
            if rings.len() != 1 {
                return Err(bad());
            }
            let ring = &rings[0];
            (
                PlanarPath {
                    start: ring[0],
                    segments: (1..=ring.len())
                        .map(|i| PathSegment::LineTo {
                            end: ring[i % ring.len()],
                        })
                        .collect(),
                },
                circles,
            )
        }
    };
    let n = path.segments.len();
    if n >= MAX_POLYGON_POINTS {
        return Err(KernelError::ResourceLimit(
            "fillet adds one profile segment".into(),
        ));
    }
    let scale = body
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(body.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => l.origin.to_vec(),
            Curve::Circle(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
            Curve::CircularArc(a) => vec![
                a.circle.origin[0],
                a.circle.origin[1],
                a.circle.origin[2],
                a.circle.radius,
            ],
        }))
        .chain(body.faces.iter().flat_map(|f| match &f.surface {
            Surface::Plane(p) => p.origin.to_vec(),
            Surface::Cylinder(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
            Surface::Cone(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
        }))
        .map(f64::abs)
        .fold(radius, f64::max);
    let error = scale * 512. * f64::EPSILON;
    if !error.is_finite() || error > tol.linear / 32. {
        return Err(KernelError::NumericRange);
    }
    // The operation must not canonicalize perturbed input support geometry under
    // the much wider user modeling tolerance.
    for f in &body.faces {
        if let Some(p) = f.try_plane() {
            if (norm(p.normal) - 1.).abs() > 128. * f64::EPSILON {
                return Err(bad());
            }
            for l in &f.loops {
                for c in &body.loops[l.0].edges {
                    for id in body.edges[c.edge.0].vertices {
                        if dot(sub(body.vertices[id.0].position, p.origin), p.normal).abs() > error
                        {
                            return Err(bad());
                        }
                    }
                }
            }
        }
    }
    for e in &body.edges {
        if let Curve::Line(l) = &e.curve {
            if (norm(l.direction) - 1.).abs() > 128. * f64::EPSILON {
                return Err(bad());
            }
            for id in e.vertices {
                if norm(cross(
                    sub(body.vertices[id.0].position, l.origin),
                    l.direction,
                )) > error
                {
                    return Err(bad());
                }
            }
        }
    }
    for edge in &body.edges {
        let (c, sweep) = match &edge.curve {
            Curve::Circle(c) => (c, std::f64::consts::TAU),
            Curve::CircularArc(a) => (&a.circle, a.sweep_radians),
            _ => continue,
        };
        if (norm(c.axis) - 1.).abs() > 128. * f64::EPSILON
            || (norm(c.x_direction) - 1.).abs() > 128. * f64::EPSILON
            || dot(c.axis, c.x_direction).abs() > 128. * f64::EPSILON
        {
            return Err(bad());
        }
        let y = cross(c.axis, c.x_direction);
        for (id, angle) in edge.vertices.into_iter().zip([0., sweep]) {
            let (sn, cs) = angle.sin_cos();
            let expected = std::array::from_fn(|k| {
                c.origin[k] + c.radius * (cs * c.x_direction[k] + sn * y[k])
            });
            if norm(sub(body.vertices[id.0].position, expected)) > error {
                return Err(bad());
            }
        }
    }
    for face in &body.faces {
        if let Surface::Cylinder(c) = &face.surface {
            if (norm(c.axis) - 1.).abs() > 128. * f64::EPSILON
                || (norm(c.x_direction) - 1.).abs() > 128. * f64::EPSILON
                || dot(c.axis, c.x_direction).abs() > 128. * f64::EPSILON
            {
                return Err(bad());
            }
            for id in face
                .loops
                .iter()
                .flat_map(|l| body.loops[l.0].edges.iter().map(|e| e.edge))
            {
                let e = &body.edges[id.0];
                let support = match &e.curve {
                    Curve::Circle(x) => Some(x),
                    Curve::CircularArc(x) => Some(&x.circle),
                    _ => None,
                };
                if let Some(s) = support {
                    if (s.radius - c.radius).abs() > error
                        || norm(cross(sub(s.origin, c.origin), c.axis)) > error
                        || norm(sub(s.axis, c.axis)) * scale > error
                    {
                        return Err(bad());
                    }
                }
                for v in e.vertices {
                    if (norm(cross(sub(body.vertices[v.0].position, c.origin), c.axis)) - c.radius)
                        .abs()
                        > error
                    {
                        return Err(bad());
                    }
                }
            }
        }
    }
    let shared: Vec<_> = edges(body, faces[0])
        .intersection(&edges(body, faces[1]))
        .copied()
        .collect();
    if shared.len() != 1 {
        return Err(bad());
    }
    let edge = &body.edges[shared[0].0];
    if !matches!(edge.curve, Curve::Line(_)) {
        return Err(bad());
    }
    let a = body.vertices[edge.vertices[0].0].position;
    let b = body.vertices[edge.vertices[1].0].position;
    if norm(cross(sub(b, a), stock.axis)) > error || (norm(sub(b, a)) - stock.height).abs() > error
    {
        return Err(bad());
    }
    let y = cross(stock.axis, stock.x);
    let d = sub(a, stock.origin);
    let corner = [dot(d, stock.x), dot(d, y)];
    let starts: Vec<_> = std::iter::once(path.start)
        .chain(path.segments.iter().take(n - 1).map(end))
        .collect();
    let matching: Vec<_> = starts
        .iter()
        .enumerate()
        .filter(|(_, p)| length(minus(**p, corner)) <= error)
        .map(|(i, _)| i)
        .collect();
    if matching.len() != 1 {
        return Err(bad());
    }
    let i = matching[0];
    let prev = (i + n - 1) % n;
    if !matches!(path.segments[prev], PathSegment::LineTo { .. })
        || !matches!(path.segments[i], PathSegment::LineTo { .. })
    {
        return Err(bad());
    }
    // A simple closed path with nonnegative curvature and convex joins is convex.
    // Native path validation supplies simplicity; no sampled convex hull is used.
    let mut tangents = vec![];
    for (j, s) in path.segments.iter().enumerate() {
        let p = starts[j];
        let q = end(s);
        tangents.push(match s {
            PathSegment::LineTo { .. } => {
                let u = unit(minus(q, p));
                (u, u)
            }
            PathSegment::ArcTo {
                center, clockwise, ..
            } => {
                if *clockwise {
                    return Err(bad());
                }
                let u = unit(minus(p, *center));
                let v = unit(minus(q, *center));
                ([-u[1], u[0]], [-v[1], v[0]])
            }
        })
    }
    for j in 0..n {
        let u = tangents[j].1;
        let v = tangents[(j + 1) % n].0;
        if cross2(u, v) < -256. * f64::EPSILON || dot2(u, v) < -1. + 1e-10 {
            return Err(bad());
        }
    }
    let u = tangents[prev].1;
    let v = tangents[i].0;
    let sine = cross2(u, v);
    let cosine = dot2(u, v);
    if sine <= 1e-8 || 1. + cosine <= 1e-8 {
        return Err(bad());
    }
    let setback = radius * sine / (1. + cosine);
    let p = starts[i];
    let t0 = [p[0] - setback * u[0], p[1] - setback * u[1]];
    let t1 = [p[0] + setback * v[0], p[1] + setback * v[1]];
    let center = [t0[0] - radius * u[1], t0[1] + radius * u[0]];
    let reserve = tol.linear + 32. * error / (sine.min(1. + cosine));
    if !reserve.is_finite()
        || reserve > tol.linear * 2.
        || setback <= reserve
        || length(minus(p, starts[prev])) - setback <= reserve
        || length(minus(end(&path.segments[i]), p)) - setback <= reserve
    {
        return Err(bad());
    }
    if (length(minus(t1, center)) - radius).abs() > error * 16.
        || dot2(minus(t1, center), v).abs() > error * 16.
    {
        return Err(KernelError::NumericRange);
    }
    // Associate native side faces through cap boundary edges, independent of
    // segment numbering in the caller's polygon or previously filleted body.
    let cap_edges = edges(body, FaceId(1));
    let mut wall_map = vec![];
    for (j, &p) in starts.iter().enumerate() {
        let q = end(&path.segments[j]);
        let candidates: Vec<_> = cap_edges
            .iter()
            .filter(|eid| {
                let e = &body.edges[eid.0];
                let project = |id: VertexId| {
                    let d = sub(body.vertices[id.0].position, stock.origin);
                    [dot(d, stock.x), dot(d, y)]
                };
                let a = project(e.vertices[0]);
                let b = project(e.vertices[1]);
                (length(minus(a, p)) <= error && length(minus(b, q)) <= error)
                    || (length(minus(a, q)) <= error && length(minus(b, p)) <= error)
            })
            .copied()
            .collect();
        if candidates.len() != 1 {
            return Err(bad());
        }
        let side: Vec<_> = (2..body.faces.len())
            .filter(|f| edges(body, FaceId(*f)).contains(&candidates[0]))
            .collect();
        if side.len() != 1 {
            return Err(bad());
        }
        wall_map.push(side[0]);
    }
    if !faces
        .iter()
        .all(|f| [wall_map[prev], wall_map[i]].contains(&f.0))
    {
        return Err(bad());
    }
    if let PathSegment::LineTo { end } = &mut path.segments[prev] {
        *end = t0;
    }
    path.segments.insert(
        i,
        PathSegment::ArcTo {
            center,
            end: t1,
            clockwise: false,
        },
    );
    if i == 0 {
        path.start = t0;
    }
    validate_path(&path, &circles, tol)?;
    let g = ArcPrismGeometry {
        path,
        circular_holes: circles,
        origin: stock.origin,
        axis: stock.axis,
        x_direction: stock.x,
        height: stock.height,
    };
    let model = crate::arc_prism::construct(&g, tol)?;
    let report = model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let mut history = TopologyHistory {
        generated_faces: BTreeMap::from([("fillet_wall".into(), vec![FaceId(i + 2)])]),
        input_faces: BTreeMap::new(),
    };
    let mut mapping: BTreeMap<FaceId, Vec<FaceId>> =
        (0..body.faces.len()).map(|f| (FaceId(f), vec![])).collect();
    mapping.insert(FaceId(0), vec![FaceId(0)]);
    mapping.insert(FaceId(1), vec![FaceId(1)]);
    for (j, f) in wall_map.into_iter().enumerate() {
        mapping.insert(FaceId(f), vec![FaceId(2 + j + usize::from(j >= i))]);
    }
    let old_holes: Vec<_> = (2..body.faces.len())
        .filter(|f| mapping[&FaceId(*f)].is_empty())
        .collect();
    if old_holes.len() != g.circular_holes.len() {
        return Err(bad());
    }
    for (h, f) in old_holes.into_iter().enumerate() {
        mapping.insert(FaceId(f), vec![FaceId(n + 3 + h)]);
    }
    history.input_faces.insert("body".into(), mapping);
    Ok(OperationResult { model, history })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plate() -> ModelSnapshot {
        extrude_polygon(
            &[[0., 0.], [20., 0.], [20., 12.], [0., 12.]],
            5.,
            Tolerance::default(),
        )
        .unwrap()
        .model
    }
    #[test]
    fn rectangle_and_four_sequential_corners() {
        let t = Tolerance::default();
        let source = plate();
        let mut m = source.clone();
        let mut faces: Vec<_> = (0..m.faces.len()).map(FaceId).collect();
        for pair in [[2, 3], [3, 4], [4, 5], [5, 2]] {
            let op = fillet_longitudinal(&m, [faces[pair[0]], faces[pair[1]]], 2., t).unwrap();
            assert_eq!(op.history.generated_faces["fillet_wall"].len(), 1);
            for f in &mut faces {
                *f = op.history.input_faces["body"][f][0];
            }
            m = op.model;
        }
        let props = m.mass_properties(t).unwrap();
        let removed = 4. * (1. - std::f64::consts::PI / 4.) * 4.;
        assert!((props.volume - (240. - removed) * 5.).abs() < 1e-8);
        assert!(norm(sub(props.centroid.unwrap(), [10., 6., 2.5])) < 1e-8);
        let pi = std::f64::consts::PI;
        let a = 4. * (1. - pi / 4.);
        let first = 8. * (5. / 6. - pi / 4.);
        let second = 16. * (1. - 5. * pi / 16.);
        let x2 = 12. * 20_f64.powi(3) / 12. - 4. * (100. * a - 20. * first + second);
        let y2 = 20. * 12_f64.powi(3) / 12. - 4. * (36. * a - 12. * first + second);
        let z2 = (240. - 4. * a) * 125. / 12.;
        let expected = [
            [5. * y2 + z2, 0., 0.],
            [0., 5. * x2 + z2, 0.],
            [0., 0., 5. * (x2 + y2)],
        ];
        for (a, b) in props
            .central_inertia
            .unwrap()
            .iter()
            .flatten()
            .zip(expected.iter().flatten())
        {
            assert!((a - b).abs() < 1e-7);
        }
        let drill = translate(&extrude_circle(1., 7., t).unwrap().model, [10., 6., -1.], t)
            .unwrap()
            .model;
        let drilled = boolean(&m, &drill, BooleanOp::Difference, t).unwrap().model;
        assert!(
            (drilled.mass_properties(t).unwrap().volume - (props.volume - 5. * pi)).abs() < 1e-8
        );
        let section = drilled
            .section(
                SectionPlane {
                    origin: [0., 0., 2.],
                    normal: [0., 0., 1.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert!((section.area - (props.volume / 5. - pi)).abs() < 1e-8);
        let mesh = tessellate_analytic(&drilled, t, TessellationPolicy::default()).unwrap();
        let mut incidence: BTreeMap<([u64; 3], [u64; 3]), (usize, i32)> = BTreeMap::new();
        for patch in mesh.patches {
            for k in 0..patch.points.len() {
                let a = patch.points[k].map(|x| if x == 0. { 0 } else { x.to_bits() });
                let b = patch.points[(k + 1) % patch.points.len()].map(|x| {
                    if x == 0. {
                        0
                    } else {
                        x.to_bits()
                    }
                });
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let entry = incidence.entry(key).or_default();
                entry.0 += 1;
                entry.1 += sign;
            }
        }
        assert!(incidence.values().all(|v| *v == (2, 0)));
    }
    #[test]
    fn radius_events_cap_edges_and_mutated_supports_reject() {
        let t = Tolerance::default();
        let m = plate();
        assert!(fillet_longitudinal(&m, [FaceId(0), FaceId(2)], 1., t).is_err());
        for r in [0., -1., 12., 20., f64::NAN] {
            assert!(fillet_longitudinal(&m, [FaceId(2), FaceId(3)], r, t).is_err());
        }
        let mut bad = m.clone();
        if let Curve::Line(l) = &mut bad.edges[0].curve {
            l.origin[2] += t.linear / 10.;
        }
        assert!(fillet_longitudinal(&bad, [FaceId(2), FaceId(3)], 1., t).is_err());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    #[test]
    fn nonright_corner_common_pose_and_circle_holes() {
        let t = Tolerance::default();
        let m = extrude_polygon_with_circular_holes(
            &[[0., 0.], [30., 0.], [12., 24.]],
            &[],
            &[CircleHole {
                center: [12., 8.],
                radius: 1.,
            }],
            6.,
            t,
        )
        .unwrap()
        .model;
        let op = fillet_longitudinal(&m, [FaceId(2), FaceId(3)], 2., t).unwrap();
        let turn = (18_f64 / 30.).acos(); // Interior angle between (-1,0) and (-18,24)/30.
        let removed = 4. * (1. / (turn / 2.).tan() - (std::f64::consts::PI - turn) / 2.);
        assert!(
            (op.model.mass_properties(t).unwrap().volume
                - (m.mass_properties(t).unwrap().volume - 6. * removed))
                .abs()
                < 1e-7
        );
        let q = [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2];
        let moved = transform(&m, [25., -12., 7.], q, t).unwrap().model;
        let actual = fillet_longitudinal(&moved, [FaceId(2), FaceId(3)], 2., t)
            .unwrap()
            .model;
        let expected = transform(&op.model, [25., -12., 7.], q, t).unwrap().model;
        let a = actual.mass_properties(t).unwrap();
        let b = expected.mass_properties(t).unwrap();
        assert!((a.volume - b.volume).abs() < 1e-7);
        assert!(norm(sub(a.centroid.unwrap(), b.centroid.unwrap())) < 1e-8);
        for (x, y) in a
            .central_inertia
            .unwrap()
            .iter()
            .flatten()
            .zip(b.central_inertia.unwrap().iter().flatten())
        {
            assert!((x - y).abs() < 1e-6);
        }
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    #[test]
    fn existing_curved_support_tampering_and_hole_topology_refuse() {
        let t = Tolerance::default();
        let source = extrude_polygon_with_circular_holes(
            &[[0., 0.], [20., 0.], [20., 12.], [0., 12.]],
            &[],
            &[CircleHole {
                center: [10., 6.],
                radius: 1.,
            }],
            5.,
            t,
        )
        .unwrap()
        .model;
        let first = fillet_longitudinal(&source, [FaceId(2), FaceId(3)], 2., t).unwrap();
        let pair = [
            first.history.input_faces["body"][&FaceId(4)][0],
            first.history.input_faces["body"][&FaceId(5)][0],
        ];
        for kind in 0..4 {
            let mut m = first.model.clone();
            match kind {
                0 => {
                    let c = m
                        .faces
                        .iter_mut()
                        .find_map(|f| match &mut f.surface {
                            Surface::Cylinder(c) => Some(c),
                            _ => None,
                        })
                        .unwrap();
                    c.radius += t.linear / 10.;
                }
                1 => {
                    let c = m
                        .faces
                        .iter_mut()
                        .find_map(|f| match &mut f.surface {
                            Surface::Cylinder(c) => Some(c),
                            _ => None,
                        })
                        .unwrap();
                    c.axis[0] += 1e-9;
                }
                2 => {
                    let a = m
                        .edges
                        .iter_mut()
                        .find_map(|e| match &mut e.curve {
                            Curve::CircularArc(a) => Some(a),
                            _ => None,
                        })
                        .unwrap();
                    a.sweep_radians += 1e-9;
                }
                _ => {
                    let c = m
                        .edges
                        .iter_mut()
                        .find_map(|e| match &mut e.curve {
                            Curve::Circle(c) => Some(c),
                            _ => None,
                        })
                        .unwrap();
                    c.radius += t.linear / 10.;
                }
            }
            assert!(
                fillet_longitudinal(&m, pair, 1., t).is_err(),
                "mutation{kind}"
            );
        }
        let near = extrude_polygon_with_circular_holes(
            &[[0., 0.], [20., 0.], [20., 12.], [0., 12.]],
            &[],
            &[CircleHole {
                center: [19., 1.],
                radius: 0.5,
            }],
            5.,
            t,
        )
        .unwrap()
        .model;
        assert!(fillet_longitudinal(&near, [FaceId(2), FaceId(3)], 5., t).is_err());
    }
    #[test]
    fn scales_box_source_and_concave_corner_refusal() {
        let t = Tolerance::default();
        for scale in [0.001, 1., 100.] {
            let source = extrude_rectangle(20. * scale, 12. * scale, 5. * scale, t)
                .unwrap()
                .model;
            let result =
                fillet_longitudinal(&source, [FaceId(2), FaceId(3)], 2. * scale, t).unwrap();
            let expected = (1200. - 20. * (1. - std::f64::consts::PI / 4.)) * scale.powi(3);
            assert!(
                (result.model.mass_properties(t).unwrap().volume - expected).abs()
                    < 1e-7 * expected.max(1.)
            );
        }
        let source = extrude_polygon_with_holes(
            &[
                [0., 0.],
                [10., 0.],
                [10., 4.],
                [4., 4.],
                [4., 10.],
                [0., 10.],
            ],
            &[],
            5.,
            t,
        )
        .unwrap()
        .model;
        assert!(fillet_longitudinal(&source, [FaceId(4), FaceId(5)], 1., t).is_err());
    }
}
