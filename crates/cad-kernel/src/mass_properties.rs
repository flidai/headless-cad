//! Signed tetrahedral integration of authoritative planar boundary faces.
//! Face fans are integration domains, not stored or display tessellation.
use crate::math::{cross, dot, sub};
use crate::*;

/// Uniform-density geometric properties. Volume is mm³, centroid is mm,
/// central inertia is mm⁵: ∫(|r|²I − rrᵀ)dV about the volume centroid.
/// Off-diagonal entries are negative products of inertia. Empty models have no
/// centroid or tensor. Disconnected solids are integrated together, not unioned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MassProperties {
    pub volume: f64,
    pub centroid: Option<[f64; 3]>,
    pub central_inertia: Option<[[f64; 3]; 3]>,
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
        if !self.value.is_finite() || !self.absolute.is_finite() {
            return Err(KernelError::NumericRange);
        }
        Ok(())
    }
    fn error(self) -> f64 {
        self.absolute * (128. * f64::EPSILON)
    }
}
fn product(a: f64, b: f64) -> Result<f64, KernelError> {
    let p = a * b;
    if !p.is_finite() || (a != 0. && b != 0. && !p.is_normal()) {
        Err(KernelError::NumericRange)
    } else {
        Ok(p)
    }
}
fn checked_add(a: f64, b: f64, tol: Tolerance) -> Result<f64, KernelError> {
    let sum = a + b;
    let recovered = sum - a;
    let residual = (a - (sum - recovered)) + (b - recovered);
    if !sum.is_finite() || !residual.is_finite() || residual.abs() > tol.linear / 16. {
        Err(KernelError::NumericRange)
    } else {
        Ok(sum)
    }
}
fn relative(a: [f64; 3], b: [f64; 3], tol: Tolerance) -> Result<[f64; 3], KernelError> {
    Ok([
        checked_add(a[0], -b[0], tol)?,
        checked_add(a[1], -b[1], tol)?,
        checked_add(a[2], -b[2], tol)?,
    ])
}
fn tetrahedra(
    model: &ModelSnapshot,
    shell: &Shell,
    mut visit: impl FnMut([[f64; 3]; 4], f64) -> Result<(), KernelError>,
) -> Result<(), KernelError> {
    let reference = model
        .loop_points(model.faces[shell.faces[0].0].loops[0])
        .unwrap()[0];
    for face in &shell.faces {
        let face = &model.faces[face.0];
        for &boundary in &face.loops {
            let p = model.loop_points(boundary).unwrap();
            for pair in p[1..].windows(2) {
                let determinant = dot(
                    sub(p[0], reference),
                    cross(sub(pair[0], reference), sub(pair[1], reference)),
                );
                let volume = determinant / 6.;
                if determinant != 0. && (!determinant.is_normal() || !volume.is_normal()) {
                    return Err(KernelError::NumericRange);
                }
                if volume != 0. {
                    visit([reference, p[0], pair[0], pair[1]], volume)?;
                }
            }
        }
    }
    Ok(())
}

