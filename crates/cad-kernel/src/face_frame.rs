//! Explicit face frames from validated planar geometry, never view geometry.
use crate::math::{cross, dot, finite, norm, sub};
use crate::*;

/// Right-handed orthonormal frame: origin in mm, dimensionless world-space axes.
/// Z follows the outward face normal. X follows the supplied projected hint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceFrame {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
    pub z_axis: [f64; 3],
}
#[derive(Clone, Copy, Default)]
struct Sum {
    value: f64,
    correction: f64,
    absolute: f64,
}
impl Sum {
    fn add(&mut self, value: f64) -> Result<(), KernelError> {
        if !value.is_finite() || (value != 0. && !value.is_normal()) {
            return Err(KernelError::NumericRange);
        }
        let adjusted = value - self.correction;
        let next = self.value + adjusted;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
        self.absolute += value.abs();
        if !next.is_finite() || !self.absolute.is_finite() {
            return Err(KernelError::NumericRange);
        }
        Ok(())
    }
}
fn add(a: f64, b: f64, tol: Tolerance) -> Result<f64, KernelError> {
    let sum = a + b;
    let recovered = sum - a;
    let residual = (a - (sum - recovered)) + (b - recovered);
    if !sum.is_finite() || !residual.is_finite() || residual.abs() > tol.linear / 16. {
        Err(KernelError::NumericRange)
    } else {
        Ok(sum)
    }
}

