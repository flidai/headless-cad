//! A circular shank through a canonical obround slot. Every admitted support
//! is reconciled to native topology; the capsule is never inferred from a mesh.
use crate::convex_clearance::support::{circular_consistency, revolution_consistency};
use crate::math::{cross, dot, norm, sub};
use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotTranslationProof {
    pub stock_slot_wall_faces: [FaceId; 4],
    pub stock_slab_cap_faces: [FaceId; 2],
    pub stock_slab_origin: [f64; 3],
    pub stock_slab_axis: [f64; 3],
    pub stock_slab_limits_mm: [f64; 2],
    pub shank_wall_face: FaceId,
    pub capsule_endpoints: [[f64; 3]; 2],
    pub capsule_radius_mm: f64,
    pub shank_radius_mm: f64,
    pub endpoint_centerline_distances_mm: [f64; 2],
    pub worst_endpoint_index: usize,
    pub other_section_axial_gap_mm: Option<f64>,
    pub arithmetic_guard_mm: f64,
    pub complete_path_lower_bound_mm: f64,
}

pub(crate) struct SlotBinding {
    pub slot_on_left: bool,
    pub slot: ArcPrismGeometry,
    pub slot_index: usize,
    pub bolt_origin: [f64; 3],
    pub bolt_axis: [f64; 3],
    pub bolt_start_z: f64,
    pub bolt_axis_sign: f64,
    pub radius: f64,
    pub shank_face: FaceId,
    pub other_gap: Option<f64>,
    pub error: f64,
    pub center: [f64; 2],
}

