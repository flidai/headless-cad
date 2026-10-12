//! Nested coaxial clearance for a circular through bore in a rounded prism.
//! A containing annular meridian proves the lower bound; native surviving
//! bore/cap and shaft boundaries prove the upper bound.
use crate::math::{cross, dot, norm, sub};
use crate::*;

fn bad() -> KernelError {
    KernelError::Unsupported(
        "nested bore clearance requires arithmetic-coaxial shaft stock, a guarded circular through bore, and surviving native boundary witnesses".into(),
    )
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|x| x * s)
}
fn shaft(m: &ModelSnapshot, t: Tolerance) -> Option<crate::trim_clearance::Envelope> {
    let g = crate::trim_clearance::admit(m, t).ok()?;
    (g.inner == 0.).then_some(g)
}
pub(crate) fn is_candidate(a: &ModelSnapshot, b: &ModelSnapshot, t: Tolerance) -> bool {
    (crate::convex_trim_clearance::is_candidate(a, t) && shaft(b, t).is_some())
        || (crate::convex_trim_clearance::is_candidate(b, t) && shaft(a, t).is_some())
}

fn outer_radius(path: &PlanarPath, center: [f64; 2]) -> f64 {
    let mut radius = (path.start[0] - center[0]).hypot(path.start[1] - center[1]);
    for segment in &path.segments {
        match segment {
            PathSegment::LineTo { end } => {
                radius = radius.max((end[0] - center[0]).hypot(end[1] - center[1]));
            }
            PathSegment::ArcTo {
                center: arc, end, ..
            } => {
                let arc_radius = (end[0] - arc[0]).hypot(end[1] - arc[1]);
                radius = radius.max((arc[0] - center[0]).hypot(arc[1] - center[1]) + arc_radius);
            }
        }
    }
    radius
}

fn oriented(
    housing: &ModelSnapshot,
    shaft_model: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let admitted = crate::convex_trim_clearance::admit(housing, t)?;
    let g = admitted.geometry;
    let stock = shaft(shaft_model, t).ok_or_else(bad)?;
    let world = housing
        .vertices
        .iter()
        .chain(&shaft_model.vertices)
        .map(|v| norm(v.position))
        .fold(norm(g.origin).max(norm(stock.origin)), f64::max);
    let extent = g
        .circular_holes
        .iter()
        .map(|hole| outer_radius(&g.path, hole.center))
        .fold(0., f64::max)
        + stock.outer
        + stock.height
        + g.height
        + norm(sub(stock.origin, g.origin));
    // Arc-prism reconstruction permits at most 8192 ulps of raw native
    // support departure; keyed stock reconstruction and reconciled shoulder
    // levels permit at most 4096 ulps each. The second extent term reserves
    // projection, frame rotation over the complete bodies, and the synthetic
    // annulus reconstruction. No modeling tolerance is used to merge axes.
    let error = ((8192. * world + 16384. * extent) * f64::EPSILON).next_up();
    if !error.is_normal() || error > t.linear / 64. {
        return Err(KernelError::NumericRange);
    }
    crate::convex_clearance::support::arc_consistency(housing, &g, error, t)?;
    let y = cross(g.axis, g.x_direction);
    let centers = g
        .circular_holes
        .iter()
        .enumerate()
        .filter_map(|(i, hole)| {
            let origin = add(
                g.origin,
                add(mul(g.x_direction, hole.center[0]), mul(y, hole.center[1])),
            );
            let delta = sub(stock.origin, origin);
            (norm(cross(g.axis, delta)) <= error).then_some(i)
        })
        .collect::<Vec<_>>();
    if centers.len() != 1 {
        return Err(bad());
    }
    let index = centers[0];
    let bore = &g.circular_holes[index];
    // A whole guarded disk around the selected bore contains every potential
    // axial cap witness and excludes the remaining mounting holes.
    let guard = (4. * error).next_up();
    let disk_radius = stock.outer.max(bore.radius);
    let inradius = crate::arc_prism::path_boundary_distance(&g.path, bore.center, t, error)?;
    if inradius <= disk_radius + guard
        || g.circular_holes.iter().enumerate().any(|(i, other)| {
            i != index
                && (other.center[0] - bore.center[0]).hypot(other.center[1] - bore.center[1])
                    - other.radius
                    <= disk_radius + guard
        })
    {
        return Err(bad());
    }
    let bore_origin = add(
        g.origin,
        add(mul(g.x_direction, bore.center[0]), mul(y, bore.center[1])),
    );
    let outer = outer_radius(&g.path, bore.center) + guard;
    if !outer.is_finite() || outer - bore.radius <= t.linear {
        return Err(KernelError::NumericRange);
    }
    let annulus = crate::revolve::construct(RevolveGeometry {
        origin: bore_origin,
        axis: g.axis,
        x_direction: g.x_direction,
        profile: vec![
            [bore.radius, 0.],
            [outer, 0.],
            [outer, g.height],
            [bore.radius, g.height],
        ],
        segments: vec![],
    })
    .0;
    let stock_model = crate::trim_clearance::original_stock(shaft_model, &stock, t)?;
    // The ideal annulus need not literally contain every raw housing point:
    // native bore/cap supports may drift inward by `error`. Every raw housing
    // point is within `guard` of this ideal containing annulus, and every raw
    // trimmed-shaft point is within `guard` of the ideal uncut stock. The
    // coaxial meridian distance is therefore a lower bound after charging
    // both displacements. Its own interval already charges solver arithmetic.
    let q = crate::solid_clearance::coaxial(&annulus, &stock_model, t)?;
    let hv = sub(q.left_point, bore_origin);
    let sv = sub(q.right_point, stock.origin);
    let hr = norm(cross(g.axis, hv));
    let hz = dot(hv, g.axis);
    let sr = norm(cross(stock.axis, sv));
    let sz = dot(sv, stock.axis);
    // The containing annulus has an artificial exterior. Its minimizer is
    // useful only on the actual bore wall or guarded cap annulus.
    if hr < bore.radius - guard
        || hr > disk_radius + guard
        || hz < -guard
        || hz > g.height + guard
        || ((hr - bore.radius).abs() > guard && hz.abs().min((hz - g.height).abs()) > guard)
    {
        return Err(bad());
    }
    let housing_axis = mul(g.axis, 1. / norm(g.axis));
    let stock_axis = mul(stock.axis, 1. / norm(stock.axis));
    let candidates = [
        mul(stock.radial, -1.),
        mul(g.x_direction, -1.),
        mul(add(stock.radial, g.x_direction), -1.),
        cross(housing_axis, stock.radial),
        mul(cross(housing_axis, stock.radial), -1.),
        cross(housing_axis, g.x_direction),
        mul(cross(housing_axis, g.x_direction), -1.),
    ];
    for candidate in candidates {
        let Some(u) = crate::trim_clearance::radial(housing_axis, candidate) else {
            continue;
        };
        let Some(v) = crate::trim_clearance::radial(stock_axis, u) else {
            continue;
        };
        if stock
            .untouched_limit
            .is_some_and(|limit| stock.outer * dot(v, stock.radial).max(0.) + guard >= limit)
        {
            continue;
        }
        let left_point = add(bore_origin, add(mul(housing_axis, hz), mul(u, hr)));
        let right_point = add(stock.origin, add(mul(stock_axis, sz), mul(v, sr)));
        let d = sub(right_point, left_point);
        let distance = norm(d);
        // The reported points lie on the ideal surviving bore/cap and stock
        // boundaries. Raw native supports are within one guard on each side.
        let lower = (q.lower_bound_mm - 2. * guard).next_down();
        let upper = (distance + 2. * guard).next_up();
        if !distance.is_normal()
            || !lower.is_finite()
            || !upper.is_finite()
            || lower <= t.linear
            || upper < lower
            || upper - lower > t.linear
        {
            return Err(KernelError::NumericRange);
        }
        return Ok(SolidClearance {
            method: ClearanceMethod::BoreTrimWitness,
            lower_bound_mm: lower,
            upper_bound_mm: upper,
            left_point,
            right_point,
            axis: mul(d, 1. / distance),
        });
    }
    Err(bad())
}