impl ModelSnapshot {
    /// Validate and integrate volume, volume centroid, and central volume inertia.
    /// Shell-local reference points avoid translation-dependent volume loss;
    /// compensated first moments find the centroid, then a separate centered
    /// pass avoids subtracting large raw second moments. Arithmetic overflow,
    /// subnormal nonzero contributions, centroid error exceeding tolerance, and
    /// excessive signed-sum cancellation return `NumericRange` explicitly.
    pub fn mass_properties(&self, tolerance: Tolerance) -> Result<MassProperties, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if let Ok(g) = crate::hub_keyway::recognize(self, tolerance) {
            return g.properties(tolerance);
        }
        if let Ok(g) = crate::keyseat::recognize(self, tolerance) {
            return g.properties(tolerance);
        }
        if let Ok(g) = crate::stepped_bore::recognize(self, tolerance) {
            return g.properties(tolerance);
        }
        if let Ok(g) = crate::bored_prism::recognize(self, tolerance) {
            return g.properties(tolerance);
        }
        if let Some(g) = self.arc_prism_geometry(tolerance)? {
            return g.properties(tolerance);
        }
        if self.has_curved_geometry() {
            return crate::analytic::recognize(self, Some(tolerance))
                .and_then(|g| g.properties())
                .or_else(|_| {
                    crate::profile_prism::recognize(self, tolerance)?.properties(tolerance)
                })
                .or_else(|_| crate::revolve::recognize(self, tolerance)?.properties());
        }
        let report = self.validate(tolerance);
        if !report.valid {
            return Err(KernelError::InvalidInput(report));
        }
        if self.vertices.is_empty() {
            return Ok(MassProperties {
                volume: 0.,
                centroid: None,
                central_inertia: None,
            });
        }
        let reference = self.vertices[0].position;
        let mut volume = Sum::default();
        let mut moment = [Sum::default(); 3];
        for shell in &self.shells {
            tetrahedra(self, shell, |points, v| {
                volume.add(v)?;
                let [a, b, c, d] = points.map(|p| relative(p, reference, tolerance));
                let local = [a?, b?, c?, d?];
                for axis in 0..3 {
                    let average = local.iter().map(|p| p[axis] / 4.).sum::<f64>();
                    moment[axis].add(product(v, average)?)?;
                }
                Ok(())
            })?;
        }
        let v = volume.value;
        if !v.is_normal() || v <= 0. || volume.error() > v * 1e-10 {
            return Err(KernelError::NumericRange);
        }
        if (v - self.volume()).abs() > volume.error() * 4.0 {
            return Err(KernelError::NumericRange);
        }
        let local_center = moment.map(|s| s.value / v);
        let mut center = [0.; 3];
        for i in 0..3 {
            if moment[i].error() / v + local_center[i].abs() * volume.error() / v
                > tolerance.linear / 4.
            {
                return Err(KernelError::NumericRange);
            }
            center[i] = checked_add(reference[i], local_center[i], tolerance)?;
        }
        let mut inertia = [[Sum::default(); 3]; 3];
        for shell in &self.shells {
            tetrahedra(self, shell, |points, v| {
                // Keep centering in relative coordinates so the represented
                // world centroid is not subtracted from every large vertex.
                let mut p = [[0.; 3]; 4];
                for (i, point) in points.into_iter().enumerate() {
                    p[i] = relative(
                        relative(point, reference, tolerance)?,
                        local_center,
                        tolerance,
                    )?;
                }
                let sums: [f64; 3] = std::array::from_fn(|i| p.iter().map(|v| v[i]).sum());
                let mut second = [[0.; 3]; 3];
                let mut second_absolute = [[0.; 3]; 3];
                for i in 0..3 {
                    for j in i..3 {
                        let mut sum = Sum::default();
                        sum.add(product(sums[i], sums[j])?)?;
                        for point in p {
                            sum.add(product(point[i], point[j])?)?;
                        }
                        let average = sum.value / 20.;
                        if sum.value != 0. && !average.is_normal() {
                            return Err(KernelError::NumericRange);
                        }
                        second[i][j] = product(v, average)?;
                        second[j][i] = second[i][j];
                        second_absolute[i][j] = product(v.abs(), sum.absolute / 20.)?;
                        second_absolute[j][i] = second_absolute[i][j];
                    }
                }
                for i in 0..3 {
                    for j in i..3 {
                        let value = if i == j {
                            second[(i + 1) % 3][(i + 1) % 3] + second[(i + 2) % 3][(i + 2) % 3]
                        } else {
                            -second[i][j]
                        };
                        inertia[i][j].add(value)?;
                        // Include inner product cancellation in the error
                        // envelope, even when the final off-diagonal is zero.
                        inertia[i][j].absolute += if i == j {
                            second_absolute[(i + 1) % 3][(i + 1) % 3]
                                + second_absolute[(i + 2) % 3][(i + 2) % 3]
                        } else {
                            second_absolute[i][j]
                        };
                    }
                }
                Ok(())
            })?;
        }
        let trace = (0..3).map(|i| inertia[i][i].value).sum::<f64>();
        if !trace.is_normal() || trace <= 0. {
            return Err(KernelError::NumericRange);
        }
        let mut tensor = [[0.; 3]; 3];
        for i in 0..3 {
            for j in i..3 {
                if inertia[i][j].error() > trace * 1e-10 {
                    return Err(KernelError::NumericRange);
                }
                tensor[i][j] = inertia[i][j].value;
                tensor[j][i] = tensor[i][j];
            }
        }
        if (0..3).any(|i| tensor[i][i] <= 0.) {
            return Err(KernelError::NumericRange);
        }
        Ok(MassProperties {
            volume: v,
            centroid: Some(center),
            central_inertia: Some(tensor),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9_f64.max(b.abs() * 1e-12), "{a} != {b}");
    }
    fn box_model(w: f64, d: f64, h: f64) -> ModelSnapshot {
        extrude_rectangle(w, d, h, Tolerance::default())
            .unwrap()
            .model
    }
    #[test]
    fn box_and_triangle_prism_formulas() {
        let t = Tolerance::default();
        let box_p = box_model(2., 3., 4.).mass_properties(t).unwrap();
        close(box_p.volume, 24.);
        assert_eq!(box_p.centroid, Some([1., 1.5, 2.]));
        let tensor = box_p.central_inertia.unwrap();
        for (i, expected) in [50., 40., 26.].into_iter().enumerate() {
            close(tensor[i][i], expected);
        }
        close(tensor[0][1], 0.);
        let (a, b, h) = (8., 6., 5.);
        let triangle = extrude_polygon(&[[0., 0.], [a, 0.], [0., b]], h, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        let v = a * b * h / 2.;
        close(triangle.volume, v);
        for (actual, expected) in
            triangle
                .centroid
                .unwrap()
                .into_iter()
                .zip([a / 3., b / 3., h / 2.])
        {
            close(actual, expected);
        }
        let tensor = triangle.central_inertia.unwrap();
        close(tensor[0][0], v * (b * b / 18. + h * h / 12.));
        close(tensor[1][1], v * (a * a / 18. + h * h / 12.));
        close(tensor[2][2], v * (a * a + b * b) / 18.);
        close(tensor[0][1], v * a * b / 36.);
        close(tensor[0][2], 0.);
        close(tensor[1][2], 0.);
        assert_eq!(tensor[0][1], tensor[1][0]);
        // Computing trace(Q)-Qxx would erase precision on this long, thin body.
        let slender = box_model(1e5, 1., 2.).mass_properties(t).unwrap();
        close(slender.central_inertia.unwrap()[0][0], 2e5 * 5. / 12.);
    }
    #[test]
    fn nonconvex_cut_and_disconnected_parallel_axis() {
        let t = Tolerance::default();
        let full = box_model(4., 4., 2.);
        let cut = translate(&box_model(2., 2., 2.), [2., 2., 0.], t)
            .unwrap()
            .model;
        let shape = boolean(&full, &cut, BooleanOp::Difference, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        close(shape.volume, 24.);
        let center = [5. / 3., 5. / 3., 1.];
        for (actual, expected) in shape.centroid.unwrap().into_iter().zip(center) {
            close(actual, expected);
        }
        let mut expected = [[0.; 3]; 3];
        for (sign, volume, c, diag) in [
            (
                1.,
                32.,
                [2., 2., 1.],
                [32. * 20. / 12., 32. * 20. / 12., 32. * 32. / 12.],
            ),
            (-1., 8., [3., 3., 1.], [16. / 3.; 3]),
        ] {
            let d = sub(c, center);
            let squared = dot(d, d);
            for i in 0..3 {
                for j in 0..3 {
                    expected[i][j] += sign
                        * (if i == j {
                            diag[i] + volume * (squared - d[i] * d[j])
                        } else {
                            -volume * d[i] * d[j]
                        });
                }
            }
        }
        let actual = shape.central_inertia.unwrap();
        for i in 0..3 {
            for j in 0..3 {
                close(actual[i][j], expected[i][j]);
            }
        }
        let cube = box_model(2., 2., 2.);
        let far = translate(&cube, [6., 0., 0.], t).unwrap().model;
        let disconnected = boolean(&cube, &far, BooleanOp::Union, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        assert_eq!(disconnected.centroid, Some([4., 1., 1.]));
        let tensor = disconnected.central_inertia.unwrap();
        close(tensor[0][0], 32. / 3.);
        close(tensor[1][1], 464. / 3.);
        close(tensor[2][2], 464. / 3.);
    }
    #[test]
    fn tensor_rotation_and_translation_invariance() {
        let t = Tolerance::default();
        let model = box_model(2., 3., 4.);
        let angle = 0.6_f64;
        let q = [0., 0., (angle / 2.).sin(), (angle / 2.).cos()];
        let moved = transform(&model, [10., -7., 5.], q, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        let r = [
            [angle.cos(), -angle.sin(), 0.],
            [angle.sin(), angle.cos(), 0.],
            [0., 0., 1.],
        ];
        let expected: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| (0..3).map(|k| r[i][k] * [50., 40., 26.][k] * r[j][k]).sum())
        });
        let tensor = moved.central_inertia.unwrap();
        for i in 0..3 {
            for j in 0..3 {
                close(tensor[i][j], expected[i][j]);
            }
        }
        assert!(tensor[0][1] > 0.);
        let original = box_model(2., 4., 6.);
        let large = translate(&original, [1e12, -1e12, 1e12], t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        assert_eq!(large.centroid, Some([1e12 + 1., -1e12 + 2., 1e12 + 3.]));
        assert_eq!(
            large.central_inertia,
            original.mass_properties(t).unwrap().central_inertia
        );
    }
    #[test]
    fn empty_invalid_and_numeric_limits() {
        let t = Tolerance::default();
        let cube = box_model(2., 2., 2.);
        let far = translate(&cube, [10., 0., 0.], t).unwrap().model;
        let empty = boolean(&cube, &far, BooleanOp::Intersection, t)
            .unwrap()
            .model
            .mass_properties(t)
            .unwrap();
        assert_eq!(
            empty,
            MassProperties {
                volume: 0.,
                centroid: None,
                central_inertia: None
            }
        );
        let mut bad = cube.clone();
        bad.edges[0].vertices[0] = VertexId(999);
        assert!(matches!(
            bad.mass_properties(t),
            Err(KernelError::InvalidInput(_))
        ));
        let tiny_tol = Tolerance { linear: 1e-80 };
        let tiny = extrude_rectangle(1e-70, 1e-70, 1e-70, tiny_tol)
            .unwrap()
            .model;
        assert!(matches!(
            tiny.mass_properties(tiny_tol),
            Err(KernelError::NumericRange)
        ));
        let huge = extrude_rectangle(1e70, 1e70, 1e70, t).unwrap().model;
        assert!(matches!(
            huge.mass_properties(t),
            Err(KernelError::NumericRange)
        ));
        let triangle = extrude_polygon(&[[0., 0.], [1., 0.], [0., 1.]], 1., t)
            .unwrap()
            .model;
        let distant = translate(&triangle, [1e12, 0., 0.], t).unwrap().model;
        assert!(matches!(
            distant.mass_properties(t),
            Err(KernelError::NumericRange)
        ));
    }
}
