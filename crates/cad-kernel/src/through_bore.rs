//! A fully contained circular through-cut appended to a canonical native prism.
use crate::math::{dot, norm, sub};
use crate::*;
use std::collections::BTreeMap;
fn bad() -> KernelError {
    KernelError::Unsupported("through bore requires a canonical right prism and a solid aligned cylinder covering its full height; the bore must be strictly within material and separated from every existing hole".into())
}
pub(crate) struct Stock {
    pub(crate) origin: [f64; 3],
    pub(crate) axis: [f64; 3],
    pub(crate) x: [f64; 3],
    pub(crate) height: f64,
    pub(crate) profile: Profile,
}
pub(crate) enum Profile {
    Polygon {
        rings: Vec<Vec<[f64; 2]>>,
        circles: Vec<CircleHole>,
    },
    Arc(ArcPrismGeometry),
}
fn polygon(
    base: &ModelSnapshot,
    circles: Vec<CircleHole>,
    origin: [f64; 3],
    axis: [f64; 3],
    x: [f64; 3],
    height: f64,
) -> Result<Stock, KernelError> {
    let y = cross(axis, x);
    let rings = base.faces[1]
        .loops
        .iter()
        .map(|id| {
            base.loop_points(*id).ok_or_else(bad).map(|points| {
                points
                    .into_iter()
                    .map(|p| {
                        let d = sub(p, origin);
                        [dot(d, x), dot(d, y)]
                    })
                    .collect()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Stock {
        origin,
        axis,
        x,
        height,
        profile: Profile::Polygon { rings, circles },
    })
}
pub(crate) fn stock(a: &ModelSnapshot, t: Tolerance) -> Result<Stock, KernelError> {
    if let Ok(g) = crate::arc_prism::recognize(a, t) {
        if !g.slot_holes.is_empty() {
            return Err(bad());
        }
        return Ok(Stock {
            origin: g.origin,
            axis: g.axis,
            x: g.x_direction,
            height: g.height,
            profile: Profile::Arc(g),
        });
    }
    if let Ok(g) = crate::profile_prism::recognize(a, t) {
        let y = cross(g.axis, g.x_direction);
        let circles = g
            .bores
            .iter()
            .map(|b| {
                let d = sub(b.origin, g.origin);
                CircleHole {
                    center: [dot(d, g.x_direction), dot(d, y)],
                    radius: b.radius,
                }
            })
            .collect();
        return polygon(&g.base, circles, g.origin, g.axis, g.x_direction, g.height);
    }
    if a.has_curved_geometry() {
        return Err(bad());
    }
    crate::prism::validate_prism_caps(a, t, true)?;
    let lower = a.faces[0].try_plane().ok_or_else(bad)?;
    let upper = a.faces[1].try_plane().ok_or_else(bad)?;
    let points = a.loop_points(a.faces[1].loops[0]).ok_or_else(bad)?;
    let e = sub(points[1], points[0]);
    let length = norm(e);
    let x = e.map(|v| v / length);
    let axis = upper.normal;
    let height = dot(sub(upper.origin, lower.origin), axis);
    polygon(a, vec![], lower.origin, axis, x, height)
}
pub(crate) fn cut(
    a: &ModelSnapshot,
    b: &ModelSnapshot,
    t: Tolerance,
) -> Result<OperationResult, KernelError> {
    let g = stock(a, t)?;
    let tool = crate::analytic::recognize(b, Some(t)).map_err(|_| bad())?;
    if tool.inner_radius != 0. || norm(cross(tool.axis, g.axis)) > 128. * f64::EPSILON {
        return Err(bad());
    }
    let scale = a
        .vertices
        .iter()
        .chain(&b.vertices)
        .flat_map(|v| v.position)
        .chain(g.origin)
        .chain(tool.origin)
        .chain(
            a.edges
                .iter()
                .chain(&b.edges)
                .flat_map(|edge| match &edge.curve {
                    Curve::Line(line) => vec![line.origin[0], line.origin[1], line.origin[2]],
                    Curve::Circle(circle) => vec![
                        circle.origin[0],
                        circle.origin[1],
                        circle.origin[2],
                        circle.radius,
                    ],
                    Curve::CircularArc(arc) => vec![
                        arc.circle.origin[0],
                        arc.circle.origin[1],
                        arc.circle.origin[2],
                        arc.circle.radius,
                    ],
                }),
        )
        .map(f64::abs)
        .fold(g.height.max(tool.height).max(tool.radius), f64::max);
    let error = scale * 4096. * f64::EPSILON;
    if !error.is_finite() || error > t.linear / 16. {
        return Err(KernelError::NumericRange);
    }
    let delta = sub(tool.origin, g.origin);
    let z = dot(delta, g.axis);
    let (lo, hi) = if dot(tool.axis, g.axis) > 0. {
        (z, z + tool.height)
    } else {
        (z - tool.height, z)
    };
    let exact = g.axis.iter().filter(|v| v.abs() == 1.).count() == 1
        && g.axis.iter().all(|v| *v == 0. || v.abs() == 1.);
    let same = |a: f64, b: f64| a == b || (!exact && (a - b).abs() <= error);
    if !(lo < -t.linear - error || same(lo, 0.))
        || !(hi > g.height + t.linear + error || same(hi, g.height))
    {
        return Err(bad());
    }
    let y = cross(g.axis, g.x);
    let center = [dot(delta, g.x), dot(delta, y)];
    // Reserve placement arithmetic in clearance classification, without changing
    // the authoritative bore radius or moving any existing stock geometry.
    let clearance = CircleHole {
        center,
        radius: tool.radius + error,
    };
    match &g.profile {
        Profile::Polygon { rings, circles } => {
            let mut circles = circles.clone();
            circles.push(clearance);
            validate_polygon_with_circular_holes(&rings[0], &rings[1..], &circles, t)?;
        }
        Profile::Arc(path) => {
            let mut circles = path.circular_holes.clone();
            circles.push(clearance);
            validate_path(&path.path, &circles, t)?;
        }
    }
    let origin = std::array::from_fn(|i| g.origin[i] + center[0] * g.x[i] + center[1] * y[i]);
    let support = transform(
        &extrude_circle(tool.radius, g.height, t)?.model,
        origin,
        crate::circular_boolean::quaternion(g.x, g.axis),
        t,
    )?
    .model;
    let native = crate::analytic::recognize(&support, Some(t))?;
    let far = std::array::from_fn(|i| native.origin[i] + native.height * native.axis[i]);
    if (native.radius - tool.radius).abs() > error
        || norm(cross(native.axis, tool.axis)) > 128. * f64::EPSILON
        || norm(sub(native.axis, g.axis)) > 128. * f64::EPSILON
        || norm(sub(native.x_direction, g.x)) > 128. * f64::EPSILON
        || [native.origin, far]
            .into_iter()
            .any(|p| norm(cross(sub(p, tool.origin), tool.axis)) > error)
    {
        return Err(KernelError::NumericRange);
    }
    for (cap, center) in [(0, native.origin), (1, far)] {
        let plane = a.faces[cap].try_plane().ok_or_else(bad)?;
        // Bound the complete circular boundary's departure from the actual cap,
        // not only its center or the reconstructed local-frame plane.
        let center_error = dot(sub(center, plane.origin), plane.normal).abs();
        let angular_error = norm(cross(native.axis, plane.normal)) * native.radius;
        if center_error + angular_error > error {
            return Err(KernelError::NumericRange);
        }
    }
    let mut model = a.clone();
    let (v, e, l, f) = (
        model.vertices.len(),
        model.edges.len(),
        model.loops.len(),
        model.faces.len(),
    );
    model.vertices.extend(support.vertices);
    model
        .edges
        .extend(support.edges.into_iter().map(|mut edge| {
            edge.vertices = edge.vertices.map(|id| VertexId(id.0 + v));
            edge
        }));
    let eu = |n, r| EdgeUse {
        edge: EdgeId(e + n),
        reversed: r,
    };
    model.loops.extend([
        Loop {
            edges: vec![eu(0, false)],
        },
        Loop {
            edges: vec![eu(1, true)],
        },
        Loop {
            edges: vec![eu(0, true), eu(2, false), eu(1, false), eu(2, true)],
        },
    ]);
    model.faces[0].loops.push(LoopId(l));
    model.faces[1].loops.push(LoopId(l + 1));
    let mut wall = support.faces[2].clone();
    if let Surface::Cylinder(c) = &mut wall.surface {
        c.inward = true;
    }
    wall.loops = vec![LoopId(l + 2)];
    model.faces.push(wall);
    model.shells[0].faces.push(FaceId(f));
    let report = model.validate(t);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    model.mass_properties(t)?;
    Ok(OperationResult {
        model,
        history: TopologyHistory {
            generated_faces: BTreeMap::from([
                ("boundary".into(), (0..=f).map(FaceId).collect()),
                ("bore_wall".into(), vec![FaceId(f)]),
            ]),
            input_faces: BTreeMap::from([
                (
                    "left".into(),
                    (0..a.faces.len())
                        .map(|i| (FaceId(i), vec![FaceId(i)]))
                        .collect(),
                ),
                (
                    "right".into(),
                    (0..b.faces.len())
                        .map(|i| {
                            (
                                FaceId(i),
                                if FaceId(i) == tool.wall {
                                    vec![FaceId(f)]
                                } else {
                                    vec![]
                                },
                            )
                        })
                        .collect(),
                ),
            ]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tool(center: [f64; 2], radius: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(
            &extrude_circle(radius, 14., t).unwrap().model,
            [center[0], center[1], -2.],
            t,
        )
        .unwrap()
        .model
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < b.abs().max(1.) * 1e-9, "{a} != {b}")
    }
    #[test]
    fn sequential_plate_bores_preserve_native_faces_and_exact_composite_moments() {
        let t = Tolerance::default();
        let a = extrude_rectangle(40., 30., 10., t).unwrap().model;
        let mut model = a.clone();
        let circles = [([10., 10.], 2.), ([30., 20.], 3.)];
        for (center, r) in circles {
            let before = model.clone();
            let cutter = tool(center, r);
            let out = boolean(&model, &cutter, BooleanOp::Difference, t).unwrap();
            assert_eq!(
                &out.model.vertices[..before.vertices.len()],
                before.vertices
            );
            assert_eq!(&out.model.edges[..before.edges.len()], before.edges);
            assert_eq!(&out.model.faces[2..before.faces.len()], &before.faces[2..]);
            for i in 0..before.faces.len() {
                assert_eq!(out.history.input_faces["left"][&FaceId(i)], vec![FaceId(i)]);
            }
            assert!(out.history.input_faces["right"][&FaceId(0)].is_empty());
            assert!(out.history.input_faces["right"][&FaceId(1)].is_empty());
            assert_eq!(
                out.history.input_faces["right"][&FaceId(2)],
                vec![FaceId(before.faces.len())]
            );
            model = out.model;
        }
        let pi = std::f64::consts::PI;
        let volume = 12000. - 130. * pi;
        let cx = (12000. * 20. - 40. * pi * 10. - 90. * pi * 30.) / volume;
        let cy = (12000. * 15. - 40. * pi * 10. - 90. * pi * 20.) / volume;
        let props = model.mass_properties(t).unwrap();
        near(props.volume, volume);
        near(props.centroid.unwrap()[0], cx);
        near(props.centroid.unwrap()[1], cy);
        let mut raw = [[0.; 3]; 3];
        let items = [
            (
                12000.,
                [20., 15., 5.],
                [40. * 40. / 12., 30. * 30. / 12., 100. / 12.],
            ),
            (-40. * pi, [10., 10., 5.], [1., 1., 100. / 12.]),
            (-90. * pi, [30., 20., 5.], [2.25, 2.25, 100. / 12.]),
        ];
        for (v, c, variances) in items {
            let d = [c[0] - cx, c[1] - cy, 0.];
            for i in 0..3 {
                for j in 0..3 {
                    raw[i][j] += v
                        * (if i == j {
                            variances.iter().sum::<f64>() - variances[i] + dot(d, d)
                        } else {
                            0.
                        } - d[i] * d[j]);
                }
            }
        }
        for (i, row) in raw.iter().enumerate() {
            for (j, x) in row.iter().enumerate() {
                near(props.central_inertia.unwrap()[i][j], *x);
            }
        }
        assert!(tessellate_analytic(&model, t, TessellationPolicy::default()).is_ok());
        for badtool in [
            tool([10., 10.], 1.),
            tool([0., 10.], 2.),
            tool([11., 10.], 2.),
            tool([100., 100.], 2.),
        ] {
            assert!(boolean(&model, &badtool, BooleanOp::Difference, t).is_err());
        }
        let short = translate(&extrude_circle(2., 8., t).unwrap().model, [20., 15., 1.], t)
            .unwrap()
            .model;
        assert!(boolean(&a, &short, BooleanOp::Difference, t).is_err());
    }
    #[test]
    fn mixed_polygon_holes_opposite_axis_and_precision_gates() {
        let t = Tolerance::default();
        let outer = [[0., 0.], [40., 0.], [40., 30.], [0., 30.]];
        let hole = vec![[4., 4.], [4., 8.], [8., 8.], [8., 4.]];
        let stock = extrude_polygon_with_holes(&outer, &[hole], 10., t)
            .unwrap()
            .model;
        let reverse = transform(
            &extrude_circle(2., 14., t).unwrap().model,
            [20., 15., 12.],
            [1., 0., 0., 0.],
            t,
        )
        .unwrap()
        .model;
        let drilled = boolean(&stock, &reverse, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let q = [(0.2_f64).sin(), 0., 0., (0.2_f64).cos()];
        let left = transform(&drilled, [15., -4., 7.], q, t).unwrap().model;
        let right = transform(&tool([30., 20.], 3.), [15., -4., 7.], q, t)
            .unwrap()
            .model;
        let out = boolean(&left, &right, BooleanOp::Difference, t)
            .unwrap()
            .model;
        near(
            out.mass_properties(t).unwrap().volume,
            (1200. - 16. - 13. * std::f64::consts::PI) * 10.,
        );
        assert!(boolean(&stock, &tool([8., 6.], 1.), BooleanOp::Difference, t).is_err());
        let near_cap = translate(
            &extrude_circle(2., 10., t).unwrap().model,
            [20., 15., 1e-12],
            t,
        )
        .unwrap()
        .model;
        assert!(boolean(&stock, &near_cap, BooleanOp::Difference, t).is_err());
        let skew = transform(
            &tool([20., 15.], 2.),
            [0.; 3],
            [0., (0.01_f64).sin(), 0., (0.01_f64).cos()],
            t,
        )
        .unwrap()
        .model;
        assert!(boolean(&stock, &skew, BooleanOp::Difference, t).is_err());
    }
    #[test]
    fn flat_arc_support_scale_and_world_offset_are_reserved() {
        let t = Tolerance::default();
        let radius = 1000_f64;
        let y = radius - (radius * radius - 100.).sqrt();
        let path = PlanarPath {
            start: [-20., -20.],
            segments: vec![
                PathSegment::LineTo { end: [20., -20.] },
                PathSegment::LineTo { end: [20., y] },
                PathSegment::LineTo { end: [10., y] },
                PathSegment::ArcTo {
                    center: [0., radius],
                    end: [-10., y],
                    clockwise: true,
                },
                PathSegment::LineTo { end: [-20., y] },
                PathSegment::LineTo { end: [-20., -20.] },
            ],
        };
        let base = extrude_path(&path, &[], 10., t).unwrap().model;
        for offset in [[0.; 3], [1000., -500., 30.]] {
            let stock = translate(&base, offset, t).unwrap().model;
            let uncertain = translate(&tool([0., -1. - t.linear - 1e-10], 1.), offset, t)
                .unwrap()
                .model;
            assert!(boolean(&stock, &uncertain, BooleanOp::Difference, t).is_err());
            let safe = translate(&tool([0., -5.], 1.), offset, t).unwrap().model;
            let out = boolean(&stock, &safe, BooleanOp::Difference, t)
                .unwrap()
                .model;
            near(
                out.mass_properties(t).unwrap().volume,
                stock.mass_properties(t).unwrap().volume - 10. * std::f64::consts::PI,
            );
        }
    }
    #[test]
    fn arc_boolean_stock_and_common_rigid_placement_compose() {
        let t = Tolerance::default();
        let a = extrude_circle(20., 10., t).unwrap().model;
        let b = translate(&a, [20., 0., 0.], t).unwrap().model;
        let stock = boolean(&a, &b, BooleanOp::Union, t).unwrap().model;
        let cut = tool([0., 0.], 3.);
        let q = [0., (0.3_f64).sin(), 0., (0.3_f64).cos()];
        let mut left = transform(&stock, [12., -8., 5.], q, t).unwrap().model;
        let right = transform(&cut, [12., -8., 5.], q, t).unwrap().model;
        left = boolean(&left, &right, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let other = transform(&tool([20., 0.], 4.), [12., -8., 5.], q, t)
            .unwrap()
            .model;
        let out = boolean(&left, &other, BooleanOp::Difference, t)
            .unwrap()
            .model;
        near(
            out.mass_properties(t).unwrap().volume,
            stock.mass_properties(t).unwrap().volume - 250. * std::f64::consts::PI,
        );
        assert!(out.validate(t).valid);
        assert!(tessellate_analytic(&out, t, TessellationPolicy::default()).is_ok());
    }
}
