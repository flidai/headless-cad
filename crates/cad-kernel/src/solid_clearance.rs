//! Native solid separation: convex support certificates or coaxial meridian regions.
use crate::convex_clearance::support::{circular_consistency, revolution_consistency};
use crate::math::{dot, norm, sub};
use crate::*;
#[path = "parallel_clearance.rs"]
mod parallel;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearanceMethod {
    ConvexSupport,
    CoaxialMeridian,
    ParallelAnnular,
    NativeTrimWitness,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolidClearance {
    pub method: ClearanceMethod,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
    pub left_point: [f64; 3],
    pub right_point: [f64; 3],
    /// Unit witness direction from left to right; not a separating plane for nested solids.
    pub axis: [f64; 3],
}
fn unsupported() -> KernelError {
    KernelError::Unsupported("solid clearance requires certified convex separation or separated coaxial full-turn linear-meridian material regions or parallel circular extrusions; overlap/contact and unresolved alignment are not classified".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], v: f64) -> [f64; 3] {
    a.map(|x| x * v)
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
struct Meridian {
    origin: [f64; 3],
    axis: [f64; 3],
    radial: [f64; 3],
    points: Vec<[f64; 2]>,
    circle: Option<CylinderGeometry>,
    revolve: Option<RevolveGeometry>,
}
impl Meridian {
    fn extent(&self) -> f64 {
        self.points
            .iter()
            .map(|p| p[0] + p[1].abs())
            .fold(0., f64::max)
    }
    fn certify(&self, m: &ModelSnapshot, error: f64) -> Result<(), KernelError> {
        if let Some(g) = &self.circle {
            circular_consistency(m, g, error)
        } else {
            revolution_consistency(m, self.revolve.as_ref().unwrap(), error)
        }
    }
}
fn meridian(m: &ModelSnapshot, t: Tolerance) -> Result<Meridian, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.solids.len() != 1 || m.shells.len() != 1 {
        return Err(unsupported());
    }
    if m.vertices.len() > MAX_CLEARANCE_VERTICES
        || m.edges.len() > MAX_CLEARANCE_EDGES
        || m.faces.len() > MAX_CLEARANCE_FACES
        || m.loops.len() > 2 * MAX_CLEARANCE_FACES
        || m.faces.iter().any(|f| f.loops.len() > 2)
        || m.shells[0].faces.len() > MAX_CLEARANCE_FACES
        || m.solids[0].shells.len() > 1
        || m.loops
            .iter()
            .fold(0usize, |n, l| n.saturating_add(l.edges.len()))
            > 2 * MAX_CLEARANCE_EDGES
    {
        return Err(KernelError::ResourceLimit(
            "solid clearance topology budget exceeded".into(),
        ));
    }
    let report = m.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    if let Ok(Some(g)) = m.cylinder_geometry(t) {
        let points = vec![
            [g.inner_radius, 0.],
            [g.radius, 0.],
            [g.radius, g.height],
            [g.inner_radius, g.height],
        ];
        return Ok(Meridian {
            origin: g.origin,
            axis: g.axis,
            radial: g.x_direction,
            points,
            circle: Some(g),
            revolve: None,
        });
    }
    let g = m.revolve_geometry(t)?.ok_or_else(unsupported)?;
    Ok(Meridian {
        origin: g.origin,
        axis: g.axis,
        radial: g.x_direction,
        points: g.profile.clone(),
        circle: None,
        revolve: Some(g),
    })
}
fn nearest(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0].hypot(d[1]);
    let u = d.map(|v| v / len);
    let t = ((p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1]).clamp(0., len);
    [a[0] + t * u[0], a[1] + t * u[1]]
}
fn coaxial(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let l = meridian(a, t)?;
    let r = meridian(b, t)?;
    if l.points
        .len()
        .saturating_mul(r.points.len())
        .saturating_mul(4)
        > MAX_CLEARANCE_CANDIDATES
    {
        return Err(KernelError::ResourceLimit(
            "coaxial clearance segment candidate budget exceeded".into(),
        ));
    }
    let delta = sub(r.origin, l.origin);
    let scale = l.extent().max(r.extent()).max(norm(delta));
    let world = a
        .vertices
        .iter()
        .chain(&b.vertices)
        .map(|v| norm(v.position))
        .fold(norm(l.origin).max(norm(r.origin)), f64::max);
    let error = (1024. * world + 4096. * scale) * f64::EPSILON;
    if !error.is_normal() || error > t.linear / 80. || !(1e-50..=1e50).contains(&scale) {
        return Err(KernelError::NumericRange);
    }
    l.certify(a, error)?;
    r.certify(b, error)?;
    let axis = mul(l.axis, 1. / norm(l.axis));
    let ra = mul(r.axis, 1. / norm(r.axis));
    let sign = if dot(axis, ra) >= 0. { 1. } else { -1. };
    let shift = dot(delta, axis);
    let offset = norm(sub(delta, mul(axis, shift)));
    let tilt = norm(sub(ra, mul(axis, sign)));
    let deviation = offset + tilt * r.extent();
    // Alignment must be arithmetic-consistent, never merged by modeling tolerance.
    if offset > error || tilt > 128. * f64::EPSILON || deviation > 4. * error {
        return Err(unsupported());
    }
    let rp: Vec<_> = r
        .points
        .iter()
        .map(|p| [p[0], shift + sign * p[1]])
        .collect();
    // The true coaxial 3D minimum equals the distance between filled meridian
    // regions: equal azimuth minimizes every radial pair. Alignment departure
    // is a Hausdorff perturbation bounded over the entire right meridian.
    let margin = t.linear + 32. * error + deviation;
    let mut best = (f64::INFINITY, [0.; 2], [0.; 2]);
    for i in 0..l.points.len() {
        let u = l.points[i];
        let v = l.points[(i + 1) % l.points.len()];
        for j in 0..rp.len() {
            let x = rp[j];
            let y = rp[(j + 1) % rp.len()];
            if !crate::simple_polygon::separated(u, v, x, y, margin) {
                return Err(unsupported());
            }
            for (p, q) in [
                (u, nearest(u, x, y)),
                (v, nearest(v, x, y)),
                (nearest(x, u, v), x),
                (nearest(y, u, v), y),
            ] {
                let d = (p[0] - q[0]).hypot(p[1] - q[1]);
                if d < best.0 {
                    best = (d, p, q)
                }
            }
        }
    }
    // With all boundary segments strictly separated, one representative vertex
    // in either interior detects containment of these simple filled polygons.
    if crate::simple_polygon::inside(l.points[0], &rp)
        || crate::simple_polygon::inside(rp[0], &l.points)
    {
        return Err(unsupported());
    }
    let radial = mul(l.radial, 1. / norm(l.radial));
    let rr = cross(ra, cross(radial, ra));
    let rr = mul(rr, 1. / norm(rr));
    let left_point = add(l.origin, add(mul(axis, best.1[1]), mul(radial, best.1[0])));
    let right_point = add(
        r.origin,
        add(mul(ra, sign * (best.2[1] - shift)), mul(rr, best.2[0])),
    );
    let distance = norm(sub(right_point, left_point));
    let lower = best.0 - 32. * error - deviation;
    let upper = best.0.max(distance) + 32. * error + deviation;
    if !lower.is_finite()
        || !upper.is_finite()
        || lower <= t.linear
        || upper < lower
        || upper - lower > t.linear
    {
        return Err(KernelError::NumericRange);
    }
    Ok(SolidClearance {
        method: ClearanceMethod::CoaxialMeridian,
        lower_bound_mm: lower,
        upper_bound_mm: upper,
        left_point,
        right_point,
        axis: mul(sub(right_point, left_point), 1. / distance),
    })
}
impl ModelSnapshot {
    /// Admit a bounded convex solid, canonical full-turn meridian, or recognized
    /// keyed circular extrusion. Pair alignment,
    /// native support consistency and separation still require `clearance`.
    pub fn validate_clearance_operand(&self, t: Tolerance) -> Result<(), KernelError> {
        if self.validate_convex_clearance_operand(t).is_ok() {
            Ok(())
        } else {
            if crate::trim_clearance::is_keyed(self, t) {
                crate::trim_clearance::admit(self, t).map(|_| ())
            } else {
                meridian(self, t).map(|_| ())
            }
        }
    }
    /// Certified positive material clearance. Unsupported/contact cases fail closed.
    pub fn clearance(&self, other: &Self, t: Tolerance) -> Result<SolidClearance, KernelError> {
        if self.validate_convex_clearance_operand(t).is_ok()
            && other.validate_convex_clearance_operand(t).is_ok()
        {
            let q = self.convex_clearance(other, t)?;
            return Ok(SolidClearance {
                method: ClearanceMethod::ConvexSupport,
                lower_bound_mm: q.lower_bound_mm,
                upper_bound_mm: q.upper_bound_mm,
                left_point: q.left_point,
                right_point: q.right_point,
                axis: q.axis,
            });
        }
        if crate::trim_clearance::is_keyed(self, t) || crate::trim_clearance::is_keyed(other, t) {
            return crate::trim_clearance::query(self, other, t);
        }
        match coaxial(self, other, t) {
            Ok(q) => Ok(q),
            Err(coaxial_error) => parallel::query(self, other, t).map_err(|_| coaxial_error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(a: &ModelSnapshot, b: &ModelSnapshot, expected: f64) {
        let t = Tolerance::default();
        for (l, r) in [(a, b), (b, a)] {
            let q = l
                .clearance(r, t)
                .unwrap_or_else(|e| panic!("expected {expected}: {e}"));
            assert_eq!(q.method, ClearanceMethod::CoaxialMeridian);
            assert!(
                q.lower_bound_mm <= expected && q.upper_bound_mm >= expected,
                "{q:?}, expected {expected}"
            );
            assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
            assert!(norm(sub(q.right_point, q.left_point)) <= q.upper_bound_mm);
            assert!((norm(q.axis) - 1.).abs() < 1e-14);
        }
    }
    #[test]
    fn nested_annulus_axial_diagonal_and_rigid_frames() {
        let t = Tolerance::default();
        for s in [0.01, 1., 100.] {
            let ring = extrude_annulus(3. * s, 2. * s, 4. * s, t).unwrap().model;
            let shaft = extrude_circle(s, 4. * s, t).unwrap().model;
            check(&ring, &shaft, s);
            let opposed = transform(&shaft, [0., 0., 4. * s], [1., 0., 0., 0.], t)
                .unwrap()
                .model;
            check(&ring, &opposed, s);
            let above = translate(&shaft, [0., 0., 7. * s], t).unwrap().model;
            check(&ring, &above, 10_f64.sqrt() * s);
            let other = translate(&ring, [0., 0., 7. * s], t).unwrap().model;
            check(&ring, &other, 3. * s);
            let q = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
            let a = transform(&ring, [20., -8., 3.], q, t).unwrap().model;
            let b = transform(&shaft, [20., -8., 3.], q, t).unwrap().model;
            check(&a, &b, s);
        }
    }
    #[test]
    fn nonconvex_and_hollow_conical_meridians_keep_material_regions() {
        let t = Tolerance::default();
        let waist = revolve_polygon(&[[0., 0.], [2., 0.], [1., 2.], [2., 4.], [0., 4.]], t)
            .unwrap()
            .model;
        let next = translate(&waist, [0., 0., 7.], t).unwrap().model;
        check(&waist, &next, 3.);
        let stepped = revolve_polygon(
            &[[0., 0.], [1., 0.], [1., 2.], [2., 2.], [2., 4.], [0., 4.]],
            t,
        )
        .unwrap()
        .model;
        let sleeve = extrude_annulus(4., 3., 4., t).unwrap().model;
        check(&stepped, &sleeve, 1.);
        let conical = revolve_polygon(&[[2., 0.], [4., 0.], [6., 4.], [4., 4.]], t)
            .unwrap()
            .model;
        let shaft = extrude_circle(1., 4., t).unwrap().model;
        check(&conical, &shaft, 1.);
        // The closest point lies inside a conical segment, not at its endpoints.
        let short = translate(&extrude_circle(1., 1., t).unwrap().model, [0., 0., 2.], t)
            .unwrap()
            .model;
        check(&conical, &short, 4_f64 / 5_f64.sqrt());
    }
    #[test]
    fn coaxial_material_overlap_contact_misalignment_and_precision_fail_closed() {
        let t = Tolerance::default();
        let ring = extrude_annulus(3., 2., 4., t).unwrap().model;
        for radius in [2., 2.5, 4.] {
            let shaft = extrude_circle(radius, 4., t).unwrap().model;
            assert!(ring.clearance(&shaft, t).is_err());
        }
        // A second annulus strictly inside the first annulus material has disjoint
        // boundaries but overlapping interiors; boundary distance alone is unsound.
        let contained = extrude_annulus(2.8, 2.2, 2., t).unwrap().model;
        let contained = translate(&contained, [0., 0., 1.], t).unwrap().model;
        assert!(ring.clearance(&contained, t).is_err());
        let shaft = extrude_circle(1., 4., t).unwrap().model;
        let tilted = transform(
            &shaft,
            [0.; 3],
            [0., (0.001_f64).sin(), 0., (0.001_f64).cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(ring.clearance(&tilted, t).is_err());
        let far = translate(&ring, [10000., 0., 0.], t).unwrap().model;
        let farshaft = translate(&shaft, [10000., 0., 0.], t).unwrap().model;
        assert!(matches!(
            far.clearance(&farshaft, t),
            Err(KernelError::NumericRange)
        ));
        let mut bad = ring.clone();
        bad.vertices[2].position[0] += 1e-8;
        assert!(bad.clearance(&shaft, t).is_err());
        let mut huge = ring;
        huge.shells[0]
            .faces
            .resize(MAX_CLEARANCE_FACES + 1, FaceId(0));
        assert!(matches!(
            huge.validate_clearance_operand(t),
            Err(KernelError::ResourceLimit(_))
        ));
    }
    #[test]
    fn solid_query_preserves_convex_method_and_legacy_api() {
        let t = Tolerance::default();
        let a = extrude_circle(1., 4., t).unwrap().model;
        let b = translate(&a, [5., 0., 0.], t).unwrap().model;
        let legacy = a.convex_clearance(&b, t).unwrap();
        let new = a.clearance(&b, t).unwrap();
        assert_eq!(new.method, ClearanceMethod::ConvexSupport);
        assert_eq!(new.lower_bound_mm, legacy.lower_bound_mm);
        assert_eq!(new.upper_bound_mm, legacy.upper_bound_mm);
    }
}
