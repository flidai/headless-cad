//! Bounded analytic two-disk arrangements extruded along a common cylinder axis.
use crate::math::{dot, norm, sub};
use crate::*;
use std::collections::BTreeMap;
fn unsupported() -> KernelError {
    KernelError::Unsupported("circular Boolean requires two resolved solid parallel cylinders; union needs matching axial intervals and cuts must pass through the left operand".into())
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
pub(crate) fn quaternion(x: [f64; 3], z: [f64; 3]) -> [f64; 4] {
    let y = cross(z, x);
    let m = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let trace = m[0][0] + m[1][1] + m[2][2];
    let mut q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            s / 4.,
        ]
    } else {
        let i = if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
            0
        } else if m[1][1] > m[2][2] {
            1
        } else {
            2
        };
        let j = (i + 1) % 3;
        let k = (j + 1) % 3;
        let s = (1. + m[i][i] - m[j][j] - m[k][k]).sqrt() * 2.;
        let mut q = [0.; 4];
        q[i] = s / 4.;
        q[j] = (m[j][i] + m[i][j]) / s;
        q[k] = (m[k][i] + m[i][k]) / s;
        q[3] = (m[k][j] - m[j][k]) / s;
        q
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    for v in &mut q {
        *v /= n;
    }
    q
}
fn history(
    model: ModelSnapshot,
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    maps: [BTreeMap<FaceId, Vec<FaceId>>; 2],
) -> OperationResult {
    let mut inputs = BTreeMap::new();
    for ((name, source), map) in [("left", a), ("right", b)].into_iter().zip(maps) {
        inputs.insert(
            name.into(),
            (0..source.faces.len())
                .map(|i| (FaceId(i), map.get(&FaceId(i)).cloned().unwrap_or_default()))
                .collect(),
        );
    }
    OperationResult {
        history: TopologyHistory {
            generated_faces: BTreeMap::from([(
                "boundary".into(),
                (0..model.faces.len()).map(FaceId).collect(),
            )]),
            input_faces: inputs,
        },
        model,
    }
}
pub(crate) fn boolean(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    op: BooleanOp,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let ga = crate::analytic::recognize(a, Some(t)).map_err(|_| unsupported())?;
    let gb = crate::analytic::recognize(b, Some(t)).map_err(|_| unsupported())?;
    if ga.inner_radius != 0. || gb.inner_radius != 0. {
        return Err(unsupported());
    }
    let mut maps: [BTreeMap<FaceId, Vec<FaceId>>; 2] = Default::default();
    if a == b {
        if op == BooleanOp::Difference {
            return Ok(history(empty(), a, b, maps));
        }
        for map in &mut maps {
            *map = (0..a.faces.len())
                .map(|i| (FaceId(i), vec![FaceId(i)]))
                .collect();
        }
        return Ok(history(a.clone(), a, b, maps));
    }
    let parallel = dot(ga.axis, gb.axis);
    if norm(cross(ga.axis, gb.axis)) > 128. * f64::EPSILON
        || (parallel.abs() - 1.).abs() > 128. * f64::EPSILON
    {
        return Err(unsupported());
    }
    let delta = sub(gb.origin, ga.origin);
    let z = dot(delta, ga.axis);
    let offset: [f64; 3] = std::array::from_fn(|i| delta[i] - z * ga.axis[i]);
    let d = norm(offset);
    let scale = ga.origin.into_iter().chain(gb.origin).map(f64::abs).fold(
        ga.radius
            .max(gb.radius)
            .max(ga.height)
            .max(gb.height)
            .max(d),
        f64::max,
    );
    let error = scale * 4096. * f64::EPSILON;
    if !error.is_finite() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    if d > 0. && d <= error {
        return Err(KernelError::NumericRange);
    }
    let ra = ga.radius;
    let rb = gb.radius;
    if d > ra + rb + t.linear + error {
        match op {
            BooleanOp::Intersection => return Ok(history(empty(), a, b, maps)),
            BooleanOp::Union => return Err(unsupported()),
            BooleanOp::Difference => {
                maps[0] = (0..a.faces.len())
                    .map(|i| (FaceId(i), vec![FaceId(i)]))
                    .collect();
                return Ok(history(a.clone(), a, b, maps));
            }
        }
    }
    let (blo, bhi, bcaps) = if parallel > 0. {
        (z, z + gb.height, [gb.start_cap, gb.end_cap])
    } else {
        (z - gb.height, z, [gb.end_cap, gb.start_cap])
    };
    // Input support planes agree only within the arithmetic certification
    // envelope, never by the much larger modeling tolerance.
    let axis_exact = ga.axis.iter().filter(|v| v.abs() == 1.).count() == 1
        && ga.axis.iter().all(|v| *v == 0. || v.abs() == 1.);
    let same = |x: f64, y: f64| x == y || (!axis_exact && (x - y).abs() <= error);
    let alo: f64 = 0.;
    let ahi = ga.height;
    let commonlo = alo.max(blo);
    let commonhi = ahi.min(bhi);
    if commonhi - commonlo <= t.linear + error {
        if commonhi < commonlo - t.linear - error {
            return match op {
                BooleanOp::Intersection => Ok(history(empty(), a, b, maps)),
                BooleanOp::Difference => {
                    maps[0] = (0..a.faces.len())
                        .map(|i| (FaceId(i), vec![FaceId(i)]))
                        .collect();
                    Ok(history(a.clone(), a, b, maps))
                }
                _ => Err(unsupported()),
            };
        }
        return Err(unsupported());
    }
    for za in [alo, ahi] {
        for zb in [blo, bhi] {
            if !same(za, zb) && (za - zb).abs() <= t.linear + error {
                return Err(unsupported());
            }
        }
    }
    let (lo, hi) = match op {
        BooleanOp::Intersection => (commonlo, commonhi),
        BooleanOp::Union if same(alo, blo) && same(ahi, bhi) => (alo, ahi),
        BooleanOp::Difference
            if (blo < alo - t.linear - error || same(blo, alo))
                && (bhi > ahi + t.linear + error || same(bhi, ahi)) =>
        {
            (alo, ahi)
        }
        _ => return Err(unsupported()),
    };
    let mut owners = vec![];
    let mut holes = vec![];
    let mut hole_owner = None;
    let mut path = PlanarPath {
        start: [0., 0.],
        segments: vec![],
    };
    let full = |center: [f64; 2], r: f64| PlanarPath {
        start: [center[0] + r, center[1]],
        segments: vec![
            PathSegment::ArcTo {
                center,
                end: [center[0] - r, center[1]],
                clockwise: false,
            },
            PathSegment::ArcTo {
                center,
                end: [center[0] + r, center[1]],
                clockwise: false,
            },
        ],
    };
    if d == 0. && ra == rb {
        path = full([0., 0.], ra);
        owners.extend([2, 2]);
        if op == BooleanOp::Difference {
            return Ok(history(empty(), a, b, maps));
        }
    } else if (d - (ra + rb)).abs() <= t.linear + error
        || (d - (ra - rb).abs()).abs() <= t.linear + error
    {
        return Err(unsupported());
    } else if d < (ra - rb).abs() - t.linear - error {
        let smaller = if ra < rb { 0 } else { 1 };
        let selected = match op {
            BooleanOp::Intersection => smaller,
            BooleanOp::Union => 1 - smaller,
            BooleanOp::Difference if smaller == 0 => return Ok(history(empty(), a, b, maps)),
            BooleanOp::Difference => 0,
        };
        path = full(
            if selected == 0 { [0., 0.] } else { [d, 0.] },
            if selected == 0 { ra } else { rb },
        );
        owners.extend([selected, selected]);
        if op == BooleanOp::Difference {
            holes.push(CircleHole {
                center: [d, 0.],
                radius: rb,
            });
            hole_owner = Some(1);
        }
    } else {
        let x = (d + (ra - rb) * (ra + rb) / d) / 2.;
        let y = ((ra - x) * (ra + x)).sqrt();
        if !y.is_finite() || y <= t.linear + error {
            return Err(unsupported());
        }
        let upper = [x, y];
        let lower = [x, -y];
        let alpha = y.atan2(x);
        let beta = y.atan2(x - d);
        path.start = upper;
        let mut append =
            |center: [f64; 2], radius: f64, start: f64, sweep: f64, end: [f64; 2], owner: usize| {
                let n = if sweep.abs() > std::f64::consts::PI {
                    2
                } else {
                    1
                };
                for k in 1..=n {
                    let angle = start + sweep * k as f64 / n as f64;
                    let end = if k == n {
                        end
                    } else {
                        [
                            center[0] + radius * angle.cos(),
                            center[1] + radius * angle.sin(),
                        ]
                    };
                    path.segments.push(PathSegment::ArcTo {
                        center,
                        end,
                        clockwise: sweep < 0.,
                    });
                    owners.push(owner);
                }
            };
        match op {
            BooleanOp::Intersection => {
                append(
                    [d, 0.],
                    rb,
                    beta,
                    std::f64::consts::TAU - 2. * beta,
                    lower,
                    1,
                );
                append([0., 0.], ra, -alpha, 2. * alpha, upper, 0);
            }
            BooleanOp::Union => {
                append(
                    [0., 0.],
                    ra,
                    alpha,
                    std::f64::consts::TAU - 2. * alpha,
                    lower,
                    0,
                );
                append([d, 0.], rb, -beta, 2. * beta, upper, 1);
            }
            BooleanOp::Difference => {
                append(
                    [0., 0.],
                    ra,
                    alpha,
                    std::f64::consts::TAU - 2. * alpha,
                    lower,
                    0,
                );
                append(
                    [d, 0.],
                    rb,
                    -beta,
                    -(std::f64::consts::TAU - 2. * beta),
                    upper,
                    1,
                );
            }
        }
    }
    let local = extrude_path(&path, &holes, hi - lo, t)?.model;
    let xdir = if d > error {
        offset.map(|v| v / d)
    } else {
        ga.x_direction
    };
    let origin = std::array::from_fn(|i| ga.origin[i] + lo * ga.axis[i]);
    let model = transform(&local, origin, quaternion(xdir, ga.axis), t)?.model;
    for (i, owner) in owners.into_iter().enumerate() {
        for (side, map) in maps.iter_mut().enumerate() {
            if owner == side || owner == 2 {
                map.entry(if side == 0 { ga.wall } else { gb.wall })
                    .or_default()
                    .push(FaceId(i + 2));
            }
        }
    }
    if let Some(side) = hole_owner {
        maps[side]
            .entry(gb.wall)
            .or_default()
            .push(FaceId(path.segments.len() + 2));
    }
    for (side, caps, zs) in [
        (0, [ga.start_cap, ga.end_cap], [alo, ahi]),
        (1, bcaps, [blo, bhi]),
    ] {
        if side == 1 && op == BooleanOp::Difference {
            continue;
        }
        for (out, zout) in [lo, hi].into_iter().enumerate() {
            for (cap, zsource) in caps.into_iter().zip(zs) {
                if same(zout, zsource) {
                    maps[side].entry(cap).or_default().push(FaceId(out));
                }
            }
        }
    }
    // Certify the reconstructed world supports against every attributed source,
    // including the far axial end (a tiny direction error grows with height).
    for (source, mapping) in [a, b].into_iter().zip(&maps) {
        for (input, outputs) in mapping {
            for output in outputs {
                match (
                    &source.faces[input.0].surface,
                    &model.faces[output.0].surface,
                ) {
                    (Surface::Cylinder(input), Surface::Cylinder(output)) => {
                        let far =
                            std::array::from_fn(|i| output.origin[i] + (hi - lo) * output.axis[i]);
                        if (input.radius - output.radius).abs() > error
                            || norm(cross(input.axis, output.axis)) > 128. * f64::EPSILON
                            || [output.origin, far]
                                .into_iter()
                                .any(|p| norm(cross(sub(p, input.origin), input.axis)) > error)
                        {
                            return Err(KernelError::NumericRange);
                        }
                    }
                    (Surface::Plane(input), Surface::Plane(output)) => {
                        if norm(cross(input.normal, output.normal)) > 128. * f64::EPSILON
                            || dot(sub(output.origin, input.origin), input.normal).abs() > error
                        {
                            return Err(KernelError::NumericRange);
                        }
                    }
                    _ => return Err(unsupported()),
                }
            }
        }
    }
    Ok(history(model, a, b, maps))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cylinder(r: f64, h: f64, p: [f64; 3]) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(&extrude_circle(r, h, t).unwrap().model, p, t)
            .unwrap()
            .model
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < b.abs().max(1.) * 1e-9, "{a} != {b}");
    }
    #[test]
    fn equal_disks_have_independent_lens_crescent_union_moments_and_history() {
        let t = Tolerance::default();
        let r = 10_f64;
        let d = 10_f64;
        let h = 8.;
        let a = cylinder(r, h, [0.; 3]);
        let b = cylinder(r, h, [d, 0., 0.]);
        // Independent integral of two symmetric circular segments about x=d/2.
        let c = d / 2.;
        let q = (r * r - c * c).sqrt();
        let angle = (c / r).acos();
        let area = 2. * (r * r * angle - c * q);
        let sx = area * c;
        let yy = r.powi(4) * angle / 2. - c * q * (5. * r * r - 2. * c * c) / 6.;
        let rawxx = r.powi(4) * angle / 2. + c * q * (r * r - 2. * c * c) / 2.
            - 8. * c * q.powi(3) / 3.
            + 2. * c * c * (r * r * angle - c * q);
        // rawxx above is centered at the midpoint; translate to the left origin.
        let xx = rawxx + area * c * c;
        for op in [
            BooleanOp::Intersection,
            BooleanOp::Difference,
            BooleanOp::Union,
        ] {
            let out = boolean(&a, &b, op, t).unwrap();
            let p = out.model.mass_properties(t).unwrap();
            let disk = std::f64::consts::PI * r * r;
            let (ar, mx, x2, y2) = match op {
                BooleanOp::Intersection => (area, sx, xx, yy),
                BooleanOp::Difference => (
                    disk - area,
                    -sx,
                    disk * r * r / 4. - xx,
                    disk * r * r / 4. - yy,
                ),
                BooleanOp::Union => (
                    2. * disk - area,
                    disk * d - sx,
                    disk * (r * r / 2. + d * d) - xx,
                    disk * r * r / 2. - yy,
                ),
            };
            let cx = mx / ar;
            near(p.volume, ar * h);
            near(p.centroid.unwrap()[0], cx);
            let tensor = p.central_inertia.unwrap();
            near(tensor[0][0], y2 * h + ar * h.powi(3) / 12.);
            near(tensor[1][1], (x2 - ar * cx * cx) * h + ar * h.powi(3) / 12.);
            near(tensor[2][2], (x2 - ar * cx * cx + y2) * h);
            assert!(out.model.validate(t).valid);
            assert!(tessellate_analytic(&out.model, t, TessellationPolicy::default()).is_ok());
            for side in ["left", "right"] {
                assert_eq!(out.history.input_faces[side].len(), 3);
            }
            assert_eq!(
                out.history.input_faces["left"][&FaceId(2)].len(),
                if op == BooleanOp::Intersection { 1 } else { 2 }
            );
            assert_eq!(
                out.history.input_faces["right"][&FaceId(2)].len(),
                if op == BooleanOp::Union { 2 } else { 1 }
            );
            assert_eq!(
                out.history.input_faces["right"][&FaceId(0)].len(),
                usize::from(op != BooleanOp::Difference)
            );
        }
    }
    #[test]
    fn conservative_numeric_gates_and_identity_do_not_snap_inputs() {
        let t = Tolerance::default();
        for scale in [1e-4, 1., 1e3] {
            let a = cylinder(10. * scale, 8. * scale, [0.; 3]);
            for gap in [-t.linear / 2., 0., t.linear / 2.] {
                let b = cylinder(8. * scale, 8. * scale, [18. * scale + gap, 0., 0.]);
                for op in [
                    BooleanOp::Union,
                    BooleanOp::Intersection,
                    BooleanOp::Difference,
                ] {
                    assert!(boolean(&a, &b, op, t).is_err());
                }
            }
            assert_eq!(boolean(&a, &a, BooleanOp::Union, t).unwrap().model, a);
            assert!(boolean(&a, &a, BooleanOp::Difference, t)
                .unwrap()
                .model
                .faces
                .is_empty());
            let b = cylinder(8. * scale, 8. * scale, [10. * scale, 0., 1e-12 * scale]);
            assert!(boolean(&a, &b, BooleanOp::Intersection, t).is_err());
        }
        let a = cylinder(10., 8., [0.; 3]);
        let b = cylinder(8., 8., [10., 0., 0.]);
        let skew = transform(&b, [0.; 3], [0., (0.01_f64).sin(), 0., (0.01_f64).cos()], t)
            .unwrap()
            .model;
        assert!(boolean(&a, &skew, BooleanOp::Intersection, t).is_err());
        let annulus = extrude_annulus(10., 2., 8., t).unwrap().model;
        assert!(boolean(&annulus, &b, BooleanOp::Intersection, t).is_err());
        let far_short = cylinder(8., 2., [40., 0., 3.]);
        assert_eq!(
            boolean(&a, &far_short, BooleanOp::Difference, t)
                .unwrap()
                .model,
            a
        );
    }
    #[test]
    fn intersection_cap_history_uses_actual_axial_support_and_opposite_axis_ids() {
        let t = Tolerance::default();
        let a = cylinder(10., 8., [0.; 3]);
        for reversed in [false, true] {
            for (bottom, top) in [(3., 11.), (2., 6.)] {
                let b = if reversed {
                    transform(
                        &cylinder(8., top - bottom, [0.; 3]),
                        [10., 0., top],
                        [1., 0., 0., 0.],
                        t,
                    )
                    .unwrap()
                    .model
                } else {
                    cylinder(8., top - bottom, [10., 0., bottom])
                };
                for rotated in [false, true] {
                    let (a, b) = if rotated {
                        let rotation = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
                        (
                            transform(&a, [12., -4., 7.], rotation, t).unwrap().model,
                            transform(&b, [12., -4., 7.], rotation, t).unwrap().model,
                        )
                    } else {
                        (a.clone(), b.clone())
                    };
                    let out = boolean(&a, &b, BooleanOp::Intersection, t).unwrap();
                    let left = &out.history.input_faces["left"];
                    let right = &out.history.input_faces["right"];
                    assert!(left[&FaceId(0)].is_empty());
                    assert_eq!(
                        left[&FaceId(1)],
                        if top > 8. { vec![FaceId(1)] } else { vec![] }
                    );
                    let lower_id = FaceId(usize::from(reversed));
                    let upper_id = FaceId(usize::from(!reversed));
                    assert_eq!(right[&lower_id], vec![FaceId(0)]);
                    assert_eq!(
                        right[&upper_id],
                        if top < 8. { vec![FaceId(1)] } else { vec![] }
                    );
                    assert_eq!(left[&FaceId(2)].len(), 1);
                    assert_eq!(right[&FaceId(2)].len(), 1);
                    // Every result cap has exactly one surviving source in these
                    // deliberately noncoplanar cases; removed source caps remain explicit.
                    for cap in [FaceId(0), FaceId(1)] {
                        let count = left
                            .iter()
                            .chain(right)
                            .filter(|(input, _)| input.0 < 2)
                            .filter(|(_, outputs)| outputs.contains(&cap))
                            .count();
                        assert_eq!(count, 1);
                    }
                }
            }
        }
    }
    #[test]
    fn axial_domains_containment_and_opposite_rigid_axes() {
        let t = Tolerance::default();
        let a = cylinder(10., 8., [0.; 3]);
        let b = cylinder(8., 20., [10., 0., -6.]);
        assert!(boolean(&a, &b, BooleanOp::Union, t).is_err());
        assert!(boolean(&a, &b, BooleanOp::Difference, t).is_ok());
        let short = cylinder(8., 4., [10., 0., 2.]);
        assert!(boolean(&a, &short, BooleanOp::Difference, t).is_err());
        let intersect = boolean(&a, &short, BooleanOp::Intersection, t).unwrap();
        near(intersect.model.bounds().min[2], 2.);
        near(intersect.model.bounds().max[2], 6.);
        let inner = cylinder(2., 8., [2., 0., 0.]);
        let cut = boolean(&a, &inner, BooleanOp::Difference, t).unwrap();
        near(
            cut.model.mass_properties(t).unwrap().volume,
            96. * 8. * std::f64::consts::PI,
        );
        let axis = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
        let moved_a = transform(&a, [12., -4., 7.], axis, t).unwrap().model;
        let moved_b = transform(&b, [12., -4., 7.], axis, t).unwrap().model;
        let expected = boolean(&a, &b, BooleanOp::Difference, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap()
            .volume;
        near(
            boolean(&moved_a, &moved_b, BooleanOp::Difference, t)
                .unwrap()
                .model
                .mass_properties(t)
                .unwrap()
                .volume,
            expected,
        );
        let reverse = transform(
            &cylinder(8., 20., [0.; 3]),
            [10., 0., 14.],
            [1., 0., 0., 0.],
            t,
        )
        .unwrap()
        .model;
        near(
            boolean(&a, &reverse, BooleanOp::Difference, t)
                .unwrap()
                .model
                .mass_properties(t)
                .unwrap()
                .volume,
            expected,
        );
        for gap in [0., t.linear / 2., -t.linear / 2.] {
            assert!(boolean(
                &a,
                &cylinder(8., 8., [18. + gap, 0., 0.]),
                BooleanOp::Intersection,
                t
            )
            .is_err());
        }
        let far = cylinder(8., 8., [30., 0., 0.]);
        assert!(boolean(&a, &far, BooleanOp::Intersection, t)
            .unwrap()
            .model
            .faces
            .is_empty());
        assert_eq!(
            boolean(&a, &far, BooleanOp::Difference, t).unwrap().model,
            a
        );
    }
}
