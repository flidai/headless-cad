//! Bounded simple-ring predicates. No repair, snapping, or tolerance merging.
use crate::*;
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    let p = |v: [f64; 2]| robust::Coord { x: v[0], y: v[1] };
    robust::orient2d(p(a), p(b), p(c))
}
pub(crate) fn distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    let u = d.map(|v| v / length);
    let q = [p[0] - a[0], p[1] - a[1]];
    let t = (q[0] * u[0] + q[1] * u[1]).clamp(0., length);
    (q[0] - t * u[0]).hypot(q[1] - t * u[1])
}
pub(crate) fn separated(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], margin: f64) -> bool {
    let s = [
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    ];
    if s.iter().any(|v| !v.is_finite()) {
        return false;
    }
    if s[0].signum() != s[1].signum() && s[2].signum() != s[3].signum() {
        return false;
    }
    [
        distance(a, c, d),
        distance(b, c, d),
        distance(c, a, b),
        distance(d, a, b),
    ]
    .into_iter()
    .all(|v| v.is_finite() && v > margin)
}
pub(crate) fn validate_ring(points: &[[f64; 2]], tol: Tolerance) -> Result<(), KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !(3..=MAX_POLYGON_POINTS).contains(&points.len()) {
        return Err(KernelError::Unsupported(
            "simple polygon requires 3..=256 vertices".into(),
        ));
    }
    let scale = points.iter().flatten().map(|v| v.abs()).fold(0., f64::max);
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1e100 || (*v != 0. && v.abs() < 1e-100))
    {
        return Err(KernelError::NumericRange);
    }
    let error = scale * 256. * f64::EPSILON;
    if error > tol.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let margin = tol.linear + error;
    let mut area = 0.;
    let mut absolute = 0.;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let turn = orient(a, b, c);
        if !length.is_normal()
            || length <= margin
            || !turn.is_finite()
            || turn.abs() / length.max((c[0] - b[0]).hypot(c[1] - b[1])) <= margin
        {
            return Err(KernelError::Unsupported(
                "polygon has short or collinear adjacent edges".into(),
            ));
        }
        for j in i + 1..points.len() {
            if j == i + 1 || (i == 0 && j == points.len() - 1) {
                continue;
            }
            if !separated(a, b, points[j], points[(j + 1) % points.len()], margin) {
                return Err(KernelError::Unsupported(
                    "polygon has crossing or insufficiently separated nonadjacent edges".into(),
                ));
            }
        }
        if i > 0 && i + 1 < points.len() {
            let term = orient(points[0], a, b);
            area += term;
            absolute += term.abs();
        }
    }
    if !area.is_normal() || !absolute.is_finite() || area <= absolute * 512. * f64::EPSILON {
        return Err(KernelError::Unsupported(
            "polygon must have resolved positive counterclockwise area".into(),
        ));
    }
    Ok(())
}
pub(crate) fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut winding = 0i32;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let s = orient(a, b, p);
        if a[1] <= p[1] && b[1] > p[1] && s > 0. {
            winding += 1;
        }
        if a[1] > p[1] && b[1] <= p[1] && s < 0. {
            winding -= 1;
        }
    }
    winding != 0
}
/// Conservative convexity check for fan triangulation. The complete model must
/// be validated separately; this predicate is not a geometry validity certificate.
pub fn face_is_convex(model: &ModelSnapshot, id: FaceId) -> bool {
    let Some(face) = model.faces.get(id.0) else {
        return false;
    };
    if model.has_curved_geometry() || face.loops.len() != 1 {
        return false;
    }
    let Some(points) = model.loop_points(face.loops[0]) else {
        return false;
    };
    if points.len() < 3 {
        return false;
    }
    convex_ring(&points, face.try_plane().unwrap().normal, 1.)
}

pub(crate) fn convex_ring(points: &[[f64; 3]], normal: [f64; 3], orientation: f64) -> bool {
    for i in 0..points.len() {
        let a = points[i];
        let d = math::sub(points[(i + 1) % points.len()], a);
        let length = math::norm(d);
        let d = d.map(|v| v / length);
        for (j, p) in points.iter().enumerate() {
            if j == i || j == (i + 1) % points.len() {
                continue;
            }
            let q = math::sub(*p, a);
            let n = normal;
            let terms = [
                d[1] * q[2] * n[0],
                -d[2] * q[1] * n[0],
                d[2] * q[0] * n[1],
                -d[0] * q[2] * n[1],
                d[0] * q[1] * n[2],
                -d[1] * q[0] * n[2],
            ];
            let side = terms.into_iter().sum::<f64>() * orientation;
            let error = terms.into_iter().map(f64::abs).sum::<f64>() * 64. * f64::EPSILON;
            if !side.is_finite() || !error.is_finite() || side <= error {
                return false;
            }
        }
    }
    true
}
