//! Complete fixed-axis rotational path clearance using native distance certificates.
use crate::*;
pub const MAX_ROTATION_QUERIES: usize = 1025;
pub const MAX_ROTATION_DEPTH: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationProofKind {
    LipschitzSubdivision,
    RotationalSymmetry,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationalSymmetryCertificate {
    pub start: f64,
    pub end: f64,
    pub reference_sample_index: usize,
    pub stationary_axis_origin: [f64; 3],
    pub stationary_axis: [f64; 3],
    pub canonical_boundary_uncertainty_mm: f64,
    pub axis_offset_mm: f64,
    pub axis_direction_departure: f64,
    pub stationary_extent_mm: f64,
    /// Bound on variation of the distance, NOT displacement of moving points.
    pub distance_variation_bound_mm: f64,
    pub lower_bound_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationSample {
    pub time: f64,
    pub query: SolidClearance,
    pub rotation_uncertainty_mm: f64,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationInterval {
    pub start: f64,
    pub end: f64,
    pub midpoint: f64,
    pub sample_index: usize,
    pub lower_bound_mm: f64,
    pub displacement_bound_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationClearance {
    pub proof_kind: RotationProofKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symmetry: Option<RotationalSymmetryCertificate>,
    /// Lower bound over the complete declared signed rotation, including endpoints.
    pub lower_bound_mm: f64,
    /// Minimum sampled upper bound; the global enclosure need not be tight.
    pub upper_bound_mm: f64,
    pub required_minimum_mm: f64,
    pub pivot: [f64; 3],
    pub axis: [f64; 3],
    pub angle_radians: f64,
    pub radius_bound_mm: f64,
    pub speed_bound_mm: f64,
    pub query_count: usize,
    pub samples: Vec<RotationSample>,
    pub intervals: Vec<RotationInterval>,
}
fn failed(message: &str) -> KernelError {
    KernelError::Unsupported(format!("continuous rotation clearance: {message}"))
}
fn norm(p: [f64; 3]) -> f64 {
    p[0].hypot(p[1]).hypot(p[2])
}
fn rotate(p: [f64; 3], q: [f64; 4]) -> [f64; 3] {
    let [x, y, z, w] = q;
    let m = [
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
    m.map(|r| r.into_iter().zip(p).map(|(a, b)| a * b).sum())
}
/// Recover the WHOLE stationary native solid, never a bounding stock cylinder.
/// The recognizers certify canonical topology; strict consistency certifies its
/// actual supports against the recovered parameterization. No intersections are
/// recomputed here (which could amplify small support errors at grazing angles).
fn symmetry_certificate(
    model: &ModelSnapshot,
    pivot: [f64; 3],
    axis: [f64; 3],
    angle: f64,
    tolerance: Tolerance,
) -> Option<RotationalSymmetryCertificate> {
    use crate::convex_clearance::support::{circular_consistency, revolution_consistency};
    use crate::math::{cross, dot, sub};
    let circle = model.cylinder_geometry(tolerance).ok().flatten();
    let revolution = if circle.is_none() {
        crate::revolve::recognize(model, tolerance).ok()
    } else {
        None
    };
    // First symmetry domain: orthogonal meridians only. On a cone, an
    // arithmetic axial support-origin departure can produce radial departure
    // amplified by |slope| at a fixed trim level. The bound below deliberately
    // does not claim to certify that conditioning. Cones retain subdivision.
    if let Some(g) = &revolution {
        if g.profile
            .iter()
            .zip(g.profile.iter().cycle().skip(1))
            .any(|(a, b)| a[0] != b[0] && a[1] != b[1])
        {
            return None;
        }
    }
    let (origin, mut native_axis, extent) = if let Some(g) = &circle {
        (g.origin, g.axis, g.radius + g.height)
    } else if let Some(g) = &revolution {
        (
            g.origin,
            g.axis,
            g.profile
                .iter()
                .map(|p| p[0] + p[1].abs())
                .fold(0., f64::max),
        )
    } else {
        return None;
    };
    let mut scale = norm(origin) + norm(pivot) + extent;
    for point in model
        .vertices
        .iter()
        .map(|v| v.position)
        .chain(model.edges.iter().map(|e| match &e.curve {
            Curve::Line(g) => g.origin,
            Curve::Circle(g) => g.origin,
            Curve::CircularArc(g) => g.circle.origin,
        }))
        .chain(model.faces.iter().map(|f| match &f.surface {
            Surface::Plane(g) => g.origin,
            Surface::Cylinder(g) => g.origin,
            Surface::Cone(g) => g.origin,
        }))
    {
        scale = scale.max(norm(point) + norm(pivot) + extent);
    }
    let error = (256. * f64::EPSILON * scale).next_up();
    if !error.is_normal() || !extent.is_normal() || !scale.is_finite() {
        return None;
    }
    if let Some(g) = &circle {
        circular_consistency(model, g, error).ok()?;
    } else {
        revolution_consistency(model, revolution.as_ref()?, error).ok()?;
    }
    // A parameter-matched plane/cylinder patch has origin, radius, two
    // orthonormal radial basis vectors, and axial direction contributions.
    // Orthogonal meridians have no slope amplification of axial trim errors.
    // Strict consistency bounds each physical contribution by
    // error; cross-product basis reconstruction contributes at most 3 error.
    // Native trim endpoints contribute another 2 error. Thus 16 error encloses
    // the pointwise patch/trim correspondence. Doubling to 32 also reserves
    // unit-frame normalization and arithmetic evaluation. Fixed native topology
    // and the same filled meridian provide a two-sided boundary correspondence;
    // this is not an inference from a meshed silhouette or an arbitrary set of
    // approximately equal supporting planes.
    let boundary_error = (32. * error).next_up();
    let n = norm(native_axis);
    if !n.is_finite() || (n - 1.).abs() > 64. * f64::EPSILON {
        return None;
    }
    native_axis = native_axis.map(|x| x / n);
    if dot(native_axis, axis) < 0. {
        native_axis = native_axis.map(|x| -x);
    }
    let departure = (norm(sub(native_axis, axis)) + 32. * f64::EPSILON).next_up();
    let offset = (norm(cross(sub(origin, pivot), axis)) + error).next_up();
    // Arithmetic-consistent axes only. A modeling-tolerance-sized offset or
    // tilt is not symmetry. Their residuals still widen the final certificate.
    if offset > 4. * error || departure * extent > 4. * error {
        return None;
    }
    // For ideal stationary L0: Q_a L0=L0. The Duhamel bound for skew rotation
    // generators gives ||Q_b-Q_a|| <= |angle| ||b-a||. Moving the pivot axis
    // contributes <=2 offset. Replacing L by L0 on both sides costs 2 delta.
    let variation =
        (2. * boundary_error + 2. * offset + angle.abs() * departure * extent + 16. * error)
            .next_up();
    if !variation.is_finite() || variation > tolerance.linear / 8. {
        return None;
    }
    Some(RotationalSymmetryCertificate {
        start: 0.,
        end: 1.,
        reference_sample_index: 0,
        stationary_axis_origin: origin,
        stationary_axis: native_axis,
        canonical_boundary_uncertainty_mm: boundary_error,
        axis_offset_mm: offset,
        axis_direction_departure: departure,
        stationary_extent_mm: (extent + boundary_error).next_up(),
        distance_variation_bound_mm: variation,
        lower_bound_mm: 0.,
    })
}
struct Evaluator<'a> {
    left: &'a ModelSnapshot,
    right: &'a ModelSnapshot,
    pivot: [f64; 3],
    axis: [f64; 3],
    angle: f64,
    minimum: f64,
    tolerance: Tolerance,
    budget: usize,
    source_scale: f64,
    samples: Vec<RotationSample>,
}
impl Evaluator<'_> {
    fn sample(&mut self, time: f64) -> Result<usize, KernelError> {
        if self.samples.len() >= self.budget {
            return Err(KernelError::ResourceLimit(
                "continuous rotation native-query budget exhausted before full interval coverage"
                    .into(),
            ));
        }
        let stationary = time == 0. || self.angle == 0.;
        // Includes angle multiplication, bounded sin/cos evaluation, quaternion
        // matrix evaluation, pivot subtraction and every native support transform.
        let uncertainty = if stationary {
            0.
        } else {
            (2048. * f64::EPSILON * self.source_scale).next_up()
        };
        if !uncertainty.is_finite() || uncertainty > self.tolerance.linear / 32. {
            return Err(KernelError::NumericRange);
        }
        let placed;
        let right = if stationary {
            self.right
        } else {
            let (sin, cos) = (self.angle * time / 2.).sin_cos();
            let q = [
                self.axis[0] * sin,
                self.axis[1] * sin,
                self.axis[2] * sin,
                cos,
            ];
            let rotated = rotate(self.pivot, q);
            let offset = std::array::from_fn(|i| self.pivot[i] - rotated[i]);
            placed = transform(self.right, offset, q, self.tolerance)?.model;
            &placed
        };
        let query = self.left.clearance(right, self.tolerance)?;
        let lower = (query.lower_bound_mm - uncertainty).next_down();
        let upper = (query.upper_bound_mm + uncertainty).next_up();
        if lower < self.minimum {
            return Err(failed("a sampled pose does not certify the required minimum; contact, insufficient clearance, or numerical threshold uncertainty cannot pass"));
        }
        let index = self.samples.len();
        self.samples.push(RotationSample {
            time,
            query,
            rotation_uncertainty_mm: uncertainty,
            lower_bound_mm: lower,
            upper_bound_mm: upper,
        });
        Ok(index)
    }
}
impl ModelSnapshot {
    /// Certify positive material separation through the complete signed angle
    /// (not quaternion shortest-path interpolation). Axis must be unit within
    /// arithmetic precision; normalization only removes that arithmetic residual.
    /// A zero angle is an explicitly supported stationary path.
    /// A complete canonical stationary orthogonal-meridian revolution with an
    /// arithmetic-consistent rotation axis can prove the whole path by rotational
    /// symmetry. Stationary cones/frustums retain subdivision. Otherwise
    /// native midpoint distances plus an outward-rounded radius/speed bound prove
    /// every dyadic interval. Unsupported samples and work exhaustion fail closed.
    #[allow(clippy::too_many_arguments)]
    pub fn rotation_clearance(
        &self,
        other: &Self,
        pivot: [f64; 3],
        axis: [f64; 3],
        angle_radians: f64,
        required_min: f64,
        tolerance: Tolerance,
        max_queries: usize,
    ) -> Result<RotationClearance, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        let axis_norm = norm(axis);
        if !axis_norm.is_finite()
            || (axis_norm - 1.).abs() > 64. * f64::EPSILON
            || pivot.iter().any(|v| !v.is_finite() || v.abs() > 1e50)
            || !angle_radians.is_finite()
            || angle_radians.abs() > std::f64::consts::TAU
            || !required_min.is_finite()
            || required_min < 0.
        {
            return Err(KernelError::NumericRange);
        }
        if !(3..=MAX_ROTATION_QUERIES).contains(&max_queries) {
            return Err(KernelError::ResourceLimit(format!(
                "continuous rotation query budget must be 3..={MAX_ROTATION_QUERIES}"
            )));
        }
        self.validate_clearance_operand(tolerance)?;
        other.validate_clearance_operand(tolerance)?;
        let axis = axis.map(|v| v / axis_norm);
        let bounds = other.bounds();
        let mut radius = 0_f64;
        for mask in 0..8 {
            let corner: [f64; 3] = std::array::from_fn(|i| {
                if mask & (1 << i) == 0 {
                    bounds.min[i]
                } else {
                    bounds.max[i]
                }
            });
            radius = radius.max(norm(std::array::from_fn(|i| corner[i] - pivot[i])));
        }
        let mut source_scale = bounds
            .min
            .iter()
            .chain(&bounds.max)
            .map(|v| v.abs())
            .fold(norm(pivot), f64::max)
            + radius;
        for origin in other
            .edges
            .iter()
            .map(|e| match &e.curve {
                Curve::Line(g) => g.origin,
                Curve::Circle(g) => g.origin,
                Curve::CircularArc(g) => g.circle.origin,
            })
            .chain(other.faces.iter().map(|f| match &f.surface {
                Surface::Plane(g) => g.origin,
                Surface::Cylinder(g) => g.origin,
                Surface::Cone(g) => g.origin,
            }))
        {
            source_scale = source_scale.max(norm(origin) + norm(pivot) + radius);
        }
        if !source_scale.is_finite() || !radius.is_normal() || radius > 1e50 {
            return Err(KernelError::NumericRange);
        }
        let radius_bound_mm = (radius + 4096. * f64::EPSILON * source_scale).next_up();
        let speed_bound_mm = if angle_radians == 0. {
            0.
        } else {
            (radius_bound_mm * angle_radians.abs() * (1. + 32. * f64::EPSILON)).next_up()
        };
        let mut evaluator = Evaluator {
            left: self,
            right: other,
            pivot,
            axis,
            angle: angle_radians,
            minimum: required_min,
            tolerance,
            budget: max_queries,
            source_scale,
            samples: vec![],
        };
        evaluator.sample(0.)?;
        evaluator.sample(1.)?;
        if let Some(mut symmetry) =
            symmetry_certificate(self, pivot, axis, angle_radians, tolerance)
        {
            let lower = (evaluator.samples[0].lower_bound_mm
                - symmetry.distance_variation_bound_mm)
                .next_down();
            if lower >= required_min {
                evaluator.sample(0.5)?;
                let upper = evaluator
                    .samples
                    .iter()
                    .map(|s| s.upper_bound_mm)
                    .fold(f64::INFINITY, f64::min);
                if !lower.is_finite() || !upper.is_finite() || lower > upper {
                    return Err(KernelError::NumericRange);
                }
                symmetry.lower_bound_mm = lower;
                return Ok(RotationClearance {
                    proof_kind: RotationProofKind::RotationalSymmetry,
                    symmetry: Some(symmetry),
                    lower_bound_mm: lower,
                    upper_bound_mm: upper,
                    required_minimum_mm: required_min,
                    pivot,
                    axis,
                    angle_radians,
                    radius_bound_mm,
                    speed_bound_mm,
                    query_count: evaluator.samples.len(),
                    samples: evaluator.samples,
                    intervals: vec![],
                });
            }
        }
        let mut pending = vec![(0., 1., 0usize)];
        let mut intervals = vec![];
        while let Some((start, end, depth)) = pending.pop() {
            let midpoint = (start + end) / 2.;
            let index = evaluator.sample(midpoint)?;
            let displacement = if speed_bound_mm == 0. {
                0.
            } else {
                (speed_bound_mm * ((end - start) / 2.)).next_up()
            };
            let lower = (evaluator.samples[index].lower_bound_mm - displacement).next_down();
            if lower >= required_min {
                intervals.push(RotationInterval {
                    start,
                    end,
                    midpoint,
                    sample_index: index,
                    lower_bound_mm: lower,
                    displacement_bound_mm: displacement,
                });
            } else {
                if depth >= MAX_ROTATION_DEPTH {
                    return Err(KernelError::ResourceLimit("continuous rotation subdivision depth exhausted before full interval coverage".into()));
                }
                pending.push((midpoint, end, depth + 1));
                pending.push((start, midpoint, depth + 1));
            }
        }
        intervals.sort_by(|a, b| a.start.total_cmp(&b.start));
        let lower_bound_mm = intervals
            .iter()
            .map(|i| i.lower_bound_mm)
            .fold(f64::INFINITY, f64::min);
        let upper_bound_mm = evaluator
            .samples
            .iter()
            .map(|s| s.upper_bound_mm)
            .fold(f64::INFINITY, f64::min);
        if !lower_bound_mm.is_finite()
            || !upper_bound_mm.is_finite()
            || lower_bound_mm > upper_bound_mm
        {
            return Err(KernelError::NumericRange);
        }
        Ok(RotationClearance {
            proof_kind: RotationProofKind::LipschitzSubdivision,
            symmetry: None,
            lower_bound_mm,
            upper_bound_mm,
            required_minimum_mm: required_min,
            pivot,
            axis,
            angle_radians,
            radius_bound_mm,
            speed_bound_mm,
            query_count: evaluator.samples.len(),
            samples: evaluator.samples,
            intervals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inspect(q: &RotationClearance, expected: f64) {
        assert!(
            q.lower_bound_mm <= expected && q.upper_bound_mm >= expected,
            "{q:?}"
        );
        assert!(q.lower_bound_mm >= q.required_minimum_mm);
        assert_eq!(q.query_count, q.samples.len());
        assert_eq!(q.samples[0].time, 0.);
        assert_eq!(q.samples[1].time, 1.);
        if let Some(proof) = &q.symmetry {
            assert_eq!(q.proof_kind, RotationProofKind::RotationalSymmetry);
            assert!(q.intervals.is_empty());
            assert_eq!((proof.start, proof.end), (0., 1.));
            assert_eq!(q.query_count, 3);
            assert_eq!(q.lower_bound_mm, proof.lower_bound_mm);
            return;
        }
        assert_eq!(q.proof_kind, RotationProofKind::LipschitzSubdivision);
        assert_eq!(q.intervals[0].start, 0.);
        assert_eq!(q.intervals.last().unwrap().end, 1.);
        for pair in q.intervals.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        for i in &q.intervals {
            assert_eq!(q.samples[i.sample_index].time, i.midpoint);
            assert!(
                i.lower_bound_mm
                    <= q.samples[i.sample_index].lower_bound_mm - i.displacement_bound_mm
            );
        }
        assert!(q.speed_bound_mm >= q.radius_bound_mm * q.angle_radians.abs());
    }
    #[test]
    fn full_signed_orbits_negative_common_pose_and_scale() {
        let t = Tolerance::default();
        for scale in [0.01, 1., 10.] {
            let a = extrude_circle(scale, 2. * scale, t).unwrap().model;
            let b = translate(&a, [10. * scale, 0., 0.], t).unwrap().model;
            for angle in [
                std::f64::consts::TAU,
                -std::f64::consts::TAU,
                std::f64::consts::PI,
            ] {
                let q = a
                    .rotation_clearance(&b, [0.; 3], [0., 0., 1.], angle, 7. * scale, t, 1025)
                    .unwrap();
                inspect(&q, 8. * scale);
                assert!(
                    q.radius_bound_mm
                        >= ((11. * scale).powi(2) + (scale).powi(2) + (2. * scale).powi(2)).sqrt()
                );
                let theta = 0.6_f64;
                let pose = [0., (theta / 2.).sin(), 0., (theta / 2.).cos()];
                let pivot = [20., -8., 3.];
                let left = transform(&a, pivot, pose, t).unwrap().model;
                let right = transform(&b, pivot, pose, t).unwrap().model;
                let axis = [theta.sin(), 0., theta.cos()];
                inspect(
                    &left
                        .rotation_clearance(&right, pivot, axis, angle, 7. * scale, t, 1025)
                        .unwrap(),
                    8. * scale,
                );
            }
        }
    }
    fn retained_pair(scale: f64) -> (ModelSnapshot, ModelSnapshot) {
        let t = Tolerance::default();
        let shaft = revolve_polygon(
            &[
                [0., 0.],
                [8., 0.],
                [8., 12.9],
                [6., 12.9],
                [6., 94.55],
                [5.4, 94.55],
                [5.4, 95.8],
                [6., 95.8],
                [6., 130.],
                [0., 130.],
            ]
            .map(|p| p.map(|x| x * scale)),
            t,
        )
        .unwrap()
        .model;
        let tool = translate(
            &extrude_rectangle(4. * scale, 4.04 * scale, 13.5 * scale, t)
                .unwrap()
                .model,
            [4. * scale, -2.02 * scale, 45. * scale],
            t,
        )
        .unwrap()
        .model;
        let shaft = boolean(&shaft, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
        let bush = revolve_polygon(
            &[
                [6.15, 0.],
                [12., 0.],
                [12., 2.],
                [10., 2.],
                [10., 18.],
                [6.15, 18.],
            ]
            .map(|p| p.map(|x| x * scale)),
            t,
        )
        .unwrap()
        .model;
        (
            translate(&bush, [0., 0., 13. * scale], t).unwrap().model,
            shaft,
        )
    }
    #[test]
    fn retained_keyed_shaft_full_turn_flanged_bush_has_three_query_native_proof() {
        let t = Tolerance::default();
        for scale in [0.5, 1., 2.] {
            let (bush, shaft) = retained_pair(scale);
            // Real drive placement: shaft local Z becomes world X at height32.
            for pose in [[0.5, 0.5, 0.5, 0.5], [0.5, 0.5, 0., 0.5_f64.sqrt()]] {
                let pivot = [10., 0., 32.];
                let a = transform(&bush, pivot, pose, t).unwrap().model;
                let b = transform(&shaft, pivot, pose, t).unwrap().model;
                let native_axis = rotate([0., 0., 1.], pose);
                for sign in [-1., 1.] {
                    for reverse_axis in [false, true] {
                        let axis = native_axis.map(|x| if reverse_axis { -x } else { x });
                        let q = a
                            .rotation_clearance(
                                &b,
                                pivot,
                                axis,
                                sign * std::f64::consts::TAU,
                                0.09 * scale,
                                t,
                                3,
                            )
                            .unwrap_or_else(|e| panic!("scale{scale} pose{pose:?}: {e}"));
                        inspect(&q, 0.1 * scale);
                        assert_eq!(q.proof_kind, RotationProofKind::RotationalSymmetry);
                        assert!(q.speed_bound_mm > 100. * scale);
                        let proof = q.symmetry.as_ref().unwrap();
                        assert!(proof.distance_variation_bound_mm > 0.);
                        assert!(proof.distance_variation_bound_mm <= t.linear / 8.);
                        for sample in &q.samples {
                            assert_eq!(sample.query.method, ClearanceMethod::NativeTrimWitness);
                            assert!(
                                sample.lower_bound_mm <= 0.1 * scale
                                    && sample.upper_bound_mm >= 0.1 * scale
                            );
                            assert!(
                                norm(std::array::from_fn(
                                    |i| sample.query.right_point[i] - sample.query.left_point[i]
                                )) <= sample.upper_bound_mm
                            );
                        }
                        let encoded = serde_json::to_value(&q).unwrap();
                        assert_eq!(encoded["proof_kind"], "rotational_symmetry");
                        assert_eq!(
                            serde_json::from_value::<RotationClearance>(encoded).unwrap(),
                            q
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn symmetry_requires_whole_native_boundary_and_arithmetic_axis_consistency() {
        let t = Tolerance::default();
        let (bush, shaft) = retained_pair(1.);
        assert!(symmetry_certificate(&bush, [0.; 3], [0., 0., 1.], 1., t).is_some());
        assert!(symmetry_certificate(&shaft, [0.; 3], [0., 0., 1.], 1., t).is_none());
        for pivot in [[1e-8, 0., 0.], [0.1, 0., 0.]] {
            assert!(symmetry_certificate(&bush, pivot, [0., 0., 1.], 1., t).is_none());
        }
        let angle = 1e-8_f64;
        assert!(
            symmetry_certificate(&bush, [0.; 3], [angle.sin(), 0., angle.cos()], 1., t).is_none()
        );
        let mut malformed = bush.clone();
        if let Curve::Circle(c) = &mut malformed.edges[0].curve {
            c.radius += 1e-8;
        }
        assert!(symmetry_certificate(&malformed, [0.; 3], [0., 0., 1.], 1., t).is_none());
        malformed = bush.clone();
        malformed.loops[0].edges.pop();
        assert!(malformed
            .rotation_clearance(&shaft, [0.; 3], [0., 0., 1.], 1., 0.09, t, 3)
            .is_err());
        // A native query passes at .1, but no fabricated symmetry report may
        // certify a minimum above its conservative lower bound.
        assert!(bush
            .rotation_clearance(
                &shaft,
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                0.1,
                t,
                3
            )
            .is_err());
    }
    #[test]
    fn conical_stationary_supports_use_subdivision_not_unconditioned_symmetry() {
        let t = Tolerance::default();
        for height in [2., 0.001] {
            let frustum = revolve_polygon(&[[0., 0.], [2., 0.], [1., height], [0., height]], t)
                .unwrap()
                .model;
            assert!(symmetry_certificate(&frustum, [0.; 3], [0., 0., 1.], 0.001, t).is_none());
            let other = translate(&extrude_circle(1., 2., t).unwrap().model, [10., 0., 0.], t)
                .unwrap()
                .model;
            let q = frustum
                .rotation_clearance(&other, [0.; 3], [0., 0., 1.], 0.001, 6., t, 3)
                .unwrap();
            assert_eq!(q.proof_kind, RotationProofKind::LipschitzSubdivision);
            assert!(q.symmetry.is_none());
            assert_eq!(q.intervals.len(), 1);
        }
    }
    #[test]
    fn arithmetic_axis_residual_is_charged_not_claimed_exact_symmetry() {
        let t = Tolerance::default();
        let a = extrude_circle(1., 2., t).unwrap().model;
        let b = translate(&a, [10., 0., 0.], t).unwrap().model;
        let offset = 1e-13;
        let q = a
            .rotation_clearance(
                &b,
                [offset, 0., 0.],
                [0., 0., 1.],
                std::f64::consts::TAU,
                7.9999,
                t,
                3,
            )
            .unwrap();
        let proof = q.symmetry.as_ref().unwrap();
        // The centers approach by exactly twice the pivot offset after half a
        // turn; this is a small departure from symmetry, not exact invariance.
        assert!(proof.axis_offset_mm >= offset);
        assert!(proof.distance_variation_bound_mm > 2. * offset);
        assert!(q.lower_bound_mm <= 8. - 2. * offset);
        assert!(q.upper_bound_mm >= 8. - 2. * offset);
        assert!(q.speed_bound_mm > 60.);
        let mut encoded = serde_json::to_value(q).unwrap();
        encoded["symmetry"]["unverified_claim"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RotationClearance>(encoded).is_err());
    }
    #[test]
    fn quarter_rotation_collision_despite_free_endpoints_and_midpoint() {
        let t = Tolerance::default();
        let cylinder = extrude_circle(1., 2., t).unwrap().model;
        let left = translate(&cylinder, [0., 10., 0.], t).unwrap().model;
        let right = translate(&cylinder, [10., 0., 0.], t).unwrap().model;
        for time in [0., 0.5, 1.] {
            let (s, c) = (std::f64::consts::TAU * time / 2.).sin_cos();
            let posed = transform(&right, [0.; 3], [0., 0., s, c], t).unwrap().model;
            assert!(left.clearance(&posed, t).is_ok());
        }
        assert!(left
            .rotation_clearance(
                &right,
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                1.,
                t,
                1025
            )
            .is_err());
    }
    #[test]
    fn work_threshold_axis_angle_and_precision_fail_closed() {
        let t = Tolerance::default();
        let a = extrude_circle(1., 2., t).unwrap().model;
        let b = translate(&a, [10., 0., 0.], t).unwrap().model;
        let off_axis = translate(&a, [0.1, 0., 0.], t).unwrap().model;
        assert!(matches!(
            off_axis.rotation_clearance(
                &b,
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                7.8999,
                t,
                3
            ),
            Err(KernelError::ResourceLimit(_))
        ));
        assert!(a
            .rotation_clearance(&b, [0.; 3], [0., 0., 1.], 1., 9., t, 1025)
            .is_err());
        for axis in [[0.; 3], [0., 0., 2.], [f64::NAN, 0., 1.]] {
            assert!(a
                .rotation_clearance(&b, [0.; 3], axis, 1., 1., t, 1025)
                .is_err());
        }
        for angle in [std::f64::consts::TAU + 1e-10, f64::NAN] {
            assert!(a
                .rotation_clearance(&b, [0.; 3], [0., 0., 1.], angle, 1., t, 1025)
                .is_err());
        }
        assert!(a
            .rotation_clearance(&b, [1e8, 0., 0.], [0., 0., 1.], 1., 1., t, 1025)
            .is_err());
        let near_unit = [0., 0., 1. + 16. * f64::EPSILON];
        let q = a
            .rotation_clearance(&b, [0.; 3], near_unit, 1., 7., t, 1025)
            .unwrap();
        inspect(&q, 8.);
        assert_eq!(q.axis, [0., 0., 1.]);
        let q = a
            .rotation_clearance(&b, [0.; 3], [0., 0., 1.], 0., 7., t, 3)
            .unwrap();
        inspect(&q, 8.);
        assert_eq!(q.query_count, 3);
    }
}
