//! Bounded full-height obround through slots in canonical convex prisms.
//! The source sketch frame is resolved by the build layer; this operation uses
//! the strictly recognized stock frame and preserves its native supports.
use crate::arc_prism::{ArcPrismGeometry, PathSegment, PlanarPath, SlotHole};
use crate::math::cross;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObroundSlot {
    pub id: String,
    pub center: [f64; 2],
    pub direction: [f64; 2],
    pub centerline_length: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotStockFrame {
    pub origin: [f64; 3],
    pub axis: [f64; 3],
    pub x_direction: [f64; 3],
    pub y_direction: [f64; 3],
    pub height: f64,
}

fn bad() -> KernelError {
    KernelError::Unsupported("obround slots require a canonical convex right prism, complete contained capsules, and strict separation from all other boundaries".into())
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub2(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}
fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn ends(s: &SlotHole) -> ([f64; 2], [f64; 2]) {
    let delta = mul(s.direction, s.centerline_length / 2.);
    (sub2(s.center, delta), add(s.center, delta))
}
fn point_segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = sub2(b, a);
    let t = (dot2(sub2(p, a), d) / dot2(d, d)).clamp(0., 1.);
    length(sub2(p, add(a, mul(d, t))))
}
fn segment_distance(a: ([f64; 2], [f64; 2]), b: ([f64; 2], [f64; 2])) -> f64 {
    let side = |p, q, r| cross2(sub2(q, p), sub2(r, p));
    let s = [
        side(a.0, a.1, b.0),
        side(a.0, a.1, b.1),
        side(b.0, b.1, a.0),
        side(b.0, b.1, a.1),
    ];
    if s[0].signum() != s[1].signum() && s[2].signum() != s[3].signum() {
        return 0.;
    }
    point_segment_distance(a.0, b.0, b.1)
        .min(point_segment_distance(a.1, b.0, b.1))
        .min(point_segment_distance(b.0, a.0, a.1))
        .min(point_segment_distance(b.1, a.0, a.1))
}

pub(crate) fn validate_slots(
    path: &PlanarPath,
    circles: &[CircleHole],
    slots: &[SlotHole],
    t: Tolerance,
) -> Result<(), KernelError> {
    if slots.len() > 4 {
        return Err(KernelError::ResourceLimit(
            "at most four obround slots".into(),
        ));
    }
    if slots.is_empty() {
        return Ok(());
    }
    // A CCW arc bulges outside its chord. A clockwise arc may intrude into
    // the chord polygon, invalidating that polygon as an inner material bound.
    if path.segments.iter().any(|s| {
        matches!(
            s,
            PathSegment::ArcTo {
                clockwise: true,
                ..
            }
        )
    }) {
        return Err(bad());
    }
    let polygon: Vec<_> = std::iter::once(path.start)
        .chain(
            path.segments
                .iter()
                .take(path.segments.len() - 1)
                .map(|s| match s {
                    PathSegment::LineTo { end } | PathSegment::ArcTo { end, .. } => *end,
                }),
        )
        .collect();
    validate_polygon(&polygon, t).map_err(|_| bad())?;
    let scale = polygon
        .iter()
        .chain(slots.iter().map(|s| &s.center))
        .flat_map(|p| *p)
        .chain(slots.iter().flat_map(|s| [s.radius, s.centerline_length]))
        .chain(
            circles
                .iter()
                .flat_map(|c| [c.center[0], c.center[1], c.radius]),
        )
        .map(f64::abs)
        .fold(0., f64::max);
    let error = scale * 8192. * f64::EPSILON;
    if !error.is_finite() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let guard = t.linear + error;
    for (i, s) in slots.iter().enumerate() {
        if !s
            .center
            .iter()
            .chain(s.direction.iter())
            .all(|x| x.is_finite())
            || !s.radius.is_normal()
            || s.radius <= guard
            || !s.centerline_length.is_normal()
            || s.centerline_length <= guard
            || (length(s.direction) - 1.).abs() > 128. * f64::EPSILON
        {
            return Err(bad());
        }
        let (a, b) = ends(s);
        if !a.iter().chain(b.iter()).all(|x| x.is_finite()) {
            return Err(KernelError::NumericRange);
        }
        for j in 0..polygon.len() {
            let p = polygon[j];
            let q = polygon[(j + 1) % polygon.len()];
            let edge = sub2(q, p);
            let norm = length(edge);
            let distance = cross2(edge, sub2(s.center, p)) / norm;
            let span = (s.centerline_length / 2.) * cross2(edge, s.direction).abs() / norm;
            if distance - span - s.radius <= guard {
                return Err(bad());
            }
        }
        for c in circles {
            if point_segment_distance(c.center, a, b) - c.radius - s.radius <= guard {
                return Err(bad());
            }
        }
        for other in &slots[..i] {
            if segment_distance((a, b), ends(other)) - s.radius - other.radius <= guard {
                return Err(bad());
            }
        }
    }
    Ok(())
}

fn geometry(body: &ModelSnapshot, t: Tolerance) -> Result<(ArcPrismGeometry, usize), KernelError> {
    let stock = crate::through_bore::stock(body, t)?;
    let (path, circles) = match stock.profile {
        crate::through_bore::Profile::Arc(g) => {
            if !g.slot_holes.is_empty() {
                return Err(bad());
            }
            (g.path, g.circular_holes)
        }
        crate::through_bore::Profile::Polygon { rings, circles } => {
            if rings.len() != 1 {
                return Err(bad());
            }
            let points = &rings[0];
            let path = PlanarPath {
                start: points[0],
                segments: (1..=points.len())
                    .map(|i| PathSegment::LineTo {
                        end: points[i % points.len()],
                    })
                    .collect(),
            };
            (path, circles)
        }
    };
    validate_path(&path, &circles, t)?;
    let n = path.segments.len();
    if body.faces.len() != n + 2 + circles.len() {
        return Err(bad());
    }
    Ok((
        ArcPrismGeometry {
            path,
            circular_holes: circles,
            slot_holes: vec![],
            origin: stock.origin,
            axis: stock.axis,
            x_direction: stock.x,
            height: stock.height,
        },
        n,
    ))
}

/// Canonical stock frame used by the native slot operation. Resolve an authored
/// sketch frame against this before passing the local capsule coordinates.
pub fn obround_slot_frame(
    body: &ModelSnapshot,
    t: Tolerance,
) -> Result<SlotStockFrame, KernelError> {
    let (g, _) = geometry(body, t)?;
    Ok(SlotStockFrame {
        origin: g.origin,
        axis: g.axis,
        x_direction: g.x_direction,
        y_direction: cross(g.axis, g.x_direction),
        height: g.height,
    })
}

pub fn cut_obround_slots(
    body: &ModelSnapshot,
    slots: &[ObroundSlot],
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    if slots.is_empty() {
        return Err(bad());
    }
    if slots.len() > 4 {
        return Err(KernelError::ResourceLimit(
            "at most four obround slots".into(),
        ));
    }
    let (mut g, n) = geometry(body, t)?;
    let mut ids = BTreeSet::new();
    for s in slots {
        if s.id.is_empty()
            || s.id.len() > 64
            || !s.id.as_bytes()[0].is_ascii_alphabetic()
            || !s
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !ids.insert(s.id.to_ascii_lowercase())
        {
            return Err(bad());
        }
        g.slot_holes.push(SlotHole {
            center: s.center,
            direction: s.direction,
            centerline_length: s.centerline_length,
            radius: s.radius,
        });
    }
    validate_slots(&g.path, &g.circular_holes, &g.slot_holes, t)?;
    g.properties(t)?;
    let model = crate::arc_prism::construct(&g, t)?;
    let report = model.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    model.mass_properties(t)?;
    let face_base = n + 2 + g.circular_holes.len();
    let mut roles = BTreeMap::from([
        ("start_cap".into(), vec![FaceId(0)]),
        ("end_cap".into(), vec![FaceId(1)]),
        (
            "boundary".into(),
            (0..model.faces.len()).map(FaceId).collect(),
        ),
    ]);
    for i in 0..n {
        roles.insert(format!("segment_{i}"), vec![FaceId(i + 2)]);
    }
    for i in 0..g.circular_holes.len() {
        roles.insert(format!("circular_hole_{i}_wall"), vec![FaceId(n + 2 + i)]);
    }
    for (i, s) in slots.iter().enumerate() {
        let walls = (0..4)
            .map(|j| FaceId(face_base + 4 * i + j))
            .collect::<Vec<_>>();
        roles.insert(format!("slot_{}_walls", s.id), walls.clone());
        for (j, face) in walls.into_iter().enumerate() {
            roles.insert(format!("slot_{}_wall_{j}", s.id), vec![face]);
        }
    }
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: roles,
            input_faces: BTreeMap::from([(
                "body".into(),
                (0..body.faces.len())
                    .map(|i| (FaceId(i), vec![FaceId(i)]))
                    .collect(),
            )]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn slot(id: &str, c: [f64; 2], l: f64, r: f64) -> ObroundSlot {
        ObroundSlot {
            id: id.into(),
            center: c,
            direction: [1., 0.],
            centerline_length: l,
            radius: r,
        }
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() <= 2e-9 * b.abs().max(1.), "{a} != {b}");
    }
    #[test]
    fn capsule_cut_has_exact_volume_centroid_inertia_and_native_mesh() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(80., 50., 8., t).unwrap().model;
        let frame = obround_slot_frame(&stock, t).unwrap();
        assert_eq!(frame.height, 8.);
        let out = cut_obround_slots(&stock, &[slot("mount", [40., 25.], 20., 5.)], t).unwrap();
        assert!(out.model.validate(t).valid);
        assert_eq!(out.history.generated_faces["slot_mount_walls"].len(), 4);
        for i in 0..stock.faces.len() {
            assert_eq!(out.history.input_faces["body"][&FaceId(i)], vec![FaceId(i)]);
        }
        let p = out.model.mass_properties(t).unwrap();
        let a = 200. + 25. * std::f64::consts::PI;
        near(p.volume, (4000. - a) * 8.);
        near(out.model.volume(), p.volume);
        near(p.centroid.unwrap()[0], 40.);
        near(p.centroid.unwrap()[1], 25.);
        near(p.centroid.unwrap()[2], 4.);
        let ix = 80. * 50f64.powi(3) / 12.
            - (2. * 20. * 5f64.powi(3) / 3. + std::f64::consts::PI * 5f64.powi(4) / 4.);
        let iy = 50. * 80f64.powi(3) / 12.
            - (5. * 20f64.powi(3) / 6.
                + std::f64::consts::PI * 25. * 400. / 4.
                + 4. * 20. * 125. / 3.
                + std::f64::consts::PI * 625. / 4.);
        let inertia = p.central_inertia.unwrap();
        near(inertia[0][0], 8. * ix + p.volume * 8f64.powi(2) / 12.);
        near(inertia[1][1], 8. * iy + p.volume * 8f64.powi(2) / 12.);
        near(inertia[2][2], 8. * (ix + iy));
        let mesh = tessellate_analytic(&out.model, t, TessellationPolicy::default()).unwrap();
        assert!(mesh.chordal_error_bound <= TessellationPolicy::default().chordal_error);
        assert!(mesh
            .patches
            .iter()
            .any(|p| p.face == out.history.generated_faces["slot_mount_wall_1"][0]));
        let section = out
            .model
            .section(
                SectionPlane {
                    origin: [0., 0., 4.],
                    normal: [0., 0., 1.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert_eq!(section.loops.len(), 2);
        near(section.area, 4000. - a);
        assert_eq!(section.loops[1].segments.len(), 4);
    }
    #[test]
    fn four_slots_and_boundary_failures() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(100., 60., 10., t).unwrap().model;
        let s = [
            slot("A", [20., 15.], 10., 3.),
            slot("B", [80., 15.], 10., 3.),
            slot("C", [20., 45.], 10., 3.),
            slot("D", [80., 45.], 10., 3.),
        ];
        let model = cut_obround_slots(&stock, &s, t).unwrap().model;
        assert_eq!(model.faces.len(), stock.faces.len() + 16);
        assert!(model.validate(t).valid);
        assert!(cut_obround_slots(&model, &[slot("E", [50., 30.], 10., 3.)], t).is_err());
        assert!(crate::through_bore::stock(&model, t).is_err());
        assert!(model.validate_convex_clearance_operand(t).is_err());
        for invalid in [
            slot("edge", [7.99999, 15.], 10., 3.),
            slot("unresolved_edge", [8. + t.linear / 2., 15.], 10., 3.),
            slot("outside", [0., 15.], 10., 3.),
            slot("bad", [20., 15.], 0., 3.),
            ObroundSlot {
                direction: [2., 0.],
                ..slot("tilted", [20., 15.], 10., 3.)
            },
        ] {
            assert!(cut_obround_slots(&stock, &[invalid], t).is_err());
        }
        assert!(cut_obround_slots(&stock, &[s[0].clone(), s[0].clone()], t).is_err());
        assert!(cut_obround_slots(
            &stock,
            &[s[0].clone(), slot("overlap", [27., 15.], 10., 3.)],
            t
        )
        .is_err());
        let mut corrupt = model.clone();
        if let Surface::Cylinder(c) = &mut corrupt.faces[stock.faces.len() + 1].surface {
            c.radius += 0.1;
        }
        assert!(!corrupt.validate(t).valid);
        let mut corrupt = model.clone();
        if let Curve::Line(line) = &mut corrupt.edges[3 * 4].curve {
            line.direction = [0., 1., 0.];
        }
        assert!(!corrupt.validate(t).valid);
        let mut corrupt = model.clone();
        if let Surface::Plane(plane) = &mut corrupt.faces[stock.faces.len()].surface {
            plane.normal = plane.normal.map(|v| -v);
        }
        assert!(!corrupt.validate(t).valid);
    }
    #[test]
    fn rounded_stock_and_rigid_pose_preserve_exact_slot_shape() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(80., 50., 8., t).unwrap().model;
        let rounded = fillet_longitudinal(&stock, [FaceId(2), FaceId(3)], 3., t)
            .unwrap()
            .model;
        let frame = obround_slot_frame(&rounded, t).unwrap();
        let world = [40., 25., 0.];
        let delta = crate::math::sub(world, frame.origin);
        let local = [
            crate::math::dot(delta, frame.x_direction),
            crate::math::dot(delta, frame.y_direction),
        ];
        let out = cut_obround_slots(&rounded, &[slot("mount", local, 20., 5.)], t).unwrap();
        assert!(out.model.validate(t).valid);
        assert!(out
            .model
            .faces
            .iter()
            .any(|f| matches!(f.surface, Surface::Cylinder(_))));
        let section = out
            .model
            .section(
                SectionPlane {
                    origin: [0., 0., 4.],
                    normal: [0., 0., 1.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert_eq!(section.loops.len(), 2);
        let quaternion = [0.5, 0.5, 0.5, 0.5];
        let posed = transform(&rounded, [100., -50., 30.], quaternion, t)
            .unwrap()
            .model;
        let posed_frame = obround_slot_frame(&posed, t).unwrap();
        near(posed_frame.height, frame.height);
        let result = cut_obround_slots(&posed, &[slot("mount", local, 20., 5.)], t)
            .unwrap()
            .model;
        assert!(result.validate(t).valid);
        near(
            result.mass_properties(t).unwrap().volume,
            out.model.mass_properties(t).unwrap().volume,
        );
    }
    #[test]
    fn concave_arc_cannot_make_chord_polygon_false_positive() {
        let t = Tolerance::default();
        let path = PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [40., 0.] },
                PathSegment::LineTo { end: [40., 30.] },
                PathSegment::ArcTo {
                    center: [20., 50.],
                    end: [0., 30.],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        };
        let stock = extrude_path(&path, &[], 8., t).unwrap().model;
        assert!(stock.validate(t).valid);
        let frame = obround_slot_frame(&stock, t).unwrap();
        let delta = crate::math::sub([20., 27., 0.], frame.origin);
        let center = [
            crate::math::dot(delta, frame.x_direction),
            crate::math::dot(delta, frame.y_direction),
        ];
        assert!(cut_obround_slots(&stock, &[slot("in_notch", center, 2., 1.)], t).is_err());
    }
    #[test]
    fn four_corner_rounded_plate_admits_four_separated_slots() {
        let t = Tolerance::default();
        let mut body = extrude_rectangle(80., 50., 8., t).unwrap().model;
        let mut surviving = [FaceId(2), FaceId(3), FaceId(4), FaceId(5)];
        for (left, right) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
            let out =
                fillet_longitudinal(&body, [surviving[left], surviving[right]], 3., t).unwrap();
            for face in &mut surviving {
                *face = out.history.input_faces["body"][face][0];
            }
            body = out.model;
        }
        assert_eq!(body.faces.len(), 10);
        let frame = obround_slot_frame(&body, t).unwrap();
        let centers = [[15., 12.], [65., 12.], [15., 38.], [65., 38.]];
        let slots = centers
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let delta = crate::math::sub([c[0], c[1], 0.], frame.origin);
                slot(
                    &format!("mount_{i}"),
                    [
                        crate::math::dot(delta, frame.x_direction),
                        crate::math::dot(delta, frame.y_direction),
                    ],
                    8.,
                    2.,
                )
            })
            .collect::<Vec<_>>();
        let out = cut_obround_slots(&body, &slots, t).unwrap();
        assert_eq!(out.model.faces.len(), 26);
        assert!(out.model.validate(t).valid);
        let expected_area = 80. * 50.
            - 4. * (3. * 3. - std::f64::consts::PI * 3. * 3. / 4.)
            - 4. * (2. * 2. * 8. + std::f64::consts::PI * 2. * 2.);
        near(
            out.model.mass_properties(t).unwrap().volume,
            expected_area * 8.,
        );
    }
}
