//! Bounded blind-hole patterns attached to one actual native prism cap.
use crate::math::{finite, norm, sub};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CAP_BLIND_HOLES: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapBlindHole {
    pub id: String,
    /// Millimetres in the selected cap's frozen X/Y frame.
    pub center: [f64; 2],
    pub radius: f64,
    pub depth: f64,
}

fn unsupported() -> KernelError {
    KernelError::Unsupported(
        "blind-hole pattern requires a canonical prism cap, 1..=16 named separated contained circular holes, and positive remaining floor thickness".into(),
    )
}

fn composed(
    faces: &mut BTreeMap<FaceId, Vec<FaceId>>,
    left: &BTreeMap<FaceId, Vec<FaceId>>,
) -> Result<(), KernelError> {
    for destinations in faces.values_mut() {
        let mut mapped = BTreeSet::new();
        for face in destinations.iter() {
            mapped.extend(left.get(face).ok_or_else(unsupported)?.iter().copied());
        }
        *destinations = mapped.into_iter().collect();
    }
    Ok(())
}

fn composed_roles(
    roles: &mut BTreeMap<String, Vec<FaceId>>,
    left: &BTreeMap<FaceId, Vec<FaceId>>,
) -> Result<(), KernelError> {
    for destinations in roles.values_mut() {
        let mut mapped = BTreeSet::new();
        for face in destinations.iter() {
            mapped.extend(left.get(face).ok_or_else(unsupported)?.iter().copied());
        }
        *destinations = mapped.into_iter().collect();
    }
    Ok(())
}