impl ModelSnapshot {
    /// Frame of one validated planar face. The origin is its area
    /// centroid, followed by `local_offset` in the returned X/Y/Z basis (mm).
    /// `x_direction_hint` is a dimensionless world-space direction: its normal
    /// component is discarded. No automatic axis or nearest-face fallback.
    /// Normalize the hint scale independently of millimetre tolerance; reject
    /// projected unit-hint length <= 1e-8 (the sine of its angle to the normal).
    /// The derived face normal and basis are normalized during construction.
    /// Non-permutation offset arithmetic must satisfy the conservative bound
    /// `128 eps * max_abs(offset) / sin(angle) <= linear_tolerance / 16`.
    /// Reject insufficient precision, invalid IDs/models, and nonfinite inputs.
    pub fn face_frame(
        &self,
        face_id: FaceId,
        x_direction_hint: [f64; 3],
        local_offset: [f64; 3],
        tolerance: Tolerance,
    ) -> Result<FaceFrame, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if !finite(x_direction_hint) || !finite(local_offset) {
            return Err(KernelError::InvalidFaceFrame(
                "hint and offset must be finite".into(),
            ));
        }
        let report = self.validate(tolerance);
        if !report.valid {
            return Err(KernelError::InvalidInput(report));
        }
        let face = self
            .faces
            .get(face_id.0)
            .ok_or_else(|| KernelError::InvalidFaceFrame("face ID does not exist".into()))?;
        let plane = face.try_plane().ok_or_else(|| {
            KernelError::InvalidFaceFrame(
                "curved faces do not have a unique planar face frame".into(),
            )
        })?;
        let z_axis = plane.normal.map(|v| v / norm(plane.normal));
        let scale = x_direction_hint
            .into_iter()
            .map(f64::abs)
            .fold(0., f64::max);
        if scale == 0. {
            return Err(KernelError::InvalidFaceFrame(
                "X direction hint must be nonzero".into(),
            ));
        }
        let hint = x_direction_hint.map(|v| v / scale);
        let hint = hint.map(|v| v / norm(hint));
        // Double cross projects while retaining stable orthogonality even for
        // hints with a large normal component. Its magnitude is sin(angle).
        let projection = cross(cross(z_axis, hint), z_axis);
        let sine = norm(projection);
        if !sine.is_finite() || sine <= 1e-8 {
            return Err(KernelError::InvalidFaceFrame(
                "X direction hint is parallel or ill-conditioned relative to the face normal"
                    .into(),
            ));
        }
        let x_axis = projection.map(|v| v / sine);
        let y = cross(z_axis, x_axis);
        let y_axis = y.map(|v| v / norm(y));
        if [x_axis, y_axis, z_axis]
            .iter()
            .any(|v| !finite(*v) || (norm(*v) - 1.).abs() > 32. * f64::EPSILON)
            || dot(x_axis, z_axis).abs() > 32. * f64::EPSILON
            || dot(x_axis, y_axis).abs() > 32. * f64::EPSILON
            || dot(y_axis, z_axis).abs() > 32. * f64::EPSILON
        {
            return Err(KernelError::NumericRange);
        }
        let centroid = if self.has_curved_geometry() {
            // The validated circular caps have their exact area centroid at
            // the supporting plane origin, independent of tessellation.
            if let Ok(g) = crate::stepped_bore::recognize(self, tolerance) {
                g.cap_centroid(face_id, tolerance)?
            } else if let Ok(g) = crate::bored_prism::recognize(self, tolerance) {
                g.cap_centroid(face_id, tolerance)?
            } else if let Some(g) = self.arc_prism_geometry(tolerance)? {
                if face_id.0 < 2 {
                    g.cap_centroid(face_id, tolerance)?
                } else {
                    let points = self.loop_points(face.loops[0]).unwrap();
                    std::array::from_fn(|i| points.iter().map(|p| p[i] / points.len() as f64).sum())
                }
            } else if let Some(g) = self.profile_prism_geometry(tolerance)? {
                if face_id == g.start_cap || face_id == g.end_cap {
                    g.cap_centroid(face_id, tolerance)?
                } else {
                    g.base
                        .face_frame(face_id, x_direction_hint, [0.; 3], tolerance)?
                        .origin
                }
            } else {
                plane.origin
            }
        } else {
            let points = self.loop_points(face.loops[0]).unwrap();
            let reference = points[0];
            let mut area = Sum::default();
            let mut moment = [Sum::default(); 3];
            for &loop_id in &face.loops {
                let points = self.loop_points(loop_id).unwrap();
                let base = sub(points[0], reference);
                for pair in points[1..].windows(2) {
                    let a = sub(pair[0], reference);
                    let b = sub(pair[1], reference);
                    let weight = dot(cross(sub(a, base), sub(b, base)), z_axis) / 2.;
                    if weight == 0. {
                        continue;
                    }
                    if !weight.is_normal() {
                        return Err(KernelError::NumericRange);
                    }
                    area.add(weight)?;
                    for i in 0..3 {
                        let center = base[i] / 3. + a[i] / 3. + b[i] / 3.;
                        let contribution = weight * center;
                        if center != 0. && !contribution.is_normal() {
                            return Err(KernelError::NumericRange);
                        }
                        moment[i].add(contribution)?;
                    }
                }
            }
            if !area.value.is_normal() || area.value <= 0. {
                return Err(KernelError::NumericRange);
            }
            let mut centroid = [0.; 3];
            for i in 0..3 {
                let local = moment[i].value / area.value;
                let error = (moment[i].absolute / area.value
                    + local.abs() * area.absolute / area.value)
                    * (128. * f64::EPSILON);
                if !error.is_finite() || error > tolerance.linear / 16. {
                    return Err(KernelError::NumericRange);
                }
                centroid[i] = add(reference[i], local, tolerance)?;
            }
            centroid
        };
        let axes = [x_axis, y_axis, z_axis];
        let permutation = axes.iter().all(|axis| {
            axis.iter().filter(|&&v| v != 0.).count() == 1
                && axis.iter().all(|v| matches!(*v, -1.0 | 0.0 | 1.0))
        });
        let offset_scale = local_offset.into_iter().map(f64::abs).fold(0., f64::max);
        if !permutation && offset_scale * (128. * f64::EPSILON) / sine > tolerance.linear / 16. {
            return Err(KernelError::NumericRange);
        }
        let mut origin = [0.; 3];
        for i in 0..3 {
            let offset = (0..3).map(|j| axes[j][i] * local_offset[j]).sum();
            origin[i] = add(centroid[i], offset, tolerance)?;
        }
        Ok(FaceFrame {
            origin,
            x_axis,
            y_axis,
            z_axis,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(actual: [f64; 3], expected: [f64; 3]) {
        assert!(
            norm(sub(actual, expected)) < 1e-10,
            "{actual:?} != {expected:?}"
        );
    }
    fn trapezoid() -> ModelSnapshot {
        extrude_polygon(
            &[[0., 0.], [6., 0.], [4., 3.], [0., 3.]],
            2.,
            Tolerance::default(),
        )
        .unwrap()
        .model
    }
    #[test]
    fn area_centroid_explicit_axes_offsets_and_handedness() {
        let t = Tolerance::default();
        let model = trapezoid();
        let top = model
            .face_frame(FaceId(1), [1., 0., 7.], [0.; 3], t)
            .unwrap();
        close(top.origin, [38. / 15., 7. / 5., 2.]);
        assert!(norm(sub(top.origin, [2.5, 1.5, 2.])) > 0.1);
        assert_eq!(top.x_axis, [1., 0., 0.]);
        assert_eq!(top.y_axis, [0., 1., 0.]);
        assert_eq!(top.z_axis, [0., 0., 1.]);
        let offset = model
            .face_frame(FaceId(1), [1., 0., 0.], [2., -3., 4.], t)
            .unwrap();
        close(offset.origin, [38. / 15. + 2., 7. / 5. - 3., 6.]);
        let bottom = model
            .face_frame(FaceId(0), [1., 0., 0.], [2., 3., 4.], t)
            .unwrap();
        close(bottom.origin, [38. / 15. + 2., 7. / 5. - 3., -4.]);
        close(cross(bottom.x_axis, bottom.y_axis), bottom.z_axis);
        assert_eq!(bottom.z_axis, [0., 0., -1.]);
        for scale in [1e-300, 1e300] {
            let scaled = model
                .face_frame(FaceId(1), [scale, 0., 0.], [0.; 3], t)
                .unwrap();
            assert_eq!(scaled.x_axis, top.x_axis);
        }
    }
    #[test]
    fn oblique_face_and_noncommuting_transformed_frame() {
        let t = Tolerance::default();
        let source = trapezoid();
        let wall = source
            .face_frame(FaceId(3), [0., 0., 1.], [0.; 3], t)
            .unwrap();
        close(wall.origin, [5., 1.5, 1.]);
        close(wall.z_axis, [3. / 13_f64.sqrt(), 2. / 13_f64.sqrt(), 0.]);
        close(wall.x_axis, [0., 0., 1.]);
        close(cross(wall.x_axis, wall.y_axis), wall.z_axis);
        let (a, b) = (0.3_f64, 0.4_f64);
        let q = [
            b.cos() * a.sin(),
            b.sin() * a.cos(),
            -b.sin() * a.sin(),
            b.cos() * a.cos(),
        ];
        // Independently evaluate Ry(0.8)*Rx(0.6), including a nonzero offset.
        let rotate = |p: [f64; 3]| {
            let [x, y, z] = p;
            let v = [
                x,
                0.6_f64.cos() * y - 0.6_f64.sin() * z,
                0.6_f64.sin() * y + 0.6_f64.cos() * z,
            ];
            [
                0.8_f64.cos() * v[0] + 0.8_f64.sin() * v[2],
                v[1],
                -0.8_f64.sin() * v[0] + 0.8_f64.cos() * v[2],
            ]
        };
        let moved = transform(&source, [10., -3., 8.], q, t).unwrap().model;
        let frame = moved
            .face_frame(FaceId(1), rotate([1., 0., 0.]), [1., 2., 3.], t)
            .unwrap();
        close(frame.x_axis, rotate([1., 0., 0.]));
        close(frame.y_axis, rotate([0., 1., 0.]));
        close(frame.z_axis, rotate([0., 0., 1.]));
        let expected = rotate([38. / 15. + 1., 7. / 5. + 2., 5.]);
        close(
            frame.origin,
            [expected[0] + 10., expected[1] - 3., expected[2] + 8.],
        );
        close(cross(frame.x_axis, frame.y_axis), frame.z_axis);
    }
    #[test]
    fn invalid_parallel_and_numeric_precision_rejections() {
        let t = Tolerance::default();
        let model = trapezoid();
        for hint in [[0.; 3], [0., 0., 1.], [1e-12, 0., 1.], [f64::NAN, 0., 0.]] {
            assert!(matches!(
                model.face_frame(FaceId(1), hint, [0.; 3], t),
                Err(KernelError::InvalidFaceFrame(_))
            ));
        }
        assert!(matches!(
            model.face_frame(FaceId(999), [1., 0., 0.], [0.; 3], t),
            Err(KernelError::InvalidFaceFrame(_))
        ));
        assert!(model
            .face_frame(FaceId(1), [1., 0., 0.], [f64::INFINITY, 0., 0.], t)
            .is_err());
        assert!(matches!(
            model.face_frame(FaceId(1), [1., 1., 0.], [1e12, 0., 0.], t),
            Err(KernelError::NumericRange)
        ));
        let distant = translate(&model, [1e12, 0., 0.], t).unwrap().model;
        assert!(matches!(
            distant.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t),
            Err(KernelError::NumericRange)
        ));
        let mut invalid = model.clone();
        invalid.faces[0].loops[0] = LoopId(999);
        assert!(matches!(
            invalid.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], t),
            Err(KernelError::InvalidInput(_))
        ));
        assert!(matches!(
            model.face_frame(FaceId(1), [1., 0., 0.], [0.; 3], Tolerance { linear: 0. }),
            Err(KernelError::InvalidTolerance)
        ));
    }
}
