//! Rigid placement of analytic geometry, without tessellation or normalization.
use crate::math::{dot, finite, norm, sub};
use crate::*;

/// Validate millimetre translation and a proper unit quaternion in XYZW order.
/// Matches assembly placement policy: no implicit normalization or snapping.
pub fn validate_transform(offset: [f64; 3], rotation: [f64; 4]) -> Result<(), KernelError> {
    if !finite(offset) || !rotation.into_iter().all(f64::is_finite) {
        return Err(KernelError::InvalidTransform(
            "components must be finite".into(),
        ));
    }
    let squared: f64 = rotation.into_iter().map(|v| v * v).sum();
    if !squared.is_finite() || (squared - 1.0).abs() > 32.0 * f64::EPSILON {
        return Err(KernelError::InvalidTransform(
            "quaternion must be unit length; normalization is explicit".into(),
        ));
    }
    Ok(())
}

/// Apply `R(rotation) * point + offset` to native line/plane geometry.
/// General rotations require one strictly convex solid, a validated right prism
/// with convex through-hole rings, or an empty model.
/// Exactly represented signed-permutation rotations additionally support the
/// existing orthogonal nonconvex/multiple-solid domain. Near-axis rotations are
/// never snapped. Topology IDs and outward winding remain unchanged; provenance
/// maps every `input` face to the identical output ID, with role `transformed`.
/// Non-permutation arithmetic requires 64 eps times the largest input coordinate
/// to fit the linear tolerance. Translation residuals and edge-vector changes
/// must also fit tolerance; overflow and unsupported geometry fail explicitly.
pub fn transform(
    model: &ModelSnapshot,
    offset: [f64; 3],
    rotation: [f64; 4],
    tolerance: Tolerance,
) -> Result<OperationResult, KernelError> {
    if !tolerance.is_valid() {
        return Err(KernelError::InvalidTolerance);
    }
    validate_transform(offset, rotation)?;
    let report = model.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidInput(report));
    }
    let [x, y, z, w] = rotation;
    let matrix = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let permutation = matrix.iter().all(|row| {
        row.iter().filter(|&&v| v != 0.0).count() == 1
            && row.iter().all(|&v| matches!(v, -1.0 | 0.0 | 1.0))
    });
    if !permutation && !model.solids.is_empty() {
        let issues = if model.faces.iter().any(|face| face.loops.len() > 1) {
            match crate::prism::validate_prism(model, tolerance) {
                Ok(()) => vec![],
                Err(error) => vec![ValidationIssue {
                    code: "unsupported_geometry".into(),
                    message: error.to_string(),
                }],
            }
        } else {
            validation::geometry::validate_convex(model, tolerance)
        };
        if let Some(issue) = issues.first() {
            return Err(if issue.code == "resource_limit" {
                KernelError::ResourceLimit(issue.message.clone())
            } else {
                KernelError::Unsupported("general rigid rotation requires a single strictly convex solid or a validated right prism with convex through holes; other nonconvex and multiple solids need an exact axis permutation".into())
            });
        }
    }
    let vector = |v| matrix.map(|row| dot(row, v));
    let point = |p: [f64; 3]| -> Result<[f64; 3], KernelError> {
        let scale = p.into_iter().map(f64::abs).fold(0.0, f64::max);
        if !permutation && scale * (64.0 * f64::EPSILON) > tolerance.linear {
            return Err(KernelError::NumericRange);
        }
        let rotated = vector(p);
        let result = std::array::from_fn(|i| rotated[i] + offset[i]);
        if !finite(result) {
            return Err(KernelError::NumericRange);
        }
        for i in 0..3 {
            let recovered = result[i] - offset[i];
            let residual = (offset[i] - (result[i] - recovered)) + (rotated[i] - recovered);
            if !residual.is_finite() || residual.abs() > tolerance.linear {
                return Err(KernelError::NumericRange);
            }
        }
        Ok(result)
    };
    let mut moved = model.clone();
    for vertex in &mut moved.vertices {
        vertex.position = point(vertex.position)?;
    }
    for edge in &mut moved.edges {
        edge.line.origin = point(edge.line.origin)?;
        edge.line.direction = vector(edge.line.direction);
    }
    for face in &mut moved.faces {
        face.plane.origin = point(face.plane.origin)?;
        face.plane.normal = vector(face.plane.normal);
    }
    for edge in &model.edges {
        let [a, b] = edge.vertices;
        let expected = vector(sub(
            model.vertices[b.0].position,
            model.vertices[a.0].position,
        ));
        let actual = sub(moved.vertices[b.0].position, moved.vertices[a.0].position);
        if norm(sub(expected, actual)) > tolerance.linear {
            return Err(KernelError::NumericRange);
        }
    }
    let report = moved.validate(tolerance);
    if !report.valid {
        return Err(KernelError::InvalidResult(report));
    }
    Ok(OperationResult {
        model: moved,
        history: TopologyHistory {
            generated_faces: BTreeMap::from([(
                "transformed".into(),
                (0..model.faces.len()).map(FaceId).collect(),
            )]),
            input_faces: BTreeMap::from([(
                "input".into(),
                (0..model.faces.len())
                    .map(|i| (FaceId(i), vec![FaceId(i)]))
                    .collect(),
            )]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: [f64; 3], b: [f64; 3]) {
        assert!(norm(sub(a, b)) < 1e-10, "{a:?} != {b:?}");
    }
    #[test]
    fn analytic_rotation_translation_and_identity_lineage() {
        let t = Tolerance::default();
        let source = extrude_polygon(&[[0., 0.], [8., 0.], [0., 6.]], 5., t).unwrap();
        let angle = 0.63_f64;
        let rotation = [0., 0., (angle / 2.).sin(), (angle / 2.).cos()];
        let moved = transform(&source.model, [12., -4., 7.], rotation, t).unwrap();
        assert_eq!(moved.model.loops, source.model.loops);
        assert_eq!(moved.model.shells, source.model.shells);
        assert_eq!(moved.model.solids, source.model.solids);
        for (a, b) in source.model.vertices.iter().zip(&moved.model.vertices) {
            let [x, y, z] = a.position;
            close(
                b.position,
                [
                    angle.cos() * x - angle.sin() * y + 12.,
                    angle.sin() * x + angle.cos() * y - 4.,
                    z + 7.,
                ],
            );
        }
        assert!((moved.model.volume() - 120.).abs() < 1e-10);
        for i in 0..source.model.faces.len() {
            assert_eq!(
                moved.history.input_faces["input"][&FaceId(i)],
                vec![FaceId(i)]
            );
        }
        assert_eq!(source.model.vertices[0].position, [0., 0., 0.]);
        let same = transform(&source.model, [12., -4., 7.], rotation.map(|q| -q), t).unwrap();
        assert_eq!(same, moved);
        let identity = transform(&source.model, [0.; 3], [0., 0., 0., 1.], t).unwrap();
        assert_eq!(identity.model, source.model);
        for (face, rotated) in source.model.faces.iter().zip(&moved.model.faces) {
            let [x, y, z] = face.plane.normal;
            close(
                rotated.plane.normal,
                [
                    angle.cos() * x - angle.sin() * y,
                    angle.sin() * x + angle.cos() * y,
                    z,
                ],
            );
        }
    }
    #[test]
    fn noncommuting_rotations_and_reverse_order_inverse() {
        let t = Tolerance::default();
        let source = extrude_rectangle(3., 4., 5., t).unwrap().model;
        let a = 0.7_f64;
        let b = -0.4_f64;
        let qx = [(a / 2.).sin(), 0., 0., (a / 2.).cos()];
        let qz = [0., 0., (b / 2.).sin(), (b / 2.).cos()];
        let first = transform(&source, [1., 2., 3.], qx, t).unwrap().model;
        let second = transform(&first, [-4., 5., 2.], qz, t).unwrap().model;
        for (v, out) in source.vertices.iter().zip(&second.vertices) {
            let [x, y, z] = v.position;
            let p = [
                x + 1.,
                a.cos() * y - a.sin() * z + 2.,
                a.sin() * y + a.cos() * z + 3.,
            ];
            close(
                out.position,
                [
                    b.cos() * p[0] - b.sin() * p[1] - 4.,
                    b.sin() * p[0] + b.cos() * p[1] + 5.,
                    p[2] + 2.,
                ],
            );
        }
        let undo_z = transform(
            &second,
            [
                4. * b.cos() - 5. * b.sin(),
                -4. * b.sin() - 5. * b.cos(),
                -2.,
            ],
            [0., 0., -qz[2], qz[3]],
            t,
        )
        .unwrap()
        .model;
        let undo_x = transform(
            &undo_z,
            [
                -1.,
                -2. * a.cos() - 3. * a.sin(),
                2. * a.sin() - 3. * a.cos(),
            ],
            [-qx[0], 0., 0., qx[3]],
            t,
        )
        .unwrap()
        .model;
        for (v, out) in source.vertices.iter().zip(&undo_x.vertices) {
            close(v.position, out.position);
        }
        assert!((second.volume() - 60.).abs() < 1e-10);
        let opposite = transform(
            &transform(&source, [-4., 5., 2.], qz, t).unwrap().model,
            [1., 2., 3.],
            qx,
            t,
        )
        .unwrap()
        .model;
        assert!(
            norm(sub(
                opposite.vertices[0].position,
                second.vertices[0].position
            )) > 1.
        );
    }
    #[test]
    fn unsupported_nonconvex_and_multisolid_but_exact_permutation_supported() {
        let t = Tolerance::default();
        let a = extrude_rectangle(4., 4., 4., t).unwrap().model;
        let b = translate(&a, [2., 2., 0.], t).unwrap().model;
        let shape = boolean(&a, &b, BooleanOp::Difference, t).unwrap().model;
        let angle = 0.4_f64;
        let q = [0., 0., angle.sin(), angle.cos()];
        assert!(matches!(
            transform(&shape, [0.; 3], q, t),
            Err(KernelError::Unsupported(_))
        ));
        let permuted = transform(&shape, [1., 2., 3.], [0.5; 4], t).unwrap();
        assert!((permuted.model.volume() - shape.volume()).abs() < 1e-10);
        let far = translate(&a, [10., 0., 0.], t).unwrap().model;
        let multi = boolean(&a, &far, BooleanOp::Union, t).unwrap().model;
        assert!(matches!(
            transform(&multi, [0.; 3], q, t),
            Err(KernelError::Unsupported(_))
        ));
        assert!(transform(&multi, [0.; 3], [0.5; 4], t).is_ok());
        let empty = boolean(&a, &far, BooleanOp::Intersection, t).unwrap().model;
        assert!(transform(&empty, [3.; 3], q, t)
            .unwrap()
            .model
            .vertices
            .is_empty());
    }
    #[test]
    fn invalid_frames_precision_and_mutated_geometry_fail() {
        let t = Tolerance::default();
        let model = extrude_rectangle(3., 4., 5., t).unwrap().model;
        for rotation in [
            [0.; 4],
            [0., 0., 0., 2.],
            [f64::NAN, 0., 0., 1.],
            [0., 0., 0., 1. + 1e-12],
        ] {
            assert!(matches!(
                transform(&model, [0.; 3], rotation, t),
                Err(KernelError::InvalidTransform(_))
            ));
        }
        assert!(transform(&model, [f64::INFINITY, 0., 0.], [0., 0., 0., 1.], t).is_err());
        assert!(matches!(
            transform(&model, [1e20, 0., 0.], [0., 0., 0., 1.], t),
            Err(KernelError::NumericRange)
        ));
        let large = translate(&model, [1e10, 0., 0.], t).unwrap().model;
        assert!(matches!(
            transform(&large, [0.; 3], [0., 0., 0.3_f64.sin(), 0.3_f64.cos()], t),
            Err(KernelError::NumericRange)
        ));
        let mut corrupt = model.clone();
        corrupt.edges[0].vertices[0] = VertexId(999);
        assert!(matches!(
            transform(&corrupt, [0.; 3], [0., 0., 0., 1.], t),
            Err(KernelError::InvalidInput(_))
        ));
        assert!(matches!(
            transform(&model, [0.; 3], [0., 0., 0., 1.], Tolerance { linear: 0. }),
            Err(KernelError::InvalidTolerance)
        ));
    }
}
