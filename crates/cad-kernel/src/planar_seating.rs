//! A local nominal seating witness on two opposed, analytic planar faces.
//! This certifies a disk in both trimmed face interiors and arithmetic-level
//! coincidence of their supports. It does not certify global body separation,
//! interference, contact force, preload, or manufacturing tolerance.
use crate::math::{cross, dot, norm, sub};
use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarSeating {
    pub left_face: FaceId,
    pub right_face: FaceId,
    /// Signed along the left face's outward normal. Both endpoints enclose zero.
    pub signed_gap_lower_mm: f64,
    pub signed_gap_upper_mm: f64,
    /// Euclidean norm of the sum of opposed unit normals, including arithmetic guard.
    pub normal_opposition_bound: f64,
    /// Center on the left native support plane; the right support is within the gap interval.
    pub disk_center: [f64; 3],
    pub disk_radius_lower_mm: f64,
    pub disk_area_lower_mm2: f64,
}

enum Trim {
    Annulus {
        center: [f64; 3],
        axis: [f64; 3],
        x: [f64; 3],
        inner: f64,
        outer: f64,
    },
    Polygon {
        origin: [f64; 3],
        axis: [f64; 3],
        x: [f64; 3],
        points: Vec<[f64; 2]>,
        holes: Vec<CircleHole>,
    },
}

struct SeatFace {
    origin: [f64; 3],
    normal: [f64; 3],
    trim: Trim,
}

fn unsupported() -> KernelError {
    KernelError::Unsupported(
        "planar seating requires opposed canonical revolution annuli or convex polygon through-bore caps with a shared positive-area material disk".into(),
    )
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|x| x * s)
}

fn admit(model: &ModelSnapshot, id: FaceId, t: Tolerance) -> Result<SeatFace, KernelError> {
    let plane = model
        .faces
        .get(id.0)
        .and_then(Face::try_plane)
        .ok_or_else(unsupported)?;
    let trim = if let Ok(g) = crate::revolve::recognize(model, t) {
        let outer = g.profile.iter().map(|p| p[0]).fold(0., f64::max);
        let height = g.profile.iter().map(|p| p[1].abs()).fold(0., f64::max);
        let raw_error = (4096. * f64::EPSILON * (norm(g.origin) + outer + height)).next_up();
        if !raw_error.is_normal() || raw_error > t.linear / 64. {
            return Err(KernelError::NumericRange);
        }
        crate::convex_clearance::support::revolution_consistency(model, &g, raw_error)?;
        let segment = g
            .segments
            .iter()
            .find(|s| s.face == id)
            .ok_or_else(unsupported)?;
        if segment.start[1] != segment.end[1] {
            return Err(unsupported());
        }
        let inner = segment.start[0].min(segment.end[0]);
        let outer = segment.start[0].max(segment.end[0]);
        if outer - inner <= t.linear || inner < 0. {
            return Err(unsupported());
        }
        Trim::Annulus {
            center: add(g.origin, mul(g.axis, segment.start[1])),
            axis: g.axis,
            x: g.x_direction,
            inner,
            outer,
        }
    } else {
        let g = crate::convex_trim_clearance::admit(model, t)?;
        if id != FaceId(0) && id != FaceId(1) {
            return Err(unsupported());
        }
        if g.path
            .segments
            .iter()
            .any(|s| !matches!(s, PathSegment::LineTo { .. }))
        {
            return Err(unsupported());
        }
        let mut points = vec![g.path.start];
        for segment in g.path.segments.iter().take(g.path.segments.len() - 1) {
            let PathSegment::LineTo { end } = segment else {
                return Err(unsupported());
            };
            points.push(*end);
        }
        Trim::Polygon {
            origin: g.origin,
            axis: g.axis,
            x: g.x_direction,
            points,
            holes: g.circular_holes,
        }
    };
    Ok(SeatFace {
        origin: plane.origin,
        normal: plane.normal,
        trim,
    })
}

fn margin(face: &SeatFace, point: [f64; 3]) -> f64 {
    match &face.trim {
        Trim::Annulus {
            center,
            axis,
            inner,
            outer,
            ..
        } => {
            let d = sub(point, *center);
            let radial = norm(sub(d, mul(*axis, dot(d, *axis))));
            if *inner == 0. {
                outer - radial
            } else {
                (radial - inner).min(outer - radial)
            }
        }
        Trim::Polygon {
            origin,
            axis,
            x,
            points,
            holes,
        } => {
            let d = sub(point, *origin);
            let y = cross(*axis, *x);
            let p = [dot(d, *x), dot(d, y)];
            if !crate::simple_polygon::inside(p, points) {
                return f64::NEG_INFINITY;
            }
            let outer = (0..points.len())
                .map(|i| {
                    crate::simple_polygon::distance(p, points[i], points[(i + 1) % points.len()])
                })
                .fold(f64::INFINITY, f64::min);
            holes
                .iter()
                .map(|h| (p[0] - h.center[0]).hypot(p[1] - h.center[1]) - h.radius)
                .fold(outer, f64::min)
        }
    }
}

