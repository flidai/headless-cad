//! Certificates for coaxial circular stock with canonically recognized keyway trims.
//! Subset envelopes provide only the lower bound. The upper bound always uses
//! points on surviving native surfaces, never points in removed stock.
use crate::math::{cross, dot, norm, sub};
use crate::*;
#[derive(Clone)]
pub(crate) struct Envelope {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub radial: [f64; 3],
    pub outer: f64,
    pub inner: f64,
    pub height: f64,
    /// All native material on rays with local x below this positive limit survives.
    pub untouched_limit: Option<f64>,
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}
fn bad() -> KernelError {
    KernelError::Unsupported("trim-witness clearance requires separated arithmetic-coaxial circular stock envelopes, at least one canonical keyed shaft/hub, and certified surviving native boundary witnesses".into())
}
pub(crate) fn is_keyed(m: &ModelSnapshot, t: Tolerance) -> bool {
    crate::keyseat::recognize(m, t).is_ok()
        || crate::hub_keyway::recognize(m, t).is_ok()
        || crate::stepped_keyseat::recognize(m, t).is_ok()
}
pub(crate) fn admit(m: &ModelSnapshot, t: Tolerance) -> Result<Envelope, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.vertices.len() > 64
        || m.edges.len() > 96
        || m.faces.len() > 48
        || m.loops.len() > 96
        || m.shells.len() != 1
        || m.solids.len() != 1
        || m.shells[0].faces.len() > 48
        || m.solids[0].shells.len() != 1
        || m.loops.iter().any(|l| l.edges.len() > 32)
        || m.faces.iter().any(|f| f.loops.len() > 2)
    {
        return Err(bad());
    }
    let (envelope, circle) = if let Ok(g) = crate::keyseat::recognize(m, t) {
        (g.clearance_envelope(), None)
    } else if let Ok(g) = crate::hub_keyway::recognize(m, t) {
        (g.clearance_envelope(), None)
    } else if let Ok(g) = crate::stepped_keyseat::recognize(m, t) {
        (g.clearance_envelope(), None)
    } else if let Ok(g) = crate::revolve::recognize(m, t) {
        let outer = g.profile.iter().map(|p| p[0]).fold(0., f64::max);
        let height = g.profile.iter().map(|p| p[1].abs()).fold(0., f64::max);
        let error = 4096. * f64::EPSILON * (norm(g.origin) + outer + height);
        crate::convex_clearance::support::revolution_consistency(m, &g, error)?;
        (
            Envelope {
                origin: g.origin,
                axis: g.axis,
                radial: g.x_direction,
                outer,
                inner: g.profile.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
                height,
                untouched_limit: None,
            },
            None,
        )
    } else {
        let g = crate::analytic::recognize(m, Some(t))?;
        (
            Envelope {
                origin: g.origin,
                axis: g.axis,
                radial: g.x_direction,
                outer: g.radius,
                inner: g.inner_radius,
                height: g.height,
                untouched_limit: None,
            },
            Some(g),
        )
    };
    let report = m.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    let e = 4096. * f64::EPSILON * (norm(envelope.origin) + envelope.outer + envelope.height);
    if !e.is_normal() || e > t.linear / 32. {
        return Err(KernelError::NumericRange);
    }
    if let Some(g) = circle {
        crate::convex_clearance::support::circular_consistency(m, &g, e)?;
    }
    Ok(envelope)
}
fn nearest_intervals(a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    if a[1] < b[0] {
        (a[1], b[0])
    } else if b[1] < a[0] {
        (a[0], b[1])
    } else {
        let v = a[0].max(b[0]);
        (v, v)
    }
}
pub(crate) fn radial(axis: [f64; 3], v: [f64; 3]) -> Option<[f64; 3]> {
    let p = cross(axis, cross(v, axis));
    let n = norm(p);
    (n > 64. * f64::EPSILON).then(|| mul(p, 1. / n))
}
pub(crate) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    if crate::stepped_keyseat::recognize(a, t).is_ok()
        || crate::stepped_keyseat::recognize(b, t).is_ok()
        || crate::revolve::recognize(a, t).is_ok()
        || crate::revolve::recognize(b, t).is_ok()
    {
        return meridian_query(a, b, t);
    }
    let l = admit(a, t)?;
    let r = admit(b, t)?;
    if l.untouched_limit.is_none() && r.untouched_limit.is_none() {
        return Err(bad());
    }
    let extent = l.outer + l.height + r.outer + r.height;
    let error = 4096. * f64::EPSILON * (norm(l.origin) + norm(r.origin) + extent);
    if !error.is_normal() || error > t.linear / 48. {
        return Err(KernelError::NumericRange);
    }
    for g in [&l, &r] {
        if (norm(g.axis) - 1.).abs() > 64. * f64::EPSILON
            || (norm(g.radial) - 1.).abs() > 64. * f64::EPSILON
            || dot(g.axis, g.radial).abs() > 64. * f64::EPSILON
        {
            return Err(KernelError::NumericRange);
        }
    }
    let axis = mul(l.axis, 1. / norm(l.axis));
    let ra = mul(r.axis, 1. / norm(r.axis));
    let delta = sub(r.origin, l.origin);
    let shift = dot(delta, axis);
    let offset = norm(cross(axis, cross(delta, axis)));
    let sign = if dot(axis, ra) >= 0. { 1. } else { -1. };
    let tilt = norm(sub(ra, mul(axis, sign)));
    let deviation = offset + tilt * (r.height + r.outer);
    if offset > error || tilt > 128. * f64::EPSILON || deviation > 4. * error {
        return Err(bad());
    }
    let (lr, rr) = nearest_intervals([l.inner, l.outer], [r.inner, r.outer]);
    let rz = [
        shift.min(shift + sign * r.height),
        shift.max(shift + sign * r.height),
    ];
    let (lz, rz) = nearest_intervals([0., l.height], rz);
    let distance = (lr - rr).hypot(lz - rz);
    // Each actual solid is a subset of this complete annular-cylinder envelope.
    let lower = (distance - 16. * error - deviation).next_down();
    if lower <= t.linear {
        return Err(bad());
    }
    // Finite deterministic candidates include a bisector of both untouched
    // half-planes and their boundary directions. No sampling decides geometry.
    let mut candidates = vec![
        mul(l.radial, -1.),
        mul(r.radial, -1.),
        mul(add(l.radial, r.radial), -1.),
        cross(axis, l.radial),
        mul(cross(axis, l.radial), -1.),
        cross(axis, r.radial),
        mul(cross(axis, r.radial), -1.),
    ];
    candidates.push(l.radial);
    candidates.push(r.radial);
    for candidate in candidates {
        let Some(u) = radial(axis, candidate) else {
            continue;
        };
        let Some(v) = radial(ra, u) else { continue };
        // This is a native trim membership certificate over an entire ray.
        // Removed regions start strictly at positive local x=a (shaft) or q
        // (hub); thus these boundary points remain on actual cap/cylinder faces.
        if [&l, &r].iter().zip([u, v]).any(|(g, d)| {
            g.untouched_limit
                .is_some_and(|limit| g.outer * dot(d, g.radial).max(0.) + 8. * error >= limit)
        }) {
            continue;
        }
        let right_z = sign * (rz - shift);
        if lz < -error || lz > l.height + error || right_z < -error || right_z > r.height + error {
            return Err(KernelError::NumericRange);
        }
        let left_point = add(l.origin, add(mul(axis, lz), mul(u, lr)));
        let right_point = add(r.origin, add(mul(ra, right_z), mul(v, rr)));
        let d = sub(right_point, left_point);
        let witness = norm(d);
        let upper = (witness + 16. * error + deviation).next_up();
        if !upper.is_finite() || upper < lower || upper - lower > t.linear {
            return Err(KernelError::NumericRange);
        }
        return Ok(SolidClearance {
            method: ClearanceMethod::NativeTrimWitness,
            lower_bound_mm: lower,
            upper_bound_mm: upper,
            left_point,
            right_point,
            axis: mul(d, 1. / witness),
        });
    }
    Err(bad())
}

