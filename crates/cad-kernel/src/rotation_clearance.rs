//! Complete fixed-axis rotational path clearance using native distance certificates.
use crate::*;
pub const MAX_ROTATION_QUERIES: usize = 1025;
pub const MAX_ROTATION_DEPTH: usize = 32;
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
    /// Native midpoint distances plus an outward-rounded radius/speed bound prove
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
        assert!(matches!(
            a.rotation_clearance(
                &b,
                [0.; 3],
                [0., 0., 1.],
                std::f64::consts::TAU,
                7.9999,
                t,
                1025
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
