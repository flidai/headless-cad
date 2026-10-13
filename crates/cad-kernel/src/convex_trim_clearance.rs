//! Clearance from convex outer envelopes with certified surviving witnesses.
//! Only circular through holes of canonical convex line/arc prisms are admitted.
use crate::convex_clearance::support;
use crate::math::{cross, dot, norm, sub};
use crate::*;

fn unsupported() -> KernelError {
    KernelError::Unsupported("convex trim witness requires separated convex outer envelopes and witnesses certified outside every circular through hole".into())
}

pub(crate) struct HoledEnvelope {
    outer: ModelSnapshot,
    pub(crate) path: PlanarPath,
    pub(crate) circular_holes: Vec<CircleHole>,
    pub(crate) bore_faces: Vec<FaceId>,
    pub(crate) origin: [f64; 3],
    pub(crate) axis: [f64; 3],
    pub(crate) x_direction: [f64; 3],
    pub(crate) height: f64,
    geometry: HoledGeometry,
    hole_error: f64,
}

enum HoledGeometry {
    Arc(ArcPrismGeometry),
    Polygon(ProfilePrismGeometry),
}

impl HoledEnvelope {
    pub(crate) fn support_consistency(
        &self,
        model: &ModelSnapshot,
        error: f64,
        tolerance: Tolerance,
    ) -> Result<(), KernelError> {
        match &self.geometry {
            HoledGeometry::Arc(g) => support::arc_consistency(model, g, error, tolerance),
            HoledGeometry::Polygon(g) => polygon_consistency(model, g, error),
        }
    }

    pub(crate) fn boundary_inradius(
        &self,
        center: [f64; 2],
        tolerance: Tolerance,
        error: f64,
    ) -> Result<f64, KernelError> {
        match &self.geometry {
            HoledGeometry::Arc(_) => {
                crate::arc_prism::path_boundary_distance(&self.path, center, tolerance, error)
            }
            HoledGeometry::Polygon(_) => {
                let mut points = vec![self.path.start];
                for segment in &self.path.segments[..self.path.segments.len() - 1] {
                    let PathSegment::LineTo { end } = segment else {
                        return Err(unsupported());
                    };
                    points.push(*end);
                }
                if !crate::simple_polygon::inside(center, &points) {
                    return Err(unsupported());
                }
                let distance = (0..points.len())
                    .map(|i| {
                        crate::simple_polygon::distance(
                            center,
                            points[i],
                            points[(i + 1) % points.len()],
                        )
                    })
                    .fold(f64::INFINITY, f64::min);
                Ok((distance - 4. * error).max(0.))
            }
        }
    }
}

