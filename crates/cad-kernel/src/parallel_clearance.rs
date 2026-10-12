//! Exact planar annulus region distance combined with independent axial intervals.
use super::*;
pub(super) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let l = meridian(a, t)?;
    let r = meridian(b, t)?;
    let (Some(lc), Some(rc)) = (&l.circle, &r.circle) else {
        return Err(unsupported());
    };
    if lc.inner_radius == 0. && rc.inner_radius == 0. {
        return Err(unsupported());
    }
    let delta = sub(r.origin, l.origin);
    let scale = l.extent().max(r.extent()).max(norm(delta));
    let world = a
        .vertices
        .iter()
        .chain(&b.vertices)
        .map(|v| norm(v.position))
        .fold(norm(l.origin).max(norm(r.origin)), f64::max);
    let error = (512. * world + 2048. * scale) * f64::EPSILON;
    if !error.is_normal() || error > t.linear / 80. || !(1e-50..=1e50).contains(&scale) {
        return Err(KernelError::NumericRange);
    }
    l.certify(a, error)?;
    r.certify(b, error)?;
    let axis = mul(l.axis, 1. / norm(l.axis));
    let ra = mul(r.axis, 1. / norm(r.axis));
    let sign = if dot(axis, ra) >= 0. { 1. } else { -1. };
    let tilt = norm(sub(ra, mul(axis, sign)));
    let deviation = tilt * r.extent();
    if tilt > 128. * f64::EPSILON || deviation > 4. * error {
        return Err(unsupported());
    }
    let shift = dot(delta, axis);
    let offset = cross(axis, cross(delta, axis));
    let d = norm(offset);
    if d <= error {
        return Err(unsupported());
    } // The coaxial branch owns arithmetic-zero offsets.
    let u = mul(offset, 1. / d);
    let v = cross(axis, u);
    let planar = (d - lc.radius - rc.radius)
        .max(lc.inner_radius - d - rc.radius)
        .max(rc.inner_radius - d - lc.radius)
        .max(0.);
    let (lp, rp) = if d > lc.radius + rc.radius {
        ([lc.radius, 0.], [d - rc.radius, 0.])
    } else if lc.inner_radius > d + rc.radius {
        ([lc.inner_radius, 0.], [d + rc.radius, 0.])
    } else if rc.inner_radius > d + lc.radius {
        ([-lc.radius, 0.], [d - rc.inner_radius, 0.])
    } else {
        // Select lengths inside both radial intervals that satisfy the triangle
        // inequalities. Their circle intersection supplies a shared material point;
        // overlapping thin rings need not intersect the line between their centers.
        let smin = lc
            .inner_radius
            .max(rc.inner_radius - d)
            .max(d - rc.radius)
            .max(0.);
        let smax = lc.radius.min(d + rc.radius);
        if smin > smax {
            return Err(KernelError::NumericRange);
        }
        let s = smin + (smax - smin) / 2.;
        let tmin = rc.inner_radius.max((d - s).abs());
        let tmax = rc.radius.min(d + s);
        if tmin > tmax {
            return Err(KernelError::NumericRange);
        }
        let rr = tmin + (tmax - tmin) / 2.;
        let x = (s - rr) * (s + rr) / (2. * d) + d / 2.;
        let yy = (s - x) * (s + x);
        if yy < 0. || !yy.is_finite() {
            return Err(KernelError::NumericRange);
        }
        let y = yy.sqrt();
        ([x, y], [x, y])
    };
    let rlo = shift.min(shift + sign * rc.height);
    let rhi = shift.max(shift + sign * rc.height);
    let (lz, rz) = if lc.height < rlo {
        (lc.height, rlo)
    } else if rhi < 0. {
        (0., rhi)
    } else {
        let z = 0_f64.max(rlo);
        (z, z)
    };
    let exact = planar.hypot(lz - rz);
    if exact <= t.linear + 32. * error + deviation {
        return Err(unsupported());
    }
    let local_left = cross(axis, cross(add(mul(u, lp[0]), mul(v, lp[1])), axis));
    let local_right = add(mul(u, rp[0] - d), mul(v, rp[1]));
    let lr = norm(local_left);
    let rr = norm(local_right);
    // Reconcile to the actual circular frames with feasible radial magnitudes.
    // Clamping only witness radii within the native material intervals cannot
    // change the analytic lower bound or create a fictitious material point.
    let lr = lr.clamp(lc.inner_radius, lc.radius);
    let rr = rr.clamp(rc.inner_radius, rc.radius);
    let left_radial = if norm(local_left) > 0. {
        mul(local_left, lr / norm(local_left))
    } else {
        mul(l.radial, lr)
    };
    let projected = cross(ra, cross(local_right, ra));
    let right_radial = if norm(projected) > 0. {
        mul(projected, rr / norm(projected))
    } else {
        mul(r.radial, rr)
    };
    let left_point = add(l.origin, add(mul(axis, lz), left_radial));
    let right_point = add(
        r.origin,
        add(
            mul(ra, (sign * (rz - shift)).clamp(0., rc.height)),
            right_radial,
        ),
    );
    let actual = norm(sub(right_point, left_point));
    let lower = exact - 32. * error - deviation;
    let upper = exact.max(actual) + 32. * error + deviation;
    if !lower.is_finite() || !upper.is_finite() || lower <= t.linear || upper - lower > t.linear {
        return Err(KernelError::NumericRange);
    }
    Ok(SolidClearance {
        method: ClearanceMethod::ParallelAnnular,
        lower_bound_mm: lower,
        upper_bound_mm: upper,
        left_point,
        right_point,
        axis: mul(sub(right_point, left_point), 1. / actual),
    })
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
            assert_eq!(q.method, ClearanceMethod::ParallelAnnular);
            assert!(
                q.lower_bound_mm <= expected && expected <= q.upper_bound_mm,
                "{q:?} expected{expected}"
            );
            assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
            for (m, p) in [(l, q.left_point), (r, q.right_point)] {
                let g = m.cylinder_geometry(t).unwrap().unwrap();
                let delta = sub(p, g.origin);
                let z = dot(delta, g.axis);
                let radius = norm(cross(delta, g.axis));
                assert!(z >= -1e-10 && z <= g.height + 1e-10);
                assert!(radius >= g.inner_radius - 1e-10 && radius <= g.radius + 1e-10);
            }
            assert!((norm(q.axis) - 1.).abs() < 1e-14);
        }
    }
    #[test]
    fn eccentric_annular_material_gap_external_nested_and_diagonal() {
        let t = Tolerance::default();
        for s in [0.01, 1., 100.] {
            let ring = extrude_annulus(3. * s, 2. * s, 4. * s, t).unwrap().model;
            let shaft = extrude_circle(s, 4. * s, t).unwrap().model;
            for (source, offset, expected) in [
                (&shaft, [0.25 * s, 0., 0.], 0.75 * s),
                (&ring, [8. * s, 0., 0.], 2. * s),
                (&ring, [8. * s, 0., 7. * s], 13_f64.sqrt() * s),
                (&ring, [4. * s, 0., 7. * s], 3. * s),
            ] {
                let b = translate(source, offset, t).unwrap().model;
                check(&ring, &b, expected);
                let opposite = transform(
                    source,
                    [offset[0], offset[1], offset[2] + 4. * s],
                    [1., 0., 0., 0.],
                    t,
                )
                .unwrap()
                .model;
                check(&ring, &opposite, expected);
                let q = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
                let left = transform(&ring, [20., -8., 3.], q, t).unwrap().model;
                let right = transform(&b, [20., -8., 3.], q, t).unwrap().model;
                check(&left, &right, expected);
            }
            let smallring = extrude_annulus(s, 0.5 * s, 4. * s, t).unwrap().model;
            check(
                &ring,
                &translate(&smallring, [0.25 * s, 0., 0.], t).unwrap().model,
                0.75 * s,
            );
        }
    }
    #[test]
    fn thin_ring_overlap_requires_off_centerline_feasible_cap_witness() {
        let t = Tolerance::default();
        let a = extrude_annulus(5., 4.9, 2., t).unwrap().model;
        let b = translate(&a, [5., 0., 5.], t).unwrap().model;
        check(&a, &b, 3.);
        let q = a.clearance(&b, t).unwrap();
        assert!(q.left_point[1].abs() > 4.);
        // Tiny true lateral offsets stay supported without division by zero.
        let ring = extrude_annulus(3., 2., 4., t).unwrap().model;
        let shaft = extrude_circle(1., 4., t).unwrap().model;
        check(
            &ring,
            &translate(&shaft, [1e-8, 0., 0.], t).unwrap().model,
            1. - 1e-8,
        );
        // A common oblique pose with a tiny lateral displacement must not
        // amplify axial cancellation into an infeasible cap witness.
        let shifted = translate(&ring, [1e-8, 0., 7.], t).unwrap().model;
        let pose = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
        let a = transform(&ring, [20., -8., 3.], pose, t).unwrap().model;
        let b = transform(&shifted, [20., -8., 3.], pose, t).unwrap().model;
        check(&a, &b, 3.);
        // Radially tangent cross-sections still have positive axial material distance.
        check(&ring, &translate(&ring, [6., 0., 7.], t).unwrap().model, 3.);
    }
    #[test]
    fn overlap_tilt_nonconstant_radial_support_and_precision_fail_closed() {
        let t = Tolerance::default();
        let a = extrude_annulus(3., 2., 4., t).unwrap().model;
        for offset in [[4., 0., 0.], [6., 0., 0.], [4., 0., 4.]] {
            let b = translate(&a, offset, t).unwrap().model;
            assert!(a.clearance(&b, t).is_err());
        }
        let b = transform(
            &a,
            [8., 0., 0.],
            [0., (0.001_f64).sin(), 0., (0.001_f64).cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(a.clearance(&b, t).is_err());
        let nonconstant = revolve_polygon(&[[2., 0.], [3., 0.], [4., 4.], [3., 4.]], t)
            .unwrap()
            .model;
        assert!(a
            .clearance(&translate(&nonconstant, [8., 0., 0.], t).unwrap().model, t)
            .is_err());
        let mut bad = translate(&a, [8., 0., 0.], t).unwrap().model;
        bad.vertices[2].position[0] += 1e-8;
        assert!(a.clearance(&bad, t).is_err());
        let far = translate(&a, [10000., 0., 0.], t).unwrap().model;
        let other = translate(&a, [10008., 0., 0.], t).unwrap().model;
        assert!(matches!(
            far.clearance(&other, Tolerance { linear: 1e-8 }),
            Err(KernelError::NumericRange)
        ));
    }
}
