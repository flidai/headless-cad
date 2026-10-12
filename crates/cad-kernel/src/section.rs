//! Analytic plane sections. No display mesh participates in this query.
use crate::math::{finite, norm};
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionPlane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub x_direction: [f64; 3],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarSection {
    pub plane: SectionPlane,
    pub loops: Vec<SectionLoop>,
    pub area: f64,
    pub centroid: Option<[f64; 2]>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionLoop {
    pub segments: Vec<SectionSegment>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionSegment {
    pub face: FaceId,
    pub curve: SectionCurve,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SectionCurve {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    },
}
fn unsupported(reason: &str) -> KernelError {
    KernelError::Unsupported(format!("planar section: {reason}"))
}
impl SectionPlane {
    /// Frames must already be finite, unit length and orthogonal. They are not normalized.
    pub fn validate(self) -> Result<(), KernelError> {
        if !finite(self.origin)
            || !finite(self.normal)
            || !finite(self.x_direction)
            || (norm(self.normal) - 1.).abs() > 128. * f64::EPSILON
            || (norm(self.x_direction) - 1.).abs() > 128. * f64::EPSILON
            || dot(self.normal, self.x_direction).abs() > 128. * f64::EPSILON
        {
            return Err(KernelError::InvalidFaceFrame(
                "section requires finite unit orthogonal frame".into(),
            ));
        }
        Ok(())
    }
    fn project(self, p: [f64; 3]) -> [f64; 2] {
        let d = sub(p, self.origin);
        [
            dot(d, self.x_direction),
            dot(d, cross(self.normal, self.x_direction)),
        ]
    }
}
struct Piece {
    start: EdgeId,
    end: EdgeId,
    segment: SectionSegment,
}
impl ModelSnapshot {
    /// Exact lines and circular arcs in millimetres, directed with material on
    /// their left (outer loops CCW, holes CW). Event planes within tolerance of
    /// native vertices or coplanar boundary edges are rejected. Curved walls
    /// require a section normal parallel to their native axis.
    pub fn section(
        &self,
        plane: SectionPlane,
        tolerance: Tolerance,
    ) -> Result<PlanarSection, KernelError> {
        plane.validate()?;
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        let validation = self.validate(tolerance);
        if !validation.valid {
            return Err(KernelError::InvalidInput(validation));
        }
        let empty = || PlanarSection {
            plane,
            loops: vec![],
            area: 0.,
            centroid: None,
        };
        if self.faces.is_empty() {
            return Ok(empty());
        }
        let bounds = self.bounds();
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for mask in 0..8 {
            let p = std::array::from_fn(|i| {
                if mask & (1 << i) == 0 {
                    bounds.min[i]
                } else {
                    bounds.max[i]
                }
            });
            let d = dot(sub(p, plane.origin), plane.normal);
            if !d.is_finite() {
                return Err(KernelError::NumericRange);
            }
            min = min.min(d);
            max = max.max(d);
        }
        if min > tolerance.linear || max < -tolerance.linear {
            return Ok(empty());
        }
        // Drawing coordinates must retain sub-tolerance resolution. In particular,
        // a full circle's area can look correct even when a distant drawing origin
        // makes all of its displayed endpoints round to the same coordinate.
        for mask in 0..8 {
            let corner = std::array::from_fn(|i| {
                if mask & (1 << i) == 0 {
                    bounds.min[i]
                } else {
                    bounds.max[i]
                }
            });
            let p = plane.project(corner);
            if p.iter()
                .any(|v| !v.is_finite() || v.abs() * (2. * f64::EPSILON) > tolerance.linear)
            {
                return Err(KernelError::NumericRange);
            }
        }
        let distances: Vec<_> = self
            .vertices
            .iter()
            .map(|v| dot(sub(v.position, plane.origin), plane.normal))
            .collect();
        if distances.iter().any(|d| !d.is_finite()) {
            return Err(KernelError::NumericRange);
        }
        if distances.iter().any(|d| d.abs() <= tolerance.linear) {
            return Err(unsupported("topology-event plane touches a native vertex"));
        }
        let mut hits = BTreeMap::new();
        for (i, e) in self.edges.iter().enumerate() {
            let a = distances[e.vertices[0].0];
            let b = distances[e.vertices[1].0];
            match &e.curve {
                Curve::Line(_) if (a < 0.) != (b < 0.) => {
                    let ratio = a / (a - b);
                    if !ratio.is_finite() || ratio <= 0. || ratio >= 1. {
                        return Err(KernelError::NumericRange);
                    }
                    let p = self.vertices[e.vertices[0].0].position;
                    let q = self.vertices[e.vertices[1].0].position;
                    let point = std::array::from_fn(|j| p[j] + ratio * (q[j] - p[j]));
                    if !finite(point) {
                        return Err(KernelError::NumericRange);
                    }
                    hits.insert(EdgeId(i), point);
                }
                Curve::Line(_) => {}
                Curve::Circle(c) | Curve::CircularArc(CircularArc { circle: c, .. }) => {
                    if norm(cross(c.axis, plane.normal)) > 128. * f64::EPSILON {
                        return Err(unsupported(
                            "oblique curved intersections are not supported",
                        ));
                    }
                    if dot(sub(c.origin, plane.origin), plane.normal).abs() <= tolerance.linear {
                        return Err(unsupported(
                            "topology-event plane contains a circular boundary",
                        ));
                    }
                }
            }
        }
        let mut pieces = vec![];
        let mut complete = vec![];
        for (fi, face) in self.faces.iter().enumerate() {
            let edges: BTreeSet<_> = face
                .loops
                .iter()
                .flat_map(|l| self.loops[l.0].edges.iter().map(|e| e.edge))
                .collect();
            let mut intersections: Vec<_> = edges
                .iter()
                .filter_map(|e| hits.get(e).map(|p| (*e, *p)))
                .collect();
            match &face.surface {
                Surface::Plane(support) => {
                    if intersections.is_empty() {
                        continue;
                    }
                    let tangent = cross(plane.normal, support.normal);
                    if norm(tangent) <= 128. * f64::EPSILON {
                        return Err(unsupported("coplanar face"));
                    }
                    let reference = intersections[0].1;
                    intersections.sort_by(|a, b| {
                        dot(sub(a.1, reference), tangent)
                            .total_cmp(&dot(sub(b.1, reference), tangent))
                    });
                    if intersections.len() % 2 != 0 {
                        return Err(unsupported("unresolved planar face incidence"));
                    }
                    for pair in intersections.as_chunks::<2>().0 {
                        pieces.push(Piece {
                            start: pair[0].0,
                            end: pair[1].0,
                            segment: SectionSegment {
                                face: FaceId(fi),
                                curve: SectionCurve::Line {
                                    start: plane.project(pair[0].1),
                                    end: plane.project(pair[1].1),
                                },
                            },
                        });
                    }
                }
                Surface::Cylinder(_) | Surface::Cone(_) => {
                    let (origin, axis, radius, slope, inward) = match &face.surface {
                        Surface::Cylinder(c) => (c.origin, c.axis, c.radius, 0., c.inward),
                        Surface::Cone(c) => (c.origin, c.axis, c.radius, c.slope, c.inward),
                        _ => unreachable!(),
                    };
                    if norm(cross(axis, plane.normal)) > 128. * f64::EPSILON {
                        return Err(unsupported(
                            "oblique curved intersections are not supported",
                        ));
                    }
                    if intersections.is_empty() {
                        continue;
                    }
                    let z = dot(sub(plane.origin, origin), axis);
                    let radius = radius + slope * z;
                    if !radius.is_finite() || radius <= tolerance.linear {
                        return Err(unsupported("unresolved conical pole"));
                    }
                    let center = plane.project(std::array::from_fn(|i| origin[i] + axis[i] * z));
                    let sign = if inward { -1. } else { 1. };
                    let arc = edges.iter().find_map(|e| match &self.edges[e.0].curve {
                        Curve::CircularArc(a) => Some(a),
                        _ => None,
                    });
                    if let Some(arc) = arc {
                        if intersections.len() != 2 {
                            return Err(unsupported("unresolved trimmed wall incidence"));
                        }
                        let radial = arc.circle.x_direction;
                        let point =
                            std::array::from_fn(|i| origin[i] + axis[i] * z + radial[i] * radius);
                        let projected = plane.project(point);
                        let mut angle = (projected[1] - center[1]).atan2(projected[0] - center[0]);
                        let raw_sweep =
                            arc.sweep_radians * dot(arc.circle.axis, plane.normal).signum();
                        if raw_sweep.signum() != sign {
                            angle += raw_sweep;
                        }
                        let start = [
                            center[0] + radius * angle.cos(),
                            center[1] + radius * angle.sin(),
                        ];
                        let distance = |p: [f64; 3]| {
                            let q = plane.project(p);
                            (q[0] - start[0]).hypot(q[1] - start[1])
                        };
                        if distance(intersections[1].1) < distance(intersections[0].1) {
                            intersections.swap(0, 1);
                        }
                        if distance(intersections[0].1) > tolerance.linear {
                            return Err(unsupported("trimmed circle endpoint mismatch"));
                        }
                        pieces.push(Piece {
                            start: intersections[0].0,
                            end: intersections[1].0,
                            segment: SectionSegment {
                                face: FaceId(fi),
                                curve: SectionCurve::Arc {
                                    center,
                                    radius,
                                    start_angle: angle,
                                    sweep_angle: sign * raw_sweep.abs(),
                                },
                            },
                        });
                    } else {
                        if intersections.len() != 1 {
                            return Err(unsupported("unresolved periodic wall incidence"));
                        }
                        complete.push(SectionLoop {
                            segments: vec![SectionSegment {
                                face: FaceId(fi),
                                curve: SectionCurve::Arc {
                                    center,
                                    radius,
                                    start_angle: 0.,
                                    sweep_angle: sign * std::f64::consts::TAU,
                                },
                            }],
                        });
                    }
                }
            }
        }
        let mut starts = BTreeMap::new();
        for (i, p) in pieces.iter().enumerate() {
            if starts.insert(p.start, i).is_some() {
                return Err(unsupported("nonmanifold section incidence"));
            }
        }
        let mut used = BTreeSet::new();
        for first in 0..pieces.len() {
            if used.contains(&first) {
                continue;
            }
            let mut i = first;
            let mut segments = vec![];
            loop {
                if !used.insert(i) {
                    return Err(unsupported("section loop overlaps itself"));
                }
                segments.push(pieces[i].segment.clone());
                if pieces[i].end == pieces[first].start {
                    break;
                }
                i = *starts
                    .get(&pieces[i].end)
                    .ok_or_else(|| unsupported("open section loop"))?;
            }
            complete.push(SectionLoop { segments });
        }
        if complete.is_empty() {
            return Ok(empty());
        }
        // Integrate near the geometry, not the caller's potentially distant
        // drawing origin, to avoid cancellation of large translated moments.
        let reference = match complete[0].segments[0].curve {
            SectionCurve::Line { start, .. } => start,
            SectionCurve::Arc { center, .. } => center,
        };
        let mut area = 0.;
        let mut absolute_area = 0.;
        let mut first = [0.; 2];
        for segment in complete.iter().flat_map(|l| &l.segments) {
            let shift = |p: [f64; 2]| [p[0] - reference[0], p[1] - reference[1]];
            let curve = match segment.curve {
                SectionCurve::Line { start, end } => SectionCurve::Line {
                    start: shift(start),
                    end: shift(end),
                },
                SectionCurve::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep_angle,
                } => SectionCurve::Arc {
                    center: shift(center),
                    radius,
                    start_angle,
                    sweep_angle,
                },
            };
            let (a, m) = integrals(&curve);
            absolute_area += a.abs();
            area += a;
            first[0] += m[0];
            first[1] += m[1];
        }
        if !area.is_normal()
            || area <= absolute_area * 4096. * f64::EPSILON
            || area <= tolerance.linear * tolerance.linear
            || first.iter().any(|v| !v.is_finite())
        {
            return Err(KernelError::NumericRange);
        }
        let centroid = std::array::from_fn(|i| reference[i] + first[i] / (2. * area));
        if centroid.iter().any(|v| !v.is_finite()) {
            return Err(KernelError::NumericRange);
        }
        Ok(PlanarSection {
            plane,
            loops: complete,
            area,
            centroid: Some(centroid),
        })
    }
}
// Green integrals: area and [integral x² dy, -integral y² dx].
fn integrals(curve: &SectionCurve) -> (f64, [f64; 2]) {
    match *curve {
        SectionCurve::Line { start: a, end: b } => (
            (a[0] * b[1] - a[1] * b[0]) / 2.,
            [
                (b[1] - a[1]) * (a[0] * a[0] + a[0] * b[0] + b[0] * b[0]) / 3.,
                -(b[0] - a[0]) * (a[1] * a[1] + a[1] * b[1] + b[1] * b[1]) / 3.,
            ],
        ),
        SectionCurve::Arc {
            center: c,
            radius: r,
            start_angle: a,
            sweep_angle: d,
        } => {
            if d.abs() == std::f64::consts::TAU {
                let area = d * r * r / 2.;
                return (area, [2. * area * c[0], 2. * area * c[1]]);
            }
            let b = a + d;
            let cos = b.sin() - a.sin();
            let sin = a.cos() - b.cos();
            let cos2 = d / 2. + ((2. * b).sin() - (2. * a).sin()) / 4.;
            let sin2 = d / 2. - ((2. * b).sin() - (2. * a).sin()) / 4.;
            let cos3 = (b.sin() - b.sin().powi(3) / 3.) - (a.sin() - a.sin().powi(3) / 3.);
            let sin3 = (-b.cos() + b.cos().powi(3) / 3.) - (-a.cos() + a.cos().powi(3) / 3.);
            (
                (r * c[0] * cos + r * c[1] * sin + r * r * d) / 2.,
                [
                    r * c[0] * c[0] * cos + 2. * c[0] * r * r * cos2 + r * r * r * cos3,
                    r * c[1] * c[1] * sin + 2. * c[1] * r * r * sin2 + r * r * r * sin3,
                ],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plane(z: f64) -> SectionPlane {
        SectionPlane {
            origin: [0., 0., z],
            normal: [0., 0., 1.],
            x_direction: [1., 0., 0.],
        }
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-8, "{a} != {b}");
    }
    #[test]
    fn planar_box_general_plane_and_empty_event_contract() {
        let t = Tolerance::default();
        let m = extrude_rectangle(10., 12., 8., t).unwrap().model;
        let s = m.section(plane(4.), t).unwrap();
        close(s.area, 120.);
        assert_eq!(s.centroid, Some([5., 6.]));
        assert_eq!(s.loops.len(), 1);
        let n = [0., 1. / 2f64.sqrt(), 1. / 2f64.sqrt()];
        let s = m
            .section(
                SectionPlane {
                    origin: [0., 6., 4.],
                    normal: n,
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        close(s.area, 80. * 2f64.sqrt());
        close(s.centroid.unwrap()[0], 5.);
        close(s.centroid.unwrap()[1], 0.);
        assert!(m.section(plane(0.), t).is_err());
        assert!(m.section(plane(8.), t).is_err());
        assert!(m.section(plane(20.), t).unwrap().loops.is_empty());
    }
    #[test]
    fn circle_annulus_revolution_and_reversed_frame() {
        let t = Tolerance::default();
        let m = extrude_circle(10., 12., t).unwrap().model;
        for normal in [[0., 0., 1.], [0., 0., -1.]] {
            let s = m
                .section(
                    SectionPlane {
                        normal,
                        ..plane(4.)
                    },
                    t,
                )
                .unwrap();
            close(s.area, 100. * std::f64::consts::PI);
            assert_eq!(s.loops.len(), 1);
        }
        let m = revolve_polygon(&[[3., 0.], [10., 0.], [6., 12.], [3., 12.]], t)
            .unwrap()
            .model;
        let s = m.section(plane(6.), t).unwrap();
        close(s.area, 55. * std::f64::consts::PI);
        assert_eq!(s.loops.len(), 2);
        let moved = transform(&m, [12., -3., 9.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
            .unwrap()
            .model;
        // Obtain the placed frame through native supports rather than assuming matrix conventions.
        let Surface::Cone(c) = moved
            .faces
            .iter()
            .find(|f| matches!(f.surface, Surface::Cone(_)))
            .unwrap()
            .surface
            .clone()
        else {
            panic!()
        };
        let origin = std::array::from_fn(|i| c.origin[i] + 6. * c.axis[i]);
        let s = moved
            .section(
                SectionPlane {
                    origin,
                    normal: c.axis,
                    x_direction: c.x_direction,
                },
                t,
            )
            .unwrap();
        close(s.area, 55. * std::f64::consts::PI);
        assert!(m
            .section(
                SectionPlane {
                    normal: [0., 1., 0.],
                    x_direction: [1., 0., 0.],
                    ..plane(6.)
                },
                t
            )
            .is_err());
    }
    #[test]
    fn countersink_sections_follow_actual_linear_radius_and_blind_depth() {
        let t = Tolerance::default();
        let stock = extrude_rectangle(40., 30., 12., t).unwrap().model;
        let drill = translate(
            &extrude_circle(3., 16., t).unwrap().model,
            [20., 15., -2.],
            t,
        )
        .unwrap()
        .model;
        let sink = translate(
            &revolve_polygon(&[[0., 8.], [3., 8.], [9., 14.], [0., 14.]], t)
                .unwrap()
                .model,
            [20., 15., 0.],
            t,
        )
        .unwrap()
        .model;
        let m = boolean(&stock, &drill, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let m = boolean(&m, &sink, BooleanOp::Difference, t).unwrap().model;
        for (z, r) in [(4., 3.), (9., 4.), (11., 6.)] {
            let s = m.section(plane(z), t).unwrap();
            close(s.area, 1200. - std::f64::consts::PI * r * r);
            close(s.centroid.unwrap()[0], 20.);
            close(s.centroid.unwrap()[1], 15.);
            assert_eq!(s.loops.len(), 2);
        }
        assert!(m.section(plane(8.), t).is_err());
        let tool = translate(&extrude_circle(3., 8., t).unwrap().model, [20., 15., 8.], t)
            .unwrap()
            .model;
        let m = boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        close(m.section(plane(4.), t).unwrap().area, 1200.);
        close(
            m.section(plane(10.), t).unwrap().area,
            1200. - 9. * std::f64::consts::PI,
        );
        assert!(m.section(plane(8.), t).is_err());
    }
    #[test]
    fn mixed_arc_boundaries_and_disconnected_planar_regions() {
        let t = Tolerance::default();
        let path = PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [10., 0.] },
                PathSegment::ArcTo {
                    center: [10., 5.],
                    end: [10., 10.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [0., 10.] },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        };
        let m = extrude_path(&path, &[], 8., t).unwrap().model;
        for frame in [
            plane(4.),
            SectionPlane {
                normal: [0., 0., -1.],
                ..plane(4.)
            },
        ] {
            let s = m.section(frame, t).unwrap();
            close(s.area, 100. + 12.5 * std::f64::consts::PI);
            assert_eq!(s.loops.len(), 1);
            assert_eq!(s.loops[0].segments.len(), 4);
            let expected_x = (500. + 125. * std::f64::consts::PI + 250. / 3.) / s.area;
            close(s.centroid.unwrap()[0], expected_x);
        }
        let a = extrude_rectangle(10., 12., 8., t).unwrap().model;
        let b = translate(&a, [20., 0., 0.], t).unwrap().model;
        let m = boolean(&a, &b, BooleanOp::Union, t).unwrap().model;
        let s = m.section(plane(4.), t).unwrap();
        assert_eq!(s.loops.len(), 2);
        close(s.area, 240.);
        close(s.centroid.unwrap()[0], 15.);
        let shifted = m
            .section(
                SectionPlane {
                    origin: [1e8, -1e8, 4.],
                    ..plane(4.)
                },
                t,
            )
            .unwrap();
        close(shifted.area, 240.);
        close(shifted.centroid.unwrap()[0], 15. - 1e8);
    }
    #[test]
    fn section_frames_are_strict_and_serializable() {
        let t = Tolerance::default();
        let m = extrude_rectangle(10., 12., 8., t).unwrap().model;
        for frame in [
            SectionPlane {
                normal: [0., 0., 2.],
                ..plane(3.)
            },
            SectionPlane {
                x_direction: [0., 0., 1.],
                ..plane(3.)
            },
            SectionPlane {
                origin: [f64::NAN, 0., 3.],
                ..plane(3.)
            },
        ] {
            assert!(m.section(frame, t).is_err());
        }
        let s = m.section(plane(3.), t).unwrap();
        assert_eq!(
            s,
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap()
        );
    }

    #[test]
    fn distant_frame_cannot_publish_unresolved_circular_coordinates() {
        let t = Tolerance::default();
        let m = extrude_circle(10., 12., t).unwrap().model;
        let result = m.section(
            SectionPlane {
                origin: [1e18, -1e18, 6.],
                ..plane(6.)
            },
            t,
        );
        assert!(matches!(result, Err(KernelError::NumericRange)));
    }

    #[test]
    fn oblique_planar_hole_and_nonconvex_multiple_regions() {
        let t = Tolerance::default();
        let outer = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]];
        let hole = vec![[2., 2.], [2., 6.], [4., 6.], [4., 2.]];
        let m = extrude_polygon_with_holes(&outer, &[hole], 30., t)
            .unwrap()
            .model;
        let q = 0.5_f64.sqrt();
        let s = m
            .section(
                SectionPlane {
                    origin: [0., 0., 15.],
                    normal: [0., q, q],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        close(s.area, 92. / q);
        close(s.centroid.unwrap()[0], (500. - 8. * 3.) / 92.);
        close(s.centroid.unwrap()[1], (500. - 8. * 4.) / (92. * q));
        assert_eq!(s.loops.len(), 2);

        // A single connected U-shaped solid has two separate sections through its arms.
        let u = [
            [0., 0.],
            [10., 0.],
            [10., 10.],
            [7., 10.],
            [7., 3.],
            [3., 3.],
            [3., 10.],
            [0., 10.],
        ];
        let m = extrude_polygon_with_holes(&u, &[], 8., t).unwrap().model;
        let s = m
            .section(
                SectionPlane {
                    origin: [0., 6., 0.],
                    normal: [0., 1., 0.],
                    x_direction: [1., 0., 0.],
                },
                t,
            )
            .unwrap();
        assert_eq!(s.loops.len(), 2);
        close(s.area, 48.);
        close(s.centroid.unwrap()[0], 5.);
        close(s.centroid.unwrap()[1], -4.);
    }

    #[test]
    fn shallow_arc_section_preserves_small_segment_area() {
        let t = Tolerance::default();
        let offset: f64 = 1_000.;
        let path = PlanarPath {
            start: [0., 0.],
            segments: vec![
                PathSegment::LineTo { end: [10., 0.] },
                PathSegment::LineTo { end: [10., 10.] },
                PathSegment::ArcTo {
                    center: [5., -offset],
                    end: [0., 10.],
                    clockwise: false,
                },
                PathSegment::LineTo { end: [0., 0.] },
            ],
        };
        let m = extrude_path(&path, &[], 8., t).unwrap().model;
        let s = m.section(plane(4.), t).unwrap();
        let radius = (offset + 10.).hypot(5.);
        let angle = 2. * (5. / (offset + 10.)).atan();
        // Independent small-angle series for the circular segment above y=10.
        let segment_area = radius * radius * angle.powi(3) / 12.
            * (1. - angle * angle / 20. + angle.powi(4) / 840.);
        close(s.area, 100. + segment_area);
        close(s.centroid.unwrap()[0], 5.);
        let segment_center_y =
            -offset + 2. * radius.powi(3) * (angle / 2.).sin().powi(3) / (3. * segment_area);
        close(
            s.centroid.unwrap()[1],
            (500. + segment_area * segment_center_y) / s.area,
        );
    }
}
