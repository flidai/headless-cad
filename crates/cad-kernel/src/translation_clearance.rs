//! Continuous rigid translation certified by native distance and Lipschitz bounds.
use crate::*;
pub const MAX_TRANSLATION_QUERIES: usize = 1025;
pub const MAX_TRANSLATION_DEPTH: usize = 32;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationSample {
    pub time: f64,
    pub query: SolidClearance,
    pub translation_uncertainty_mm: f64,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationInterval {
    pub start: f64,
    pub end: f64,
    pub midpoint: f64,
    pub sample_index: usize,
    pub lower_bound_mm: f64,
    pub displacement_bound_mm: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationClearance {
    /// Certified lower bound on minimum clearance over the complete closed path.
    pub lower_bound_mm: f64,
    /// Minimum sampled upper bound; not a tight enclosure requirement.
    pub upper_bound_mm: f64,
    pub required_minimum_mm: f64,
    pub translation: [f64; 3],
    pub query_count: usize,
    pub samples: Vec<TranslationSample>,
    /// Sorted disjoint interval interiors whose union is exactly [0,1] for
    /// the generic Lipschitz proof. Empty only with `slot_motion_proof`.
    pub intervals: Vec<TranslationInterval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_motion_proof: Option<crate::slot_trim_clearance::SlotTranslationProof>,
}
fn failed(message: &str) -> KernelError {
    KernelError::Unsupported(format!("continuous translation clearance: {message}"))
}
struct Evaluator<'a> {
    left: &'a ModelSnapshot,
    right: &'a ModelSnapshot,
    translation: [f64; 3],
    minimum: f64,
    tolerance: Tolerance,
    budget: usize,
    source_scale: f64,
    samples: Vec<TranslationSample>,
}
impl Evaluator<'_> {
    fn sample(&mut self, time: f64) -> Result<usize, KernelError> {
        if self.samples.len() >= self.budget {
            return Err(KernelError::ResourceLimit("continuous translation native-query budget exhausted before full interval coverage".into()));
        }
        let offset = self.translation.map(|v| v * time);
        let stationary = offset == [0.; 3];
        let uncertainty = if stationary {
            0.
        } else {
            (512.
                * f64::EPSILON
                * (self.source_scale + offset.iter().map(|v| v.abs()).sum::<f64>()))
            .next_up()
        };
        if !uncertainty.is_finite() || uncertainty > self.tolerance.linear / 32. {
            return Err(KernelError::NumericRange);
        }
        let placed;
        let right = if stationary {
            self.right
        } else {
            placed = translate(self.right, offset, self.tolerance)?.model;
            &placed
        };
        let query = self.left.clearance(right, self.tolerance)?;
        let lower = (query.lower_bound_mm - uncertainty).next_down();
        let upper = (query.upper_bound_mm + uncertainty).next_up();
        if lower < self.minimum {
            return Err(failed("a sampled pose does not certify the required minimum; contact, insufficient clearance, or numerical threshold uncertainty cannot pass"));
        }
        let index = self.samples.len();
        self.samples.push(TranslationSample {
            time,
            query,
            translation_uncertainty_mm: uncertainty,
            lower_bound_mm: lower,
            upper_bound_mm: upper,
        });
        Ok(index)
    }
}
impl ModelSnapshot {
    /// Certify a minimum material separation during a straight world-space
    /// translation of `other`. Adaptive midpoint samples prove entire intervals
    /// using distance's 1-Lipschitz bound, not a sample-only collision heuristic.
    /// Unsupported samples, insufficient separation and work exhaustion fail closed.
    pub fn translation_clearance(
        &self,
        other: &Self,
        translation: [f64; 3],
        required_min: f64,
        tolerance: Tolerance,
        max_queries: usize,
    ) -> Result<TranslationClearance, KernelError> {
        if !tolerance.is_valid() {
            return Err(KernelError::InvalidTolerance);
        }
        if !required_min.is_finite()
            || required_min < 0.
            || translation.iter().any(|v| !v.is_finite() || v.abs() > 1e50)
        {
            return Err(KernelError::NumericRange);
        }
        if !(3..=MAX_TRANSLATION_QUERIES).contains(&max_queries) {
            return Err(KernelError::ResourceLimit(format!(
                "continuous translation query budget must be 3..={MAX_TRANSLATION_QUERIES}"
            )));
        }
        self.validate_clearance_operand(tolerance)?;
        other.validate_clearance_operand(tolerance)?;
        let bounds = other.bounds();
        let mut source_scale = bounds
            .min
            .iter()
            .chain(&bounds.max)
            .map(|v| v.abs())
            .fold(0., f64::max);
        // Native support origins may lie far outside the body's bounds (for
        // example an offset cone frame). Translation rounds those values too.
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
            source_scale = source_scale.max(origin.iter().map(|v| v.abs()).fold(0., f64::max));
        }
        let length = translation[0].hypot(translation[1]).hypot(translation[2]);
        if !source_scale.is_finite() || (!length.is_normal() && length != 0.) {
            return Err(KernelError::NumericRange);
        }
        let speed = if length == 0. {
            0.
        } else {
            (length * (1. + 16. * f64::EPSILON)).next_up()
        };
        let mut evaluator = Evaluator {
            left: self,
            right: other,
            translation,
            minimum: required_min,
            tolerance,
            budget: max_queries,
            source_scale,
            samples: vec![],
        };
        if crate::slot_trim_clearance::is_candidate(self, other, tolerance) {
            let binding = crate::slot_trim_clearance::bind(self, other, tolerance)?;
            let relative = if binding.slot_on_left {
                translation
            } else {
                translation.map(|x| -x)
            };
            let axial = crate::math::dot(relative, binding.slot.axis);
            let motion_guard =
                ((2048. * f64::EPSILON) * (source_scale + length + binding.slot.height)).next_up();
            if axial.abs() <= motion_guard && motion_guard <= tolerance.linear / 128. {
                let x = crate::math::dot(relative, binding.slot.x_direction);
                let y = crate::math::dot(
                    relative,
                    crate::math::cross(binding.slot.axis, binding.slot.x_direction),
                );
                let next_center = [binding.center[0] + x, binding.center[1] + y];
                let slot = &binding.slot.slot_holes[binding.slot_index];
                let d0 = crate::slot_trim_clearance::distance_to_centerline(binding.center, slot).0;
                let d1 = crate::slot_trim_clearance::distance_to_centerline(next_center, slot).0;
                let guard = (32. * binding.error + 4. * motion_guard).next_up();
                let lower = (slot.radius - binding.radius - d0.max(d1) - guard).next_down();
                if lower >= required_min
                    && lower > tolerance.linear
                    && binding
                        .other_gap
                        .is_none_or(|gap| gap > slot.radius - binding.radius + guard)
                {
                    evaluator.sample(0.)?;
                    evaluator.sample(1.)?;
                    let upper = evaluator
                        .samples
                        .iter()
                        .map(|s| s.upper_bound_mm)
                        .fold(f64::INFINITY, f64::min);
                    if lower <= upper && upper - lower <= tolerance.linear {
                        let (a, b) = {
                            let h = slot.centerline_length / 2.;
                            let v = [slot.direction[0] * h, slot.direction[1] * h];
                            (
                                crate::slot_trim_clearance::point(
                                    &binding.slot,
                                    [slot.center[0] - v[0], slot.center[1] - v[1]],
                                    0.,
                                ),
                                crate::slot_trim_clearance::point(
                                    &binding.slot,
                                    [slot.center[0] + v[0], slot.center[1] + v[1]],
                                    0.,
                                ),
                            )
                        };
                        return Ok(TranslationClearance {
                            lower_bound_mm: lower,
                            upper_bound_mm: upper,
                            required_minimum_mm: required_min,
                            translation,
                            query_count: evaluator.samples.len(),
                            samples: evaluator.samples,
                            intervals: vec![],
                            slot_motion_proof: Some(
                                crate::slot_trim_clearance::SlotTranslationProof {
                                    stock_slot_wall_faces: crate::slot_trim_clearance::slot_faces(
                                        &binding,
                                    ),
                                    stock_slab_cap_faces: [FaceId(0), FaceId(1)],
                                    stock_slab_origin: binding.slot.origin,
                                    stock_slab_axis: binding.slot.axis,
                                    stock_slab_limits_mm: [0., binding.slot.height],
                                    shank_wall_face: binding.shank_face,
                                    capsule_endpoints: [a, b],
                                    capsule_radius_mm: slot.radius,
                                    shank_radius_mm: binding.radius,
                                    endpoint_centerline_distances_mm: [d0, d1],
                                    worst_endpoint_index: usize::from(d1 > d0),
                                    other_section_axial_gap_mm: binding.other_gap,
                                    arithmetic_guard_mm: guard,
                                    complete_path_lower_bound_mm: lower,
                                },
                            ),
                        });
                    }
                }
            }
            return Err(failed("slot travel needs a planar affine path wholly inside one capsule, a separated head, and an endpoint bound above the required minimum"));
        }
        evaluator.sample(0.)?;
        evaluator.sample(1.)?;
        let mut pending = vec![(0., 1., 0usize)];
        let mut intervals = vec![];
        while let Some((start, end, depth)) = pending.pop() {
            // Dyadic endpoints and midpoints are exact through the fixed depth bound.
            let midpoint = (start + end) / 2.;
            let index = evaluator.sample(midpoint)?;
            let displacement = if speed == 0. {
                0.
            } else {
                (speed * ((end - start) / 2.)).next_up()
            };
            let lower = (evaluator.samples[index].lower_bound_mm - displacement).next_down();
            if lower >= required_min {
                intervals.push(TranslationInterval {
                    start,
                    end,
                    midpoint,
                    sample_index: index,
                    lower_bound_mm: lower,
                    displacement_bound_mm: displacement,
                });
            } else {
                if depth >= MAX_TRANSLATION_DEPTH {
                    return Err(KernelError::ResourceLimit("continuous translation subdivision depth exhausted before full interval coverage".into()));
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
        Ok(TranslationClearance {
            lower_bound_mm,
            upper_bound_mm,
            required_minimum_mm: required_min,
            translation,
            query_count: evaluator.samples.len(),
            samples: evaluator.samples,
            intervals,
            slot_motion_proof: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inspect(q: &TranslationClearance, expected: f64) {
        assert!(q.lower_bound_mm <= expected && q.upper_bound_mm >= expected);
        assert!(q.lower_bound_mm >= q.required_minimum_mm);
        assert_eq!(q.query_count, q.samples.len());
        assert!(q.query_count <= MAX_TRANSLATION_QUERIES);
        assert_eq!(q.samples[0].time, 0.);
        assert_eq!(q.samples[1].time, 1.);
        assert_eq!(q.intervals[0].start, 0.);
        assert_eq!(q.intervals.last().unwrap().end, 1.);
        for adjacent in q.intervals.windows(2) {
            assert_eq!(adjacent[0].end, adjacent[1].start);
        }
        for i in &q.intervals {
            assert_eq!(i.midpoint, (i.start + i.end) / 2.);
            assert_eq!(q.samples[i.sample_index].time, i.midpoint);
            assert!(
                i.lower_bound_mm
                    <= q.samples[i.sample_index].lower_bound_mm - i.displacement_bound_mm
            );
        }
    }
    #[test]
    fn continuous_bearing_travel_and_eccentric_endpoint_limit() {
        let t = Tolerance::default();
        let bearing = extrude_annulus(3., 2., 4., t).unwrap().model;
        let shaft = extrude_circle(1., 12., t).unwrap().model;
        let shaft = translate(&shaft, [0., 0., -4.], t).unwrap().model;
        let q = bearing
            .translation_clearance(&shaft, [0., 0., 8.], 0.5, t, 1025)
            .unwrap();
        inspect(&q, 1.);
        assert!(q.query_count > 3);
        let q = bearing
            .translation_clearance(&shaft, [0.5, 0., 0.], 0.25, t, 1025)
            .unwrap();
        inspect(&q, 0.5);
        let q = bearing
            .translation_clearance(&shaft, [0.; 3], 0.9, t, 3)
            .unwrap();
        inspect(&q, 1.);
        assert_eq!(q.query_count, 3);
    }
    #[test]
    fn free_endpoints_and_midpoint_cannot_hide_quarter_path_collision() {
        let t = Tolerance::default();
        let a = extrude_rectangle(2., 2., 2., t).unwrap().model;
        let b = translate(&a, [-10., 0., 0.], t).unwrap().model;
        for time in [0., 0.5, 1.] {
            let posed = translate(&b, [40. * time, 0., 0.], t).unwrap().model;
            assert!(a.clearance(&posed, t).is_ok());
        }
        assert!(a
            .translation_clearance(&b, [40., 0., 0.], 1., t, 1025)
            .is_err());
    }
    #[test]
    fn interior_positive_minimum_reverse_pose_and_scale() {
        let t = Tolerance::default();
        for scale in [0.01, 1., 10.] {
            let a = extrude_rectangle(2. * scale, 2. * scale, 2. * scale, t)
                .unwrap()
                .model;
            let b = translate(&a, [-10. * scale, 5. * scale, 0.], t)
                .unwrap()
                .model;
            let motion = [20. * scale, 0., 0.];
            let q = a
                .translation_clearance(&b, motion, 2.5 * scale, t, 1025)
                .unwrap();
            inspect(&q, 3. * scale);
            let endpoint = translate(&b, motion, t).unwrap().model;
            let reversed = a
                .translation_clearance(&endpoint, [-20. * scale, 0., 0.], 2.5 * scale, t, 1025)
                .unwrap();
            inspect(&reversed, 3. * scale);
            let angle = 0.6_f64;
            let rotation = [0., 0., (angle / 2.).sin(), (angle / 2.).cos()];
            let left = transform(&a, [20., -8., 3.], rotation, t).unwrap().model;
            let right = transform(&b, [20., -8., 3.], rotation, t).unwrap().model;
            let rotated = [20. * scale * angle.cos(), 20. * scale * angle.sin(), 0.];
            inspect(
                &left
                    .translation_clearance(&right, rotated, 2.5 * scale, t, 1025)
                    .unwrap(),
                3. * scale,
            );
        }
    }
    #[test]
    fn continuous_threshold_work_and_numeric_failures_never_partial_success() {
        let t = Tolerance::default();
        let a = extrude_annulus(3., 2., 4., t).unwrap().model;
        let b = extrude_circle(1., 4., t).unwrap().model;
        assert!(matches!(
            a.translation_clearance(&b, [0., 0., 4.], 0.9, t, 3),
            Err(KernelError::ResourceLimit(_))
        ));
        assert!(matches!(
            a.translation_clearance(&b, [0., 0., 40.], 0.9999, t, 1025),
            Err(KernelError::ResourceLimit(_))
        ));
        for budget in [0, 2, 1026] {
            assert!(a
                .translation_clearance(&b, [0.; 3], 0.5, t, budget)
                .is_err());
        }
        assert!(a.translation_clearance(&b, [0.; 3], 1.1, t, 1025).is_err());
        for motion in [[f64::NAN, 0., 0.], [1e51, 0., 0.], [1e-310, 0., 0.]] {
            assert!(a.translation_clearance(&b, motion, 0.5, t, 1025).is_err());
        }
        assert!(a
            .translation_clearance(&b, [0., 0., 1.], -1., t, 1025)
            .is_err());
    }
}
