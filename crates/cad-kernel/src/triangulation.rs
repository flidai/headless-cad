//! Certified bounded display/export triangles. Native caps retain their loops.
use crate::math::{cross, dot, norm, sub};
use crate::*;
fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    let point = |p: [f64; 2]| robust::Coord { x: p[0], y: p[1] };
    robust::orient2d(point(a), point(b), point(c))
}

/// Triangulate one validated planar face using native vertex IDs. Supporting
/// earcutr only proposes triangles; orientation, directed boundary incidence,
/// area and pairwise absence of interior overlap are independently checked.
/// This is derived geometry, not a replacement of authoritative inner loops.
pub fn triangulate_face(
    model: &ModelSnapshot,
    face_id: FaceId,
    tol: Tolerance,
) -> Result<Vec<[VertexId; 3]>, KernelError> {
    if !tol.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    let report = model.validate(tol);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    if model.has_curved_geometry() {
        return Err(KernelError::Unsupported(
            "analytic curved boundaries require bounded-error tessellation with derived vertices"
                .into(),
        ));
    }
    let face = model
        .faces
        .get(face_id.0)
        .ok_or_else(|| KernelError::Unsupported("missing face for triangulation".into()))?;
    let rings: Vec<Vec<VertexId>> = face
        .loops
        .iter()
        .map(|id| {
            model.loops[id.0]
                .edges
                .iter()
                .map(|e| model.endpoints(*e).unwrap()[0])
                .collect()
        })
        .collect();
    let ids: Vec<_> = rings.iter().flatten().copied().collect();
    if ids.len() > MAX_POLYGON_POINTS {
        return Err(KernelError::ResourceLimit(
            "face triangulation vertex limit".into(),
        ));
    }
    let origin = model.vertices[ids[0].0].position;
    let delta = sub(model.vertices[ids[1].0].position, origin);
    let x = delta.map(|v| v / norm(delta));
    let y = cross(face.try_plane().unwrap().normal, x);
    let points: Vec<_> = ids
        .iter()
        .map(|id| {
            let d = sub(model.vertices[id.0].position, origin);
            [dot(d, x), dot(d, y)]
        })
        .collect();
    // Keep adaptive predicate expansions clear of overflow/underflow. This is
    // a derived triangulation envelope, not a new native coordinate limit.
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1e100 || (*v != 0. && v.abs() < 1e-100))
    {
        return Err(KernelError::NumericRange);
    }
    let mut offset = 0;
    let mut holes = vec![];
    let mut boundary = BTreeMap::new();
    let mut expected_area = 0.;
    for (ri, ring) in rings.iter().enumerate() {
        if ri > 0 {
            holes.push(offset);
        }
        for i in 0..ring.len() {
            boundary.insert((ring[i], ring[(i + 1) % ring.len()]), 1usize);
        }
        for k in 1..ring.len() - 1 {
            expected_area +=
                orient(points[offset], points[offset + k], points[offset + k + 1]) / 2.;
        }
        offset += ring.len();
    }
    let data: Vec<_> = points.iter().flatten().copied().collect();
    let indices = if holes.is_empty() && crate::face_is_convex(model, face_id) {
        (1..ids.len() - 1)
            .flat_map(|i| [0, i, i + 1])
            .collect::<Vec<_>>()
    } else {
        earcutr::earcut(&data, &holes, 2)
            .map_err(|e| KernelError::Unsupported(format!("face triangulation failed: {e:?}")))?
    };
    let bad =
        || KernelError::Unsupported("triangulation did not certify oriented hole coverage".into());
    if indices.len() % 3 != 0 || indices.iter().any(|&i| i >= ids.len()) {
        return Err(bad());
    }
    // Earcut can remove collinear bridge occurrences between aligned holes.
    // Restore every native vertex on a proposed triangle edge: exact predicate
    // incidence only, no insertion of new coordinates and no tolerance weld.
    let mut proposed: Vec<[usize; 3]> = indices.as_chunks::<3>().0.to_vec();
    let limit = ids.len() + 2 * holes.len() - 2;
    let mut cursor = 0;
    while cursor < proposed.len() {
        let t = proposed[cursor];
        let mut split = None;
        'edges: for k in 0..3 {
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            for p in 0..points.len() {
                if t.contains(&p) {
                    continue;
                }
                if orient(points[a], points[b], points[p]) == 0.0 {
                    let axis = if (points[a][0] - points[b][0]).abs()
                        >= (points[a][1] - points[b][1]).abs()
                    {
                        0
                    } else {
                        1
                    };
                    if points[p][axis] > points[a][axis].min(points[b][axis])
                        && points[p][axis] < points[a][axis].max(points[b][axis])
                    {
                        split = Some(([a, p, c], [p, b, c]));
                        break 'edges;
                    }
                }
            }
        }
        if let Some((a, b)) = split {
            if proposed.len() >= limit {
                return Err(bad());
            }
            proposed[cursor] = a;
            proposed.push(b);
        } else {
            cursor += 1;
        }
    }
    if proposed.len() != limit {
        return Err(bad());
    }
    let mut triangles = vec![];
    let mut indexed = vec![];
    let mut incidence: BTreeMap<(VertexId, VertexId), Vec<(VertexId, VertexId)>> = BTreeMap::new();
    let mut area = 0.;
    for chunk in &proposed {
        let mut t = [chunk[0], chunk[1], chunk[2]];
        let signed = orient(points[t[0]], points[t[1]], points[t[2]]);
        if !signed.is_normal() {
            return Err(bad());
        }
        if signed < 0. {
            t.swap(1, 2);
        }
        area += signed.abs() / 2.;
        let triangle = t.map(|i| ids[i]);
        for k in 0..3 {
            let (a, b) = (triangle[k], triangle[(k + 1) % 3]);
            incidence
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((a, b));
        }
        indexed.push(t);
        triangles.push(triangle);
    }
    for (_, uses) in incidence {
        match uses.as_slice() {
            [edge] => {
                if boundary.remove(edge).is_none() {
                    return Err(bad());
                }
            }
            [a, b] if a.0 == b.1 && a.1 == b.0 => {}
            _ => return Err(bad()),
        }
    }
    if !boundary.is_empty()
        || !area.is_finite()
        || !expected_area.is_normal()
        || expected_area <= 0.
        || (area - expected_area).abs()
            > area.max(expected_area) * 512. * f64::EPSILON * ids.len() as f64
    {
        return Err(bad());
    }
    // Strict separating-axis test: shared edges/vertices are allowed, triangle
    // interiors must not overlap. Uses actual projected triangle coordinates.
    for i in 0..indexed.len() {
        for j in i + 1..indexed.len() {
            let a = indexed[i].map(|k| points[k]);
            let b = indexed[j].map(|k| points[k]);
            let mut separate = false;
            for (triangle, other) in [(a, b), (b, a)] {
                for k in 0..3 {
                    let signs = other.map(|p| orient(triangle[k], triangle[(k + 1) % 3], p));
                    if signs.iter().any(|v| !v.is_finite()) {
                        return Err(KernelError::NumericRange);
                    }
                    if signs.into_iter().all(|v| v <= 0.) {
                        separate = true;
                    }
                }
            }
            if !separate {
                return Err(bad());
            }
        }
    }
    Ok(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adaptive_sign_survives_large_product_cancellation() {
        let a = [0., 0.];
        let b = [1e8, 1e8 - 1.];
        let c = [1e8 + 1., 1e8];
        let naive = b[0] * c[1] - b[1] * c[0];
        assert_eq!(naive, 0.);
        assert!(orient(a, b, c) > 0.);
        assert!(orient(a, c, b) < 0.);
    }
}
