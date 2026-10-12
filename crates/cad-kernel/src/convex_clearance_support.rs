//! Analytic support search; only an independent positive interval certifies success.
use super::*;
#[derive(Clone)]
enum Shape {
    Poly(Vec<[f64; 3]>),
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        height: f64,
    },
}
impl Shape {
    fn support(&self, d: [f64; 3]) -> [f64; 3] {
        match self {
            Self::Poly(p) => *p
                .iter()
                .max_by(|a, b| dot(**a, d).total_cmp(&dot(**b, d)))
                .unwrap(),
            Self::Cylinder {
                origin,
                axis,
                radius,
                height,
            } => {
                let z = dot(d, *axis);
                // Double cross preserves radial orthogonality even when the query
                // direction is almost axial; subtracting two near-equal vectors
                // then normalizing could amplify axial leakage.
                let radial = cross(*axis, cross(d, *axis));
                let length = norm(radial);
                let radial = if length > 128. * f64::EPSILON * norm(d) {
                    mul(radial, *radius / length)
                } else {
                    [0.; 3]
                };
                add(
                    *origin,
                    add(mul(*axis, if z >= 0. { *height } else { 0. }), radial),
                )
            }
        }
    }
    fn support_value(&self, d: [f64; 3]) -> f64 {
        match self {
            Self::Poly(p) => p
                .iter()
                .map(|v| dot(*v, d))
                .fold(f64::NEG_INFINITY, f64::max),
            Self::Cylinder {
                origin,
                axis,
                radius,
                height,
            } => dot(*origin, d) + height * dot(*axis, d).max(0.) + radius * norm(cross(*axis, d)),
        }
    }
}
#[derive(Clone, Copy)]
struct Pair {
    a: [f64; 3],
    b: [f64; 3],
}
impl Pair {
    fn delta(self) -> [f64; 3] {
        sub(self.a, self.b)
    }
}
fn support(a: &Shape, b: &Shape, d: [f64; 3]) -> Pair {
    Pair {
        a: a.support(d),
        b: b.support(mul(d, -1.)),
    }
}
fn solve(mut m: [[f64; 4]; 3], n: usize) -> Option<[f64; 3]> {
    let mut x = [0.; 3];
    let scale = m
        .iter()
        .take(n)
        .flat_map(|r| r[..n].iter())
        .map(|v| v.abs())
        .fold(0., f64::max);
    for i in 0..n {
        let pivot = (i..n).max_by(|a, b| m[*a][i].abs().total_cmp(&m[*b][i].abs()))?;
        m.swap(i, pivot);
        if m[i][i].abs() <= scale * 128. * f64::EPSILON {
            return None;
        }
        let pivot_row = m[i];
        for row in m.iter_mut().take(n).skip(i + 1) {
            let f = row[i] / pivot_row[i];
            for (entry, pivot_entry) in row[i..=n].iter_mut().zip(&pivot_row[i..=n]) {
                *entry -= f * pivot_entry;
            }
        }
    }
    for i in (0..n).rev() {
        x[i] = (m[i][n] - (i + 1..n).map(|j| m[i][j] * x[j]).sum::<f64>()) / m[i][i];
    }
    Some(x)
}
// Enumerate all simplex faces. Every retained witness uses nonnegative convex
// weights, regardless of minimizer conditioning; the final support gap decides
// whether this candidate is accurate enough. At most five support pairs exist.
fn reduce(simplex: &[Pair]) -> Option<(Vec<Pair>, Pair)> {
    let mut best: Option<(f64, Vec<Pair>, Pair)> = None;
    for mask in 1usize..1 << simplex.len() {
        let ids = (0..simplex.len())
            .filter(|i| mask & (1 << i) != 0)
            .collect::<Vec<_>>();
        if ids.len() > 4 {
            continue;
        }
        let base = simplex[ids[0]].delta();
        let n = ids.len() - 1;
        let mut weights = vec![1.; ids.len()];
        if n > 0 {
            let dirs = ids
                .iter()
                .skip(1)
                .map(|i| sub(simplex[*i].delta(), base))
                .collect::<Vec<_>>();
            let mut m = [[0.; 4]; 3];
            for i in 0..n {
                for j in 0..n {
                    m[i][j] = dot(dirs[i], dirs[j]);
                }
                m[i][n] = -dot(dirs[i], base);
            }
            let Some(x) = solve(m, n) else {
                continue;
            };
            weights[0] = 1. - x[..n].iter().sum::<f64>();
            weights[1..].copy_from_slice(&x[..n]);
        }
        if weights.iter().any(|w| !w.is_finite() || *w < 0.) {
            continue;
        }
        let sum = weights.iter().sum::<f64>();
        for w in &mut weights {
            *w /= sum;
        }
        let mut pair = Pair {
            a: [0.; 3],
            b: [0.; 3],
        };
        let mut retained = vec![];
        for (i, w) in ids.into_iter().zip(weights) {
            if w > 0. {
                pair.a = add(pair.a, mul(simplex[i].a, w));
                pair.b = add(pair.b, mul(simplex[i].b, w));
                retained.push(simplex[i]);
            }
        }
        let d = norm(pair.delta());
        if d.is_finite() && best.as_ref().is_none_or(|v| d < v.0) {
            best = Some((d, retained, pair));
        }
    }
    best.map(|(_, s, p)| (s, p))
}
fn validate(m: &ModelSnapshot, t: Tolerance) -> Result<Option<CylinderGeometry>, KernelError> {
    // These limits precede generic topology walks, including repeated handles.
    if m.solids.len() != 1 || m.shells.len() != 1 {
        return Err(unsupported());
    }
    if m.vertices.len() > MAX_CLEARANCE_VERTICES
        || m.edges.len() > MAX_CLEARANCE_EDGES
        || m.faces.len() > MAX_CLEARANCE_FACES
        || m.loops.len() > MAX_CLEARANCE_FACES
        || m.faces.iter().any(|f| f.loops.len() > 1)
        || m.shells[0].faces.len() > MAX_CLEARANCE_FACES
        || m.solids[0].shells.len() > 1
        || m.loops
            .iter()
            .fold(0usize, |n, l| n.saturating_add(l.edges.len()))
            > 2 * MAX_CLEARANCE_EDGES
    {
        return Err(KernelError::ResourceLimit(
            "convex clearance input topology exceeds bounded query limits".into(),
        ));
    }
    let report = m.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    if m.has_curved_geometry() {
        let g = m.cylinder_geometry(t)?.ok_or_else(unsupported)?;
        if g.inner_radius > 0. {
            return Err(unsupported());
        }
        Ok(Some(g))
    } else {
        preflight(m)?;
        if !validation::geometry::validate_convex(m, t).is_empty() {
            return Err(unsupported());
        }
        Ok(None)
    }
}
fn consistency(
    m: &ModelSnapshot,
    g: Option<&CylinderGeometry>,
    error: f64,
) -> Result<(), KernelError> {
    if let Some(g) = g {
        if (norm(g.axis) - 1.).abs() > 64. * f64::EPSILON
            || (norm(g.x_direction) - 1.).abs() > 64. * f64::EPSILON
            || dot(g.axis, g.x_direction).abs() > 64. * f64::EPSILON
        {
            return Err(KernelError::NumericRange);
        }
        let end = add(g.origin, mul(g.axis, g.height));
        for edge in &m.edges {
            match &edge.curve {
                Curve::Circle(c) => {
                    if (c.radius - g.radius).abs() > error
                        || norm(sub(c.axis, g.axis)) > 64. * f64::EPSILON
                        || norm(sub(c.origin, g.origin)).min(norm(sub(c.origin, end))) > error
                    {
                        return Err(KernelError::NumericRange);
                    }
                }
                Curve::Line(l) => {
                    let a = m.vertices[edge.vertices[0].0].position;
                    let b = m.vertices[edge.vertices[1].0].position;
                    if norm(sub(l.origin, a)) > error || norm(cross(sub(b, a), l.direction)) > error
                    {
                        return Err(KernelError::NumericRange);
                    }
                }
                _ => return Err(unsupported()),
            }
        }
        for face in &m.faces {
            match &face.surface {
                Surface::Cylinder(c) => {
                    if (c.radius - g.radius).abs() > error
                        || norm(sub(c.axis, g.axis)) > 64. * f64::EPSILON
                        || norm(sub(c.origin, g.origin)) > error
                    {
                        return Err(KernelError::NumericRange);
                    }
                }
                Surface::Plane(p) => {
                    let departure = dot(sub(p.origin, g.origin), p.normal)
                        .abs()
                        .min(dot(sub(p.origin, end), p.normal).abs())
                        + g.radius * norm(cross(p.normal, g.axis));
                    if departure > error {
                        return Err(KernelError::NumericRange);
                    }
                }
                _ => return Err(unsupported()),
            }
        }
    } else {
        for face in &m.faces {
            let p = face.try_plane().ok_or_else(unsupported)?;
            if (norm(p.normal) - 1.).abs() > 64. * f64::EPSILON {
                return Err(KernelError::NumericRange);
            }
            for vertex in m.loop_points(face.loops[0]).ok_or_else(unsupported)? {
                if dot(sub(vertex, p.origin), p.normal).abs() > error {
                    return Err(KernelError::NumericRange);
                }
            }
        }
        for edge in &m.edges {
            let l = edge.try_line().ok_or_else(unsupported)?;
            let a = m.vertices[edge.vertices[0].0].position;
            let b = m.vertices[edge.vertices[1].0].position;
            if norm(sub(l.origin, a)) > error
                || (norm(l.direction) - 1.).abs() > 64. * f64::EPSILON
                || norm(cross(sub(b, a), l.direction)) > error
            {
                return Err(KernelError::NumericRange);
            }
        }
    }
    Ok(())
}
pub(super) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<ConvexClearance, KernelError> {
    query_bounded(a, b, t, MAX_CLEARANCE_ITERATIONS)
}
fn query_bounded(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
    iterations: usize,
) -> Result<ConvexClearance, KernelError> {
    let geometries = [validate(a, t)?, validate(b, t)?];
    let reference = geometries[0]
        .as_ref()
        .map(|g| g.origin)
        .unwrap_or(a.vertices[0].position);
    let mut scale = 0_f64;
    let mut world = 0_f64;
    for (m, g) in [a, b].into_iter().zip(&geometries) {
        for p in m
            .vertices
            .iter()
            .map(|v| v.position)
            .chain(g.iter().map(|g| g.origin))
        {
            scale = scale.max(norm(sub(p, reference)));
            world = world.max(norm(p));
        }
        if let Some(g) = g {
            scale = scale.max(norm(sub(g.origin, reference)) + g.radius + g.height);
            world = world.max(norm(g.origin) + g.radius + g.height);
        }
    }
    let error = (512. * world + 2048. * scale) * f64::EPSILON;
    if !error.is_normal() || error > t.linear / 64. || !(1e-50..=1e50).contains(&scale) {
        return Err(KernelError::NumericRange);
    }
    for (m, g) in [a, b].into_iter().zip(&geometries) {
        consistency(m, g.as_ref(), error)?;
    }
    let shapes: [Shape; 2] = std::array::from_fn(|i| {
        if let Some(g) = &geometries[i] {
            Shape::Cylinder {
                origin: mul(sub(g.origin, reference), 1. / scale),
                axis: mul(g.axis, 1. / norm(g.axis)),
                radius: g.radius / scale,
                height: g.height / scale,
            }
        } else {
            Shape::Poly(
                [a, b][i]
                    .vertices
                    .iter()
                    .map(|v| mul(sub(v.position, reference), 1. / scale))
                    .collect(),
            )
        }
    });
    let mut simplex = vec![support(&shapes[0], &shapes[1], [1., 0., 0.])];
    for _ in 0..iterations {
        let (retained, p) = reduce(&simplex).ok_or(KernelError::NumericRange)?;
        simplex = retained;
        let distance = norm(p.delta()) * scale;
        if !distance.is_finite() {
            return Err(KernelError::NumericRange);
        }
        if distance <= t.linear + 16. * error {
            return Err(unsupported());
        }
        let axis = mul(sub(p.b, p.a), scale / distance);
        let axis_norm = norm(axis);
        if (axis_norm - 1.).abs() > 64. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        let lower = (-shapes[1].support_value(mul(axis, -1.)) - shapes[0].support_value(axis))
            * scale
            / axis_norm
            - 8. * error;
        let left_point = add(reference, mul(p.a, scale));
        let right_point = add(reference, mul(p.b, scale));
        let upper = distance.max(norm(sub(right_point, left_point))) + 8. * error;
        if lower > t.linear && upper >= lower && upper - lower <= t.linear {
            return Ok(ConvexClearance {
                lower_bound_mm: lower,
                upper_bound_mm: upper,
                left_point,
                right_point,
                axis,
            });
        }
        let next = support(&shapes[0], &shapes[1], axis);
        if simplex
            .iter()
            .any(|v| norm(sub(v.delta(), next.delta())) <= 64. * f64::EPSILON)
        {
            return Err(KernelError::NumericRange);
        }
        simplex.push(next);
    }
    Err(KernelError::Unsupported("analytic clearance did not converge to a certified separation within 256 support iterations; this is not a collision classification".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cylinder() -> ModelSnapshot {
        extrude_circle(1., 4., Tolerance::default()).unwrap().model
    }
    fn check(a: &ModelSnapshot, b: &ModelSnapshot, expected: f64) {
        let t = Tolerance::default();
        for (a, b) in [(a, b), (b, a)] {
            let q = a.convex_clearance(b, t).unwrap();
            assert!(
                q.lower_bound_mm <= expected && q.upper_bound_mm >= expected,
                "{q:?} expected {expected}"
            );
            assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
            assert!(norm(sub(q.right_point, q.left_point)) <= q.upper_bound_mm);
        }
    }
    #[test]
    fn parallel_cylinders_have_exact_radial_axial_and_diagonal_distances() {
        let t = Tolerance::default();
        let a = cylinder();
        for (offset, d) in [
            ([5., 0., 0.], 3.),
            ([0., 0., 7.], 3.),
            ([5., 0., 8.], 5.),
            ([0.5, 0., 7.], 3.),
        ] {
            let b = translate(&a, offset, t).unwrap().model;
            check(&a, &b, d);
            let q = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
            check(
                &transform(&a, [20., -8., 3.], q, t).unwrap().model,
                &transform(&b, [20., -8., 3.], q, t).unwrap().model,
                d,
            );
        }
    }
    #[test]
    fn mixed_and_skew_cylinders_use_full_analytic_support() {
        let t = Tolerance::default();
        let a = cylinder();
        let b = translate(
            &extrude_rectangle(2., 3., 4., t).unwrap().model,
            [5., 0., 0.],
            t,
        )
        .unwrap()
        .model;
        check(&a, &b, 4.);
        // Axes are perpendicular, with closest radial surfaces separated along X.
        let q = [
            (std::f64::consts::FRAC_PI_4).sin(),
            0.,
            0.,
            (std::f64::consts::FRAC_PI_4).cos(),
        ];
        let skew = transform(&a, [5., 2., 2.], q, t).unwrap().model;
        check(&a, &skew, 3.);
        let moved_a = transform(&a, [0.; 3], [0., 0., (0.61_f64).sin(), (0.61_f64).cos()], t)
            .unwrap()
            .model;
        // Rotating about the native cylinder axis moves seam vertices only.
        check(&moved_a, &b, 4.);
        let c = translate(&a, [5., 0., 0.], t).unwrap().model;
        let before = a.convex_clearance(&c, t).unwrap();
        let _ = tessellate_analytic(&a, t, TessellationPolicy::default()).unwrap();
        assert_eq!(before, a.convex_clearance(&c, t).unwrap());
    }
    #[test]
    fn nearly_axial_support_witnesses_remain_inside_the_finite_cylinder() {
        let axis = mul([1., 2., 3.], 1. / 14_f64.sqrt());
        let tangent = cross(axis, [1., 0., 0.]);
        let shape = Shape::Cylinder {
            origin: [0.; 3],
            axis,
            radius: 3.,
            height: 7.,
        };
        for tilt in [0., 1e-15, 1e-12, 1e-9, 1e-6] {
            for sign in [-1., 1.] {
                let point = shape.support(add(mul(axis, sign), mul(tangent, tilt)));
                let axial = dot(point, axis);
                assert!(
                    (-1e-13..=7. + 1e-13).contains(&axial),
                    "tilt {tilt}: {point:?}, axial {axial}"
                );
                assert!(norm(cross(point, axis)) <= 3. + 1e-13);
            }
        }
        let t = Tolerance::default();
        let a = cylinder();
        for tilt in [1e-12_f64, 1e-9, 1e-6] {
            let b = transform(
                &a,
                [0., 0., 7.],
                [0., (tilt / 2.).sin(), 0., (tilt / 2.).cos()],
                t,
            )
            .unwrap()
            .model;
            check(&a, &b, 3. - tilt.sin());
        }
    }
    #[test]
    fn skew_centerline_distance_remains_analytic_across_scales() {
        let t = Tolerance::default();
        for scale in [1e-3, 1., 100.] {
            let a = extrude_circle(scale, 4. * scale, t).unwrap().model;
            for angle in [0.2_f64, 0.8, 1.4] {
                let b = transform(
                    &a,
                    [
                        5. * scale,
                        2. * scale * angle.sin(),
                        2. * scale * (1. - angle.cos()),
                    ],
                    [(angle / 2.).sin(), 0., 0., (angle / 2.).cos()],
                    t,
                )
                .unwrap()
                .model;
                check(&a, &b, 3. * scale);
            }
        }
    }
    #[test]
    fn curved_unsupported_and_unresolved_cases_fail_closed() {
        let t = Tolerance::default();
        let a = cylinder();
        for offset in [[0.; 3], [1., 0., 0.], [2., 0., 0.], [0., 0., 4.]] {
            let b = translate(&a, offset, t).unwrap().model;
            assert!(a.convex_clearance(&b, t).is_err());
        }
        let annulus = extrude_annulus(2., 1., 4., t).unwrap().model;
        assert!(a.convex_clearance(&annulus, t).is_err());
        let b = translate(&a, [5., 0., 0.], t).unwrap().model;
        assert!(query_bounded(&a, &b, t, 0).is_err());
        let mut malformed = b.clone();
        malformed.vertices[0].position[0] += 1e-4;
        assert!(a.convex_clearance(&malformed, t).is_err());
        let offset = [10000., 0., 0.];
        let shifted_a = translate(&a, offset, t).unwrap().model;
        let shifted_b = translate(&b, offset, t).unwrap().model;
        assert!(matches!(
            shifted_a.convex_clearance(&shifted_b, Tolerance { linear: 1e-8 }),
            Err(KernelError::NumericRange)
        ));
    }
}