fn bad() -> KernelError {
    KernelError::Unsupported("slot clearance requires one containing canonical capsule, an arithmetic-parallel constant-radius shank spanning the slab, and axially separated larger bolt sections".into())
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: [f64; 3], k: f64) -> [f64; 3] {
    a.map(|x| x * k)
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
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn length2(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn raw_support_scale(models: [&ModelSnapshot; 2]) -> f64 {
    let mut scale = 0f64;
    for m in models {
        for v in &m.vertices {
            scale = v.position.into_iter().map(f64::abs).fold(scale, f64::max);
        }
        for e in &m.edges {
            let (origin, radius) = match &e.curve {
                Curve::Line(g) => (g.origin, 0.),
                Curve::Circle(g) => (g.origin, g.radius),
                Curve::CircularArc(g) => (g.circle.origin, g.circle.radius),
            };
            scale = origin
                .into_iter()
                .map(f64::abs)
                .fold(scale, f64::max)
                .max(radius.abs());
        }
        for f in &m.faces {
            let (origin, radius) = match &f.surface {
                Surface::Plane(g) => (g.origin, 0.),
                Surface::Cylinder(g) => (g.origin, g.radius),
                Surface::Cone(g) => (g.origin, g.radius),
            };
            scale = origin
                .into_iter()
                .map(f64::abs)
                .fold(scale, f64::max)
                .max(radius.abs());
        }
    }
    scale
}
fn segment(slot: &SlotHole) -> ([f64; 2], [f64; 2]) {
    let v = mul2(slot.direction, slot.centerline_length / 2.);
    (sub2(slot.center, v), add2(slot.center, v))
}
pub(crate) fn distance_to_centerline(q: [f64; 2], slot: &SlotHole) -> (f64, [f64; 2], f64) {
    let (a, b) = segment(slot);
    let v = sub2(b, a);
    let u = (dot2(sub2(q, a), v) / dot2(v, v)).clamp(0., 1.);
    let nearest = add2(a, mul2(v, u));
    (length2(sub2(q, nearest)), nearest, u)
}
pub(crate) fn admit_slot(m: &ModelSnapshot, t: Tolerance) -> Result<ArcPrismGeometry, KernelError> {
    let g = crate::arc_prism::recognize(m, t)?;
    if g.slot_holes.is_empty() {
        return Err(bad());
    }
    // The strict recognizer reconstructs every vertex, edge and face support.
    Ok(g)
}
pub(crate) fn is_candidate(a: &ModelSnapshot, b: &ModelSnapshot, t: Tolerance) -> bool {
    admit_slot(a, t).is_ok() || admit_slot(b, t).is_ok()
}

fn oriented(
    slot_model: &ModelSnapshot,
    bolt: &ModelSnapshot,
    t: Tolerance,
) -> Result<SlotBinding, KernelError> {
    let g = admit_slot(slot_model, t)?;
    let y = cross(g.axis, g.x_direction);
    let world = raw_support_scale([slot_model, bolt]);
    let extent = g.height
        + g.slot_holes
            .iter()
            .map(|s| s.centerline_length + s.radius)
            .fold(0., f64::max)
        + world;
    // Slot reconstruction permits at most 8192 source ulps, revolution and
    // circular stock recognizers at most 2048, with the remaining 2048 source
    // and 2048 span ulps reserved for frame projection and witness arithmetic.
    let error = ((12288. * world + 2048. * extent) * f64::EPSILON).next_up();
    if !error.is_normal() || error > t.linear / 64. {
        return Err(KernelError::NumericRange);
    }
    let (origin, axis, radius, shank_face, profile): (_, _, _, _, Option<Vec<[f64; 2]>>) =
        if let Ok(Some(c)) = bolt.cylinder_geometry(t) {
            if c.inner_radius != 0. {
                return Err(bad());
            }
            circular_consistency(bolt, &c, error)?;
            (
                c.origin,
                c.axis,
                c.radius,
                FaceId(2),
                Some(vec![[c.radius, 0.], [c.radius, c.height]]),
            )
        } else {
            let r = crate::revolve::recognize(bolt, t)?;
            revolution_consistency(bolt, &r, error)?;
            let (origin, axis) = (r.origin, r.axis);
            let profile = r.profile.clone();
            (origin, axis, 0., FaceId(usize::MAX), Some(profile))
        };
    let sign = if dot(axis, g.axis) >= 0. { 1. } else { -1. };
    let tilt = norm(sub(axis, mul(g.axis, sign)));
    if tilt > 128. * f64::EPSILON || tilt * extent > error {
        return Err(bad());
    }
    let z0 = dot(sub(origin, g.origin), g.axis);
    let projected = sub(origin, add(g.origin, mul(g.axis, z0)));
    let center = [dot(projected, g.x_direction), dot(projected, y)];
    let profile = profile.ok_or_else(bad)?;
    // Find a cylindrical boundary that covers the complete stock slab.
    // A circular extrusion has one, and a full-turn meridian can have several.
    let mut candidates = vec![];
    let edge_count = if radius > 0. { 1 } else { profile.len() };
    for i in 0..edge_count {
        let a = profile[i];
        let b = profile[(i + 1) % profile.len()];
        let za = z0 + sign * a[1];
        let zb = z0 + sign * b[1];
        if a[0] == b[0]
            && a[0] > t.linear
            && za.min(zb) <= -4. * error
            && za.max(zb) >= g.height + 4. * error
        {
            candidates.push((
                a[0],
                if radius > 0. {
                    shank_face
                } else {
                    // Revolution segments omit the axis edge; use their explicit face IDs.
                    let r = crate::revolve::recognize(bolt, t)?;
                    r.segments
                        .iter()
                        .find(|s| s.start == a && s.end == b)
                        .map(|s| s.face)
                        .ok_or_else(bad)?
                },
            ));
        }
    }
    if candidates.len() != 1 {
        return Err(bad());
    }
    let (radius, shank_face) = candidates[0];
    let mut other_gap = f64::INFINITY;
    // A larger section may be elsewhere on the bolt, but its entire edge
    // must remain axially outside the slab. This bound covers its filled region.
    if edge_count > 1 {
        for i in 0..profile.len() {
            let a = profile[i];
            let b = profile[(i + 1) % profile.len()];
            if a[0].max(b[0]) <= radius + 4. * error {
                continue;
            }
            let za = z0 + sign * a[1];
            let zb = z0 + sign * b[1];
            let lo = za.min(zb);
            let hi = za.max(zb);
            let gap = if hi < 0. {
                -hi
            } else if lo > g.height {
                lo - g.height
            } else {
                return Err(bad());
            };
            other_gap = other_gap.min(gap);
        }
    }
    let containing: Vec<_> = g
        .slot_holes
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let d = distance_to_centerline(center, s).0;
            s.radius - radius - d > t.linear + 16. * error
        })
        .map(|(i, _)| i)
        .collect();
    if containing.len() != 1 {
        return Err(bad());
    }
    let slot_index = containing[0];
    let radial = g.slot_holes[slot_index].radius - radius;
    if other_gap.is_finite() && other_gap <= radial + 32. * error {
        return Err(bad());
    }
    Ok(SlotBinding {
        slot_on_left: true,
        slot: g,
        slot_index,
        bolt_origin: origin,
        bolt_axis: axis,
        bolt_start_z: z0,
        bolt_axis_sign: sign,
        radius,
        shank_face,
        other_gap: other_gap.is_finite().then_some(other_gap),
        error,
        center,
    })
}

