//! Complete capsule-footprint fit in one actual blind pocket. Floor coincidence
//! is a separate seating obligation; axial nonpenetration is nominal within the
//! explicit arithmetic interval, not an exact contact or interference solver.
use crate::math::{cross, dot, norm, sub};
use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsulePocketFit {
    pub pocket_floor_face: FaceId,
    pub pocket_wall_faces: [FaceId; 4],
    pub insert_floor_face: FaceId,
    pub pocket_witness_face: FaceId,
    pub insert_witness_face: FaceId,
    pub side_clearance_lower_mm: f64,
    pub side_clearance_upper_mm: f64,
    pub pocket_point: [f64; 3],
    pub insert_point: [f64; 3],
    pub axial_overlap_lower_mm: f64,
    pub axial_overlap_upper_mm: f64,
    pub signed_floor_gap_lower_mm: f64,
    pub signed_floor_gap_upper_mm: f64,
    pub arithmetic_uncertainty_mm: f64,
}
fn bad() -> KernelError {
    KernelError::Unsupported("capsule pocket fit requires a strict native blind floor and a parallel solid capsule insert with positive side clearance and wall overlap, without resolved floor penetration".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|x| x * s)
}
fn point(g: &ArcPrismGeometry, p: [f64; 2]) -> [f64; 3] {
    add(
        g.origin,
        add(
            mul(g.x_direction, p[0]),
            mul(cross(g.axis, g.x_direction), p[1]),
        ),
    )
}
fn end(segment: &PathSegment) -> [f64; 2] {
    match segment {
        PathSegment::LineTo { end } | PathSegment::ArcTo { end, .. } => *end,
    }
}
fn nearest(q: [f64; 3], center: [f64; 3], along: [f64; 3], half: f64) -> [f64; 3] {
    add(
        center,
        mul(along, dot(sub(q, center), along).clamp(-half, half)),
    )
}
fn insert_capsule(
    g: &ArcPrismGeometry,
    error: f64,
) -> Result<([f64; 3], [f64; 3], f64), KernelError> {
    if !g.circular_holes.is_empty() || !g.slot_holes.is_empty() || g.path.segments.len() != 4 {
        return Err(bad());
    }
    let mut start = g.path.start;
    let mut arcs = Vec::new();
    for (i, s) in g.path.segments.iter().enumerate() {
        if let PathSegment::ArcTo {
            center,
            end,
            clockwise,
        } = s
        {
            let r = (start[0] - center[0]).hypot(start[1] - center[1]);
            if *clockwise
                || r <= error
                || (0..2).any(|k| (start[k] + end[k] - 2. * center[k]).abs() > error)
            {
                return Err(bad());
            }
            arcs.push((i, *center, r));
        }
        start = end(s);
    }
    if arcs.len() != 2 || (arcs[0].0 + 2) % 4 != arcs[1].0 || (arcs[0].2 - arcs[1].2).abs() > error
    {
        return Err(bad());
    }
    let a = point(g, arcs[0].1);
    let b = point(g, arcs[1].1);
    let length = norm(sub(b, a));
    if length <= error {
        return Err(bad());
    }
    let along = mul(sub(b, a), 1. / length);
    // Every join must be a true tangent endpoint: diameter perpendicular to the
    // centerline, with the declared common radius. This excludes general biarcs.
    start = g.path.start;
    for s in &g.path.segments {
        if let PathSegment::ArcTo { center, .. } = s {
            let radial = sub(point(g, start), point(g, *center));
            if dot(radial, along).abs() > error || (norm(radial) - arcs[0].2).abs() > error {
                return Err(bad());
            }
        } else {
            let delta = sub(point(g, end(s)), point(g, start));
            if (norm(delta) - length).abs() > error || norm(cross(delta, along)) > error {
                return Err(bad());
            }
        }
        start = end(s);
    }
    Ok((a, b, arcs[0].2))
}
fn wall_at(g: &ArcPrismGeometry, p: [f64; 3], error: f64) -> Result<FaceId, KernelError> {
    let d = sub(p, g.origin);
    let local = [dot(d, g.x_direction), dot(d, cross(g.axis, g.x_direction))];
    let mut start = g.path.start;
    for (i, s) in g.path.segments.iter().enumerate() {
        let finish = end(s);
        let hit = match s {
            PathSegment::LineTo { .. } => {
                let v = [finish[0] - start[0], finish[1] - start[1]];
                let l = v[0] * v[0] + v[1] * v[1];
                let u = (((local[0] - start[0]) * v[0] + (local[1] - start[1]) * v[1]) / l)
                    .clamp(0., 1.);
                (local[0] - start[0] - u * v[0]).hypot(local[1] - start[1] - u * v[1]) <= error
            }
            PathSegment::ArcTo { center, .. } => {
                let r = (start[0] - center[0]).hypot(start[1] - center[1]);
                let radial = [local[0] - center[0], local[1] - center[1]];
                let start_v = [start[0] - center[0], start[1] - center[1]];
                // Recognized arcs are CCW semicircles; this half-plane is their
                // actual trim, rather than just the full cylindrical support.
                (radial[0].hypot(radial[1]) - r).abs() <= error
                    && start_v[0] * radial[1] - start_v[1] * radial[0] >= -error * r
            }
        };
        if hit {
            return Ok(FaceId(2 + i));
        }
        start = finish;
    }
    Err(KernelError::NumericRange)
}
/// Certify the full capsule footprint, not sampled surface points. Both actual
/// operands are canonically reconstructed at the query's arithmetic budget.
/// Reported axial floor bounds may straddle zero at nominal contact; resolved
/// penetration, absent wall overlap, and nonpositive side clearance refuse.
pub fn certify_capsule_pocket_fit(
    pocket_body: &ModelSnapshot,
    floor_face: FaceId,
    insert_body: &ModelSnapshot,
    t: Tolerance,
) -> Result<CapsulePocketFit, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    for m in [pocket_body, insert_body] {
        if m.vertices.len() > 1024
            || m.edges.len() > 1536
            || m.faces.len() > 512
            || m.loops.len() > 1024
            || m.loops.iter().map(|l| l.edges.len()).sum::<usize>() > 4096
        {
            return Err(KernelError::ResourceLimit(
                "capsule pocket fit geometry budget exceeded".into(),
            ));
        }
    }
    let pocket = crate::blind_slot_prism::recognize(pocket_body, t)?;
    let g = crate::arc_prism::recognize(insert_body, t)?;
    let floor_start = pocket_body.faces.len() - pocket.slots.len();
    if floor_face.0 < floor_start || floor_face.0 >= pocket_body.faces.len() {
        return Err(bad());
    }
    let index = floor_face.0 - floor_start;
    let slot = &pocket.slots[index];
    let raw_scale = [pocket_body, insert_body]
        .into_iter()
        .flat_map(|m| m.vertices.iter())
        .map(|v| norm(v.position))
        .fold(1., f64::max)
        + norm(pocket.stock.origin)
        + norm(g.origin)
        + pocket.stock.height
        + g.height
        + slot.radius;
    let error = (4096. * f64::EPSILON * raw_scale).next_up();
    if !error.is_finite() || error > t.linear / 64. {
        return Err(KernelError::NumericRange);
    }
    crate::blind_slot_prism::consistency(pocket_body, &pocket, error / 16., t)?;
    crate::convex_clearance::support::arc_consistency(insert_body, &g, error / 16., t)?;
    let (a, b, r) = insert_capsule(&g, error / 16.)?;
    let direction = if pocket.opening == FaceId(1) {
        pocket.stock.axis
    } else {
        mul(pocket.stock.axis, -1.)
    };
    let departure = norm(cross(direction, g.axis));
    if departure > 128. * f64::EPSILON {
        return Err(bad());
    }
    let uncertainty = (16. * error + departure * (g.height + norm(sub(b, a)) + r)).next_up();
    let floor_z = if pocket.opening == FaceId(1) {
        pocket.stock.height - pocket.depths[index]
    } else {
        pocket.depths[index]
    };
    let center = add(
        point(&pocket.stock, slot.center),
        mul(pocket.stock.axis, floor_z),
    );
    let along = add(
        mul(pocket.stock.x_direction, slot.direction[0]),
        mul(
            cross(pocket.stock.axis, pocket.stock.x_direction),
            slot.direction[1],
        ),
    );
    let half = slot.centerline_length / 2.;
    let first = dot(sub(g.origin, center), direction);
    let last = first + dot(g.axis, direction) * g.height;
    let low = first.min(last);
    let high = first.max(last);
    let overlap = high.min(pocket.depths[index]) - low.max(0.);
    if low < -uncertainty || overlap <= 2. * uncertainty {
        return Err(bad());
    }
    let projected = |p: [f64; 3]| sub(p, mul(direction, dot(sub(p, center), direction)));
    let endpoints = [projected(a), projected(b)];
    let closest = endpoints.map(|p| nearest(p, center, along, half));
    let distances = std::array::from_fn::<_, 2, _>(|i| norm(sub(endpoints[i], closest[i])));
    let mut selected = usize::from(distances[1] > distances[0]);
    let distance = distances[selected];
    let n = if distance > error {
        mul(sub(endpoints[selected], closest[selected]), 1. / distance)
    } else {
        let n = cross(direction, along);
        selected = usize::from(dot(endpoints[1], n) > dot(endpoints[0], n));
        n
    };
    let gap = slot.radius - r - distance;
    if !gap.is_finite() || gap <= uncertainty {
        return Err(bad());
    }
    let z = (low.max(0.) + high.min(pocket.depths[index])) / 2.;
    let pocket_point = add(
        add(closest[selected], mul(n, slot.radius)),
        mul(direction, z),
    );
    let insert_axis_point = add(
        [a, b][selected],
        mul(
            g.axis,
            (z - dot(sub([a, b][selected], center), direction)) / dot(g.axis, direction),
        ),
    );
    let radial = sub(n, mul(g.axis, dot(n, g.axis)));
    let insert_point = add(insert_axis_point, mul(radial, r / norm(radial)));
    let insert_witness_face = wall_at(&g, insert_point, uncertainty)?;
    let base = 2
        + pocket.stock.path.segments.len()
        + pocket.stock.circular_holes.len()
        + 4 * pocket.stock.slot_holes.len();
    let wall_start = base + 4 * index;
    let axial = dot(sub(closest[selected], center), along);
    let side = dot(n, cross(pocket.stock.axis, along));
    let wall = if axial >= half - error && dot(n, along) > 0. {
        1
    } else if axial <= -half + error && dot(n, along) < 0. {
        3
    } else if side < 0. {
        0
    } else {
        2
    };
    let witness_distance = norm(sub(pocket_point, insert_point));
    if (witness_distance - gap).abs() > uncertainty {
        return Err(KernelError::NumericRange);
    }
    Ok(CapsulePocketFit {
        pocket_floor_face: floor_face,
        pocket_wall_faces: std::array::from_fn(|i| FaceId(wall_start + i)),
        insert_floor_face: FaceId(usize::from(last < first)),
        pocket_witness_face: FaceId(wall_start + wall),
        insert_witness_face,
        side_clearance_lower_mm: (gap - uncertainty).next_down(),
        side_clearance_upper_mm: (witness_distance + uncertainty).next_up(),
        pocket_point,
        insert_point,
        axial_overlap_lower_mm: (overlap - uncertainty).next_down(),
        axial_overlap_upper_mm: (overlap + uncertainty).next_up(),
        signed_floor_gap_lower_mm: (low - uncertainty).next_down(),
        signed_floor_gap_upper_mm: (low + uncertainty).next_up(),
        arithmetic_uncertainty_mm: uncertainty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn capsule(r: f64, l: f64, h: f64, t: Tolerance) -> ModelSnapshot {
        extrude_path(
            &PlanarPath {
                start: [r, -l / 2.],
                segments: vec![
                    PathSegment::LineTo { end: [r, l / 2.] },
                    PathSegment::ArcTo {
                        center: [0., l / 2.],
                        end: [-r, l / 2.],
                        clockwise: false,
                    },
                    PathSegment::LineTo { end: [-r, -l / 2.] },
                    PathSegment::ArcTo {
                        center: [0., -l / 2.],
                        end: [r, -l / 2.],
                        clockwise: false,
                    },
                ],
            },
            &[],
            h,
            t,
        )
        .unwrap()
        .model
    }
    pub(super) fn fixture(cap: usize) -> (ModelSnapshot, FaceId, ModelSnapshot) {
        let t = Tolerance::default();
        let stock = extrude_rectangle(120., 80., 12., t).unwrap().model;
        let stock = cut_obround_slots(
            &stock,
            &[ObroundSlot {
                id: "mount".into(),
                center: [20., 15.],
                direction: [0., 1.],
                centerline_length: 8.,
                radius: 3.,
            }],
            t,
        )
        .unwrap()
        .model;
        let f = blind_obround_slot_frame(&stock, t).unwrap();
        let d = sub([40., 40., 0.], f.origin);
        let first = cut_blind_obround_slots(
            &stock,
            &[ObroundSlot {
                id: "first".into(),
                center: [dot(d, f.x_direction), dot(d, f.y_direction)],
                direction: [f.x_direction[1], f.y_direction[1]],
                centerline_length: 20.,
                radius: 5.,
            }],
            FaceId(cap),
            3.,
            t,
        )
        .unwrap();
        let floor = first.history.generated_faces["slot_first_floor"][0];
        let f = blind_obround_slot_frame(&first.model, t).unwrap();
        let d = sub([80., 40., 0.], f.origin);
        let second = cut_blind_obround_slots(
            &first.model,
            &[ObroundSlot {
                id: "second".into(),
                center: [dot(d, f.x_direction), dot(d, f.y_direction)],
                direction: [f.x_direction[1], f.y_direction[1]],
                centerline_length: 22.,
                radius: 6.,
            }],
            FaceId(cap),
            7.,
            t,
        )
        .unwrap();
        let floor = second.history.input_faces["body"][&floor][0];
        let pad = capsule(4.8, 20., 5., t);
        let pad = transform(
            &pad,
            [40., 40., if cap == 1 { 9. } else { 3. }],
            if cap == 1 {
                [0., 0., 0., 1.]
            } else {
                [1., 0., 0., 0.]
            },
            t,
        )
        .unwrap()
        .model;
        (second.model, floor, pad)
    }
    pub(super) fn surface_residual(m: &ModelSnapshot, f: FaceId, p: [f64; 3]) -> f64 {
        match &m.faces[f.0].surface {
            Surface::Plane(v) => dot(sub(p, v.origin), v.normal).abs(),
            Surface::Cylinder(v) => {
                let d = sub(p, v.origin);
                (norm(sub(d, mul(v.axis, dot(d, v.axis)))) - v.radius).abs()
            }
            _ => panic!("unsupported witness surface"),
        }
    }
    #[test]
    fn both_caps_serial_floor_and_real_wall_witnesses_survive_common_pose() {
        let t = Tolerance::default();
        for cap in [0, 1] {
            for posed in [false, true] {
                let (mut plate, floor, mut pad) = fixture(cap);
                if posed {
                    let q = [0.5, 0.5, 0., std::f64::consts::FRAC_1_SQRT_2];
                    plate = transform(&plate, [20., -7., 4.], q, t).unwrap().model;
                    pad = transform(&pad, [20., -7., 4.], q, t).unwrap().model;
                }
                let report = certify_capsule_pocket_fit(&plate, floor, &pad, t).unwrap();
                assert!(
                    report.side_clearance_lower_mm < 0.2 && report.side_clearance_upper_mm > 0.2
                );
                assert!(report.axial_overlap_lower_mm < 3. && report.axial_overlap_upper_mm > 3.);
                assert!(
                    report.signed_floor_gap_lower_mm < 0. && report.signed_floor_gap_upper_mm > 0.
                );
                assert_eq!(report.insert_floor_face, FaceId(0));
                assert!(
                    surface_residual(&plate, report.pocket_witness_face, report.pocket_point)
                        < 1e-9
                );
                assert!(
                    surface_residual(&pad, report.insert_witness_face, report.insert_point) < 1e-9
                );
                assert!(report
                    .pocket_wall_faces
                    .contains(&report.pocket_witness_face));
            }
        }
    }
    #[test]
    fn endpoint_containment_handles_offsets_rotation_and_refuses_penetration() {
        let t = Tolerance::default();
        let (plate, floor, _) = fixture(1);
        for angle in [0., 0.005, -0.005] {
            let raw = capsule(4., 16., 5., t);
            let q = [0., 0., (angle / 2f64).sin(), (angle / 2f64).cos()];
            let pad = transform(&raw, [40.1, 40., 9.], q, t).unwrap().model;
            let r = certify_capsule_pocket_fit(&plate, floor, &pad, t).unwrap();
            let expected = 1. - (0.1 + 8. * angle.sin().abs());
            assert!(r.side_clearance_lower_mm < expected && r.side_clearance_upper_mm > expected);
        }
        for (radius, x, z, h) in [
            (5.1, 40., 9., 5.),
            (4.8, 40.3, 9., 5.),
            (4.8, 40., 8.99, 5.),
            (4.8, 40., 12., 5.),
        ] {
            let pad = transform(
                &capsule(radius, 20., h, t),
                [x, 40., z],
                [0., 0., 0., 1.],
                t,
            )
            .unwrap()
            .model;
            assert!(certify_capsule_pocket_fit(&plate, floor, &pad, t).is_err());
        }
        let pad = transform(
            &capsule(4.8, 20., 5., t),
            [40., 40., 9.],
            [0., 0., 0., 1.],
            t,
        )
        .unwrap()
        .model;
        let mut bad_plate = plate.clone();
        bad_plate.faces[floor.0].surface = Surface::Plane(Plane {
            origin: [40., 40., 9.001],
            normal: [0., 0., 1.],
        });
        assert!(certify_capsule_pocket_fit(&bad_plate, floor, &pad, t).is_err());
        assert!(certify_capsule_pocket_fit(&plate, FaceId(1), &pad, t).is_err());
        let rectangle = extrude_rectangle(1., 1., 1., t).unwrap().model;
        assert!(certify_capsule_pocket_fit(&plate, floor, &rectangle, t).is_err());
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    #[test]
    fn semicircular_witness_reversed_insert_and_raw_corruption() {
        let t = Tolerance::default();
        let (plate, floor, pad) = super::tests::fixture(1);
        let shifted = transform(&pad, [0., 0.1, 0.], [0., 0., 0., 1.], t)
            .unwrap()
            .model;
        let r = certify_capsule_pocket_fit(&plate, floor, &shifted, t).unwrap();
        assert!(r.side_clearance_lower_mm < 0.1 && r.side_clearance_upper_mm > 0.1);
        assert!(matches!(
            plate.faces[r.pocket_witness_face.0].surface,
            Surface::Cylinder(_)
        ));
        assert!(
            super::tests::surface_residual(&plate, r.pocket_witness_face, r.pocket_point) < 1e-10
        );
        assert!(
            super::tests::surface_residual(&shifted, r.insert_witness_face, r.insert_point) < 1e-10
        );
        let raw = super::tests::capsule(4.8, 20., 5., t);
        let reversed = transform(&raw, [40., 40., 14.], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        assert_eq!(
            certify_capsule_pocket_fit(&plate, floor, &reversed, t)
                .unwrap()
                .insert_floor_face,
            FaceId(1)
        );
        for model_is_pocket in [false, true] {
            let mut m = if model_is_pocket {
                plate.clone()
            } else {
                pad.clone()
            };
            let f = if model_is_pocket {
                r.pocket_witness_face
            } else {
                r.insert_witness_face
            };
            let Surface::Cylinder(c) = &mut m.faces[f.0].surface else {
                panic!("cylinder expected")
            };
            c.axis[0] += 1e-13;
            let result = if model_is_pocket {
                certify_capsule_pocket_fit(&m, floor, &pad, t)
            } else {
                certify_capsule_pocket_fit(&plate, floor, &m, t)
            };
            assert!(result.is_err());
        }
        let tilted = transform(&pad, [0.; 3], [1e-5, 0., 0., (1. - 1e-10_f64).sqrt()], t)
            .unwrap()
            .model;
        assert!(certify_capsule_pocket_fit(&plate, floor, &tilted, t).is_err());
        let strict = Tolerance { linear: 1e-11 };
        assert!(certify_capsule_pocket_fit(&plate, floor, &pad, strict).is_err());
    }
}