pub(crate) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    if crate::convex_trim_clearance::is_candidate(a, t) && shaft(b, t).is_some() {
        return oriented(a, b, t);
    }
    let mut q = oriented(b, a, t)?;
    std::mem::swap(&mut q.left_point, &mut q.right_point);
    q.axis = mul(q.axis, -1.);
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn housing(t: Tolerance) -> ModelSnapshot {
        housing_with_mounts(t, 14.)
    }

    fn housing_with_mounts(t: Tolerance, mount_x: f64) -> ModelSnapshot {
        let path = PlanarPath {
            start: [-16., -15.],
            segments: vec![
                PathSegment::LineTo { end: [16., -15.] },
                PathSegment::ArcTo {
                    center: [16., -11.],
                    end: [20., -11.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [20., 11.] },
                PathSegment::ArcTo {
                    center: [16., 11.],
                    end: [16., 15.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [-16., 15.] },
                PathSegment::ArcTo {
                    center: [-16., 11.],
                    end: [-20., 11.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [-20., -11.] },
                PathSegment::ArcTo {
                    center: [-16., -11.],
                    end: [-16., -15.],
                    clockwise: false,
                },
            ],
        };
        let holes = [
            CircleHole {
                center: [0., 0.],
                radius: 6.15,
            },
            CircleHole {
                center: [-mount_x, 0.],
                radius: 2.,
            },
            CircleHole {
                center: [mount_x, 0.],
                radius: 2.,
            },
        ];
        extrude_path(&path, &holes, 12., t).unwrap().model
    }

    fn keyed(t: Tolerance) -> ModelSnapshot {
        let stock = extrude_circle(6., 40., t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(4., 4., 8., t).unwrap().model,
            [4., -2., 16.],
            t,
        )
        .unwrap()
        .model;
        let cut = boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        translate(&cut, [0., 0., -10.], t).unwrap().model
    }

    fn stepped_keyed(t: Tolerance) -> ModelSnapshot {
        let stock = revolve_polygon(
            &[
                [0., 0.],
                [8., 0.],
                [8., 9.95],
                [6., 9.95],
                [6., 40.],
                [0., 40.],
            ],
            t,
        )
        .unwrap()
        .model;
        let tool = translate(
            &extrude_rectangle(4., 4., 8., t).unwrap().model,
            [4., -2., 16.],
            t,
        )
        .unwrap()
        .model;
        let cut = boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        translate(&cut, [0., 0., -10.], t).unwrap().model
    }

    #[test]
    fn nested_rounded_bore_plain_and_keyed_running_gap() {
        let t = Tolerance::default();
        let h = housing(t);
        let plain = translate(
            &extrude_circle(6., 40., t).unwrap().model,
            [0., 0., -10.],
            t,
        )
        .unwrap()
        .model;
        for shaft in [plain, keyed(t)] {
            assert!(
                query(&h, &shaft, t).is_ok(),
                "nested: {:?}",
                query(&h, &shaft, t).err()
            );
            for (a, b) in [(&h, &shaft), (&shaft, &h)] {
                let q = a.clearance(b, t).unwrap();
                assert_eq!(q.method, ClearanceMethod::BoreTrimWitness);
                assert!(
                    q.lower_bound_mm <= 0.15 && q.upper_bound_mm >= 0.15,
                    "{q:?}"
                );
                assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
            }
        }
    }

    #[test]
    fn stepped_keyed_shaft_shoulder_gap_beats_running_gap() {
        let t = Tolerance::default();
        let h = housing(t);
        let shaft = stepped_keyed(t);
        for (a, b) in [(&h, &shaft), (&shaft, &h)] {
            let q = a.clearance(b, t).unwrap();
            assert_eq!(q.method, ClearanceMethod::BoreTrimWitness);
            assert!(
                q.lower_bound_mm <= 0.05 && q.upper_bound_mm >= 0.05,
                "{q:?}"
            );
        }
    }

    #[test]
    fn nested_bore_refuses_eccentric_contact_and_corrupt_support() {
        let t = Tolerance::default();
        let h = housing(t);
        let shaft = keyed(t);
        let offset = translate(&shaft, [0.001, 0., 0.], t).unwrap().model;
        assert!(query(&h, &offset, t).is_err());
        let contact = translate(&shaft, [0.15, 0., 0.], t).unwrap().model;
        assert!(query(&h, &contact, t).is_err());
        let mut corrupt = h.clone();
        let edge = corrupt
            .edges
            .iter_mut()
            .find(|edge| matches!(edge.curve, Curve::Circle(_)))
            .unwrap();
        if let Curve::Circle(circle) = &mut edge.curve {
            circle.radius += 0.01;
        }
        assert!(query(&corrupt, &shaft, t).is_err());
    }

    #[test]
    fn opposed_axis_and_small_raw_bore_support_drift_remain_bounded() {
        let t = Tolerance::default();
        let plain = extrude_circle(6., 40., t).unwrap().model;
        let opposed = transform(&plain, [0., 0., 30.], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        let mut h = housing(t);
        let n = 8;
        let drift = 1e-12;
        if let Curve::Circle(circle) = &mut h.edges[3 * n + 1].curve {
            circle.origin[0] += drift;
            circle.radius += drift;
        } else {
            panic!("expected upper bore circle");
        }
        if let Surface::Cylinder(wall) = &mut h.faces[n + 2].surface {
            wall.origin[0] += drift;
            wall.radius += drift;
        } else {
            panic!("expected bore wall");
        }
        let q = query(&h, &opposed, t).unwrap();
        assert!(q.lower_bound_mm <= 0.15 && q.upper_bound_mm >= 0.15);
        assert!(q.upper_bound_mm - q.lower_bound_mm <= t.linear);
    }

    #[test]
    fn tilt_mounting_hole_intrusion_near_contact_and_far_world_refuse() {
        let t = Tolerance::default();
        let h = housing(t);
        let shaft = keyed(t);
        let tilted = transform(
            &shaft,
            [0.; 3],
            [0., 0.001_f64.sin(), 0., 0.001_f64.cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(query(&h, &tilted, t).is_err());
        let intruding = housing_with_mounts(t, 9.);
        assert!(query(&intruding, &stepped_keyed(t), t).is_err());
        let near = translate(&stepped_keyed(t), [0., 0., 0.04999999], t)
            .unwrap()
            .model;
        assert!(query(&h, &near, t).is_err());
        let far_h = translate(&h, [1e9, 0., 0.], t);
        let far_shaft = translate(&shaft, [1e9, 0., 0.], t);
        assert!(match (far_h, far_shaft) {
            (Ok(far_h), Ok(far_shaft)) => query(&far_h.model, &far_shaft.model, t).is_err(),
            _ => true, // Transform itself may reach its stricter precision gate.
        });
    }
}
