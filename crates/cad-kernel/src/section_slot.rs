//! Exact obround-slot dimensions from one native analytic section.
use crate::math::{cross, dot, norm, sub};
use crate::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionSlotSegmentWitness {
    pub face: FaceId,
    pub loop_index: usize,
    pub segment_index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionSlotMeasurement {
    /// Coordinates in the supplied section plane, in millimetres.
    pub center: [f64; 2],
    pub centerline_endpoints: [[f64; 2]; 2],
    /// Unit vector from endpoint zero to endpoint one in section coordinates.
    pub axis: [f64; 2],
    pub radius_mm: f64,
    pub width_mm: f64,
    pub centerline_length_mm: f64,
    pub overall_length_mm: f64,
    pub loop_index: usize,
    /// Witnesses in actual section-loop order, independent of input face order.
    pub segments: [SectionSlotSegmentWitness; 4],
    /// Bound for native projection, support and dimension arithmetic; this is
    /// not a manufacturing tolerance.
    pub arithmetic_uncertainty_mm: f64,
}

fn bad() -> KernelError {
    KernelError::Unsupported(
        "section slot dimension requires one complete canonical four-wall obround through-slot in the actual analytic section".into(),
    )
}
fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn add2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul2(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}
fn len2(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn project(plane: SectionPlane, point: [f64; 3]) -> [f64; 2] {
    let d = sub(point, plane.origin);
    [
        dot(d, plane.x_direction),
        dot(d, cross(plane.normal, plane.x_direction)),
    ]
}
fn position(g: &ArcPrismGeometry, p: [f64; 2], z: f64) -> [f64; 3] {
    let y = cross(g.axis, g.x_direction);
    std::array::from_fn(|i| g.origin[i] + g.x_direction[i] * p[0] + y[i] * p[1] + g.axis[i] * z)
}
fn arc_ends(center: [f64; 2], radius: f64, start: f64, sweep: f64) -> ([f64; 2], [f64; 2]) {
    let point = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
    (point(start), point(start + sweep))
}

/// Measure a full-height native capsule using exactly its four selected wall
/// faces and the section produced from this placed model. The selected faces
/// may be in any order. Face IDs remain stable under rigid placement.
pub fn measure_section_slot(
    model: &ModelSnapshot,
    section: &PlanarSection,
    faces: [FaceId; 4],
    tolerance: Tolerance,
) -> Result<SectionSlotMeasurement, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    section.plane.validate()?;
    let selected: BTreeSet<_> = faces.into_iter().collect();
    if selected.len() != 4 {
        return Err(bad());
    }
    let mixed = crate::blind_slot_prism::through_slot_witness(model, tolerance).ok();
    let g = if let Some((g, _)) = &mixed {
        g.clone()
    } else {
        crate::arc_prism::recognize(model, tolerance)?
    };
    let support_model = mixed.as_ref().map_or(model, |(_, through)| through);
    if g.slot_holes.is_empty() {
        return Err(bad());
    }
    let offset = g.path.segments.len() + 2 + g.circular_holes.len();
    let slot_index = (0..g.slot_holes.len())
        .find(|i| selected == (0..4).map(|j| FaceId(offset + 4 * *i + j)).collect())
        .ok_or_else(bad)?;
    let slot = &g.slot_holes[slot_index];
    let normal_alignment = dot(section.plane.normal, g.axis).abs();
    if (1. - normal_alignment).abs() > 128. * f64::EPSILON
        || norm(cross(section.plane.normal, g.axis)) > 128. * f64::EPSILON
    {
        return Err(bad());
    }
    let z = dot(sub(section.plane.origin, g.origin), g.axis);
    if !z.is_finite() || z <= tolerance.linear || z >= g.height - tolerance.linear {
        return Err(bad());
    }

    let scale = model
        .vertices
        .iter()
        .map(|v| norm(v.position))
        .chain(model.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => vec![norm(l.origin)],
            Curve::Circle(c) => vec![norm(c.origin), c.radius],
            Curve::CircularArc(a) => vec![norm(a.circle.origin), a.circle.radius],
        }))
        .chain([
            norm(section.plane.origin),
            g.height,
            slot.radius,
            slot.centerline_length,
        ])
        .fold(1., f64::max);
    let error = (65536. * f64::EPSILON * scale).next_up();
    if !error.is_normal() || error > tolerance.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    crate::convex_clearance::support::arc_consistency(support_model, &g, error, tolerance)?;

    let half = slot.centerline_length / 2.;
    let local_ends = [
        [
            slot.center[0] - half * slot.direction[0],
            slot.center[1] - half * slot.direction[1],
        ],
        [
            slot.center[0] + half * slot.direction[0],
            slot.center[1] + half * slot.direction[1],
        ],
    ];
    let endpoints = local_ends.map(|p| project(section.plane, position(&g, p, z)));
    let center = project(section.plane, position(&g, slot.center, z));
    let delta = sub2(endpoints[1], endpoints[0]);
    let length = len2(delta);
    let radius = slot.radius;
    if !length.is_normal()
        || (length - slot.centerline_length).abs() > error
        || len2(sub2(center, mul2(add2(endpoints[0], endpoints[1]), 0.5))) > error
    {
        return Err(KernelError::NumericRange);
    }
    let axis = mul2(delta, 1. / length);
    let normal = [-axis[1], axis[0]];
    let mut matching = section.loops.iter().enumerate().filter(|(_, l)| {
        l.segments.len() == 4
            && l.segments.iter().map(|s| s.face).collect::<BTreeSet<_>>() == selected
    });
    let (loop_index, loop_) = matching.next().ok_or_else(bad)?;
    if matching.next().is_some() {
        return Err(bad());
    }
    // A selected wall must occur nowhere else in this section, including in
    // another partial or duplicated loop.
    if section
        .loops
        .iter()
        .enumerate()
        .any(|(i, l)| i != loop_index && l.segments.iter().any(|s| selected.contains(&s.face)))
    {
        return Err(bad());
    }
    let mut line_count = 0;
    let mut arc_count = 0;
    let mut line_sides = Vec::new();
    let mut arc_centers = Vec::new();
    let mut contour = Vec::new();
    for segment in &loop_.segments {
        match &segment.curve {
            SectionCurve::Line { start, end } => {
                let (start, end) = (*start, *end);
                if !start.into_iter().chain(end).all(f64::is_finite) {
                    return Err(KernelError::NumericRange);
                }
                let v = sub2(end, start);
                let measured = len2(v);
                if (measured - length).abs() > error || (dot2(v, axis).abs() - length).abs() > error
                {
                    return Err(bad());
                }
                let middle = mul2(add2(start, end), 0.5);
                let side = dot2(sub2(middle, center), normal);
                if (side.abs() - radius).abs() > error
                    || dot2(sub2(middle, center), axis).abs() > error
                {
                    return Err(bad());
                }
                // Endpoints must be tangent points on opposite semicircles.
                if ![start, end].iter().all(|p| {
                    endpoints
                        .iter()
                        .any(|e| (len2(sub2(*p, *e)) - radius).abs() <= error)
                }) {
                    return Err(bad());
                }
                line_sides.push(side);
                contour.push((start, end));
                line_count += 1;
            }
            SectionCurve::Arc {
                center: c,
                radius: r,
                start_angle,
                sweep_angle,
            } => {
                let (c, r, start_angle, sweep_angle) = (*c, *r, *start_angle, *sweep_angle);
                if !c
                    .into_iter()
                    .chain([r, start_angle, sweep_angle])
                    .all(f64::is_finite)
                {
                    return Err(KernelError::NumericRange);
                }
                if (r - radius).abs() > error
                    || (sweep_angle.abs() - std::f64::consts::PI).abs() * radius > error
                    || !endpoints.iter().any(|e| len2(sub2(c, *e)) <= error)
                {
                    return Err(bad());
                }
                arc_centers.push(c);
                contour.push(arc_ends(c, r, start_angle, sweep_angle));
                arc_count += 1;
            }
        }
    }
    if line_count != 2
        || arc_count != 2
        || line_sides[0] * line_sides[1] >= 0.
        || !endpoints
            .iter()
            .all(|e| arc_centers.iter().any(|c| len2(sub2(*e, *c)) <= error))
    {
        return Err(bad());
    }
    for i in 0..4 {
        if len2(sub2(contour[i].1, contour[(i + 1) % 4].0)) > error {
            return Err(bad());
        }
    }
    let width = 2. * radius;
    let overall = length + 2. * radius;
    if !width.is_normal() || !overall.is_normal() {
        return Err(KernelError::NumericRange);
    }
    let witnesses = std::array::from_fn(|i| SectionSlotSegmentWitness {
        face: loop_.segments[i].face,
        loop_index,
        segment_index: i,
    });
    Ok(SectionSlotMeasurement {
        center,
        centerline_endpoints: endpoints,
        axis,
        radius_mm: radius,
        width_mm: width,
        centerline_length_mm: length,
        overall_length_mm: overall,
        loop_index,
        segments: witnesses,
        arithmetic_uncertainty_mm: error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(t: Tolerance) -> (ModelSnapshot, [FaceId; 4]) {
        let stock = extrude_rectangle(80., 50., 8., t).unwrap().model;
        let out = cut_obround_slots(
            &stock,
            &[ObroundSlot {
                id: "mount".into(),
                center: [40., 25.],
                direction: [1., 0.],
                centerline_length: 8.,
                radius: 3.,
            }],
            t,
        )
        .unwrap();
        let faces = out.history.generated_faces["slot_mount_walls"]
            .clone()
            .try_into()
            .unwrap();
        (out.model, faces)
    }
    fn section(model: &ModelSnapshot, t: Tolerance) -> PlanarSection {
        let g = crate::arc_prism::recognize(model, t).unwrap();
        let plane = SectionPlane {
            origin: std::array::from_fn(|i| g.origin[i] + g.axis[i] * g.height / 2.),
            normal: g.axis,
            x_direction: g.x_direction,
        };
        model.section(plane, t).unwrap()
    }
    #[test]
    fn exact_native_capsule_and_arbitrary_face_order() {
        let t = Tolerance::default();
        let (model, faces) = fixture(t);
        let section = section(&model, t);
        let m = measure_section_slot(
            &model,
            &section,
            [faces[3], faces[1], faces[0], faces[2]],
            t,
        )
        .unwrap();
        let expected = project(section.plane, [40., 25., 4.]);
        assert!(len2(sub2(m.center, expected)) < 1e-9);
        assert!((m.radius_mm - 3.).abs() < 1e-9);
        assert!((m.width_mm - 6.).abs() < 1e-9);
        assert!((m.centerline_length_mm - 8.).abs() < 1e-9);
        assert!((m.overall_length_mm - 14.).abs() < 1e-9);
        assert_eq!(
            m.segments.iter().map(|w| w.face).collect::<BTreeSet<_>>(),
            faces.into_iter().collect()
        );
        assert!(m.arithmetic_uncertainty_mm < t.linear / 16.);
    }
    #[test]
    fn common_oblique_placement_and_reversed_section_normal() {
        let t = Tolerance::default();
        let (model, faces) = fixture(t);
        let q = [0.2, 0.3, 0.4, 0.71_f64.sqrt()];
        let placed = transform(&model, [41., -27., 12.], q, t).unwrap().model;
        let section = section(&placed, t);
        let m = measure_section_slot(&placed, &section, faces, t).unwrap();
        assert!((m.width_mm - 6.).abs() < 1e-7);
        assert!((m.overall_length_mm - 14.).abs() < 1e-7);
        let reverse = SectionPlane {
            origin: section.plane.origin,
            normal: section.plane.normal.map(|v| -v),
            x_direction: section.plane.x_direction,
        };
        let flipped = placed.section(reverse, t).unwrap();
        let r = measure_section_slot(&placed, &flipped, faces, t).unwrap();
        assert!((r.width_mm - 6.).abs() < 1e-7);
    }
    #[test]
    fn rejects_corruption_partial_or_mixed_selection_and_event_plane() {
        let t = Tolerance::default();
        let (model, faces) = fixture(t);
        let section = section(&model, t);
        let mut partial = section.clone();
        let index = partial
            .loops
            .iter()
            .position(|l| l.segments.iter().any(|s| s.face == faces[0]))
            .unwrap();
        partial.loops[index].segments.pop();
        assert!(measure_section_slot(&model, &partial, faces, t).is_err());
        let mut distorted = section.clone();
        let loop_ = &mut distorted.loops[index];
        let arc = loop_
            .segments
            .iter_mut()
            .find(|s| matches!(s.curve, SectionCurve::Arc { .. }))
            .unwrap();
        if let SectionCurve::Arc { radius, .. } = &mut arc.curve {
            *radius += 0.01;
        }
        assert!(measure_section_slot(&model, &distorted, faces, t).is_err());
        assert!(measure_section_slot(
            &model,
            &section,
            [faces[0], faces[1], faces[2], FaceId(2)],
            t
        )
        .is_err());
        let mut corrupted = model.clone();
        if let Surface::Cylinder(c) = &mut corrupted.faces[faces[1].0].surface {
            c.radius += 0.001;
        }
        assert!(measure_section_slot(&corrupted, &section, faces, t).is_err());
        let event = SectionPlane {
            origin: [0., 0., 0.],
            normal: [0., 0., 1.],
            x_direction: [1., 0., 0.],
        };
        assert!(model.section(event, t).is_err());
    }
}