// The stripped polygon is the exact prefix of the raw body, not an envelope
// inferred by fitting. Verify that every native plane and line stays within
// the arithmetic guard of that prefix's vertices. The profile recognizer
// separately proves canonical circle incidence and exact through topology;
// these checks bind its reconstructed bore axes, ends and radii to raw walls
// and edges without using modeling tolerance to move a support.
fn polygon_consistency(
    model: &ModelSnapshot,
    geometry: &ProfilePrismGeometry,
    error: f64,
) -> Result<(), KernelError> {
    let base = &geometry.base;
    let direction = |a: [f64; 3], b: [f64; 3], extent: f64| {
        norm(sub(a, b)) <= 128. * f64::EPSILON && norm(sub(a, b)) * extent <= error
    };
    for face in &base.faces {
        let plane = face.try_plane().ok_or_else(unsupported)?;
        if (norm(plane.normal) - 1.).abs() > 64. * f64::EPSILON {
            return Err(KernelError::NumericRange);
        }
        for ring in &face.loops {
            for point in base.loop_points(*ring).ok_or_else(unsupported)? {
                if dot(sub(point, plane.origin), plane.normal).abs() > error {
                    return Err(KernelError::NumericRange);
                }
            }
        }
    }
    for edge in &base.edges {
        let line = edge.try_line().ok_or_else(unsupported)?;
        let a = base.vertices[edge.vertices[0].0].position;
        let b = base.vertices[edge.vertices[1].0].position;
        if norm(sub(line.origin, a)) > error
            || (norm(line.direction) - 1.).abs() > 64. * f64::EPSILON
            || norm(cross(sub(b, a), line.direction)) > error
        {
            return Err(KernelError::NumericRange);
        }
    }
    let extent = base
        .vertices
        .iter()
        .map(|vertex| norm(sub(vertex.position, geometry.origin)))
        .fold(geometry.height, f64::max);
    for (side, expected_normal) in [(0, geometry.axis.map(|value| -value)), (1, geometry.axis)] {
        let cap = base.faces[side].try_plane().ok_or_else(unsupported)?;
        if !direction(cap.normal, expected_normal, extent) {
            return Err(KernelError::NumericRange);
        }
    }
    for (i, bore) in geometry.bores.iter().enumerate() {
        let f = base.faces.len() + i;
        if bore.wall != FaceId(f) {
            return Err(unsupported());
        }
        let Surface::Cylinder(wall) = &model.faces[f].surface else {
            return Err(unsupported());
        };
        if norm(sub(wall.origin, bore.origin)) > error
            || (wall.radius - bore.radius).abs() > error
            || !direction(wall.axis, bore.axis, bore.height)
            || !direction(wall.x_direction, bore.x_direction, bore.radius)
        {
            return Err(KernelError::NumericRange);
        }
        let lower = bore.origin;
        let upper = std::array::from_fn(|j| bore.origin[j] + bore.height * bore.axis[j]);
        if (bore.height - geometry.height).abs() > error
            || !direction(bore.axis, geometry.axis, extent)
        {
            return Err(KernelError::NumericRange);
        }
        for (side, center) in [(0, lower), (1, upper)] {
            let cap = base.faces[side].try_plane().ok_or_else(unsupported)?;
            if dot(sub(center, cap.origin), cap.normal).abs()
                + bore.radius * norm(cross(cap.normal, bore.axis))
                > error
            {
                return Err(KernelError::NumericRange);
            }
        }
        let rim = bore.x_direction.map(|value| value * bore.radius);
        for (vertex, center) in [
            (&model.vertices[base.vertices.len() + 2 * i], lower),
            (&model.vertices[base.vertices.len() + 2 * i + 1], upper),
        ] {
            let expected = std::array::from_fn(|j| center[j] + rim[j]);
            if norm(sub(vertex.position, expected)) > error {
                return Err(KernelError::NumericRange);
            }
        }
        for (edge, expected) in [
            (&model.edges[base.edges.len() + 3 * i], lower),
            (&model.edges[base.edges.len() + 3 * i + 1], upper),
        ] {
            let Curve::Circle(circle) = &edge.curve else {
                return Err(unsupported());
            };
            if norm(sub(circle.origin, expected)) > error
                || (circle.radius - bore.radius).abs() > error
                || !direction(circle.axis, bore.axis, bore.height)
                || !direction(circle.x_direction, bore.x_direction, bore.radius)
            {
                return Err(KernelError::NumericRange);
            }
        }
        let seam = &model.edges[base.edges.len() + 3 * i + 2];
        let line = seam.try_line().ok_or_else(unsupported)?;
        let a = model.vertices[seam.vertices[0].0].position;
        let b = model.vertices[seam.vertices[1].0].position;
        if norm(sub(line.origin, a)) > error
            || (norm(line.direction) - 1.).abs() > 64. * f64::EPSILON
            || norm(cross(sub(b, a), line.direction)) > error
        {
            return Err(KernelError::NumericRange);
        }
    }
    Ok(())
}

