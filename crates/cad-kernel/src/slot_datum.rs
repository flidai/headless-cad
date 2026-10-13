//! Associative frames at strictly recognized native obround-slot centers.
use crate::math::{cross, dot, finite, norm, sub};
use crate::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotCenterFrame {
    /// Origin is on the selected cap at the slot center. X follows the native
    /// capsule centerline and Z follows the outward cap normal.
    pub frame: FaceFrame,
    /// Native centerline endpoints on that cap, in frame X order.
    pub centerline_endpoints: [[f64; 3]; 2],
    pub radius_mm: f64,
    pub centerline_length_mm: f64,
    pub cap_face: FaceId,
    /// Four slot wall IDs in canonical native order, independent of input order.
    pub wall_faces: [FaceId; 4],
    /// Conservative floating point construction bound, not a manufacturing
    /// tolerance or a promise about arbitrary slot topology.
    pub arithmetic_uncertainty_mm: f64,
}

fn bad() -> KernelError {
    KernelError::Unsupported(
        "slot center datum requires exactly one canonical native four-wall obround slot and its start or end cap".into(),
    )
}

/// Resolve one exact slot and cap in the same validated native ArcPrism.
/// This works directly on a rigidly transformed snapshot; face IDs are native
/// handles, not authoring identifiers or nearest-face guesses.
pub fn slot_center_frame(
    model: &ModelSnapshot,
    walls: [FaceId; 4],
    cap: FaceId,
    tolerance: Tolerance,
) -> Result<SlotCenterFrame, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if cap != FaceId(0) && cap != FaceId(1) {
        return Err(bad());
    }
    let selected: BTreeSet<_> = walls.into_iter().collect();
    if selected.len() != 4 {
        return Err(bad());
    }
    let g = crate::arc_prism::recognize(model, tolerance)?;
    let offset = g.path.segments.len() + 2 + g.circular_holes.len();
    let slot_index = (0..g.slot_holes.len())
        .find(|i| selected == (0..4).map(|j| FaceId(offset + 4 * *i + j)).collect())
        .ok_or_else(bad)?;
    let canonical_walls = std::array::from_fn(|j| FaceId(offset + 4 * slot_index + j));
    let slot = &g.slot_holes[slot_index];

    let scale = model
        .vertices
        .iter()
        .flat_map(|v| v.position)
        .chain(model.edges.iter().flat_map(|e| match &e.curve {
            Curve::Line(l) => vec![l.origin[0], l.origin[1], l.origin[2]],
            Curve::Circle(c) => vec![c.origin[0], c.origin[1], c.origin[2], c.radius],
            Curve::CircularArc(a) => vec![
                a.circle.origin[0],
                a.circle.origin[1],
                a.circle.origin[2],
                a.circle.radius,
            ],
        }))
        .chain([
            g.height,
            slot.center[0],
            slot.center[1],
            slot.radius,
            slot.centerline_length,
        ])
        .map(f64::abs)
        .fold(1., f64::max);
    let error = (65536. * f64::EPSILON * scale).next_up();
    if !error.is_normal() || error > tolerance.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    crate::convex_clearance::support::arc_consistency(model, &g, error, tolerance)?;

    let y_native = cross(g.axis, g.x_direction);
    let x_axis = std::array::from_fn(|i| {
        g.x_direction[i] * slot.direction[0] + y_native[i] * slot.direction[1]
    });
    let z_axis = if cap == FaceId(0) {
        g.axis.map(|v| -v)
    } else {
        g.axis
    };
    let y_axis = cross(z_axis, x_axis);
    if [x_axis, y_axis, z_axis]
        .iter()
        .any(|v| !finite(*v) || (norm(*v) - 1.).abs() > 4096. * f64::EPSILON)
        || dot(x_axis, y_axis).abs() > 4096. * f64::EPSILON
        || dot(x_axis, z_axis).abs() > 4096. * f64::EPSILON
    {
        return Err(KernelError::NumericRange);
    }
    let z = if cap == FaceId(0) { 0. } else { g.height };
    let point = |p: [f64; 2]| -> [f64; 3] {
        std::array::from_fn(|i| {
            g.origin[i] + g.x_direction[i] * p[0] + y_native[i] * p[1] + g.axis[i] * z
        })
    };
    let half = slot.centerline_length / 2.;
    let local_endpoints =
        [-1., 1.].map(|sign| [0, 1].map(|i| slot.center[i] + sign * half * slot.direction[i]));
    let centerline_endpoints = local_endpoints.map(point);
    let origin = point(slot.center);
    let cap_plane = model.faces[cap.0].try_plane().ok_or_else(bad)?;
    if !finite(origin)
        || centerline_endpoints.iter().any(|p| !finite(*p))
        || dot(sub(origin, cap_plane.origin), cap_plane.normal).abs() > error
        || dot(cap_plane.normal, z_axis) < 1. - 4096. * f64::EPSILON
        || (norm(sub(centerline_endpoints[1], centerline_endpoints[0])) - slot.centerline_length)
            .abs()
            > error
    {
        return Err(KernelError::NumericRange);
    }
    Ok(SlotCenterFrame {
        frame: FaceFrame {
            origin,
            x_axis,
            y_axis,
            z_axis,
        },
        centerline_endpoints,
        radius_mm: slot.radius,
        centerline_length_mm: slot.centerline_length,
        cap_face: cap,
        wall_faces: canonical_walls,
        arithmetic_uncertainty_mm: error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(t: Tolerance) -> (ModelSnapshot, [FaceId; 4], [FaceId; 4]) {
        let stock = extrude_rectangle(80., 50., 8., t).unwrap().model;
        let out = cut_obround_slots(
            &stock,
            &[
                ObroundSlot {
                    id: "left".into(),
                    center: [20., 25.],
                    direction: [1., 0.],
                    centerline_length: 8.,
                    radius: 3.,
                },
                ObroundSlot {
                    id: "right".into(),
                    center: [60., 25.],
                    direction: [1., 0.],
                    centerline_length: 8.,
                    radius: 3.,
                },
            ],
            t,
        )
        .unwrap();
        let left = out.history.generated_faces["slot_left_walls"]
            .clone()
            .try_into()
            .unwrap();
        let right = out.history.generated_faces["slot_right_walls"]
            .clone()
            .try_into()
            .unwrap();
        (out.model, left, right)
    }

    #[test]
    fn both_caps_have_exact_center_and_outward_right_handed_basis() {
        let t = Tolerance::default();
        let (model, walls, _) = fixture(t);
        for (cap, z, outward) in [(FaceId(0), 0., -1.), (FaceId(1), 8., 1.)] {
            let m = slot_center_frame(&model, [walls[2], walls[0], walls[3], walls[1]], cap, t)
                .unwrap();
            assert_eq!(m.cap_face, cap);
            assert_eq!(m.wall_faces, walls);
            assert_eq!(m.frame.origin, [20., 25., z]);
            assert_eq!(m.frame.x_axis, [1., 0., 0.]);
            assert_eq!(m.frame.z_axis, [0., 0., outward]);
            assert_eq!(m.frame.y_axis, [0., outward, 0.]);
            assert_eq!(m.centerline_endpoints, [[16., 25., z], [24., 25., z]]);
            assert_eq!(m.radius_mm, 3.);
            assert_eq!(m.centerline_length_mm, 8.);
            assert!(m.arithmetic_uncertainty_mm < t.linear / 16.);
        }
    }

    #[test]
    fn rigid_pose_preserves_native_cap_slot_frame() {
        let t = Tolerance::default();
        let (model, walls, _) = fixture(t);
        let q = [0.2, 0.3, 0.4, 0.71_f64.sqrt()];
        let translation = [41., -27., 12.];
        let placed = transform(&model, translation, q, t).unwrap().model;
        for cap in [FaceId(0), FaceId(1)] {
            let local = slot_center_frame(&model, walls, cap, t).unwrap();
            let world = slot_center_frame(&placed, walls, cap, t).unwrap();
            let expected = transform_point(local.frame.origin, translation, q);
            assert!(norm(sub(world.frame.origin, expected)) < 1e-8);
            assert!((dot(world.frame.x_axis, world.frame.z_axis)).abs() < 1e-12);
            assert!(
                (norm(sub(
                    world.centerline_endpoints[1],
                    world.centerline_endpoints[0]
                )) - 8.)
                    .abs()
                    < 1e-8
            );
            assert!(world.arithmetic_uncertainty_mm < t.linear / 16.);
        }
    }

    fn transform_point(p: [f64; 3], translation: [f64; 3], q: [f64; 4]) -> [f64; 3] {
        let v = [q[0], q[1], q[2]];
        let uv = cross(v, p);
        let uuv = cross(v, uv);
        std::array::from_fn(|i| p[i] + 2. * (q[3] * uv[i] + uuv[i]) + translation[i])
    }

    #[test]
    fn rejects_mixed_partial_duplicate_wall_and_foreign_cap() {
        let t = Tolerance::default();
        let (model, left, right) = fixture(t);
        for faces in [
            [left[0], left[1], left[2], right[3]],
            [left[0], left[1], left[2], left[2]],
            [left[0], left[1], left[2], FaceId(2)],
        ] {
            assert!(slot_center_frame(&model, faces, FaceId(0), t).is_err());
        }
        assert!(slot_center_frame(&model, left, FaceId(2), t).is_err());
        assert!(slot_center_frame(&model, left, FaceId(usize::MAX), t).is_err());
    }

    #[test]
    fn rejects_corrupted_native_wall_or_cap_support() {
        let t = Tolerance::default();
        let (model, walls, _) = fixture(t);
        let mut broken = model.clone();
        if let Surface::Cylinder(c) = &mut broken.faces[walls[1].0].surface {
            c.radius += 0.001;
        }
        assert!(slot_center_frame(&broken, walls, FaceId(0), t).is_err());
        let mut broken = model.clone();
        if let Surface::Plane(p) = &mut broken.faces[1].surface {
            p.origin[2] += 0.001;
        }
        assert!(slot_center_frame(&broken, walls, FaceId(1), t).is_err());
    }
}
