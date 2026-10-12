//! Bounded native convex-polyhedral distance with an independent support certificate.
use crate::math::{dot, norm, sub};
use crate::*;
pub const MAX_CLEARANCE_VERTICES: usize = 128;
pub const MAX_CLEARANCE_EDGES: usize = 256;
pub const MAX_CLEARANCE_FACES: usize = 128;
pub const MAX_CLEARANCE_CANDIDATES: usize = 200_000;
pub const MAX_CLEARANCE_ITERATIONS: usize = 256;
#[path = "convex_clearance_support.rs"]
mod support;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvexClearance {
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
    pub left_point: [f64; 3],
    pub right_point: [f64; 3],
    /// Unit separating direction from left toward right, within arithmetic error.
    pub axis: [f64; 3],
}
fn unsupported() -> KernelError {
    KernelError::Unsupported("convex clearance requires two separated, validated single convex planar solids or finite solid circular cylinders with a resolved support certificate".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], v: f64) -> [f64; 3] {
    a.map(|x| x * v)
}
fn segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let d = sub(b, a);
    add(a, mul(d, (dot(sub(p, a), d) / dot(d, d)).clamp(0., 1.)))
}
fn triangle(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = sub(b, a);
    let v = sub(c, a);
    let w = sub(p, a);
    let uu = dot(u, u);
    let uv = dot(u, v);
    let vv = dot(v, v);
    let wu = dot(w, u);
    let wv = dot(w, v);
    let det = uu * vv - uv * uv;
    if det > 64. * f64::EPSILON * uu * vv {
        let s = (wu * vv - wv * uv) / det;
        let t = (wv * uu - wu * uv) / det;
        if s >= 0. && t >= 0. && s + t <= 1. {
            return add(a, add(mul(u, s), mul(v, t)));
        }
    }
    [segment(p, a, b), segment(p, b, c), segment(p, c, a)]
        .into_iter()
        .min_by(|x, y| norm(sub(*x, p)).total_cmp(&norm(sub(*y, p))))
        .unwrap()
}
fn segments(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let u = sub(b, a);
    let v = sub(d, c);
    let w = sub(a, c);
    let aa = dot(u, u);
    let bb = dot(u, v);
    let cc = dot(v, v);
    let dd = dot(u, w);
    let ee = dot(v, w);
    let det = aa * cc - bb * bb;
    let mut s = if det > 64. * f64::EPSILON * aa * cc {
        ((bb * ee - cc * dd) / det).clamp(0., 1.)
    } else {
        0.
    };
    let mut t = (bb * s + ee) / cc;
    if t < 0. {
        t = 0.;
        s = (-dd / aa).clamp(0., 1.);
    } else if t > 1. {
        t = 1.;
        s = ((bb - dd) / aa).clamp(0., 1.);
    }
    (add(a, mul(u, s)), add(c, mul(v, t)))
}
fn preflight(m: &ModelSnapshot) -> Result<(), KernelError> {
    if m.solids.len() != 1 || m.shells.len() != 1 {
        return Err(unsupported());
    }
    if m.vertices.len() > MAX_CLEARANCE_VERTICES
        || m.edges.len() > MAX_CLEARANCE_EDGES
        || m.faces.len() > MAX_CLEARANCE_FACES
        || m.loops.len() > MAX_CLEARANCE_FACES
        || m.loops
            .iter()
            .fold(0usize, |n, l| n.saturating_add(l.edges.len()))
            > 2 * MAX_CLEARANCE_EDGES
        || m.faces.iter().any(|f| f.loops.len() > 1)
        || m.shells[0].faces.len() > MAX_CLEARANCE_FACES
        || m.solids[0].shells.len() > 1
    {
        return Err(KernelError::ResourceLimit("convex clearance supports 128 vertices, 256 edges and 128 single-loop faces per operand".into()));
    }
    if m.has_curved_geometry()
        || m.vertices.is_empty()
        || m.solids.len() != 1
        || m.shells.len() != 1
    {
        return Err(unsupported());
    }
    Ok(())
}
impl ModelSnapshot {
    /// Certify strictly positive Euclidean clearance for native convex planar solids
    /// and finite solid circular cylinders in arbitrary rigid orientations.
    /// Other curves, nonconvexity, contact/intersection and unresolved precision fail;
    /// failure is not a collision classification. Bounds include arithmetic and
    /// accepted support/witness uncertainty and must differ by at most tolerance.
    pub fn convex_clearance(
        &self,
        other: &ModelSnapshot,
        tolerance: Tolerance,
    ) -> Result<ConvexClearance, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if self.has_curved_geometry() || other.has_curved_geometry() {
            return support::query(self, other, tolerance);
        }
        for m in [self, other] {
            preflight(m)?;
            let report = m.validate(tolerance);
            if !report.valid {
                return Err(KernelError::InvalidInput(report));
            }
            if !validation::geometry::validate_convex(m, tolerance).is_empty() {
                return Err(unsupported());
            }
        }
        let reference = self.vertices[0].position;
        let points = [self, other].map(|m| {
            m.vertices
                .iter()
                .map(|v| sub(v.position, reference))
                .collect::<Vec<_>>()
        });
        let scale = points
            .iter()
            .flatten()
            .flatten()
            .map(|v| v.abs())
            .fold(0., f64::max);
        let world_scale = self
            .vertices
            .iter()
            .chain(&other.vertices)
            .flat_map(|v| v.position)
            .map(f64::abs)
            .fold(scale, f64::max);
        // World subtraction/addition plus bounded local dot, barycentric and
        // normalization operations. Large translations cannot hide lost precision.
        let error = (32. * world_scale + 1024. * scale) * f64::EPSILON;
        if !error.is_normal() || error > tolerance.linear / 64. || scale < 1e-50 || scale > 1e50 {
            return Err(KernelError::NumericRange);
        }
        let mut triangles: [Vec<[usize; 3]>; 2] = Default::default();
        for (side, m) in [self, other].into_iter().enumerate() {
            for face in &m.faces {
                let plane = face.try_plane().ok_or_else(unsupported)?;
                if (norm(plane.normal) - 1.).abs() > 64. * f64::EPSILON {
                    return Err(KernelError::NumericRange);
                }
                let ids = m.loops[face.loops[0].0]
                    .edges
                    .iter()
                    .map(|e| m.endpoints(*e).unwrap()[0].0)
                    .collect::<Vec<_>>();
                for id in &ids {
                    if dot(sub(m.vertices[*id].position, plane.origin), plane.normal).abs() > error
                    {
                        return Err(KernelError::NumericRange);
                    }
                }
                for i in 1..ids.len() - 1 {
                    triangles[side].push([ids[0], ids[i], ids[i + 1]]);
                }
            }
            for edge in &m.edges {
                let line = edge.try_line().ok_or_else(unsupported)?;
                let a = m.vertices[edge.vertices[0].0].position;
                let b = m.vertices[edge.vertices[1].0].position;
                if norm(sub(line.origin, a)) > error
                    || (norm(line.direction) - 1.).abs() > 64. * f64::EPSILON
                    || norm(cross(sub(b, a), line.direction)) > error
                {
                    return Err(KernelError::NumericRange);
                }
            }
        }
        let work = points[0].len() * triangles[1].len()
            + points[1].len() * triangles[0].len()
            + self.edges.len() * other.edges.len();
        if work > MAX_CLEARANCE_CANDIDATES {
            return Err(KernelError::ResourceLimit(
                "convex clearance candidate budget exceeded".into(),
            ));
        }
        let mut best = None;
        let mut consider = |a: [f64; 3], b: [f64; 3]| {
            let distance = norm(sub(b, a));
            if distance.is_finite() && best.as_ref().is_none_or(|(old, _, _)| distance < *old) {
                best = Some((distance, a, b));
            }
        };
        for side in 0..2 {
            for p in &points[side] {
                for [i, j, k] in &triangles[1 - side] {
                    let q = triangle(
                        *p,
                        points[1 - side][*i],
                        points[1 - side][*j],
                        points[1 - side][*k],
                    );
                    if side == 0 {
                        consider(*p, q)
                    } else {
                        consider(q, *p)
                    }
                }
            }
        }
        for ea in &self.edges {
            for eb in &other.edges {
                let (a, b) = segments(
                    points[0][ea.vertices[0].0],
                    points[0][ea.vertices[1].0],
                    points[1][eb.vertices[0].0],
                    points[1][eb.vertices[1].0],
                );
                consider(a, b);
            }
        }
        let (distance, left, right) = best.ok_or(KernelError::NumericRange)?;
        if distance <= tolerance.linear + 8. * error {
            return Err(unsupported());
        }
        let axis = mul(sub(right, left), 1. / distance);
        let axis_norm = norm(axis);
        if !axis_norm.is_finite() || (axis_norm - 1.).abs() > 64. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        let max_left = points[0]
            .iter()
            .map(|v| dot(*v, axis))
            .fold(f64::NEG_INFINITY, f64::max);
        let min_right = points[1]
            .iter()
            .map(|v| dot(*v, axis))
            .fold(f64::INFINITY, f64::min);
        let left_point = add(reference, left);
        let right_point = add(reference, right);
        let lower_bound_mm = (min_right - max_left) / axis_norm - 8. * error;
        let upper_bound_mm = distance.max(norm(sub(right_point, left_point))) + 8. * error;
        if !lower_bound_mm.is_finite()
            || !upper_bound_mm.is_finite()
            || lower_bound_mm <= tolerance.linear
        {
            return Err(unsupported());
        }
        if upper_bound_mm < lower_bound_mm || upper_bound_mm - lower_bound_mm > tolerance.linear {
            return Err(KernelError::NumericRange);
        }
        Ok(ConvexClearance {
            lower_bound_mm,
            upper_bound_mm,
            left_point,
            right_point,
            axis,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube() -> ModelSnapshot {
        extrude_rectangle(2., 2., 2., Tolerance::default())
            .unwrap()
            .model
    }
    fn check(report: &ConvexClearance, expected: f64) {
        assert!(
            report.lower_bound_mm <= expected && report.upper_bound_mm >= expected,
            "{report:?} expected {expected}"
        );
        assert!(report.upper_bound_mm - report.lower_bound_mm <= Tolerance::default().linear);
        assert!(norm(sub(report.right_point, report.left_point)) <= report.upper_bound_mm);
    }
    #[test]
    fn axis_diagonal_symmetry_and_rigid_transform_have_independent_bounds() {
        let t = Tolerance::default();
        let a = cube();
        for (offset, d) in [([5., 0., 0.], 3.), ([5., 6., 0.], 5.), ([5., 6., 14.], 13.)] {
            let b = translate(&a, offset, t).unwrap().model;
            let q = a.convex_clearance(&b, t).unwrap();
            check(&q, d);
            check(&b.convex_clearance(&a, t).unwrap(), d);
            assert!(dot(q.axis, sub(q.right_point, q.left_point)) > 0.);
            for angle in [0.1_f64, 0.7, 1.5] {
                let rotation = [0., (angle / 2.).sin(), 0., (angle / 2.).cos()];
                let moved_a = transform(&a, [1000., -200., 30.], rotation, t)
                    .unwrap()
                    .model;
                let moved_b = transform(&b, [1000., -200., 30.], rotation, t)
                    .unwrap()
                    .model;
                check(&moved_a.convex_clearance(&moved_b, t).unwrap(), d);
            }
            assert_eq!(q, a.convex_clearance(&b, t).unwrap());
        }
    }
    #[test]
    fn rotated_face_and_edge_interior_witnesses_are_certified() {
        let t = Tolerance::default();
        let a = cube();
        let angle = std::f64::consts::FRAC_PI_4;
        let b = transform(
            &a,
            [5., 0., 0.],
            [0., 0., (angle / 2.).sin(), (angle / 2.).cos()],
            t,
        )
        .unwrap()
        .model;
        check(&a.convex_clearance(&b, t).unwrap(), 3. - 2_f64.sqrt());
        // Perpendicular slender bars: nearest points lie in edge interiors.
        let bar = extrude_rectangle(10., 1., 1., t).unwrap().model;
        let other = transform(
            &bar,
            [5., -4., 4.],
            [
                0.,
                0.,
                (std::f64::consts::FRAC_PI_4).sin(),
                (std::f64::consts::FRAC_PI_4).cos(),
            ],
            t,
        )
        .unwrap()
        .model;
        check(&bar.convex_clearance(&other, t).unwrap(), 3.);
        // Exercise genuinely skew segment closest points independently of face candidates.
        let (p, q) = segments([-2., 0., 0.], [2., 0., 0.], [0., -2., 3.], [0., 2., 3.]);
        assert_eq!(p, [0., 0., 0.]);
        assert_eq!(q, [0., 0., 3.]);
    }
    #[test]
    fn no_separation_claim_for_contact_overlap_containment_or_unsupported_geometry() {
        let t = Tolerance::default();
        let a = cube();
        for x in [0., 1., 2., 2. + t.linear / 2.] {
            let b = translate(&a, [x, 0., 0.], t).unwrap().model;
            assert!(a.convex_clearance(&b, t).is_err());
        }
        let inner = translate(
            &extrude_rectangle(1., 1., 1., t).unwrap().model,
            [0.5; 3],
            t,
        )
        .unwrap()
        .model;
        assert!(a.convex_clearance(&inner, t).is_err());
        let circle = extrude_circle(1., 2., t).unwrap().model;
        assert!(a.convex_clearance(&circle, t).is_err());
        let concave = extrude_polygon(
            &[[0., 0.], [4., 0.], [4., 1.], [1., 1.], [1., 4.], [0., 4.]],
            2.,
            t,
        );
        let concave = concave.unwrap_or_else(|_| {
            extrude_polygon_with_holes(
                &[[0., 0.], [4., 0.], [4., 1.], [1., 1.], [1., 4.], [0., 4.]],
                &[],
                2.,
                t,
            )
            .unwrap()
        });
        assert!(a.convex_clearance(&concave.model, t).is_err());
        let mut malformed = a.clone();
        malformed.edges[0].vertices[0] = VertexId(999);
        assert!(a.convex_clearance(&malformed, t).is_err());
        let mut too_large = a.clone();
        too_large
            .vertices
            .resize(MAX_CLEARANCE_VERTICES + 1, a.vertices[0].clone());
        assert!(matches!(
            a.convex_clearance(&too_large, t),
            Err(KernelError::ResourceLimit(_))
        ));
        let mut repeated = a.clone();
        repeated.shells[0].faces = vec![FaceId(0); MAX_CLEARANCE_FACES + 1];
        assert!(matches!(
            a.convex_clearance(&repeated, t),
            Err(KernelError::ResourceLimit(_))
        ));
        let mut repeated = a.clone();
        repeated.solids[0].shells = vec![ShellId(0); 2];
        assert!(matches!(
            a.convex_clearance(&repeated, t),
            Err(KernelError::ResourceLimit(_))
        ));
        let far = translate(&a, [1e8, 0., 0.], t).unwrap().model;
        assert!(matches!(
            a.convex_clearance(&far, t),
            Err(KernelError::NumericRange)
        ));
        assert!(a.convex_clearance(&a, Tolerance { linear: 0. }).is_err());
    }
    #[test]
    fn stored_support_uncertainty_cannot_be_hidden_by_modeling_tolerance() {
        let t = Tolerance::default();
        let a = cube();
        let mut b = translate(&a, [5., 0., 0.], t).unwrap().model;
        if let Surface::Plane(p) = &mut b.faces[0].surface {
            p.origin[2] += t.linear / 4.;
        }
        assert!(a.convex_clearance(&b, t).is_err());
    }
}
