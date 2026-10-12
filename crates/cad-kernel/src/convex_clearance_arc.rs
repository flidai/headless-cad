//! Analytic supports for bounded convex line/arc prism sections.
use super::*;
#[derive(Clone)]
pub(super) struct ArcShape {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x: [f64; 3],
    pub height: f64,
    segments: Vec<ArcSegment>,
}
#[derive(Clone)]
struct ArcSegment {
    a: [f64; 2],
    b: [f64; 2],
    circle: Option<([f64; 2], f64, [f64; 2], f64)>,
}
fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn unit2(a: [f64; 2]) -> [f64; 2] {
    let n = a[0].hypot(a[1]);
    a.map(|v| v / n)
}
impl ArcShape {
    pub(super) fn new(g: &ArcPrismGeometry, reference: [f64; 3], scale: f64) -> Self {
        let mut a = g.path.start;
        let mut segments = vec![];
        for s in &g.path.segments {
            let b = match s {
                PathSegment::LineTo { end } | PathSegment::ArcTo { end, .. } => *end,
            };
            let circle = match s {
                PathSegment::ArcTo { center, .. } => {
                    let u = sub2(a, *center);
                    let v = sub2(b, *center);
                    let sweep = cross2(u, v)
                        .atan2(dot2(u, v))
                        .rem_euclid(std::f64::consts::TAU);
                    Some((
                        center.map(|x| x / scale),
                        u[0].hypot(u[1]) / scale,
                        unit2(u),
                        sweep,
                    ))
                }
                _ => None,
            };
            segments.push(ArcSegment {
                a: a.map(|x| x / scale),
                b: b.map(|x| x / scale),
                circle,
            });
            a = b;
        }
        Self {
            origin: mul(sub(g.origin, reference), 1. / scale),
            axis: g.axis,
            x: g.x_direction,
            height: g.height / scale,
            segments,
        }
    }
    fn planar(&self, d: [f64; 2], upper: bool) -> ([f64; 2], f64) {
        let mut point = self.segments[0].a;
        let mut value = dot2(point, d);
        let len = d[0].hypot(d[1]);
        for s in &self.segments {
            for p in [s.a, s.b] {
                let v = dot2(p, d);
                if v > value {
                    point = p;
                    value = v;
                }
            }
            if let Some((c, r, u, sweep)) = s.circle {
                if len > 0. {
                    let angle = cross2(u, d)
                        .atan2(dot2(u, d))
                        .rem_euclid(std::f64::consts::TAU);
                    if angle <= sweep {
                        let p = [c[0] + r * d[0] / len, c[1] + r * d[1] / len];
                        let v = dot2(c, d) + r * len;
                        if v > value {
                            point = p;
                            value = v;
                        }
                    } else if upper {
                        let end = [
                            u[0] * sweep.cos() - u[1] * sweep.sin(),
                            u[0] * sweep.sin() + u[1] * sweep.cos(),
                        ];
                        let margin = 128. * f64::EPSILON * len;
                        if cross2(u, d) >= -margin && cross2(d, end) >= -margin {
                            value = value.max(dot2(c, d) + r * len);
                        }
                    }
                }
            }
        }
        (point, value)
    }
    pub(super) fn support(&self, d: [f64; 3]) -> [f64; 3] {
        let y = cross(self.axis, self.x);
        let (p, _) = self.planar([dot(d, self.x), dot(d, y)], false);
        add(
            self.origin,
            add(
                add(mul(self.x, p[0]), mul(y, p[1])),
                mul(
                    self.axis,
                    if dot(d, self.axis) >= 0. {
                        self.height
                    } else {
                        0.
                    },
                ),
            ),
        )
    }
    pub(super) fn support_value(&self, d: [f64; 3]) -> f64 {
        let y = cross(self.axis, self.x);
        let (_, v) = self.planar([dot(d, self.x), dot(d, y)], true);
        dot(self.origin, d) + v + self.height * dot(d, self.axis).max(0.)
    }
}
pub(super) fn convex(g: &ArcPrismGeometry) -> Result<(), KernelError> {
    if !g.circular_holes.is_empty() {
        return Err(unsupported());
    }
    let shape = ArcShape::new(g, [0.; 3], 1.);
    let mut tangents = vec![];
    for s in &shape.segments {
        let pair = if let Some((_, _, u, sweep)) = s.circle {
            if sweep <= 0. || sweep > std::f64::consts::PI + 128. * f64::EPSILON {
                return Err(unsupported());
            }
            let v = [
                u[0] * sweep.cos() - u[1] * sweep.sin(),
                u[0] * sweep.sin() + u[1] * sweep.cos(),
            ];
            ([-u[1], u[0]], [-v[1], v[0]])
        } else {
            let u = unit2(sub2(s.b, s.a));
            (u, u)
        };
        tangents.push(pair);
    }
    if g.path.segments.iter().any(|s| {
        matches!(
            s,
            PathSegment::ArcTo {
                clockwise: true,
                ..
            }
        )
    }) {
        return Err(unsupported());
    }
    for i in 0..tangents.len() {
        let u = tangents[i].1;
        let v = tangents[(i + 1) % tangents.len()].0;
        if cross2(u, v) < -128. * f64::EPSILON || dot2(u, v) < -1. + 1e-10 {
            return Err(unsupported());
        }
    }
    Ok(())
}
// Accepted negative joins are arithmetic-size only, never user-tolerance
// concavity. A simple CCW boundary with nonnegative arc curvature can depart
// from its convex hull only at these reflex joins. Removing a reflex pocket by
// its supporting chord moves it by at most 2*D*turn; summing the bounds also
// covers nested pockets. Here D <= 2*extent, and atan2(cross,dot) at a permitted
// negative join has magnitude <= 256 eps (antiparallel joins are excluded).
// This separate allowance is charged for BOTH operands in the upper certificate:
// convex-combination candidates need not be exactly feasible under join roundoff.
// The query's 128-vertex cap limits this family to 64 joins; no complexity factor
// is hidden in the fixed raw-support arithmetic padding.
pub(super) fn join_allowance(g: &ArcPrismGeometry) -> f64 {
    (4. * extent(g) * (g.path.segments.len() as f64) * 256. * f64::EPSILON).next_up()
}
pub(super) fn extent(g: &ArcPrismGeometry) -> f64 {
    g.path
        .segments
        .iter()
        .map(|s| match s {
            PathSegment::LineTo { end } => end[0].hypot(end[1]),
            PathSegment::ArcTo { center, end, .. } => {
                center[0].hypot(center[1]) + (end[0] - center[0]).hypot(end[1] - center[1])
            }
        })
        .fold(g.path.start[0].hypot(g.path.start[1]), f64::max)
        + g.height
}
pub(super) fn consistency(
    m: &ModelSnapshot,
    g: &ArcPrismGeometry,
    error: f64,
    tolerance: Tolerance,
) -> Result<(), KernelError> {
    let extent = extent(g);
    if (norm(g.axis) - 1.).abs() > 64. * f64::EPSILON
        || (norm(g.x_direction) - 1.).abs() > 64. * f64::EPSILON
        || dot(g.axis, g.x_direction).abs() > 64. * f64::EPSILON
    {
        return Err(KernelError::NumericRange);
    }
    let expected = crate::arc_prism::construct(g, tolerance)?;
    let near = |a, b| norm(sub(a, b)) <= error;
    let direction =
        |a, b| norm(sub(a, b)) * extent <= error && norm(sub(a, b)) <= 128. * f64::EPSILON;
    let circle = |a: &Circle, b: &Circle| {
        near(a.origin, b.origin)
            && (a.radius - b.radius).abs() <= error
            && direction(a.axis, b.axis)
            && direction(a.x_direction, b.x_direction)
    };
    for (a, b) in m.vertices.iter().zip(&expected.vertices) {
        if !near(a.position, b.position) {
            return Err(KernelError::NumericRange);
        }
    }
    for (a, b) in m.edges.iter().zip(&expected.edges) {
        let valid = match (&a.curve, &b.curve) {
            (Curve::Line(a), Curve::Line(b)) => {
                near(a.origin, b.origin) && direction(a.direction, b.direction)
            }
            (Curve::CircularArc(a), Curve::CircularArc(b)) => {
                circle(&a.circle, &b.circle)
                    && (a.sweep_radians - b.sweep_radians).abs() * b.circle.radius <= error
            }
            _ => false,
        };
        if !valid {
            return Err(KernelError::NumericRange);
        }
    }
    for (a, b) in m.faces.iter().zip(&expected.faces) {
        let valid = match (&a.surface, &b.surface) {
            (Surface::Plane(a), Surface::Plane(b)) => {
                near(a.origin, b.origin) && direction(a.normal, b.normal)
            }
            (Surface::Cylinder(a), Surface::Cylinder(b)) => {
                near(a.origin, b.origin)
                    && (a.radius - b.radius).abs() <= error
                    && direction(a.axis, b.axis)
                    && direction(a.x_direction, b.x_direction)
                    && a.inward == b.inward
            }
            _ => false,
        };
        if !valid {
            return Err(KernelError::NumericRange);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cam(r: f64, s: f64, t: Tolerance) -> ModelSnapshot {
        let mut m = extrude_polygon(
            &[
                [-10. * s, -6. * s],
                [10. * s, -6. * s],
                [10. * s, 6. * s],
                [-10. * s, 6. * s],
            ],
            6. * s,
            t,
        )
        .unwrap()
        .model;
        let mut f: Vec<_> = (0..6).map(FaceId).collect();
        for pair in [[2, 3], [3, 4], [4, 5], [5, 2]] {
            let op = fillet_longitudinal(&m, [f[pair[0]], f[pair[1]]], r * s, t).unwrap();
            for x in &mut f {
                *x = op.history.input_faces["body"][x][0];
            }
            m = op.model;
        }
        m
    }
    fn check(a: &ModelSnapshot, b: &ModelSnapshot, expected: f64, t: Tolerance) {
        for (l, r) in [(a, b), (b, a)] {
            let q = l.convex_clearance(r, t).unwrap();
            assert!(
                q.lower_bound_mm <= expected && q.upper_bound_mm >= expected,
                "{q:?} expected{expected}"
            );
            assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
        }
    }
    #[test]
    fn rounded_prism_exact_static_extrema_and_tiny_query() {
        for s in [1e-8, 0.01, 1., 10.] {
            let t = Tolerance {
                linear: if s < 1e-6 { 1e-12 } else { 1e-7 },
            };
            let a = cam(2., s, t);
            for p in [[18., 0., 0.], [0., 14., 0.], [14., 10., 0.], [18., 0., 10.]] {
                let b = translate(
                    &extrude_circle(2. * s, 6. * s, t).unwrap().model,
                    p.map(|x| x * s),
                    t,
                )
                .unwrap()
                .model;
                let radial =
                    ((p[0].abs() - 8.).max(0.).hypot((p[1].abs() - 4.).max(0.)) - 4.).max(0.);
                let axial = (p[2] - 6.).max(0.);
                check(&a, &b, radial.hypot(axial) * s, t);
            }
        }
    }
    #[test]
    fn full_turn_cam_interior_minimum_and_failure() {
        let t = Tolerance::default();
        let roller = translate(&extrude_circle(2., 6., t).unwrap().model, [18., 0., 0.], t)
            .unwrap()
            .model;
        let a = cam(2., 1., t);
        let report = roller
            .rotation_clearance(
                &a,
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                5.,
                t,
                1025,
            )
            .unwrap();
        assert!(report.lower_bound_mm >= 5.);
        assert!(roller
            .rotation_clearance(
                &cam(1., 1., t),
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                5.,
                t,
                1025
            )
            .is_err());
    }
    #[test]
    fn trimmed_extrema_mixed_bodies_and_rigid_witness_membership() {
        let t = Tolerance::default();
        let d = extrude_path(
            &PlanarPath {
                start: [0., -5.],
                segments: vec![
                    PathSegment::ArcTo {
                        center: [0., 0.],
                        end: [0., 5.],
                        clockwise: false,
                    },
                    PathSegment::LineTo { end: [0., -5.] },
                ],
            },
            &[],
            6.,
            t,
        )
        .unwrap()
        .model;
        let roller = translate(&extrude_circle(1., 6., t).unwrap().model, [-10., 0., 0.], t)
            .unwrap()
            .model;
        check(&d, &roller, 9., t); // Infinite supporting circle would falsely give 4.
        let a = cam(2., 1., t);
        let block = translate(
            &extrude_rectangle(2., 2., 6., t).unwrap().model,
            [18., -1., 0.],
            t,
        )
        .unwrap()
        .model;
        check(&a, &block, 8., t);
        let cone = translate(
            &revolve_polygon(&[[0., 0.], [2., 0.], [0., 6.]], t)
                .unwrap()
                .model,
            [18., 0., 0.],
            t,
        )
        .unwrap()
        .model;
        check(&a, &cone, 6., t);
        let b = translate(&cam(2., 1., t), [30., 0., 0.], t).unwrap().model;
        check(&a, &b, 10., t);
        let q = [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2];
        let offset = [25., -12., 7.];
        let moved = transform(&a, offset, q, t).unwrap().model;
        let other = transform(&block, offset, q, t).unwrap().model;
        check(&moved, &other, 8., t);
        let witness = moved.convex_clearance(&other, t).unwrap().left_point;
        // Independently undo the quaternion placement and check the actual
        // rounded rectangle material, not a display hull or only its AABB.
        let v = sub(witness, offset);
        let u = [q[0], q[1], q[2]];
        let local = sub(
            add(v, mul(cross(u, cross(u, v)), 2.)),
            mul(cross(u, v), 2. * q[3]),
        );
        assert!(
            (local[0].abs() - 8.)
                .max(0.)
                .hypot((local[1].abs() - 4.).max(0.))
                <= 2. + t.linear
        );
        assert!(local[2] >= -t.linear && local[2] <= 6. + t.linear);
        let horizontal = transform(
            &extrude_circle(2., 6., t).unwrap().model,
            [0., 0., 10.],
            [
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
                0.,
                std::f64::consts::FRAC_1_SQRT_2,
            ],
            t,
        )
        .unwrap()
        .model;
        check(&a, &horizontal, 2., t);
    }
    #[test]
    fn nonconvex_holes_contact_and_native_support_mutations_reject() {
        let t = Tolerance::default();
        let a = cam(2., 1., t);
        let b = translate(&extrude_circle(2., 6., t).unwrap().model, [18., 0., 0.], t)
            .unwrap()
            .model;
        let touching = translate(&extrude_circle(2., 6., t).unwrap().model, [12., 0., 0.], t)
            .unwrap()
            .model;
        assert!(a.convex_clearance(&touching, t).is_err());
        let bore = translate(&extrude_circle(1., 8., t).unwrap().model, [0., 0., -1.], t)
            .unwrap()
            .model;
        let hollow = boolean(&a, &bore, BooleanOp::Difference, t).unwrap().model;
        assert!(hollow.validate_convex_clearance_operand(t).is_err());
        assert!(hollow.clearance(&b, t).is_err());
        let concave = extrude_path(
            &PlanarPath {
                start: [0., 0.],
                segments: vec![
                    PathSegment::LineTo { end: [20., 0.] },
                    PathSegment::LineTo { end: [20., 10.] },
                    PathSegment::LineTo { end: [12., 10.] },
                    PathSegment::ArcTo {
                        center: [10., 10.],
                        end: [8., 10.],
                        clockwise: true,
                    },
                    PathSegment::LineTo { end: [0., 10.] },
                    PathSegment::LineTo { end: [0., 0.] },
                ],
            },
            &[],
            6.,
            t,
        )
        .unwrap()
        .model;
        assert!(concave.validate_convex_clearance_operand(t).is_err());
        let dent = extrude_path(
            &PlanarPath {
                start: [0., -5.],
                segments: vec![
                    PathSegment::ArcTo {
                        center: [0., 0.],
                        end: [0., 5.],
                        clockwise: false,
                    },
                    PathSegment::LineTo { end: [1., 0.] },
                    PathSegment::LineTo { end: [0., -5.] },
                ],
            },
            &[],
            6.,
            t,
        )
        .unwrap()
        .model;
        assert!(dent.validate_convex_clearance_operand(t).is_err());
        for kind in 0..3 {
            let mut m = a.clone();
            if kind == 0 {
                let c = m
                    .faces
                    .iter_mut()
                    .find_map(|f| match &mut f.surface {
                        Surface::Cylinder(c) => Some(c),
                        _ => None,
                    })
                    .unwrap();
                c.radius += 1e-8;
            } else {
                let a = m
                    .edges
                    .iter_mut()
                    .find_map(|e| match &mut e.curve {
                        Curve::CircularArc(a) => Some(a),
                        _ => None,
                    })
                    .unwrap();
                if kind == 1 {
                    a.circle.axis[0] += 1e-9
                } else {
                    a.sweep_radians += 1e-9;
                }
            }
            assert!(m.convex_clearance(&b, t).is_err());
        }
    }
}