pub(crate) fn original_stock(
    m: &ModelSnapshot,
    g: &Envelope,
    t: Tolerance,
) -> Result<ModelSnapshot, KernelError> {
    if let Ok(s) = crate::stepped_keyseat::recognize(m, t) {
        return Ok(crate::revolve::construct(RevolveGeometry {
            segments: vec![],
            ..s.stock
        })
        .0);
    }
    if crate::revolve::recognize(m, t).is_ok() {
        return Ok(m.clone());
    }
    let mut stock = extrude_annulus(g.outer, g.inner, g.height, t)?.model;
    let y = cross(g.axis, g.radial);
    let vector = |v: [f64; 3]| add(mul(g.radial, v[0]), add(mul(y, v[1]), mul(g.axis, v[2])));
    let point = |p| add(g.origin, vector(p));
    for v in &mut stock.vertices {
        v.position = point(v.position);
    }
    for e in &mut stock.edges {
        match &mut e.curve {
            Curve::Circle(c) => {
                c.origin = point(c.origin);
                c.axis = vector(c.axis);
                c.x_direction = vector(c.x_direction);
            }
            Curve::Line(l) => {
                l.origin = point(l.origin);
                l.direction = vector(l.direction);
            }
            _ => return Err(bad()),
        }
    }
    for f in &mut stock.faces {
        match &mut f.surface {
            Surface::Plane(p) => {
                p.origin = point(p.origin);
                p.normal = vector(p.normal);
            }
            Surface::Cylinder(c) => {
                c.origin = point(c.origin);
                c.axis = vector(c.axis);
                c.x_direction = vector(c.x_direction);
            }
            _ => return Err(bad()),
        }
    }
    Ok(stock)
}
fn meridian_query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let l = admit(a, t)?;
    let r = admit(b, t)?;
    let left = original_stock(a, &l, t)?;
    let right = original_stock(b, &r, t)?;
    // The actual stepped filled meridian retains shoulders and recessed groove
    // radii. A maximum-radius cylindrical hull is deliberately not queried.
    let q = crate::solid_clearance::coaxial(&left, &right, t)?;
    let error = 4096.
        * f64::EPSILON
        * (norm(l.origin) + norm(r.origin) + l.outer + l.height + r.outer + r.height);
    if !error.is_normal() || error > t.linear / 80. {
        return Err(KernelError::NumericRange);
    }
    let axis = mul(l.axis, 1. / norm(l.axis));
    let ra = mul(r.axis, 1. / norm(r.axis));
    let lp = sub(q.left_point, l.origin);
    let rp = sub(q.right_point, r.origin);
    let lr = norm(cross(axis, lp));
    let rr = norm(cross(ra, rp));
    let lz = dot(lp, axis);
    let rz = dot(rp, ra);
    let candidates = [
        mul(l.radial, -1.),
        mul(r.radial, -1.),
        mul(add(l.radial, r.radial), -1.),
        cross(axis, l.radial),
        mul(cross(axis, l.radial), -1.),
        cross(axis, r.radial),
        mul(cross(axis, r.radial), -1.),
    ];
    for c in candidates {
        let Some(u) = radial(axis, c) else { continue };
        let Some(v) = radial(ra, u) else { continue };
        if [&l, &r].iter().zip([u, v]).any(|(g, d)| {
            g.untouched_limit
                .is_some_and(|limit| g.outer * dot(d, g.radial).max(0.) + 8. * error >= limit)
        }) {
            continue;
        }
        let left_point = add(l.origin, add(mul(axis, lz), mul(u, lr)));
        let right_point = add(r.origin, add(mul(ra, rz), mul(v, rr)));
        let delta = sub(right_point, left_point);
        let distance = norm(delta);
        let lower = (q.lower_bound_mm - 16. * error).next_down();
        let upper = (distance + 16. * error + (q.upper_bound_mm - q.lower_bound_mm)).next_up();
        if !lower.is_finite()
            || !upper.is_finite()
            || lower <= t.linear
            || upper < lower
            || upper - lower > t.linear
        {
            return Err(KernelError::NumericRange);
        }
        return Ok(SolidClearance {
            method: ClearanceMethod::NativeTrimWitness,
            lower_bound_mm: lower,
            upper_bound_mm: upper,
            left_point,
            right_point,
            axis: mul(delta, 1. / distance),
        });
    }
    Err(bad())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shaft(s: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        let stock = extrude_circle(6. * s, 40. * s, t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(4. * s, 4. * s, 14. * s, t).unwrap().model,
            [4. * s, -2. * s, 10. * s],
            t,
        )
        .unwrap()
        .model;
        boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model
    }
    fn hub(s: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        let stock = extrude_annulus(10. * s, 6.15 * s, 14. * s, t)
            .unwrap()
            .model;
        let tool = translate(
            &extrude_rectangle(8.05 * s, 4.04 * s, 16. * s, t)
                .unwrap()
                .model,
            [0., -2.02 * s, -s],
            t,
        )
        .unwrap()
        .model;
        boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model
    }
    fn check(a: &ModelSnapshot, b: &ModelSnapshot, d: f64) {
        let t = Tolerance::default();
        for (l, r) in [(a, b), (b, a)] {
            let q = l.clearance(r, t).unwrap();
            assert_eq!(q.method, ClearanceMethod::NativeTrimWitness);
            assert!(
                q.lower_bound_mm <= d && q.upper_bound_mm >= d,
                "{q:?} expected {d}"
            );
            assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
            for (m, p) in [(l, q.left_point), (r, q.right_point)] {
                let g = admit(m, t).unwrap();
                let v = sub(p, g.origin);
                let z = dot(v, g.axis);
                let radius = norm(cross(v, g.axis));
                assert!(z >= -1e-9 && z <= g.height + 1e-9);
                assert!(radius >= g.inner - 1e-9 && radius <= g.outer + 1e-9);
                if let Some(limit) = g.untouched_limit {
                    assert!(dot(v, g.radial) < limit - 1e-9);
                }
                assert!(
                    (z.abs().min((z - g.height).abs()))
                        .min((radius - g.outer).abs())
                        .min(if g.inner > 0. {
                            (radius - g.inner).abs()
                        } else {
                            f64::INFINITY
                        })
                        < 1e-8
                );
            }
        }
    }
    #[test]
    fn trim_witness_radial_axial_diagonal_and_actual_trim_membership() {
        let t = Tolerance::default();
        for s in [0.01, 1., 10.] {
            let shaft = shaft(s);
            for z in [2., 8., 12., 22., 30.] {
                let ring = translate(
                    &extrude_annulus(10. * s, 6.15 * s, 4. * s, t).unwrap().model,
                    [0., 0., z * s],
                    t,
                )
                .unwrap()
                .model;
                check(&shaft, &ring, 0.15 * s);
            }
            for z in [12., 45.] {
                let hub = translate(&hub(s), [0., 0., z * s], t).unwrap().model;
                check(
                    &shaft,
                    &hub,
                    if z < 40. {
                        0.15 * s
                    } else {
                        0.15_f64.hypot(5.) * s
                    },
                );
            }
            let next = translate(&shaft, [0., 0., 45. * s], t).unwrap().model;
            check(&shaft, &next, 5. * s);
        }
    }
    #[test]
    fn trim_witness_relative_keyway_angles_antiparallel_and_common_pose() {
        let t = Tolerance::default();
        let shaft = shaft(1.);
        for angle in [0_f64, std::f64::consts::FRAC_PI_2, std::f64::consts::PI] {
            let hub = transform(
                &hub(1.),
                [0., 0., 12.],
                [0., 0., (angle / 2.).sin(), (angle / 2.).cos()],
                t,
            )
            .unwrap()
            .model;
            check(&shaft, &hub, 0.15);
            let opposed = transform(&hub, [0., 0., 40.], [1., 0., 0., 0.], t)
                .unwrap()
                .model;
            check(&shaft, &opposed, 0.15);
            let pose = [0.5, 0.5, 0., 0.5_f64.sqrt()];
            let offset = [25., -12., 7.];
            let a = transform(&shaft, offset, pose, t).unwrap().model;
            let b = transform(&opposed, offset, pose, t).unwrap().model;
            check(&a, &b, 0.15);
        }
    }
    #[test]
    fn trim_witness_does_not_promote_envelope_witnesses_or_relax_alignment() {
        let t = Tolerance::default();
        let a = shaft(1.);
        for radius in [6., 5., 2.] {
            let ring = extrude_annulus(10., radius, 40., t).unwrap().model;
            assert!(a.clearance(&ring, t).is_err());
        }
        let b = hub(1.);
        assert!(a
            .clearance(&translate(&b, [0.01, 0., 0.], t).unwrap().model, t)
            .is_err());
        let tilted = transform(&b, [0.; 3], [0., 0.001_f64.sin(), 0., 0.001_f64.cos()], t)
            .unwrap()
            .model;
        assert!(a.clearance(&tilted, t).is_err());
        assert!(translate(&a, [1e6, 0., 0.], t).is_err());
        let mut bad = a;
        bad.vertices[3].position[0] += 1e-8;
        assert!(bad.clearance(&b, t).is_err());
        assert_eq!(
            serde_json::to_string(&ClearanceMethod::NativeTrimWitness).unwrap(),
            "\"native_trim_witness\""
        );
    }
}
