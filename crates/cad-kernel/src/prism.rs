//! Simple-profile right prisms with disjoint convex through holes.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::BTreeSet;
pub const MAX_POLYGON_HOLES: usize = 32;

/// Simple CCW outer ring, convex CW disjoint strictly interior holes, in mm.
/// No ring reordering, repair, tangent contacts, nested holes, or touching edges.
pub fn validate_polygon_with_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    tol: Tolerance,
) -> Result<(), KernelError> {
    if holes.len() > MAX_POLYGON_HOLES
        || holes
            .iter()
            .fold(outer.len(), |n, h| n.saturating_add(h.len()))
            > MAX_POLYGON_POINTS
    {
        return Err(KernelError::ResourceLimit(
            "polygon supports 256 total vertices and 32 holes".into(),
        ));
    }
    if validate_polygon(outer, tol).is_err() {
        crate::simple_polygon::validate_ring(outer, tol)?;
    }
    for hole in holes {
        let reverse: Vec<_> = hole.iter().rev().copied().collect();
        validate_polygon(&reverse, tol)?;
        let scale = outer
            .iter()
            .chain(hole)
            .flatten()
            .map(|v| v.abs())
            .fold(0., f64::max);
        let margin = tol.linear + scale * 256. * f64::EPSILON;
        for i in 0..outer.len() {
            for j in 0..hole.len() {
                if !crate::simple_polygon::separated(
                    outer[i],
                    outer[(i + 1) % outer.len()],
                    hole[j],
                    hole[(j + 1) % hole.len()],
                    margin,
                ) {
                    return Err(KernelError::Unsupported(
                        "hole boundary crosses or touches outer boundary".into(),
                    ));
                }
            }
        }
        if !crate::simple_polygon::inside(hole[0], outer) {
            return Err(KernelError::Unsupported(
                "holes must lie strictly inside outer boundary".into(),
            ));
        }
    }

    for i in 0..holes.len() {
        for j in i + 1..holes.len() {
            let mut separated = false;
            for ring in [&holes[i], &holes[j]] {
                for k in 0..ring.len() {
                    let a = ring[k];
                    let b = ring[(k + 1) % ring.len()];
                    let e = [b[0] - a[0], b[1] - a[1]];
                    let length = e[0].hypot(e[1]);
                    let n = [-e[1] / length, e[0] / length];
                    let range = |points: &Vec<[f64; 2]>| {
                        points
                            .iter()
                            .map(|p| {
                                let v = (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                                let uncertainty = ((p[0].abs() + a[0].abs()) * n[0].abs()
                                    + (p[1].abs() + a[1].abs()) * n[1].abs())
                                    * 64.
                                    * f64::EPSILON;
                                [v - uncertainty, v + uncertainty]
                            })
                            .fold([f64::INFINITY, f64::NEG_INFINITY], |r, v| {
                                [r[0].min(v[0]), r[1].max(v[1])]
                            })
                    };
                    let x = range(&holes[i]);
                    let y = range(&holes[j]);
                    let scale = x.into_iter().chain(y).map(f64::abs).fold(0., f64::max);
                    let margin = tol.linear + scale * 64. * f64::EPSILON;
                    if x[0] - y[1] > margin || y[0] - x[1] > margin {
                        separated = true;
                    }
                }
            }
            if !separated {
                return Err(KernelError::Unsupported(
                    "holes overlap, nest, touch, or have unresolved separation".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Native planar caps retain one outer boundary and all inner hole boundaries.
pub fn extrude_polygon_with_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    height: f64,
    tol: Tolerance,
) -> Result<OperationResult, KernelError> {
    validate_polygon_with_holes(outer, holes, tol)?;
    if holes.is_empty() && validate_polygon(outer, tol).is_ok() {
        return extrude_polygon(outer, height, tol);
    }
    if !height.is_finite() || height <= tol.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "height",
        });
    }
    let rings: Vec<&[[f64; 2]]> = std::iter::once(outer)
        .chain(holes.iter().map(Vec::as_slice))
        .collect();
    let flat: Vec<_> = rings.iter().flat_map(|r| r.iter().copied()).collect();
    let n = flat.len();
    let positions: Vec<_> = [0., height]
        .into_iter()
        .flat_map(|z| flat.iter().map(move |p| [p[0], p[1], z]))
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
    let mut face_rings: Vec<(Vec<Vec<usize>>, [f64; 3])> =
        vec![(vec![], [0., 0., -1.]), (vec![], [0., 0., 1.])];
    let mut base = 0;
    let mut hole_roles = BTreeMap::new();
    for (ri, ring) in rings.iter().enumerate() {
        face_rings[0]
            .0
            .push((base..base + ring.len()).rev().collect());
        face_rings[1]
            .0
            .push((base + n..base + n + ring.len()).collect());
        for k in 0..ring.len() {
            let a = base + k;
            let b = base + (k + 1) % ring.len();
            let d = sub(positions[b], positions[a]);
            let length = norm(d);
            let id = FaceId(face_rings.len());
            face_rings.push((
                vec![vec![a, b, b + n, a + n]],
                [d[1] / length, -d[0] / length, 0.],
            ));
            let role = if ri == 0 {
                format!("wall_{k}")
            } else {
                format!("hole_{}_wall_{k}", ri - 1)
            };
            hole_roles.insert(role, vec![id]);
        }
        base += ring.len();
    }
    let mut edges = BTreeMap::new();
    for (rings, normal) in face_rings {
        let mut loops = vec![];
        for ring in &rings {
            let mut uses = vec![];
            for k in 0..ring.len() {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                let key = (a.min(b), a.max(b));
                let edge = *edges.entry(key).or_insert_with(|| {
                    let id = EdgeId(model.edges.len());
                    let d = sub(positions[key.1], positions[key.0]);
                    let length = norm(d);
                    model.edges.push(Edge {
                        vertices: [VertexId(key.0), VertexId(key.1)],
                        curve: Curve::Line(Line {
                            origin: positions[key.0],
                            direction: d.map(|v| v / length),
                        }),
                    });
                    id
                });
                uses.push(EdgeUse {
                    edge,
                    reversed: a != key.0,
                });
            }
            loops.push(LoopId(model.loops.len()));
            model.loops.push(Loop { edges: uses });
        }
        model.faces.push(Face {
            surface: Surface::Plane(Plane {
                origin: positions[rings[0][0]],
                normal,
            }),
            loops,
        });
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
    hole_roles.insert("start_cap".into(), vec![FaceId(0)]);
    hole_roles.insert("end_cap".into(), vec![FaceId(1)]);
    hole_roles.insert("walls".into(), (2..2 + n).map(FaceId).collect());
    hole_roles.insert(
        "hole_walls".into(),
        (2 + outer.len()..2 + n).map(FaceId).collect(),
    );
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: hole_roles,
            input_faces: BTreeMap::new(),
        },
    })
}

/// Called only after all native references/local face geometry/topology validate.
pub(crate) fn validate_prism(model: &ModelSnapshot, tol: Tolerance) -> Result<(), KernelError> {
    let bad = || {
        KernelError::Unsupported("multi-loop geometry must be one right prism with matching simple-ring caps and complete straight wall strips".into())
    };
    if model.solids.len() != 1 || model.shells.len() != 1 {
        return Err(bad());
    }
    let caps: Vec<_> = model
        .faces
        .iter()
        .enumerate()
        .filter(|(i, f)| f.loops.len() > 1 || !crate::face_is_convex(model, FaceId(*i)))
        .collect();
    if caps.len() != 2 {
        return Err(bad());
    }
    let (ai, a) = caps[0];
    let (bi, b) = caps[1];
    if a.loops.len() != b.loops.len() || a.loops.len() > MAX_POLYGON_HOLES + 1 {
        return Err(bad());
    }
    let z = a
        .try_plane()
        .unwrap()
        .normal
        .map(|v| v / norm(a.try_plane().unwrap().normal));
    if norm(std::array::from_fn(|i| {
        z[i] + b.try_plane().unwrap().normal[i]
    })) > 1e-12
    {
        return Err(bad());
    }
    let ids = |face: &Face| -> Vec<Vec<VertexId>> {
        face.loops
            .iter()
            .map(|id| {
                model.loops[id.0]
                    .edges
                    .iter()
                    .map(|e| model.endpoints(*e).unwrap()[0])
                    .collect()
            })
            .collect()
    };
    let ar = ids(a);
    let br = ids(b);
    let all_a: Vec<_> = ar.iter().flatten().copied().collect();
    let all_b: Vec<_> = br.iter().flatten().copied().collect();
    let n = all_a.len();
    if n > MAX_POLYGON_POINTS
        || all_b.len() != n
        || model.vertices.len() != 2 * n
        || model.edges.len() != 3 * n
        || model.faces.len() != n + 2
    {
        return Err(bad());
    }
    let origin = model.vertices[ar[0][0].0].position;
    let e = sub(model.vertices[ar[0][1].0].position, origin);
    let x = e.map(|v| v / norm(e));
    let y = cross(z, x);
    let project = |ring: &Vec<VertexId>| {
        ring.iter()
            .map(|id| {
                let d = sub(model.vertices[id.0].position, origin);
                [dot(d, x), dot(d, y)]
            })
            .collect::<Vec<_>>()
    };
    let outer = project(&ar[0]);
    let holes: Vec<_> = ar[1..].iter().map(project).collect();
    validate_polygon_with_holes(&outer, &holes, tol)?;
    let height = -dot(sub(b.try_plane().unwrap().origin, origin), z);
    if !height.is_finite() || height <= tol.linear {
        return Err(bad());
    }
    let extent = model
        .vertices
        .iter()
        .map(|v| norm(sub(v.position, origin)))
        .fold(height, f64::max);
    let geometry_error = extent * (256. * f64::EPSILON);
    if !geometry_error.is_normal() || geometry_error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    for face in [a, b] {
        for ring in ids(face) {
            for id in ring {
                if dot(
                    sub(
                        model.vertices[id.0].position,
                        face.try_plane().unwrap().origin,
                    ),
                    face.try_plane().unwrap().normal,
                )
                .abs()
                    > geometry_error
                {
                    return Err(bad());
                }
            }
        }
    }
    let cap_b: BTreeSet<_> = all_b.iter().copied().collect();
    let mut pairs = BTreeMap::new();
    let mut used = BTreeSet::new();
    for &source in &all_a {
        // Native longitudinal edge incidence determines correspondence. Geometry
        // only verifies it; tolerance never selects a nearest cap vertex.
        let candidates: Vec<_> = model
            .edges
            .iter()
            .filter_map(|edge| {
                let [u, v] = edge.vertices;
                if u == source && cap_b.contains(&v) {
                    Some(v)
                } else if v == source && cap_b.contains(&u) {
                    Some(u)
                } else {
                    None
                }
            })
            .collect();
        if candidates.len() != 1 || !used.insert(candidates[0]) {
            return Err(bad());
        }
        let delta = sub(
            model.vertices[candidates[0].0].position,
            model.vertices[source.0].position,
        );
        if norm(std::array::from_fn(|i| delta[i] + height * z[i])) > geometry_error {
            return Err(bad());
        }
        pairs.insert(source, candidates[0]);
    }
    let mut expected = BTreeSet::new();
    for ring in &ar {
        let paired: BTreeSet<_> = ring.iter().map(|id| pairs[id]).collect();
        if !br
            .iter()
            .any(|r| r.iter().copied().collect::<BTreeSet<_>>() == paired)
        {
            return Err(bad());
        }
        for k in 0..ring.len() {
            let (u, v) = (ring[k], ring[(k + 1) % ring.len()]);
            let mut face = [u, v, pairs[&u], pairs[&v]];
            face.sort();
            expected.insert(face);
        }
    }
    for (index, face) in model.faces.iter().enumerate() {
        if index == ai || index == bi {
            continue;
        }
        if face.loops.len() != 1 {
            return Err(bad());
        }
        let ring = &ids(face)[0];
        if ring.len() != 4 {
            return Err(bad());
        }
        let mut key = [ring[0], ring[1], ring[2], ring[3]];
        key.sort();
        if !expected.remove(&key) {
            return Err(bad());
        }
    }
    if !expected.is_empty() {
        return Err(bad());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> OperationResult {
        extrude_polygon_with_holes(
            &[[0., 0.], [10., 0.], [10., 8.], [0., 8.]],
            &[vec![[2., 2.], [2., 4.], [5., 4.], [5., 2.]]],
            3.,
            Tolerance::default(),
        )
        .unwrap()
    }
    #[test]
    fn analytic_hole_geometry_measurements_and_triangles() {
        let t = Tolerance::default();
        let out = sample();
        let model = &out.model;
        assert_eq!(model.faces.len(), 10);
        assert_eq!(model.faces[0].loops.len(), 2);
        assert_eq!(model.edges.len(), 24);
        assert!((model.volume() - 222.).abs() < 1e-10);
        let properties = model.mass_properties(t).unwrap();
        let expected = [(80. * 5. - 6. * 3.5) / 74., (80. * 4. - 6. * 3.) / 74., 1.5];
        let mut tensor = [[0.; 3]; 3];
        for (sign, volume, center, diagonal) in [
            (1., 240., [5., 4., 1.5], [1460., 2180., 3280.]),
            (-1., 18., [3.5, 3., 1.5], [19.5, 27., 19.5]),
        ] {
            let delta = sub(center, expected);
            let squared = dot(delta, delta);
            for i in 0..3 {
                for j in 0..3 {
                    tensor[i][j] += sign
                        * (if i == j {
                            diagonal[i] + volume * (squared - delta[i] * delta[j])
                        } else {
                            -volume * delta[i] * delta[j]
                        });
                }
            }
        }
        let actual = properties.central_inertia.unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!((actual[i][j] - tensor[i][j]).abs() < 1e-9);
            }
        }
        for (a, b) in properties.centroid.unwrap().into_iter().zip(expected) {
            assert!((a - b).abs() < 1e-10);
        }
        let frame = model
            .face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t)
            .unwrap();
        assert!((frame.origin[0] - expected[0]).abs() < 1e-10);
        assert!((frame.origin[1] - expected[1]).abs() < 1e-10);
        for i in 0..model.faces.len() {
            let triangles = triangulate_face(model, FaceId(i), t).unwrap();
            assert_eq!(triangles.len(), if i < 2 { 8 } else { 2 });
        }
        let a = 0.3_f64;
        let rotated = transform(model, [4., 5., 6.], [a.sin(), 0., 0., a.cos()], t).unwrap();
        assert!((rotated.model.volume() - 222.).abs() < 1e-9);
        assert!(triangulate_face(&rotated.model, FaceId(1), t).is_ok());
        assert!(matches!(
            boolean(model, model, BooleanOp::Intersection, t),
            Err(KernelError::Unsupported(_))
        ));
    }
    #[test]
    fn invalid_holes_and_tampered_prisms_rejected() {
        let t = Tolerance::default();
        let outer = [[0., 0.], [10., 0.], [10., 8.], [0., 8.]];
        for hole in [
            vec![[2., 2.], [5., 2.], [5., 4.], [2., 4.]],
            vec![[0., 2.], [0., 4.], [2., 4.], [2., 2.]],
            vec![[9., 2.], [9., 4.], [11., 4.], [11., 2.]],
        ] {
            assert!(validate_polygon_with_holes(&outer, &[hole], t).is_err());
        }
        let hole = vec![[2., 2.], [2., 4.], [5., 4.], [5., 2.]];
        assert!(validate_polygon_with_holes(&outer, &[hole.clone(), hole], t).is_err());
        let mut model = sample().model;
        model.faces[2].try_plane_mut().unwrap().normal =
            model.faces[2].try_plane().unwrap().normal.map(|v| -v);
        assert!(!model.validate(t).valid);
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    #[test]
    fn two_hole_alignment_triangulates() {
        let t = Tolerance::default();
        let m = extrude_polygon_with_holes(
            &[[0., 0.], [20., 0.], [20., 10.], [0., 10.]],
            &[
                vec![[2., 2.], [2., 6.], [6., 6.], [6., 2.]],
                vec![[12., 2.], [12., 6.], [16., 2.]],
            ],
            3.,
            t,
        )
        .unwrap()
        .model;
        for f in [FaceId(0), FaceId(1)] {
            triangulate_face(&m, f, t).unwrap();
        }
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    #[test]
    fn slight_shear_is_not_a_right_prism_and_large_translation_keeps_holes() {
        let t = Tolerance::default();
        let make = || {
            extrude_polygon_with_holes(
                &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
                &[vec![[2., 2.], [2., 4.], [4., 4.], [4., 2.]]],
                3.,
                t,
            )
            .unwrap()
            .model
        };
        let moved = translate(&make(), [1e12, -1e12, 1e12], t).unwrap().model;
        assert!(triangulate_face(&moved, FaceId(1), t).is_ok());
        let mut model = make();
        for vertex in &mut model.vertices {
            if vertex.position[2] == 3. {
                vertex.position[0] += 1e-9;
            }
        }
        for edge in &mut model.edges {
            let a = model.vertices[edge.vertices[0].0].position;
            let b = model.vertices[edge.vertices[1].0].position;
            let d = sub(b, a);
            edge.curve = Curve::Line(Line {
                origin: a,
                direction: d.map(|v| v / norm(d)),
            });
        }
        for i in 0..model.faces.len() {
            let points = model.loop_points(model.faces[i].loops[0]).unwrap();
            let normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
            model.faces[i].surface = Surface::Plane(Plane {
                origin: points[0],
                normal: normal.map(|v| v / norm(normal)),
            });
        }
        assert!(
            !model.validate(t).valid,
            "sub-tolerance shear must not be relabeled a right prism"
        );
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    #[test]
    fn two_holes_in_multiple_oblique_frames() {
        let t = Tolerance::default();
        let m = extrude_polygon_with_holes(
            &[[0., 0.], [20., 0.], [20., 10.], [0., 10.]],
            &[
                vec![[2., 2.], [2., 6.], [6., 6.], [6., 2.]],
                vec![[12., 2.], [12., 6.], [16., 2.]],
            ],
            3.,
            t,
        )
        .unwrap()
        .model;
        for i in 1..9 {
            let angle = i as f64 * 0.09;
            let s = angle.sin() / 14_f64.sqrt();
            let q = [s, 2. * s, 3. * s, angle.cos()];
            let rotated = transform(&m, [4., 5., 6.], q, t).unwrap().model;
            for f in [FaceId(0), FaceId(1)] {
                triangulate_face(&rotated, f, t).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod concave_tests {
    use super::*;
    fn u() -> Vec<[f64; 2]> {
        vec![
            [0., 0.],
            [10., 0.],
            [10., 10.],
            [8., 10.],
            [8., 2.],
            [2., 2.],
            [2., 10.],
            [0., 10.],
        ]
    }
    #[test]
    fn convex_large_offset_preserves_previous_domain_and_l_tensor_matches_boxes() {
        let t = Tolerance::default();
        let ring = [
            [1e12, 1e12],
            [1e12 + 10., 1e12],
            [1e12 + 10., 1e12 + 8.],
            [1e12, 1e12 + 8.],
        ];
        let old = extrude_polygon(&ring, 3., t).unwrap();
        let new = extrude_polygon_with_holes(&ring, &[], 3., t).unwrap();
        assert_eq!(old.model, new.model);
        assert_eq!(old.history, new.history);
        let ring = [[0., 0.], [6., 0.], [6., 2.], [2., 2.], [2., 6.], [0., 6.]];
        let p = extrude_polygon_with_holes(&ring, &[], 3., t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        let center = [2.2, 2.2, 1.5];
        let mut expected = [[0.; 3]; 3];
        for (volume, c, dimensions) in [
            (36., [3., 1., 1.5], [6., 2., 3.]),
            (24., [1., 4., 1.5], [2., 4., 3.]),
        ] {
            let delta = sub(c, center);
            let squared = dot(delta, delta);
            for i in 0..3 {
                for j in 0..3 {
                    expected[i][j] += if i == j {
                        volume
                            * ((dimensions[(i + 1) % 3] * dimensions[(i + 1) % 3]
                                + dimensions[(i + 2) % 3] * dimensions[(i + 2) % 3])
                                / 12.
                                + squared
                                - delta[i] * delta[i])
                    } else {
                        -volume * delta[i] * delta[j]
                    };
                }
            }
        }
        let actual = p.central_inertia.unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!((actual[i][j] - expected[i][j]).abs() < 1e-9);
            }
        }
    }
    #[test]
    fn concave_caps_have_signed_centroids_and_certified_triangles() {
        let t = Tolerance::default();
        let profiles = vec![
            (
                vec![[0., 0.], [6., 0.], [6., 2.], [2., 2.], [2., 6.], [0., 6.]],
                20.,
                [2.2, 2.2],
            ),
            (u(), 52., [5., 212. / 52.]),
            (
                vec![
                    [0., 0.],
                    [4., 1.],
                    [8., 0.],
                    [7., 4.],
                    [8., 8.],
                    [4., 7.],
                    [0., 8.],
                    [1., 4.],
                ],
                48.,
                [4., 4.],
            ),
        ];
        for (ring, area, center) in profiles {
            let model = extrude_polygon_with_holes(&ring, &[], 3., t).unwrap().model;
            assert!((model.volume() - area * 3.).abs() < 1e-9);
            let p = model.mass_properties(t).unwrap();
            let centroid = p.centroid.unwrap();
            assert!((centroid[0] - center[0]).abs() < 1e-10);
            assert!((centroid[1] - center[1]).abs() < 1e-10);
            for f in [FaceId(0), FaceId(1)] {
                assert!(!face_is_convex(&model, f));
                assert_eq!(
                    triangulate_face(&model, f, t).unwrap().len(),
                    ring.len() - 2
                );
                let frame = model.face_frame(f, [1., 0., 0.], [0.; 3], t).unwrap();
                assert!((frame.origin[0] - center[0]).abs() < 1e-10);
                assert!((frame.origin[1] - center[1]).abs() < 1e-10);
            }
            assert!(!crate::validation::geometry::validate_convex(&model, t).is_empty());
        }
    }
    #[test]
    fn concave_hole_rotations_and_bay_crossings() {
        let t = Tolerance::default();
        let outer = u();
        let hole = vec![[0.5, 3.], [0.5, 5.], [1.5, 5.], [1.5, 3.]];
        let m = extrude_polygon_with_holes(&outer, &[hole], 3., t)
            .unwrap()
            .model;
        assert!((m.volume() - 150.).abs() < 1e-9);
        let properties = m.mass_properties(t).unwrap();
        for i in 1..8 {
            let a = i as f64 * 0.1;
            let s = a.sin() / 14f64.sqrt();
            let q = [s, 2. * s, 3. * s, a.cos()];
            let rotated = transform(&m, [4., 5., 6.], q, t).unwrap().model;
            assert!(rotated.validate(t).valid);
            assert!((rotated.mass_properties(t).unwrap().volume - properties.volume).abs() < 1e-9);
            for f in [FaceId(0), FaceId(1)] {
                assert_eq!(triangulate_face(&rotated, f, t).unwrap().len(), 12);
                rotated.face_frame(f, [1., 0., 0.], [0.; 3], t).unwrap();
            }
        }
        // All vertices are in material, but two edges cross the open bay.
        let bridge = vec![[1., 4.], [1., 5.], [9., 5.], [9., 4.]];
        assert!(validate_polygon_with_holes(&outer, &[bridge], t).is_err());
        let notch = vec![[4., 4.], [4., 5.], [5., 5.], [5., 4.]];
        assert!(validate_polygon_with_holes(&outer, &[notch], t).is_err());
    }
    #[test]
    fn invalid_simple_profiles_and_nonprismatic_mutation() {
        let t = Tolerance::default();
        for ring in [
            vec![[0., 0.], [4., 4.], [0., 4.], [4., 0.]],
            vec![[0., 0.], [4., 0.], [4., 4.], [0., 0.], [0., 4.]],
            vec![[0., 0.], [2., 0.], [4., 0.], [4., 4.], [0., 4.]],
            vec![[0., 0.], [4., 0.], [4., 4.], [2., 1e-9], [0., 4.]],
        ] {
            assert!(validate_polygon_with_holes(&ring, &[], t).is_err());
        }
        let mut m = extrude_polygon_with_holes(&u(), &[], 3., t).unwrap().model;
        // Rebuild exact planes/lines after a shear. Local topology/planarity is
        // valid, but the global right-prism certificate must reject it.
        for v in &mut m.vertices {
            if v.position[2] == 3. {
                v.position[0] += 0.25;
            }
        }
        for edge in &mut m.edges {
            let a = m.vertices[edge.vertices[0].0].position;
            let b = m.vertices[edge.vertices[1].0].position;
            let d = sub(b, a);
            edge.curve = Curve::Line(Line {
                origin: a,
                direction: d.map(|v| v / norm(d)),
            });
        }
        for i in 0..m.faces.len() {
            let p = m.loop_points(m.faces[i].loops[0]).unwrap();
            let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            m.faces[i].surface = Surface::Plane(Plane {
                origin: p[0],
                normal: n.map(|v| v / norm(n)),
            });
        }
        assert!(!m.validate(t).valid);
    }
}