/// Drill one pattern into the actual selected start/end cap of a canonical
/// right prism. `origin_offset` and hole centers are tangent-plane millimetres;
/// no normal offset or modeling-tolerance snapping is applied. The frame is
/// evaluated once before drilling, so earlier holes cannot move later centers.
/// Every cutter is checked by the native blind-bore operation, including its
/// entire opening, floor, lateral material clearance, and exact boundary.
pub fn drill_cap_pattern(
    body: &ModelSnapshot,
    cap: FaceId,
    x_direction_hint: [f64; 3],
    origin_offset: [f64; 2],
    holes: &[CapBlindHole],
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    if !(1..=MAX_CAP_BLIND_HOLES).contains(&holes.len()) || cap.0 > 1 {
        return Err(unsupported());
    }
    let frame = body.face_frame(
        cap,
        x_direction_hint,
        [origin_offset[0], origin_offset[1], 0.],
        tolerance,
    )?;
    let (stock_axis, stock_height) = if let Ok(stock) = crate::through_bore::stock(body, tolerance)
    {
        (stock.axis, stock.height)
    } else {
        let recognized = crate::bored_prism::recognize(body, tolerance)?;
        (recognized.axis, recognized.height)
    };
    let outward = if cap.0 == 0 {
        stock_axis.map(|value| -value)
    } else {
        stock_axis
    };
    if norm(sub(frame.z_axis, outward)) > 128. * f64::EPSILON {
        return Err(unsupported());
    }
    let inward = frame.z_axis.map(|value| -value);
    let mut ids = BTreeSet::new();
    let mut centers = Vec::with_capacity(holes.len());
    for hole in holes {
        if hole.id.is_empty()
            || hole.id.len() > 64
            || !hole
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || !ids.insert(hole.id.as_str())
            || !hole.center.into_iter().all(f64::is_finite)
            || !hole.radius.is_finite()
            || !hole.depth.is_finite()
        {
            return Err(unsupported());
        }
        let scale = frame
            .origin
            .into_iter()
            .chain(hole.center)
            .chain([hole.radius, hole.depth, stock_height])
            .map(f64::abs)
            .fold(0., f64::max);
        let error = scale * 4096. * f64::EPSILON;
        if !error.is_finite() || error > tolerance.linear / 16. {
            return Err(KernelError::NumericRange);
        }
        if hole.radius <= tolerance.linear + error {
            return Err(KernelError::InvalidDimension {
                dimension: "blind-hole radius",
            });
        }
        if hole.depth <= tolerance.linear + error {
            return Err(KernelError::InvalidDimension {
                dimension: "blind-hole depth",
            });
        }
        if stock_height - hole.depth <= tolerance.linear + error {
            return Err(KernelError::Unsupported(format!(
                "blind hole '{}' leaves insufficient floor thickness",
                hole.id
            )));
        }
        let center = std::array::from_fn(|axis| {
            frame.origin[axis]
                + hole.center[0] * frame.x_axis[axis]
                + hole.center[1] * frame.y_axis[axis]
        });
        if !finite(center) {
            return Err(KernelError::NumericRange);
        }
        centers.push(center);
    }
    // A pair of new cuts may not touch even when their circular footprints
    // could individually fit inside the stock. Existing holes are checked by
    // bored_prism::cut below against the complete native material.
    for (i, a) in holes.iter().enumerate() {
        for b in &holes[..i] {
            let separation = (a.center[0] - b.center[0]).hypot(a.center[1] - b.center[1]);
            let scale = a
                .center
                .into_iter()
                .chain(b.center)
                .chain([a.radius, b.radius])
                .map(f64::abs)
                .fold(0., f64::max);
            let margin = tolerance.linear + scale * 4096. * f64::EPSILON;
            if !separation.is_finite() || separation <= a.radius + b.radius + margin {
                return Err(unsupported());
            }
        }
    }

    let mut current = body.clone();
    let mut lineage: BTreeMap<FaceId, Vec<FaceId>> = (0..body.faces.len())
        .map(|index| (FaceId(index), vec![FaceId(index)]))
        .collect();
    let mut roles = BTreeMap::new();
    for (hole, center) in holes.iter().zip(centers) {
        let cylinder = extrude_circle(hole.radius, hole.depth, tolerance)?.model;
        let cutter = transform(
            &cylinder,
            center,
            crate::circular_boolean::quaternion(frame.x_axis, inward),
            tolerance,
        )?
        .model;
        let result = crate::bored_prism::cut(&current, &cutter, tolerance)?;
        let left = result
            .history
            .input_faces
            .get("left")
            .ok_or_else(unsupported)?;
        composed(&mut lineage, left)?;
        composed_roles(&mut roles, left)?;
        if roles.values().any(|faces| faces.len() != 1) {
            return Err(unsupported());
        }
        for (suffix, source) in [("wall", "bore_wall"), ("floor", "bore_floor")] {
            let faces = result
                .history
                .generated_faces
                .get(source)
                .filter(|faces| faces.len() == 1)
                .ok_or_else(unsupported)?;
            roles.insert(format!("hole_{}_{}", hole.id, suffix), faces.clone());
        }
        current = result.model;
    }
    roles.insert(
        "boundary".into(),
        (0..current.faces.len()).map(FaceId).collect(),
    );
    Ok(OperationResult {
        model: current,
        history: TopologyHistory {
            generated_faces: roles,
            input_faces: BTreeMap::from([("body".into(), lineage)]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holes() -> Vec<CapBlindHole> {
        vec![
            CapBlindHole {
                id: "left".into(),
                center: [-10., 0.],
                radius: 2.,
                depth: 5.,
            },
            CapBlindHole {
                id: "right".into(),
                center: [10., 0.],
                radius: 2.,
                depth: 5.,
            },
        ]
    }

    fn plate() -> ModelSnapshot {
        extrude_rectangle(40., 30., 12., Tolerance::default())
            .unwrap()
            .model
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }

    #[test]
    fn top_bottom_and_reordered_pattern_have_actual_floors_and_composed_lineage() {
        let t = Tolerance::default();
        let base = plate();
        let expected = base.volume() - 2. * std::f64::consts::PI * 2f64.powi(2) * 5.;
        for (cap, floor_z) in [(FaceId(1), 7.), (FaceId(0), 5.)] {
            let mut pattern = holes();
            for _ in 0..2 {
                let out =
                    drill_cap_pattern(&base, cap, [1., 0., 0.], [0., 0.], &pattern, t).unwrap();
                assert!(out.model.validate(t).valid);
                close(out.model.volume(), expected);
                for hole in ["left", "right"] {
                    let wall = &out.history.generated_faces[&format!("hole_{hole}_wall")];
                    let floor = &out.history.generated_faces[&format!("hole_{hole}_floor")];
                    assert_eq!((wall.len(), floor.len()), (1, 1));
                    let plane = out.model.faces[floor[0].0].try_plane().unwrap();
                    close(plane.origin[2], floor_z);
                }
                let lineage = &out.history.input_faces["body"];
                assert_eq!(lineage.len(), base.faces.len());
                for index in 0..base.faces.len() {
                    assert_eq!(lineage[&FaceId(index)], vec![FaceId(index)]);
                }
                pattern.reverse();
            }
        }
    }

    #[test]
    fn through_bored_and_posed_stock_accept_same_named_pattern() {
        let t = Tolerance::default();
        let base = plate();
        let tool = transform(
            &extrude_circle(4., 12., t).unwrap().model,
            [20., 15., 0.],
            [0., 0., 0., 1.],
            t,
        )
        .unwrap()
        .model;
        let through = boolean(&base, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let local =
            drill_cap_pattern(&through, FaceId(1), [1., 0., 0.], [0., 0.], &holes(), t).unwrap();
        let mut overlaps_through = holes();
        overlaps_through[0].center = [-5., 0.];
        assert!(drill_cap_pattern(
            &through,
            FaceId(1),
            [1., 0., 0.],
            [0., 0.],
            &overlaps_through,
            t
        )
        .is_err());
        close(
            local.model.volume(),
            40. * 30. * 12.
                - std::f64::consts::PI * 4f64.powi(2) * 12.
                - 2. * std::f64::consts::PI * 2f64.powi(2) * 5.,
        );
        let quaternion = [0.5, 0.5, 0.5, 0.5];
        let moved = transform(&through, [7., -3., 11.], quaternion, t)
            .unwrap()
            .model;
        let posed =
            drill_cap_pattern(&moved, FaceId(1), [0., 1., 0.], [0., 0.], &holes(), t).unwrap();
        assert!(posed.model.validate(t).valid);
        close(posed.model.volume(), local.model.volume());
        assert_eq!(posed.history.input_faces["body"].len(), moved.faces.len());
        for hole in ["left", "right"] {
            assert_eq!(
                posed.history.generated_faces[&format!("hole_{hole}_floor")].len(),
                1
            );
        }
    }

    #[test]
    fn invalid_caps_contacts_breakthrough_and_names_fail_closed() {
        let t = Tolerance::default();
        let base = plate();
        let one = vec![holes()[0].clone()];
        for cap in [FaceId(2), FaceId(999)] {
            assert!(drill_cap_pattern(&base, cap, [1., 0., 0.], [0., 0.], &one, t).is_err());
        }
        assert!(drill_cap_pattern(&base, FaceId(1), [0., 0., 1.], [0., 0.], &one, t).is_err());
        assert!(drill_cap_pattern(&base, FaceId(1), [1., 0., 0.], [0., 0.], &[], t).is_err());
        let mut cases = vec![];
        let mut contact = one.clone();
        contact[0].center = [18., 0.];
        cases.push(contact);
        let mut breakthrough = one.clone();
        breakthrough[0].depth = 12.;
        cases.push(breakthrough);
        let mut near_breakthrough = one.clone();
        near_breakthrough[0].depth = 12. - t.linear / 2.;
        cases.push(near_breakthrough);
        assert!(matches!(
            drill_cap_pattern(&base, FaceId(1), [1., 0., 0.], [0., 0.], &cases[1], t),
            Err(KernelError::Unsupported(message)) if message.contains("floor thickness")
        ));
        let mut overlap = holes();
        overlap[1].center = [-6., 0.];
        cases.push(overlap);
        let mut duplicate = holes();
        duplicate[1].id = duplicate[0].id.clone();
        cases.push(duplicate);
        let mut nonfinite = one.clone();
        nonfinite[0].radius = f64::NAN;
        cases.push(nonfinite);
        cases.push(
            (0..MAX_CAP_BLIND_HOLES + 1)
                .map(|index| CapBlindHole {
                    id: format!("h{index}"),
                    center: [index as f64, 0.],
                    radius: 1.,
                    depth: 2.,
                })
                .collect(),
        );
        for pattern in cases {
            assert!(
                drill_cap_pattern(&base, FaceId(1), [1., 0., 0.], [0., 0.], &pattern, t).is_err(),
                "{pattern:?}"
            );
        }
    }
}