fn candidates(face: &SeatFace) -> Vec<[f64; 3]> {
    let Trim::Annulus {
        center,
        axis,
        x,
        inner,
        outer,
    } = &face.trim
    else {
        return Vec::new();
    };
    let y = cross(*axis, *x);
    let mut points = Vec::with_capacity(161);
    if *inner == 0. {
        points.push(*center);
    }
    for fraction in [0.25, 0.5, 0.75, 0.875, 0.9] {
        let r = inner + (outer - inner) * fraction;
        for i in 0..32 {
            let angle = std::f64::consts::TAU * f64::from(i) / 32.;
            points.push(add(
                *center,
                add(mul(*x, r * angle.cos()), mul(y, r * angle.sin())),
            ));
        }
    }
    points
}

/// Certify local nominal seating from two already placed native snapshots.
/// Both selected faces must be opposed planar boundaries. At least one must be
/// a planar annulus of a full-turn linear-meridian revolution; the other may
/// also be such an annulus or a convex polygon prism cap with circular through
/// bores. The protected disk lies strictly in both trimmed face interiors.
pub fn certify_planar_seating(
    left: &ModelSnapshot,
    left_face: FaceId,
    right: &ModelSnapshot,
    right_face: FaceId,
    min_disk_radius_mm: f64,
    tolerance: Tolerance,
) -> Result<PlanarSeating, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !min_disk_radius_mm.is_finite() || min_disk_radius_mm <= tolerance.linear {
        return Err(KernelError::InvalidDimension {
            dimension: "minimum seating disk radius",
        });
    }
    let a = admit(left, left_face, tolerance)?;
    let b = admit(right, right_face, tolerance)?;
    let scale = left
        .vertices
        .iter()
        .chain(&right.vertices)
        .map(|v| norm(v.position))
        .fold(norm(a.origin).max(norm(b.origin)), f64::max);
    let extent = left
        .bounds()
        .max
        .into_iter()
        .chain(left.bounds().min)
        .chain(right.bounds().max)
        .chain(right.bounds().min)
        .map(f64::abs)
        .fold(1., f64::max);
    let arithmetic = ((16384. * scale + 16384. * extent) * f64::EPSILON).next_up();
    if !arithmetic.is_normal() || arithmetic > tolerance.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let opposition = norm(add(a.normal, b.normal));
    if !opposition.is_finite()
        || opposition > 128. * f64::EPSILON
        || opposition * extent > arithmetic
    {
        return Err(unsupported());
    }
    let signed = dot(sub(b.origin, a.origin), a.normal);
    let gap_guard = (2. * arithmetic + opposition * extent).next_up();
    if !signed.is_finite() || signed.abs() > gap_guard {
        return Err(unsupported());
    }
    let lower = (signed - gap_guard).next_down();
    let upper = (signed + gap_guard).next_up();
    if lower > 0. || upper < 0. || upper - lower > tolerance.linear / 4. {
        return Err(KernelError::NumericRange);
    }
    let mut best = None;
    for point in candidates(&a).into_iter().chain(candidates(&b)) {
        let center = sub(point, mul(a.normal, dot(sub(point, a.origin), a.normal)));
        let radius = (margin(&a, center).min(margin(&b, center)) - 4. * arithmetic).next_down();
        if radius.is_finite()
            && radius >= min_disk_radius_mm
            && best.as_ref().is_none_or(|(_, r)| radius > *r)
        {
            best = Some((center, radius));
        }
    }
    let (disk_center, disk_radius_lower_mm) = best.ok_or_else(unsupported)?;
    let squared = (disk_radius_lower_mm * disk_radius_lower_mm).next_down();
    let disk_area_lower_mm2 = (std::f64::consts::PI.next_down() * squared).next_down();
    if !disk_area_lower_mm2.is_normal() {
        return Err(KernelError::NumericRange);
    }
    Ok(PlanarSeating {
        left_face,
        right_face,
        signed_gap_lower_mm: lower,
        signed_gap_upper_mm: upper,
        normal_opposition_bound: (opposition + 128. * f64::EPSILON).next_up(),
        disk_center,
        disk_radius_lower_mm,
        disk_area_lower_mm2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover(t: Tolerance) -> ModelSnapshot {
        extrude_polygon_with_circular_holes(
            &[
                [-22., -26.],
                [22., -26.],
                [22., 14.],
                [14., 22.],
                [-14., 22.],
                [-22., 14.],
            ],
            &[],
            &[
                CircleHole {
                    center: [0., 0.],
                    radius: 10.5,
                },
                CircleHole {
                    center: [-17., 0.],
                    radius: 2.05,
                },
                CircleHole {
                    center: [17., 0.],
                    radius: 2.05,
                },
            ],
            2.,
            t,
        )
        .unwrap()
        .model
    }

    #[test]
    fn cover_flange_and_screw_underhead_have_native_disks() {
        let t = Tolerance::default();
        let cover = cover(t);
        let sleeve = revolve_polygon(
            &[
                [6.15, 0.],
                [12., 0.],
                [12., 2.],
                [10.5, 2.],
                [10.5, 18.],
                [6.15, 18.],
            ],
            t,
        )
        .unwrap();
        let flange_face = sleeve.history.generated_faces["segment_0"][0];
        let placed_sleeve = translate(&sleeve.model, [0., 0., 2.], t).unwrap().model;
        let flange =
            certify_planar_seating(&cover, FaceId(1), &placed_sleeve, flange_face, 0.2, t).unwrap();
        assert!(flange.disk_radius_lower_mm > 0.2);
        assert!(flange.disk_area_lower_mm2 > 0.);
        assert!(flange.signed_gap_lower_mm < 0. && flange.signed_gap_upper_mm > 0.);
        let reversed =
            certify_planar_seating(&placed_sleeve, flange_face, &cover, FaceId(1), 0.2, t).unwrap();
        assert!(reversed.disk_radius_lower_mm > 0.2);
        let oblique = [0.2, 0.3, 0.4, 0.71_f64.sqrt()];
        let world_cover = transform(&cover, [41., -27., 12.], oblique, t)
            .unwrap()
            .model;
        let world_sleeve = transform(&placed_sleeve, [41., -27., 12.], oblique, t)
            .unwrap()
            .model;
        assert!(certify_planar_seating(
            &world_cover,
            FaceId(1),
            &world_sleeve,
            flange_face,
            0.2,
            t
        )
        .is_ok());
        let right_sleeve = transform(&sleeve.model, [0.; 3], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        assert!(
            certify_planar_seating(&cover, FaceId(0), &right_sleeve, flange_face, 0.2, t).is_ok()
        );

        let screw = revolve_polygon(
            &[
                [0., 0.],
                [1.65, 0.],
                [1.65, 9.],
                [3., 9.],
                [3., 11.],
                [0., 11.],
            ],
            t,
        )
        .unwrap();
        let underhead = screw.history.generated_faces["segment_2"][0];
        let screw = transform(&screw.model, [17., 0., 9.], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        let seat = certify_planar_seating(&cover, FaceId(0), &screw, underhead, 0.2, t).unwrap();
        assert!(seat.disk_radius_lower_mm > 0.2);
        assert!((seat.disk_center[0] - 17.).abs() < 4.);
        let right_screw = translate(
            &revolve_polygon(
                &[
                    [0., 0.],
                    [1.65, 0.],
                    [1.65, 9.],
                    [3., 9.],
                    [3., 11.],
                    [0., 11.],
                ],
                t,
            )
            .unwrap()
            .model,
            [17., 0., -7.],
            t,
        )
        .unwrap()
        .model;
        assert!(certify_planar_seating(&cover, FaceId(1), &right_screw, underhead, 0.2, t).is_ok());
    }

    #[test]
    fn rejects_open_gap_tilt_and_corrupt_native_support() {
        let t = Tolerance::default();
        let cover = cover(t);
        let sleeve = revolve_polygon(
            &[
                [6.15, 0.],
                [12., 0.],
                [12., 2.],
                [10.5, 2.],
                [10.5, 18.],
                [6.15, 18.],
            ],
            t,
        )
        .unwrap();
        let face = sleeve.history.generated_faces["segment_0"][0];
        let placed = translate(&sleeve.model, [0., 0., 2.], t).unwrap().model;
        assert!(certify_planar_seating(&cover, FaceId(1), &placed, face, 0.2, t).is_ok());
        let gap = translate(&placed, [0., 0., 0.01], t).unwrap().model;
        assert!(certify_planar_seating(&cover, FaceId(1), &gap, face, 0.2, t).is_err());
        let tilted = transform(
            &placed,
            [0.; 3],
            [0.001_f64.sin(), 0., 0., 0.001_f64.cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(certify_planar_seating(&cover, FaceId(1), &tilted, face, 0.2, t).is_err());
        let mut malformed = placed.clone();
        if let Surface::Plane(plane) = &mut malformed.faces[face.0].surface {
            plane.origin[2] += 1e-4;
        }
        assert!(certify_planar_seating(&cover, FaceId(1), &malformed, face, 0.2, t).is_err());
    }

    #[test]
    fn axis_closed_disk_uses_center_witness() {
        let t = Tolerance::default();
        let disk = revolve_polygon(&[[0., 0.], [4., 0.], [4., 2.], [0., 2.]], t).unwrap();
        let face = disk.history.generated_faces["segment_0"][0];
        let opposed = transform(&disk.model, [0.; 3], [1., 0., 0., 0.], t)
            .unwrap()
            .model;
        let witness = certify_planar_seating(&disk.model, face, &opposed, face, 0.2, t).unwrap();
        assert!(witness.disk_radius_lower_mm > 3.9);
        assert!(norm(witness.disk_center) < 1e-9);
    }
}