pub(crate) fn bind(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SlotBinding, KernelError> {
    let al = admit_slot(a, t).is_ok();
    let bl = admit_slot(b, t).is_ok();
    if al == bl {
        return Err(bad());
    }
    if al {
        oriented(a, b, t)
    } else {
        let mut binding = oriented(b, a, t)?;
        binding.slot_on_left = false;
        Ok(binding)
    }
}

pub(crate) fn point(g: &ArcPrismGeometry, p: [f64; 2], z: f64) -> [f64; 3] {
    add(
        g.origin,
        add(
            mul(g.x_direction, p[0]),
            add(mul(cross(g.axis, g.x_direction), p[1]), mul(g.axis, z)),
        ),
    )
}
pub(crate) fn query_bound(
    binding: &SlotBinding,
    center: [f64; 2],
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let slot = &binding.slot.slot_holes[binding.slot_index];
    let (d, nearest, fraction) = distance_to_centerline(center, slot);
    let radial = slot.radius - binding.radius - d;
    let guard = 16. * binding.error;
    if radial <= t.linear + guard || binding.other_gap.is_some_and(|gap| gap <= radial + guard) {
        return Err(bad());
    }
    let u = if d > binding.error {
        mul2(sub2(center, nearest), 1. / d)
    } else {
        [-slot.direction[1], slot.direction[0]]
    };
    let wall = add2(nearest, mul2(u, slot.radius));
    let shank = add2(center, mul2(u, binding.radius));
    let z = binding.slot.height / 2.;
    let wall_point = point(&binding.slot, wall, z);
    let candidate_radial = add(
        mul(binding.slot.x_direction, shank[0] - binding.center[0]),
        mul(
            cross(binding.slot.axis, binding.slot.x_direction),
            shank[1] - binding.center[1],
        ),
    );
    let projected_radial = sub(
        candidate_radial,
        mul(binding.bolt_axis, dot(candidate_radial, binding.bolt_axis)),
    );
    let radial_length = norm(projected_radial);
    if !radial_length.is_normal() {
        return Err(KernelError::NumericRange);
    }
    let shank_point = add(
        binding.bolt_origin,
        add(
            mul(
                binding.bolt_axis,
                binding.bolt_axis_sign * (z - binding.bolt_start_z),
            ),
            mul(projected_radial, binding.radius / radial_length),
        ),
    );
    let (left_point, right_point) = if binding.slot_on_left {
        (wall_point, shank_point)
    } else {
        (shank_point, wall_point)
    };
    let distance = norm(sub(right_point, left_point));
    let lower = (radial - guard).next_down();
    let upper = (radial.max(distance) + guard).next_up();
    if lower <= t.linear || upper - lower > t.linear {
        return Err(KernelError::NumericRange);
    }
    let axis = mul(sub(right_point, left_point), 1. / distance);
    let _wall_face = slot_faces(binding)[if fraction <= binding.error {
        3
    } else if fraction >= 1. - binding.error {
        1
    } else if dot2(u, [-slot.direction[1], slot.direction[0]]) >= 0. {
        2
    } else {
        0
    }];
    Ok(SolidClearance {
        method: ClearanceMethod::SlotTrimWitness,
        lower_bound_mm: lower,
        upper_bound_mm: upper,
        left_point,
        right_point,
        axis,
    })
}

pub(crate) fn slot_faces(binding: &SlotBinding) -> [FaceId; 4] {
    let first = binding.slot.path.segments.len()
        + 2
        + binding.slot.circular_holes.len()
        + 4 * binding.slot_index;
    std::array::from_fn(|i| FaceId(first + i))
}

pub(crate) fn query(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<SolidClearance, KernelError> {
    let binding = bind(a, b, t)?;
    query_bound(&binding, binding.center, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ModelSnapshot, ModelSnapshot) {
        let t = Tolerance::default();
        let plate = extrude_rectangle(30., 20., 6., t).unwrap().model;
        let plate = cut_obround_slots(
            &plate,
            &[ObroundSlot {
                id: "adjust".into(),
                center: [15., 10.],
                direction: [1., 0.],
                centerline_length: 6.,
                radius: 3.,
            }],
            t,
        )
        .unwrap()
        .model;
        let bolt = revolve_polygon(
            &[
                [0., -2.],
                [2.5, -2.],
                [2.5, 8.],
                [5., 8.],
                [5., 10.],
                [0., 10.],
            ],
            t,
        )
        .unwrap()
        .model;
        let bolt = translate(&bolt, [14., 10., 0.], t).unwrap().model;
        (plate, bolt)
    }
    #[test]
    fn actual_slot_and_stepped_bolt_witnesses_and_endpoint_path() {
        let t = Tolerance::default();
        let (plate, bolt) = fixture();
        for (a, b) in [(&plate, &bolt), (&bolt, &plate)] {
            let q = a.clearance(b, t).unwrap();
            assert_eq!(q.method, ClearanceMethod::SlotTrimWitness);
            assert!(q.lower_bound_mm <= 0.5 && q.upper_bound_mm >= 0.5);
            assert!(q.upper_bound_mm - q.lower_bound_mm < t.linear);
            let direction = if std::ptr::eq(a, &plate) {
                [2., 0., 0.]
            } else {
                [-2., 0., 0.]
            };
            let sweep = a.translation_clearance(b, direction, 0.4, t, 3).unwrap();
            assert_eq!(sweep.query_count, 2);
            assert!(sweep.intervals.is_empty());
            assert!(sweep.slot_motion_proof.is_some());
            assert!(sweep.lower_bound_mm <= 0.5 && sweep.upper_bound_mm >= 0.5);
        }
        let q = [0.1, 0.2, 0.3, 0.9273618495495704];
        let length = q.iter().map(|x| x * x).sum::<f64>().sqrt();
        let q = q.map(|x| x / length);
        let moved_plate = transform(&plate, [20., -7., 4.], q, t).unwrap().model;
        let moved_bolt = transform(&bolt, [20., -7., 4.], q, t).unwrap().model;
        assert_eq!(
            moved_plate.clearance(&moved_bolt, t).unwrap().method,
            ClearanceMethod::SlotTrimWitness
        );
    }
    #[test]
    fn head_slit_tilt_corruption_and_budget_refuse() {
        let t = Tolerance::default();
        let (plate, bolt) = fixture();
        let close_head = translate(&bolt, [0., 0., -2.], t).unwrap().model;
        assert!(plate.clearance(&close_head, t).is_err());
        let tilted = transform(
            &bolt,
            [0.; 3],
            [(0.002f64).sin(), 0., 0., (0.002f64).cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(plate.clearance(&tilted, t).is_err());
        let mut corrupt = plate.clone();
        let face = corrupt
            .faces
            .iter_mut()
            .find(|f| matches!(f.surface, Surface::Cylinder(_)))
            .unwrap();
        if let Surface::Cylinder(c) = &mut face.surface {
            c.radius += 0.05;
        }
        assert!(corrupt.clearance(&bolt, t).is_err());
        assert!(plate
            .translation_clearance(&bolt, [2., 0., 0.], 0.4, t, 2)
            .is_err());
        assert!(plate
            .translation_clearance(&bolt, [0., 0., 2.], 0.4, t, 3)
            .is_err());
        assert!(plate
            .translation_clearance(&bolt, [5., 0., 0.], 0.4, t, 3)
            .is_err());
        let wide = revolve_polygon(
            &[
                [0., -2.],
                [3.1, -2.],
                [3.1, 8.],
                [5., 8.],
                [5., 10.],
                [0., 10.],
            ],
            t,
        )
        .unwrap()
        .model;
        let wide = translate(&wide, [14., 10., 0.], t).unwrap().model;
        assert!(plate.clearance(&wide, t).is_err());
    }
}