// ArcPrism::construct appends each hole after all outer primitives. Its
// recognizer proves this indexing and the canonical cap/wall incidence before
// slicing. Keeping the raw outer primitives avoids reprojection of supports.
pub(crate) fn admit(m: &ModelSnapshot, t: Tolerance) -> Result<HoledEnvelope, KernelError> {
    if !t.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if m.vertices.len() > MAX_CLEARANCE_VERTICES
        || m.edges.len() > MAX_CLEARANCE_EDGES
        || m.faces.len() > MAX_CLEARANCE_FACES
        || m.loops.len() > 2 * MAX_CLEARANCE_FACES
        || m.loops
            .iter()
            .fold(0usize, |n, l| n.saturating_add(l.edges.len()))
            > 2 * MAX_CLEARANCE_EDGES
        || m.faces
            .iter()
            .any(|f| f.loops.len() > MAX_POLYGON_HOLES + 1)
        || m.solids.len() != 1
        || m.shells.len() != 1
    {
        return Err(unsupported());
    }
    let report = m.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    if m.edges
        .iter()
        .any(|edge| matches!(edge.curve, Curve::CircularArc(_)))
    {
        admit_arc(m, t)
    } else {
        admit_polygon(m, t)
    }
}

fn admit_arc(m: &ModelSnapshot, t: Tolerance) -> Result<HoledEnvelope, KernelError> {
    let geometry = crate::arc_prism::recognize(m, t)?;
    if geometry.circular_holes.is_empty()
        || !geometry
            .path
            .segments
            .iter()
            .any(|s| matches!(s, PathSegment::ArcTo { .. }))
    {
        return Err(unsupported());
    }
    let n = geometry.path.segments.len();
    let holes = geometry.circular_holes.len();
    if m.vertices.len() != 2 * n + 2 * holes
        || m.edges.len() != 3 * n + 3 * holes
        || m.loops.len() != n + 2 + 3 * holes
        || m.faces.len() != n + 2 + holes
    {
        return Err(unsupported());
    }
    let mut outer = m.clone();
    outer.vertices.truncate(2 * n);
    outer.edges.truncate(3 * n);
    outer.loops.truncate(n + 2);
    outer.faces.truncate(n + 2);
    outer.faces[0].loops.truncate(1);
    outer.faces[1].loops.truncate(1);
    outer.shells[0].faces.truncate(n + 2);
    outer.validate_convex_clearance_operand(t)?;

    // The recognized hole centers and radii are projections of native circles.
    // This separate roundoff allowance covers their reconciliation with all
    // stored circle edges/walls and their world-frame projection below.
    let extent = geometry
        .path
        .segments
        .iter()
        .map(|s| match s {
            PathSegment::LineTo { end } => end[0].hypot(end[1]),
            PathSegment::ArcTo { center, end, .. } => {
                center[0].hypot(center[1]) + norm2(sub2(*end, *center))
            }
        })
        .fold(geometry.height, f64::max);
    let world = m
        .vertices
        .iter()
        .map(|v| norm(v.position))
        .fold(norm(geometry.origin), f64::max);
    let hole_error = ((32768. * world + 65536. * extent) * f64::EPSILON).next_up();
    if !hole_error.is_normal() || hole_error > t.linear / 2. {
        return Err(KernelError::NumericRange);
    }
    let admitted = HoledEnvelope {
        outer,
        path: geometry.path.clone(),
        circular_holes: geometry.circular_holes.clone(),
        bore_faces: (0..holes).map(|i| FaceId(n + 2 + i)).collect(),
        origin: geometry.origin,
        axis: geometry.axis,
        x_direction: geometry.x_direction,
        height: geometry.height,
        geometry: HoledGeometry::Arc(geometry),
        hole_error,
    };
    admitted.support_consistency(m, hole_error, t)?;
    Ok(admitted)
}

