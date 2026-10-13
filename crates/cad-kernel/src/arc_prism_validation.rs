//! Analytic simplicity and containment predicates for trimmed line/circle paths.
//! No tessellation participates in native validation.
use super::*;
const TAU: f64 = std::f64::consts::TAU;
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: [f64; 2], s: f64) -> [f64; 2] {
    a.map(|v| v * s)
}
fn on_arc(s: &Segment, p: [f64; 2], error: f64) -> bool {
    let Some((c, r, start, sweep)) = s.arc else {
        return false;
    };
    let v = sub2(p, c);
    let angle = v[1].atan2(v[0]);
    let progress = ((angle - start) * sweep.signum()).rem_euclid(TAU);
    let margin = error / r + 128. * f64::EPSILON;
    progress <= sweep.abs() + margin || TAU - progress <= margin
}
fn on_line(s: &Segment, p: [f64; 2], error: f64) -> bool {
    let d = sub2(s.b, s.a);
    let length = len(d);
    let t = dot2(sub2(p, s.a), d) / length;
    t >= -error && t <= length + error
}
pub(super) fn distance(s: &Segment, p: [f64; 2], error: f64) -> f64 {
    let mut result = len(sub2(p, s.a)).min(len(sub2(p, s.b)));
    if let Some((c, r, _, _)) = s.arc {
        let v = sub2(p, c);
        let d = len(v);
        if d > 0. {
            let q = add(c, mul(v, r / d));
            if on_arc(s, q, error) {
                result = result.min((d - r).abs());
            }
        } else {
            result = result.min(r);
        }
    } else {
        let d = sub2(s.b, s.a);
        let length = len(d);
        let t = (dot2(sub2(p, s.a), d) / length / length).clamp(0., 1.);
        result = result.min(len(sub2(p, add(s.a, mul(d, t)))));
    }
    result
}
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    let point = |v: [f64; 2]| robust::Coord { x: v[0], y: v[1] };
    robust::orient2d(point(a), point(b), point(c))
}
fn intersections(a: &Segment, b: &Segment, error: f64) -> Vec<[f64; 2]> {
    match (a.arc, b.arc) {
        (None, None) => {
            let u = sub2(a.b, a.a);
            let v = sub2(b.b, b.a);
            let den = orient([0., 0.], u, v);
            if den == 0. {
                return [a.a, a.b, b.a, b.b]
                    .into_iter()
                    .filter(|&p| distance(a, p, error) <= error && distance(b, p, error) <= error)
                    .collect();
            }
            if let Some(p) = [a.a, a.b].into_iter().find(|p| *p == b.a || *p == b.b) {
                return vec![p];
            }
            let w = sub2(b.a, a.a);
            let t = cross2(w, v) / den;
            let q = cross2(w, u) / den;
            if t >= -error / len(u)
                && t <= 1. + error / len(u)
                && q >= -error / len(v)
                && q <= 1. + error / len(v)
            {
                vec![add(a.a, mul(u, t))]
            } else {
                vec![]
            }
        }
        (Some(_), None) => intersections(b, a, error),
        (None, Some((c, r, _, _))) => {
            let v = sub2(a.b, a.a);
            let u = mul(v, 1. / len(v));
            let w = sub2(c, a.a);
            let along = dot2(w, u);
            let normal = cross2(u, w);
            let abs = normal.abs();
            if abs > r + error {
                return vec![];
            }
            let middle = add(a.a, mul(u, along));
            // A declared common endpoint factors the quadratic and avoids
            // sqrt cancellation at valid line/arc tangent joins.
            let shared = [a.a, a.b].into_iter().find(|p| *p == b.a || *p == b.b);
            let candidates = if let Some(p) = shared {
                vec![p, add(middle, sub2(middle, p))]
            } else {
                let h = ((r - abs).max(0.) * (r + abs)).sqrt();
                vec![add(middle, mul(u, h)), add(middle, mul(u, -h))]
            };
            candidates
                .into_iter()
                .filter(|&p| on_line(a, p, error) && on_arc(b, p, error))
                .collect()
        }
        (Some((ca, ra, sa, wa)), Some((cb, rb, _, _))) => {
            let v = sub2(cb, ca);
            let d = len(v);
            if d <= error && (ra - rb).abs() <= error {
                let midpoint = |c, r, start: f64, sweep: f64| {
                    add(
                        c,
                        [
                            r * (start + sweep / 2.).cos(),
                            r * (start + sweep / 2.).sin(),
                        ],
                    )
                };
                let (_, _, sb, wb) = b.arc.unwrap();
                return [
                    a.a,
                    a.b,
                    b.a,
                    b.b,
                    midpoint(ca, ra, sa, wa),
                    midpoint(cb, rb, sb, wb),
                ]
                .into_iter()
                .filter(|&p| on_arc(a, p, error) && on_arc(b, p, error))
                .collect();
            }
            if d <= error || d > ra + rb + error || d < (ra - rb).abs() - error {
                return vec![];
            }
            let u = mul(v, 1. / d);
            let n = [-u[1], u[0]];
            let shared = [a.a, a.b].into_iter().find(|p| *p == b.a || *p == b.b);
            let candidates = if let Some(p) = shared {
                let foot = add(ca, mul(u, dot2(sub2(p, ca), u)));
                vec![p, add(foot, sub2(foot, p))]
            } else {
                let along = (d + (ra - rb) * (ra + rb) / d) / 2.;
                let middle = add(ca, mul(u, along));
                let h = (((ra + rb + d)
                    * (ra + rb - d).max(0.)
                    * (d + ra - rb).max(0.)
                    * (d - ra + rb).max(0.))
                .sqrt())
                    / (2. * d);
                vec![add(middle, mul(n, h)), add(middle, mul(n, -h))]
            };
            candidates
                .into_iter()
                .filter(|&p| on_arc(a, p, error) && on_arc(b, p, error))
                .collect()
        }
    }
}
pub(super) fn pair_distance(a: &Segment, b: &Segment, error: f64) -> f64 {
    // Adaptive signs prevent a rounded intersection coordinate from hiding a
    // proper crossing of almost parallel line segments.
    if a.arc.is_none() && b.arc.is_none() {
        let sides = [
            orient(a.a, a.b, b.a),
            orient(a.a, a.b, b.b),
            orient(b.a, b.b, a.a),
            orient(b.a, b.b, a.b),
        ];
        if sides.iter().all(|v| *v != 0.)
            && sides[0].signum() != sides[1].signum()
            && sides[2].signum() != sides[3].signum()
        {
            return 0.;
        }
    }
    if !intersections(a, b, error).is_empty() {
        return 0.;
    }
    let mut result = distance(a, b.a, error)
        .min(distance(a, b.b, error))
        .min(distance(b, a.a, error))
        .min(distance(b, a.b, error));
    match (a.arc, b.arc) {
        (Some(_), None) => return pair_distance(b, a, error),
        (None, Some((c, r, _, _))) => {
            let u = sub2(a.b, a.a);
            let n = mul([-u[1], u[0]], 1. / len(u));
            for sign in [-1., 1.] {
                let p = add(c, mul(n, sign * r));
                if on_arc(b, p, error) {
                    result = result.min(distance(a, p, error));
                }
            }
        }
        (Some((ca, ra, _, _)), Some((cb, rb, _, _))) => {
            let v = sub2(cb, ca);
            let d = len(v);
            if d > error {
                let u = mul(v, 1. / d);
                for sa in [-1., 1.] {
                    for sb in [-1., 1.] {
                        let p = add(ca, mul(u, sa * ra));
                        let q = add(cb, mul(u, sb * rb));
                        if on_arc(a, p, error) && on_arc(b, q, error) {
                            result = result.min(len(sub2(p, q)));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    result
}
pub(super) fn simple(seg: &[Segment], tol: Tolerance, error: f64) -> Result<(), KernelError> {
    for i in 0..seg.len() {
        for j in i + 1..seg.len() {
            let a = &seg[i];
            let b = &seg[j];
            let adjacent = j == i + 1 || (i == 0 && j == seg.len() - 1);
            if adjacent {
                let shared: Vec<_> = [a.a, a.b]
                    .into_iter()
                    .filter(|p| *p == b.a || *p == b.b)
                    .collect();
                // A second root can collapse onto a near-tangent join. Permit
                // roundoff-sized smooth joins, but reject the unresolved band
                // between that arithmetic floor and the support error budget.
                if a.arc.is_some() || b.arc.is_some() {
                    for p in &shared {
                        let tangent = |s: &Segment| {
                            if let Some((c, r, _, _)) = s.arc {
                                let v = sub2(*p, c);
                                [-v[1] / r, v[0] / r]
                            } else {
                                let v = sub2(s.b, s.a);
                                mul(v, 1. / len(v))
                            }
                        };
                        let sine = cross2(tangent(a), tangent(b)).abs();
                        let length = len(sub2(a.b, a.a)).min(len(sub2(b.b, b.a)));
                        if sine > 256. * f64::EPSILON && sine <= 16. * error / length {
                            return Err(KernelError::NumericRange);
                        }
                    }
                }
                if intersections(a, b, error)
                    .iter()
                    .any(|p| !shared.iter().any(|q| len(sub2(*p, *q)) <= error * 8.))
                {
                    return Err(bad());
                }
                // Endpoints other than the declared join cannot approach the other
                // segment within tolerance (including a narrow adjacent hairpin).
                for (s, p) in [(a, b.a), (a, b.b), (b, a.a), (b, a.b)] {
                    if !shared.contains(&p) && distance(s, p, error) <= tol.linear + error {
                        return Err(bad());
                    }
                }
            } else if pair_distance(a, b, error) <= tol.linear + error {
                return Err(bad());
            }
        }
    }
    Ok(())
}
pub(super) fn inside(seg: &[Segment], p: [f64; 2], error: f64) -> Result<bool, KernelError> {
    // Select a ray whose ordinate avoids vertices and support tangencies; all
    // intersections then lie in open trimmed intervals with unambiguous signs.
    for attempt in 0..64 {
        let angle = attempt as f64 * 2.399963229728653;
        let u = [angle.cos(), angle.sin()];
        let n = [-u[1], u[0]];
        if seg.iter().any(|s| dot2(sub2(s.a, p), n).abs() <= error) {
            continue;
        }
        let mut winding = 0;
        let mut ambiguous = false;
        for s in seg {
            if let Some((c, r, _, sweep)) = s.arc {
                let relative = sub2(c, p);
                let cy = dot2(relative, n);
                if (cy.abs() - r).abs() <= error {
                    ambiguous = true;
                    break;
                }
                if cy.abs() > r {
                    continue;
                }
                let dx = ((r - cy.abs()) * (r + cy.abs())).sqrt();
                let cx = dot2(relative, u);
                for radial_x in [-dx, dx] {
                    let point = add(c, add(mul(u, radial_x), mul(n, -cy)));
                    if cx + radial_x > 0. && on_arc(s, point, error) {
                        winding += if radial_x * sweep > 0. { 1 } else { -1 };
                    }
                }
            } else {
                let a = sub2(s.a, p);
                let b = sub2(s.b, p);
                let ay = dot2(a, n);
                let by = dot2(b, n);
                if (ay > 0.) != (by > 0.) {
                    let t = ay / (ay - by);
                    let x = dot2(add(a, mul(sub2(b, a), t)), u);
                    if x > 0. {
                        winding += if by > ay { 1 } else { -1 };
                    }
                }
            }
        }
        if !ambiguous {
            return Ok(winding == 1);
        }
    }
    Err(KernelError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arc(c: [f64; 2], r: f64, start: f64, sweep: f64) -> Segment {
        let p = |t: f64| [c[0] + r * t.cos(), c[1] + r * t.sin()];
        Segment {
            a: p(start),
            b: p(start + sweep),
            arc: Some((c, r, start, sweep)),
        }
    }
    #[test]
    fn trimmed_supports_do_not_imply_curve_intersections() {
        let pi = std::f64::consts::PI;
        // The support circles intersect, but these outward-facing quarters do not.
        let a = arc([0., 0.], 2., pi * 0.75, pi * 0.5);
        let b = arc([2., 0.], 2., -pi * 0.25, pi * 0.5);
        assert!(intersections(&a, &b, 1e-12).is_empty());
        assert!(pair_distance(&a, &b, 1e-12) > 4.);
        let c = arc([0., 0.], 2., 0., pi * 0.25);
        let d = arc([0., 0.], 2., pi, pi * 0.25);
        assert!(intersections(&c, &d, 1e-12).is_empty());
        assert!(pair_distance(&c, &d, 1e-12) > 3.);
        let crossing = arc([2., 0.], 2., pi / 2., pi);
        let crossing_a = arc([0., 0.], 2., -pi / 2., pi);
        assert_eq!(intersections(&crossing_a, &crossing, 1e-12).len(), 2);
        let tangent = arc([4., 0.], 2., pi / 2., pi);
        assert_eq!(pair_distance(&crossing_a, &tangent, 1e-12), 0.);
    }
    #[test]
    fn unresolved_join_and_near_contacts_fail_at_multiple_scales() {
        for scale in [1e-4, 1., 1e4] {
            let e = scale * 1e-12;
            let t = Tolerance {
                linear: scale * 1e-7,
            };
            let a = Segment {
                a: [-scale, scale * 1e-12],
                b: [0., 0.],
                arc: None,
            };
            let mut b = arc(
                [0., scale],
                scale,
                -std::f64::consts::FRAC_PI_2,
                -std::f64::consts::FRAC_PI_2,
            );
            b.a = [0., 0.];
            b.b = [-scale, scale];
            assert!(simple(&[a, b], t, e).is_err());
            let upper = arc([0., 0.], scale, 0., std::f64::consts::PI);
            let line = Segment {
                a: [-scale, scale + t.linear / 2.],
                b: [scale, scale + t.linear / 2.],
                arc: None,
            };
            assert!(pair_distance(&upper, &line, e) <= t.linear + e);
            let a = Segment {
                a: [0., 0.],
                b: [scale, scale * 1e-10],
                arc: None,
            };
            let b = Segment {
                a: [0., scale * 1e-10],
                b: [scale, 0.],
                arc: None,
            };
            assert_eq!(pair_distance(&a, &b, e), 0.);
        }
    }
    #[test]
    fn hole_clearance_reserves_arc_support_scale_roundoff() {
        let t = Tolerance::default();
        let r = 10000_f64;
        let y = r - (r * r - 100.).sqrt();
        let path = PlanarPath {
            start: [-20., -20.],
            segments: vec![
                PathSegment::LineTo { end: [20., -20.] },
                PathSegment::LineTo { end: [20., y] },
                PathSegment::LineTo { end: [10., y] },
                PathSegment::ArcTo {
                    center: [0., r],
                    end: [-10., y],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [-20., y] },
                PathSegment::LineTo { end: [-20., -20.] },
            ],
        };
        let hole = |extra| CircleHole {
            center: [0., -1. - (t.linear + extra)],
            radius: 1.,
        };
        assert!(validate_path(&path, &[hole(1e-9)], t).is_err());
        assert!(validate_path(&path, &[hole(1e-8)], t).is_ok());
    }
    fn saddle(r: f64) -> PlanarPath {
        PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [80., 0.] },
                PathSegment::LineTo { end: [80., 40.] },
                PathSegment::LineTo {
                    end: [40. + r, 40.],
                },
                PathSegment::ArcTo {
                    center: [40., 40.],
                    end: [40. - r, 40.],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [0., 40.] },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8 * b.abs().max(1.), "{a} != {b}")
    }
    #[test]
    fn inward_semicircle_has_independent_composite_moments_and_transforms() {
        let pi = std::f64::consts::PI;
        let t = Tolerance::default();
        for r in [10., 14., 18.] {
            let h = 16.;
            let holes = [
                CircleHole {
                    center: [10., 12.],
                    radius: 3.,
                },
                CircleHole {
                    center: [70., 12.],
                    radius: 3.,
                },
            ];
            let m = extrude_path(&saddle(r), &holes, h, t).unwrap().model;
            let p = m.mass_properties(t).unwrap();
            let area = 3200. - pi * r * r / 2. - 18. * pi;
            let sy = 3200. * 20. - pi * r * r / 2. * (40. - 4. * r / (3. * pi)) - 18. * pi * 12.;
            let cy = sy / area;
            // Raw second area moments from rectangle, lower half-disk and circles.
            let xx = 3200. * 80. * 80. / 12. - pi * r.powi(4) / 8. - 18. * pi * (900. + 9. / 4.);
            let yy = 3200. * (40. * 40. / 3.) - pi * r * r / 2. * 1600. + 80. * 2. * r.powi(3) / 3.
                - pi * r.powi(4) / 8.
                - 18. * pi * (144. + 9. / 4.)
                - area * cy * cy;
            near(p.volume, area * h);
            near(p.centroid.unwrap()[0], 40.);
            near(p.centroid.unwrap()[1], cy);
            let tensor = p.central_inertia.unwrap();
            near(tensor[0][0], h * yy + area * h.powi(3) / 12.);
            near(tensor[1][1], h * xx + area * h.powi(3) / 12.);
            near(tensor[2][2], h * (xx + yy));
            near(tensor[0][1], 0.);
            assert!(tessellate_analytic(&m, t, TessellationPolicy::default()).is_ok());
            let moved = transform(
                &m,
                [17., -8., 3.],
                [0., (0.3_f64).sin(), 0., (0.3_f64).cos()],
                t,
            )
            .unwrap()
            .model;
            assert!(moved.validate(t).valid);
            near(moved.mass_properties(t).unwrap().volume, p.volume);
            assert!(tessellate_analytic(&moved, t, TessellationPolicy::default()).is_ok());
            // The removed seat and exterior are not material, even when a disk avoids the boundary.
            for center in [[40., 39.], [100., 10.]] {
                assert!(validate_path(
                    &saddle(r),
                    &[CircleHole {
                        center,
                        radius: 0.5
                    }],
                    t
                )
                .is_err());
            }
            for gap in [0., t.linear / 2.] {
                assert!(validate_path(
                    &saddle(r),
                    &[CircleHole {
                        center: [40., 40. - r - 1. - gap],
                        radius: 1.
                    }],
                    t
                )
                .is_err());
            }
        }
    }
    #[test]
    fn adjacent_same_circle_semicircles_and_concave_lines_are_supported() {
        let path = PlanarPath {
            start: [5., 0.],
            segments: vec![
                PathSegment::ArcTo {
                    center: [0., 0.],
                    end: [-5., 0.],
                    clockwise: false,
                },
                PathSegment::ArcTo {
                    center: [0., 0.],
                    end: [5., 0.],
                    clockwise: false,
                },
            ],
        };
        let t = Tolerance::default();
        near(
            extrude_path(&path, &[], 3., t)
                .unwrap()
                .model
                .mass_properties(t)
                .unwrap()
                .volume,
            75. * std::f64::consts::PI,
        );
        let path = PlanarPath {
            start: [0., 0.],
            segments: [
                [10., 0.],
                [10., 3.],
                [3., 3.],
                [3., 10.],
                [0., 10.],
                [0., 0.],
            ]
            .into_iter()
            .map(|end| PathSegment::LineTo { end })
            .collect(),
        };
        near(
            extrude_path(&path, &[], 3., t)
                .unwrap()
                .model
                .mass_properties(t)
                .unwrap()
                .volume,
            153.,
        );
    }
}
