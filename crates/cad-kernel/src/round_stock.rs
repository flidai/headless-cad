//! Exact boundary adaptation of periodic round stock for contained drilling.
//!
//! Splitting the outer periodic cylinder into two analytic semicylinder patches
//! changes only its topology. The radius, axis, frame and cap supports are copied
//! from the validated native solid. A pre-existing concentric through hole stays
//! periodic. Original face history is composed through the split explicitly.
use crate::*;
use std::collections::BTreeMap;

pub(crate) fn cut(
    stock: &ModelSnapshot,
    tool: &ModelSnapshot,
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    let cylinder = crate::analytic::recognize(stock, Some(tolerance))?;
    let radius = cylinder.radius;
    let geometry = ArcPrismGeometry {
        path: PlanarPath {
            start: [radius, 0.],
            segments: vec![
                PathSegment::ArcTo {
                    center: [0., 0.],
                    end: [-radius, 0.],
                    clockwise: false,
                },
                PathSegment::ArcTo {
                    center: [0., 0.],
                    end: [radius, 0.],
                    clockwise: false,
                },
            ],
        },
        circular_holes: if cylinder.inner_radius == 0. {
            vec![]
        } else {
            vec![CircleHole {
                center: [0., 0.],
                radius: cylinder.inner_radius,
            }]
        },
        origin: cylinder.origin,
        axis: cylinder.axis,
        x_direction: cylinder.x_direction,
        height: cylinder.height,
    };
    let model = crate::arc_prism::construct(&geometry, tolerance)?;
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    let mut result = crate::through_bore::cut(&model, tool, tolerance)
        .or_else(|_| crate::bored_prism::cut(&model, tool, tolerance))
        .or_else(|_| crate::stepped_bore::cut(&model, tool, tolerance))?;
    let mut split = BTreeMap::from([
        (cylinder.start_cap, vec![FaceId(0)]),
        (cylinder.end_cap, vec![FaceId(1)]),
        (cylinder.wall, vec![FaceId(2), FaceId(3)]),
    ]);
    if let Some(inner) = cylinder.inner_wall {
        split.insert(inner, vec![FaceId(4)]);
    }
    let downstream = &result.history.input_faces["left"];
    let composed = split
        .into_iter()
        .map(|(source, intermediate)| {
            let mut faces: Vec<_> = intermediate
                .iter()
                .flat_map(|face| downstream[face].iter().copied())
                .collect();
            faces.sort();
            faces.dedup();
            (source, faces)
        })
        .collect();
    result.history.input_faces.insert("left".into(), composed);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{dot, sub};

    fn cutter(radius: f64, center: [f64; 2], z: f64, height: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        translate(
            &extrude_circle(radius, height, t).unwrap().model,
            [center[0], center[1], z],
            t,
        )
        .unwrap()
        .model
    }

    fn stock(inner: f64) -> ModelSnapshot {
        let t = Tolerance::default();
        if inner == 0. {
            extrude_circle(30., 16., t).unwrap().model
        } else {
            extrude_annulus(30., inner, 16., t).unwrap().model
        }
    }

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 2e-8 * expected.abs().max(1.),
            "{actual} != {expected}"
        );
    }

    // Independent signed-cylinder integration, including all products of inertia.
    fn expected(inner: f64, matrix: [[f64; 3]; 3], offset: [f64; 3]) -> MassProperties {
        let mut pieces = vec![
            (1., 30., 16., [0., 0., 8.]),
            (-1., 3., 16., [18., 0., 8.]),
            // Counterbore enlarges only the upper four millimetres.
            (-1., 5., 4., [18., 0., 14.]),
            (1., 3., 4., [18., 0., 14.]),
            (-1., 2., 5., [-18., 0., 13.5]),
        ];
        if inner > 0. {
            pieces.push((-1., inner, 16., [0., 0., 8.]));
        }
        let pieces: Vec<_> = pieces
            .into_iter()
            .map(|(sign, radius, height, center)| {
                let volume = sign * std::f64::consts::PI * radius * radius * height;
                let center = std::array::from_fn::<_, 3, _>(|i| dot(matrix[i], center) + offset[i]);
                let variances = [
                    radius * radius / 4.,
                    radius * radius / 4.,
                    height * height / 12.,
                ];
                (volume, center, variances)
            })
            .collect();
        let volume: f64 = pieces.iter().map(|p| p.0).sum();
        let center =
            std::array::from_fn(|i| pieces.iter().map(|p| p.0 * p.1[i]).sum::<f64>() / volume);
        let mut tensor = [[0.; 3]; 3];
        for (v, c, variance) in pieces {
            let d = sub(c, center);
            let diagonal = variance.map(|x| variance.iter().sum::<f64>() - x);
            for i in 0..3 {
                for j in 0..3 {
                    tensor[i][j] += v
                        * ((0..3)
                            .map(|k| matrix[i][k] * diagonal[k] * matrix[j][k])
                            .sum::<f64>()
                            + if i == j { dot(d, d) } else { 0. }
                            - d[i] * d[j]);
                }
            }
        }
        MassProperties {
            volume,
            centroid: Some(center),
            central_inertia: Some(tensor),
        }
    }

    fn closed(model: &ModelSnapshot) {
        let mesh = tessellate_analytic(model, Tolerance::default(), TessellationPolicy::default())
            .unwrap();
        let key = |p: [f64; 3]| p.map(|v| if v == 0. { 0 } else { v.to_bits() });
        let mut edges = BTreeMap::<([u64; 3], [u64; 3]), (usize, i32)>::new();
        for patch in mesh.patches {
            for i in 1..patch.points.len() - 1 {
                let tri = [patch.points[0], patch.points[i], patch.points[i + 1]];
                assert!(
                    dot(
                        cross(sub(tri[1], tri[0]), sub(tri[2], tri[0])),
                        patch.normal
                    ) > 0.
                );
                for j in 0..3 {
                    let a = key(tri[j]);
                    let b = key(tri[(j + 1) % 3]);
                    let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let value = edges.entry(edge).or_default();
                    value.0 += 1;
                    value.1 += sign;
                }
            }
        }
        assert!(
            edges.values().all(|value| *value == (2, 0)),
            "mesh is not closed and oriented"
        );
    }

    #[test]
    fn disk_and_tube_drilling_preserve_original_face_lineage() {
        let t = Tolerance::default();
        for inner in [0., 8.] {
            let a = stock(inner);
            let tool = cutter(2., [-18., 0.], 11., 7.);
            let result = boolean(&a, &tool, BooleanOp::Difference, t).unwrap();
            let left = &result.history.input_faces["left"];
            assert_eq!(left.len(), a.faces.len());
            assert_eq!(left[&FaceId(0)], [FaceId(0)]);
            assert_eq!(left[&FaceId(1)], [FaceId(1)]);
            assert_eq!(left[&FaceId(2)], [FaceId(2), FaceId(3)]);
            if inner > 0. {
                assert_eq!(left[&FaceId(3)], [FaceId(4)]);
                let Surface::Cylinder(wall) = &result.model.faces[4].surface else {
                    panic!()
                };
                assert_eq!(wall.radius, inner);
                assert!(wall.inward);
            }
            assert_eq!(result.history.generated_faces["bore_floor"].len(), 1);
            assert_eq!(
                result.history.input_faces["right"][&FaceId(0)],
                result.history.generated_faces["bore_floor"]
            );
            assert_eq!(
                result.history.input_faces["right"][&FaceId(2)],
                result.history.generated_faces["bore_wall"]
            );
            closed(&result.model);
        }
    }

    #[test]
    fn round_stock_sequential_machining_full_moments_and_closed_meshes() {
        let t = Tolerance::default();
        let q = [0.2, 0.3, -0.1, 0.86_f64.sqrt()];
        let [x, y, z, w] = q;
        let rotated = [
            [
                1. - 2. * (y * y + z * z),
                2. * (x * y - z * w),
                2. * (x * z + y * w),
            ],
            [
                2. * (x * y + z * w),
                1. - 2. * (x * x + z * z),
                2. * (y * z - x * w),
            ],
            [
                2. * (x * z - y * w),
                2. * (y * z + x * w),
                1. - 2. * (x * x + y * y),
            ],
        ];
        for inner in [0., 8.] {
            for (rotation, matrix, offset) in [
                (
                    [0., 0., 0., 1.],
                    [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                    [0.; 3],
                ),
                (q, rotated, [100., -200., 50.]),
            ] {
                for blind_first in [false, true] {
                    let moved = |model: &ModelSnapshot| {
                        transform(model, offset, rotation, t).unwrap().model
                    };
                    let mut model = moved(&stock(inner));
                    let tools = [
                        cutter(3., [18., 0.], -2., 20.),
                        cutter(5., [18., 0.], 12., 6.),
                        cutter(2., [-18., 0.], 11., 7.),
                    ];
                    let order = if blind_first { [2, 0, 1] } else { [0, 1, 2] };
                    for index in order {
                        model = boolean(&model, &moved(&tools[index]), BooleanOp::Difference, t)
                            .unwrap()
                            .model;
                        assert!(model.validate(t).valid);
                        closed(&model);
                    }
                    let actual = model.mass_properties(t).unwrap();
                    let reference = expected(inner, matrix, offset);
                    near(actual.volume, reference.volume);
                    for i in 0..3 {
                        near(actual.centroid.unwrap()[i], reference.centroid.unwrap()[i]);
                        for j in 0..3 {
                            near(
                                actual.central_inertia.unwrap()[i][j],
                                reference.central_inertia.unwrap()[i][j],
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn round_stock_uncertain_clearance_and_enclosed_or_skew_tools_reject() {
        let t = Tolerance::default();
        let tube = stock(8.);
        for tool in [
            cutter(2., [29., 0.], -2., 20.),
            cutter(2., [10., 0.], -2., 20.),
            cutter(2., [0., 0.], -2., 20.),
            cutter(2., [18., 0.], 4., 5.),
            cutter(2., [28. - t.linear / 2., 0.], -2., 20.),
            transform(
                &cutter(2., [18., 0.], -2., 20.),
                [0.; 3],
                [0., 0.1_f64.sin(), 0., 0.1_f64.cos()],
                t,
            )
            .unwrap()
            .model,
        ] {
            assert!(boolean(&tube, &tool, BooleanOp::Difference, t).is_err());
        }
        assert!(boolean(
            &stock(0.),
            &cutter(2., [29., 0.], 11., 7.),
            BooleanOp::Difference,
            t
        )
        .is_err());
    }
}