fn admit_polygon(m: &ModelSnapshot, t: Tolerance) -> Result<HoledEnvelope, KernelError> {
    let geometry = crate::profile_prism::recognize(m, t)?;
    if geometry.bores.is_empty()
        || geometry.base.faces[0].loops.len() != 1
        || geometry.base.faces[1].loops.len() != 1
    {
        return Err(unsupported());
    }
    geometry.base.validate_convex_clearance_operand(t)?;
    let vertices = geometry
        .base
        .loop_points(geometry.base.faces[1].loops[0])
        .ok_or_else(unsupported)?;
    if vertices.len() < 3 {
        return Err(unsupported());
    }
    let y = cross(geometry.axis, geometry.x_direction);
    let points = vertices
        .into_iter()
        .map(|p| {
            let d = sub(p, geometry.origin);
            [dot(d, geometry.x_direction), dot(d, y)]
        })
        .collect::<Vec<_>>();
    let path = PlanarPath {
        start: points[0],
        segments: points[1..]
            .iter()
            .chain(std::iter::once(&points[0]))
            .map(|end| PathSegment::LineTo { end: *end })
            .collect(),
    };
    let circular_holes = geometry
        .bores
        .iter()
        .map(|bore| {
            let d = sub(bore.origin, geometry.origin);
            CircleHole {
                center: [dot(d, geometry.x_direction), dot(d, y)],
                radius: bore.radius,
            }
        })
        .collect::<Vec<_>>();
    let extent = points
        .iter()
        .map(|point| point[0].hypot(point[1]))
        .fold(geometry.height, f64::max);
    let world = m
        .vertices
        .iter()
        .map(|vertex| norm(vertex.position))
        .fold(norm(geometry.origin), f64::max);
    let hole_error = ((32768. * world + 65536. * extent) * f64::EPSILON).next_up();
    if !hole_error.is_normal() || hole_error > t.linear / 2. {
        return Err(KernelError::NumericRange);
    }
    let admitted = HoledEnvelope {
        outer: geometry.base.clone(),
        path,
        circular_holes,
        bore_faces: geometry.bores.iter().map(|bore| bore.wall).collect(),
        origin: geometry.origin,
        axis: geometry.axis,
        x_direction: geometry.x_direction,
        height: geometry.height,
        geometry: HoledGeometry::Polygon(geometry),
        hole_error,
    };
    admitted.support_consistency(m, hole_error, t)?;
    Ok(admitted)
}

fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn norm2(v: [f64; 2]) -> f64 {
    v[0].hypot(v[1])
}

pub(crate) fn is_candidate(m: &ModelSnapshot, t: Tolerance) -> bool {
    m.faces.first().is_some_and(|f| f.loops.len() > 1) && admit(m, t).is_ok()
}

fn witness_survives(h: &HoledEnvelope, point: [f64; 3], guard: f64) -> bool {
    let delta = sub(point, h.origin);
    let y = cross(h.axis, h.x_direction);
    let projected = [dot(delta, h.x_direction), dot(delta, y)];
    // `arc_consistency` independently permits one hole_error of center,
    // radius, and supporting-axis drift. The transformed bore rim can move by
    // their sum; reserve another allowance for witness projection and cap
    // reconstruction. This is deliberately separate from modeling tolerance.
    let margin = (guard + 4. * h.hole_error).next_up();
    projected.iter().all(|v| v.is_finite())
        && h.circular_holes.iter().all(|hole| {
            let separation = norm2(sub2(projected, hole.center));
            separation.is_finite() && separation > hole.radius + margin
        })
}

pub(crate) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let l = admit(a, t).ok();
    let r = admit(b, t).ok();
    if l.is_none() && r.is_none() {
        return Err(unsupported());
    }
    let left = l.as_ref().map_or(a, |e| &e.outer);
    let right = r.as_ref().map_or(b, |e| &e.outer);
    let certificate = support::query_certificate(left, right, t)?;
    let q = certificate.query;
    // The support solver already charges witness displacement from its outer
    // material (including permitted tiny reflex joins). Add its arithmetic
    // error again for the projection into each hole frame; hole_error covers
    // the native hole recognition and world-coordinate reconstruction.
    let guard = (certificate.witness_padding + 8. * certificate.arithmetic_error).next_up();
    if !guard.is_finite()
        || l.as_ref()
            .is_some_and(|h| !witness_survives(h, q.left_point, guard))
        || r.as_ref()
            .is_some_and(|h| !witness_survives(h, q.right_point, guard))
    {
        return Err(unsupported());
    }
    Ok(SolidClearance {
        method: ClearanceMethod::ConvexTrimWitness,
        lower_bound_mm: q.lower_bound_mm,
        upper_bound_mm: q.upper_bound_mm,
        left_point: q.left_point,
        right_point: q.right_point,
        axis: q.axis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotor(holes: &[CircleHole]) -> ModelSnapshot {
        let path = PlanarPath {
            start: [-8., -6.],
            segments: vec![
                PathSegment::LineTo { end: [8., -6.] },
                PathSegment::ArcTo {
                    center: [8., -4.],
                    end: [10., -4.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [10., 4.] },
                PathSegment::ArcTo {
                    center: [8., 4.],
                    end: [8., 6.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [-8., 6.] },
                PathSegment::ArcTo {
                    center: [-8., 4.],
                    end: [-10., 4.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [-10., -4.] },
                PathSegment::ArcTo {
                    center: [-8., -4.],
                    end: [-8., -6.],
                    clockwise: false,
                },
            ],
        };
        extrude_path(&path, holes, 6., Tolerance::default())
            .unwrap()
            .model
    }

    #[test]
    fn exterior_roller_certifies_bored_rotor_in_both_orders() {
        let t = Tolerance::default();
        let holes = [
            CircleHole {
                center: [0., 0.],
                radius: 2.,
            },
            CircleHole {
                center: [-5., 0.],
                radius: 1.,
            },
            CircleHole {
                center: [5., 0.],
                radius: 1.,
            },
        ];
        let rotor = rotor(&holes);
        assert!(admit(&rotor, t).is_ok(), "{:?}", admit(&rotor, t).err());
        let roller = translate(&extrude_circle(2., 6., t).unwrap().model, [18., 0., 0.], t)
            .unwrap()
            .model;
        for (a, b) in [(&rotor, &roller), (&roller, &rotor)] {
            let q = a.clearance(b, t).unwrap();
            assert_eq!(q.method, ClearanceMethod::ConvexTrimWitness);
            assert!(q.lower_bound_mm <= 6. && q.upper_bound_mm >= 6., "{q:?}");
        }
    }

    #[test]
    fn cut_generated_holes_retain_canonical_envelope() {
        let t = Tolerance::default();
        let mut body = rotor(&[]);
        for (x, radius) in [(0., 2.), (-5., 1.), (5., 1.)] {
            let tool = translate(
                &extrude_circle(radius, 8., t).unwrap().model,
                [x, 0., -1.],
                t,
            )
            .unwrap()
            .model;
            body = boolean(&body, &tool, BooleanOp::Difference, t)
                .unwrap()
                .model;
        }
        assert!(admit(&body, t).is_ok(), "{:?}", admit(&body, t).err());
    }

    #[test]
    fn rigid_pose_and_full_turn_preserve_external_witness() {
        let t = Tolerance::default();
        let rotor = rotor(&[CircleHole {
            center: [0., 0.],
            radius: 2.,
        }]);
        let roller = translate(&extrude_circle(2., 6., t).unwrap().model, [18., 0., 0.], t)
            .unwrap()
            .model;
        let q = [0.5, 0.5, 0.5, 0.5];
        let placed_rotor = transform(&rotor, [13., -7., 15.], q, t).unwrap().model;
        let placed_roller = transform(&roller, [13., -7., 15.], q, t).unwrap().model;
        for (a, b) in [
            (&placed_rotor, &placed_roller),
            (&placed_roller, &placed_rotor),
        ] {
            let result = a.clearance(b, t).unwrap();
            assert_eq!(result.method, ClearanceMethod::ConvexTrimWitness);
            assert!(result.lower_bound_mm <= 6. && result.upper_bound_mm >= 6.);
        }
        let report = roller
            .rotation_clearance(
                &rotor,
                [0., 0., 3.],
                [0., 0., 1.],
                std::f64::consts::TAU,
                5.,
                t,
                1025,
            )
            .unwrap();
        assert!(report.lower_bound_mm >= 5.);
    }

    #[test]
    fn two_bored_envelopes_and_world_translation() {
        let t = Tolerance::default();
        let left = rotor(&[CircleHole {
            center: [0., 0.],
            radius: 2.,
        }]);
        let right = translate(&left, [30., 0., 0.], t).unwrap().model;
        for offset in [[0., 0., 0.], [1_000., -2_000., 3_000.]] {
            let a = translate(&left, offset, t).unwrap().model;
            let b = translate(&right, offset, t).unwrap().model;
            assert!(
                admit(&a, t).is_ok(),
                "left {offset:?}: {:?}",
                admit(&a, t).err()
            );
            assert!(
                admit(&b, t).is_ok(),
                "right {offset:?}: {:?}",
                admit(&b, t).err()
            );
            let q = a.clearance(&b, t).unwrap();
            assert_eq!(q.method, ClearanceMethod::ConvexTrimWitness);
            assert!(q.lower_bound_mm <= 10. && q.upper_bound_mm >= 10., "{q:?}");
        }
    }

    #[test]
    fn overlap_near_hole_and_malformed_bore_refuse() {
        let t = Tolerance::default();
        let rotor = rotor(&[CircleHole {
            center: [0., 0.],
            radius: 2.,
        }]);
        let shaft = extrude_circle(1., 6., t).unwrap().model;
        assert!(query(&rotor, &shaft, t).is_err()); // Envelopes intersect.
        let above_bore = translate(&extrude_circle(0.5, 1., t).unwrap().model, [0., 0., 8.], t)
            .unwrap()
            .model;
        // Envelopes have an axial gap, but their nearest cap witness can land
        // in the bore. That envelope upper bound cannot certify material.
        assert!(query(&rotor, &above_bore, t).is_err());
        let h = admit(&rotor, t).unwrap();
        assert!(!witness_survives(&h, [0., 0., 3.], 0.));
        assert!(!witness_survives(
            &h,
            [2. + h.hole_error, 0., 3.],
            h.hole_error
        ));
        let mut damaged = rotor.clone();
        if let Surface::Cylinder(wall) = &mut damaged.faces.last_mut().unwrap().surface {
            wall.inward = false;
        }
        assert!(admit(&damaged, t).is_err());
        let mut damaged_loop = rotor.clone();
        damaged_loop.faces[0].loops.pop();
        assert!(admit(&damaged_loop, t).is_err());
    }

    #[test]
    fn independent_native_bore_drift_uses_combined_membership_margin() {
        let t = Tolerance::default();
        let mut body = rotor(&[CircleHole {
            center: [0., 0.],
            radius: 2.,
        }]);
        let n = 8;
        let drift = 1e-12;
        if let Curve::Circle(circle) = &mut body.edges[3 * n + 1].curve {
            circle.origin[0] += drift;
            circle.radius += drift;
        } else {
            panic!("expected upper bore circle");
        }
        if let Surface::Cylinder(wall) = &mut body.faces[n + 2].surface {
            wall.origin[0] += drift;
            wall.radius += drift;
        } else {
            panic!("expected bore wall");
        }
        let admitted = admit(&body, t).unwrap();
        let q = [2. + 2. * admitted.hole_error, 0., 3.];
        assert!(!witness_survives(&admitted, q, 0.));
    }
}
