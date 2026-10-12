//! Global native material separation obligations for explicit leaf pairs.
use super::*;
pub const MAX_CLEARANCE_CHECKS: usize = 64;
pub const MAX_CLEARANCE_RESOLVE_WORK: usize = 10_000_000;
pub const MAX_CLEARANCE_SWEEPS: usize = 16;
pub const MAX_CLEARANCE_SWEEP_QUERIES: usize = 4096;
pub const MAX_CLEARANCE_TRANSLATION_MM: f64 = 1_000_000.;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RotationPath {
    pub pivot: [f64; 3],
    pub axis: [f64; 3],
    pub angle_radians: f64,
}
impl<'de> Deserialize<'de> for RotationPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            pivot: [f64; 3],
            axis: [f64; 3],
            angle_radians: f64,
        }
        let value = serde_json::Value::deserialize(d)?;
        if !value.is_object() {
            return Err(serde::de::Error::custom("rotation path must be an object"));
        }
        let r: Raw = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            pivot: r.pivot,
            axis: r.axis,
            angle_radians: r.angle_radians,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClearanceCheck {
    pub id: String,
    pub output: String,
    pub left_instance_path: Vec<String>,
    pub right_instance_path: Vec<String>,
    pub min_clearance: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_translation: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_rotation: Option<RotationPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_group: Option<String>,
}
impl<'de> Deserialize<'de> for ClearanceCheck {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            output: String,
            left_instance_path: Vec<String>,
            right_instance_path: Vec<String>,
            min_clearance: f64,
            #[serde(default)]
            right_translation: Option<[f64; 3]>,
            #[serde(default)]
            right_rotation: Option<RotationPath>,
            #[serde(default)]
            rotation_group: Option<String>,
        }
        let v = serde_json::Value::deserialize(d)?;
        if !v.is_object() {
            return Err(serde::de::Error::custom(
                "clearance check must be an object",
            ));
        }
        let r: Raw = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: r.id,
            output: r.output,
            left_instance_path: r.left_instance_path,
            right_instance_path: r.right_instance_path,
            min_clearance: r.min_clearance,
            right_translation: r.right_translation,
            right_rotation: r.right_rotation,
            rotation_group: r.rotation_group,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ClearanceOccurrenceWitness {
    pub instance_id: String,
    pub instance_path: Vec<String>,
    pub part_id: String,
    pub feature_id: String,
    pub part_content_hash: String,
    pub frame: cad_assembly::RigidTransform,
}
#[derive(Debug, Clone, Serialize)]
pub struct ClearanceCheckReport {
    pub declaration: ClearanceCheck,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moving_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_rotation_world: Option<RotationPath>,
    pub status: String,
    pub assembly_content_hash: String,
    pub left: ClearanceOccurrenceWitness,
    pub right: ClearanceOccurrenceWitness,
    /// Static certificate for the initial rounded, placed native snapshots.
    pub query: cad_kernel::SolidClearance,
    /// Additional conservative displacement allowance for both native placements.
    pub placement_uncertainty_mm: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sweep: Option<cad_kernel::TranslationClearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_translation_world: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_sweep: Option<cad_kernel::RotationClearance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_rotation_world: Option<RotationPath>,
    /// Total initial-placement and source-frame path mapping allowance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_uncertainty_mm: Option<f64>,
    pub lower_bound_mm: f64,
    pub upper_bound_mm: f64,
}
impl ClearanceCheck {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidClearanceCheck {
            id: self.id.clone(),
            output: self.output.clone(),
            message: message.into(),
        };
        let text = |s: &str| !s.is_empty() && s.len() <= 1024 && s.trim() == s;
        let path = |p: &Vec<String>| !p.is_empty() && p.len() <= 64 && p.iter().all(|s| text(s));
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !text(&self.output)
        {
            return Err(fail("bounded portable check ID and output name required"));
        }
        if !path(&self.left_instance_path)
            || !path(&self.right_instance_path)
            || self.left_instance_path == self.right_instance_path
        {
            return Err(fail(
                "two distinct literal leaf paths of 1..64 nonempty bounded components required",
            ));
        }
        if !self.min_clearance.is_finite() || self.min_clearance <= 0. {
            return Err(fail(
                "minimum clearance must be finite and strictly positive millimetres",
            ));
        }
        if usize::from(self.right_translation.is_some())
            + usize::from(self.right_rotation.is_some())
            + usize::from(self.rotation_group.is_some())
            > 1
        {
            return Err(fail(
                "right_translation, right_rotation and rotation_group are mutually exclusive",
            ));
        }
        if self.rotation_group.as_ref().is_some_and(|id| {
            id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        }) {
            return Err(fail("rotation_group requires a portable group ID"));
        }
        if let Some(r) = &self.right_rotation {
            let norm = r.axis[0].hypot(r.axis[1]).hypot(r.axis[2]);
            let pivot_length = r.pivot[0].hypot(r.pivot[1]).hypot(r.pivot[2]);
            if !r.axis.into_iter().chain(r.pivot).all(f64::is_finite)
                || (norm - 1.).abs() > 32. * f64::EPSILON
                || pivot_length > MAX_CLEARANCE_TRANSLATION_MM
                || !r.angle_radians.is_finite()
                || r.angle_radians == 0.
                || r.angle_radians.abs() > std::f64::consts::TAU
            {
                return Err(fail("right_rotation requires a finite local pivot within 1000000 mm, unit axis, and nonzero signed angle of at most 2pi radians"));
            }
        }
        if let Some(v) = self.right_translation {
            let length = v[0].hypot(v[1]).hypot(v[2]);
            if !v.into_iter().all(f64::is_finite)
                || length == 0.
                || length > MAX_CLEARANCE_TRANSLATION_MM
            {
                return Err(fail("right_translation must be a nonzero finite local vector of length at most 1000000 mm"));
            }
        }
        Ok(())
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.clearance_checks.len() > MAX_CLEARANCE_CHECKS {
        return Err(BuildError::ResourceLimit {
            resource: "clearance checks",
            limit: MAX_CLEARANCE_CHECKS,
        });
    }
    if recipe
        .clearance_checks
        .iter()
        .filter(|c| {
            c.right_translation.is_some()
                || c.right_rotation.is_some()
                || c.rotation_group.is_some()
        })
        .count()
        > MAX_CLEARANCE_SWEEPS
    {
        return Err(BuildError::ResourceLimit {
            resource: "clearance motion sweeps",
            limit: MAX_CLEARANCE_SWEEPS,
        });
    }
    let mut ids = BTreeSet::new();
    for c in &recipe.clearance_checks {
        c.validate()?;
        let fail = |message: &str| BuildError::InvalidClearanceCheck {
            id: c.id.clone(),
            output: c.output.clone(),
            message: message.into(),
        };
        if recipe.schema_version != 2 {
            return Err(fail("clearance checks require recipe schema version 2"));
        }
        if !ids.insert(c.id.to_ascii_lowercase()) {
            return Err(fail("duplicate case-insensitive clearance check ID"));
        }
        if !recipe.assembly_outputs.iter().any(|o| o.name == c.output) {
            return Err(fail("clearance output must name an assembly output"));
        }
    }
    Ok(())
}
pub(super) fn evaluate(
    recipe: &Recipe,
    assemblies: &BTreeMap<String, cad_assembly::BuiltAssembly>,
    groups: &BTreeMap<String, RigidRotationGroupReport>,
    tolerance: cad_kernel::Tolerance,
) -> Result<BTreeMap<String, ClearanceCheckReport>, BuildError> {
    let mut work = 0usize;
    let mut sweep_queries = 0usize;
    let mut reports = BTreeMap::new();
    for c in &recipe.clearance_checks {
        let fail = |message: String, lower_bound_mm: Option<f64>, upper_bound_mm: Option<f64>| {
            BuildError::ClearanceCheckFailed {
                id: c.id.clone(),
                output: c.output.clone(),
                message,
                lower_bound_mm,
                upper_bound_mm,
            }
        };
        let assembly = assemblies
            .get(&c.output)
            .ok_or_else(|| fail("declared assembly was not evaluated".into(), None, None))?;
        let mut left = None;
        let mut right = None;
        for instance in &assembly.instances {
            let path = assembly
                .instance_paths
                .get(&instance.id)
                .ok_or_else(|| fail("missing authoritative leaf path".into(), None, None))?;
            work = work
                .checked_add(1 + path.len())
                .ok_or(BuildError::ResourceLimit {
                    resource: "clearance path resolution",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                })?;
            if work > MAX_CLEARANCE_RESOLVE_WORK {
                return Err(BuildError::ResourceLimit {
                    resource: "clearance path resolution",
                    limit: MAX_CLEARANCE_RESOLVE_WORK,
                });
            }
            if path == &c.left_instance_path {
                left = Some(instance)
            }
            if path == &c.right_instance_path {
                right = Some(instance)
            }
        }
        let left = left.ok_or_else(|| {
            fail(
                "left path is not an exact leaf occurrence".into(),
                None,
                None,
            )
        })?;
        let right = right.ok_or_else(|| {
            fail(
                "right path is not an exact leaf occurrence".into(),
                None,
                None,
            )
        })?;
        // Admit both source bodies before allocating either placed snapshot. The
        // kernel still performs the full native convexity/certificate validation.
        for instance in [left, right] {
            let m = &assembly.parts[&instance.part].result.model;
            m.validate_clearance_operand(tolerance).map_err(|e| {
                fail(
                    format!("pre-placement clearance admission: {e}"),
                    None,
                    None,
                )
            })?;
        }
        let placed =
            |instance: &cad_assembly::Instance, path: &Vec<String>| -> Result<_, BuildError> {
                let part = &assembly.parts[&instance.part];
                let model = &part.result.model;
                // Native bounds include full analytic circular extrema, which may
                // be absent from the periodic boundary's seam vertices.
                let bounds = model.bounds();
                let source_scale = bounds
                    .min
                    .into_iter()
                    .chain(bounds.max)
                    .map(f64::abs)
                    .fold(0., f64::max)
                    .max((0..3).map(|i| bounds.max[i] - bounds.min[i]).sum());
                let placement_scale = instance
                    .transform
                    .translation
                    .into_iter()
                    .map(f64::abs)
                    .fold(0., f64::max);
                // Bound quaternion/matrix evaluation, rotation and translation from
                // absolute operands, not potentially cancelled world coordinates.
                let error = ((source_scale + placement_scale) * (1024. * f64::EPSILON)).next_up();
                if !error.is_finite() || error > tolerance.linear / 32. {
                    return Err(fail(
                        "native occurrence placement exceeds clearance precision budget".into(),
                        None,
                        None,
                    ));
                }
                let placed = cad_kernel::transform(
                    model,
                    instance.transform.translation,
                    instance.transform.rotation,
                    tolerance,
                )
                .map_err(|e| fail(e.to_string(), None, None))?
                .model;
                Ok((
                    placed,
                    error,
                    ClearanceOccurrenceWitness {
                        instance_id: instance.id.clone(),
                        instance_path: path.clone(),
                        part_id: instance.part.clone(),
                        feature_id: part.feature_id.clone(),
                        part_content_hash: part.content_hash.clone(),
                        frame: instance.transform,
                    },
                ))
            };
        let (left_model, left_error, left_witness) = placed(left, &c.left_instance_path)?;
        let (right_model, right_error, right_witness) = placed(right, &c.right_instance_path)?;
        let uncertainty = (left_error + right_error).next_up();
        if !uncertainty.is_finite() || uncertainty > tolerance.linear / 16. {
            return Err(fail(
                "combined placement uncertainty exceeds clearance precision budget".into(),
                None,
                None,
            ));
        }
        let mut sweep = None;
        let mut right_translation_world = None;
        let mut rotation_sweep = None;
        let mut right_rotation_world = None;
        let mut path_uncertainty_mm = None;
        let mut motion_proof = None;
        let mut moving_endpoint = None;
        let mut group_rotation_world = None;
        let (query, lower, upper) = if let Some(group_id) = &c.rotation_group {
            let group = groups
                .get(group_id)
                .ok_or_else(|| fail("rotation group was not resolved".into(), None, None))?;
            let left_moves = group
                .members
                .iter()
                .any(|m| m.instance_path == c.left_instance_path);
            let right_moves = group
                .members
                .iter()
                .any(|m| m.instance_path == c.right_instance_path);
            group_rotation_world = Some(group.rotation_world.clone());
            if left_moves && right_moves {
                if sweep_queries >= MAX_CLEARANCE_SWEEP_QUERIES {
                    return Err(BuildError::ResourceLimit {
                        resource: "clearance sweep queries",
                        limit: MAX_CLEARANCE_SWEEP_QUERIES,
                    });
                }
                let query = left_model
                    .clearance(&right_model, tolerance)
                    .map_err(|e| fail(e.to_string(), None, None))?;
                sweep_queries += 1;
                motion_proof = Some("common_rigid_motion".into());
                moving_endpoint = Some("both".into());
                // Exactly the same ideal rigid map acts on both placed snapshots;
                // Euclidean material distance is invariant for every path time.
                let lo = (query.lower_bound_mm - uncertainty).next_down();
                let hi = (query.upper_bound_mm + uncertainty).next_up();
                if hi - lo > tolerance.linear {
                    return Err(fail(
                        "common rigid pair initial clearance unresolved".into(),
                        Some(lo),
                        Some(hi),
                    ));
                }
                (query, lo, hi)
            } else if left_moves || right_moves {
                let (stationary, moving) = if right_moves {
                    (&left_model, &right_model)
                } else {
                    (&right_model, &left_model)
                };
                let world = &group.rotation_world;
                let b = moving.bounds();
                let radius = (0..3)
                    .map(|i| {
                        (b.min[i] - world.pivot[i])
                            .abs()
                            .max((b.max[i] - world.pivot[i]).abs())
                    })
                    .fold(0_f64, f64::hypot)
                    .next_up();
                let error = (uncertainty
                    + 2. * group.pivot_uncertainty_mm
                    + 4. * world.angle_radians.abs() * radius * group.axis_uncertainty)
                    .next_up();
                if !error.is_finite() || error > tolerance.linear / 8. {
                    return Err(fail(
                        "group path mapping exceeds precision budget".into(),
                        None,
                        None,
                    ));
                }
                let budget = cad_kernel::MAX_ROTATION_QUERIES
                    .min(MAX_CLEARANCE_SWEEP_QUERIES - sweep_queries);
                if budget < 3 {
                    return Err(BuildError::ResourceLimit {
                        resource: "clearance sweep queries",
                        limit: MAX_CLEARANCE_SWEEP_QUERIES,
                    });
                }
                let required = (c.min_clearance + error).next_up();
                if !required.is_finite() {
                    return Err(fail("group threshold overflow".into(), None, None));
                }
                let result = stationary
                    .rotation_clearance(
                        moving,
                        world.pivot,
                        world.axis,
                        world.angle_radians,
                        required,
                        tolerance,
                        budget,
                    )
                    .map_err(|e| fail(e.to_string(), None, None))?;
                sweep_queries += result.query_count;
                let mut query = result
                    .samples
                    .iter()
                    .find(|s| s.time == 0.)
                    .ok_or_else(|| fail("group rotation missing initial query".into(), None, None))?
                    .query
                    .clone();
                if left_moves {
                    std::mem::swap(&mut query.left_point, &mut query.right_point);
                    query.axis = query.axis.map(|v| -v);
                }
                let lower = (result.lower_bound_mm - error).next_down();
                let upper = (result.upper_bound_mm + error).next_up();
                motion_proof = Some("continuous_rotation".into());
                moving_endpoint = Some(if left_moves { "left" } else { "right" }.into());
                if right_moves {
                    right_rotation_world = Some(world.clone());
                }
                path_uncertainty_mm = Some(error);
                rotation_sweep = Some(result);
                (query, lower, upper)
            } else {
                return Err(fail(
                    "clearance pair has no group member".into(),
                    None,
                    None,
                ));
            }
        } else if let Some(local) = c.right_translation {
            let world = right.transform.vector(local);
            let rotation_error =
                (local.into_iter().map(f64::abs).sum::<f64>() * (1024. * f64::EPSILON)).next_up();
            let path_error = (uncertainty + rotation_error).next_up();
            if !world.into_iter().all(f64::is_finite)
                || !path_error.is_finite()
                || path_error > tolerance.linear / 8.
            {
                return Err(fail(
                    "translation path rotation exceeds precision budget".into(),
                    None,
                    None,
                ));
            }
            let budget = cad_kernel::MAX_TRANSLATION_QUERIES
                .min(MAX_CLEARANCE_SWEEP_QUERIES - sweep_queries);
            if budget < 3 {
                return Err(BuildError::ResourceLimit {
                    resource: "clearance sweep queries",
                    limit: MAX_CLEARANCE_SWEEP_QUERIES,
                });
            }
            let required = (c.min_clearance + path_error).next_up();
            if !required.is_finite() {
                return Err(fail("translation threshold overflow".into(), None, None));
            }
            let result = left_model
                .translation_clearance(&right_model, world, required, tolerance, budget)
                .map_err(|e| fail(e.to_string(), None, None))?;
            sweep_queries += result.query_count;
            let query = result
                .samples
                .iter()
                .find(|s| s.time == 0.)
                .ok_or_else(|| {
                    fail(
                        "translation certificate missing initial query".into(),
                        None,
                        None,
                    )
                })?
                .query
                .clone();
            let lower = (result.lower_bound_mm - path_error).next_down();
            let upper = (result.upper_bound_mm + path_error).next_up();
            right_translation_world = Some(world);
            path_uncertainty_mm = Some(path_error);
            sweep = Some(result);
            (query, lower, upper)
        } else if let Some(local) = &c.right_rotation {
            let world = RotationPath {
                pivot: right.transform.point(local.pivot),
                axis: right.transform.vector(local.axis),
                angle_radians: local.angle_radians,
            };
            let pivot_error = ((local
                .pivot
                .into_iter()
                .chain(right.transform.translation)
                .map(f64::abs)
                .sum::<f64>())
                * (1024. * f64::EPSILON))
                .next_up();
            let axis_error = (local.axis.into_iter().map(f64::abs).sum::<f64>()
                * (1024. * f64::EPSILON))
                .next_up();
            let bounds = right_model.bounds();
            let radius = (0..3)
                .map(|i| {
                    (bounds.min[i] - world.pivot[i])
                        .abs()
                        .max((bounds.max[i] - world.pivot[i]).abs())
                })
                .fold(0_f64, f64::hypot)
                .next_up();
            // Pivot perturbations move a rotated point by at most twice their
            // norm. Axis perturbation is bounded uniformly over the signed arc.
            let path_error = (uncertainty
                + 2. * pivot_error
                + 4. * local.angle_radians.abs() * radius * axis_error)
                .next_up();
            if !world
                .pivot
                .into_iter()
                .chain(world.axis)
                .all(f64::is_finite)
                || !path_error.is_finite()
                || path_error > tolerance.linear / 8.
            {
                return Err(fail(
                    "rotation path frame exceeds precision budget".into(),
                    None,
                    None,
                ));
            }
            let budget =
                cad_kernel::MAX_ROTATION_QUERIES.min(MAX_CLEARANCE_SWEEP_QUERIES - sweep_queries);
            if budget < 3 {
                return Err(BuildError::ResourceLimit {
                    resource: "clearance sweep queries",
                    limit: MAX_CLEARANCE_SWEEP_QUERIES,
                });
            }
            let required = (c.min_clearance + path_error).next_up();
            if !required.is_finite() {
                return Err(fail("rotation threshold overflow".into(), None, None));
            }
            let result = left_model
                .rotation_clearance(
                    &right_model,
                    world.pivot,
                    world.axis,
                    world.angle_radians,
                    required,
                    tolerance,
                    budget,
                )
                .map_err(|e| fail(e.to_string(), None, None))?;
            sweep_queries += result.query_count;
            let query = result
                .samples
                .iter()
                .find(|s| s.time == 0.)
                .ok_or_else(|| {
                    fail(
                        "rotation certificate missing initial query".into(),
                        None,
                        None,
                    )
                })?
                .query
                .clone();
            let lower = (result.lower_bound_mm - path_error).next_down();
            let upper = (result.upper_bound_mm + path_error).next_up();
            right_rotation_world = Some(world);
            path_uncertainty_mm = Some(path_error);
            rotation_sweep = Some(result);
            (query, lower, upper)
        } else {
            let query = left_model
                .clearance(&right_model, tolerance)
                .map_err(|e| fail(e.to_string(), None, None))?;
            let lower = (query.lower_bound_mm - uncertainty).next_down();
            let upper = (query.upper_bound_mm + uncertainty).next_up();
            if upper - lower > tolerance.linear {
                return Err(fail(
                    "placed clearance is unresolved at modeling tolerance".into(),
                    Some(lower),
                    Some(upper),
                ));
            }
            (query, lower, upper)
        };
        if !lower.is_finite() || !upper.is_finite() || lower <= tolerance.linear || upper < lower {
            return Err(fail(
                "placed clearance bounds are unresolved".into(),
                lower.is_finite().then_some(lower),
                upper.is_finite().then_some(upper),
            ));
        }
        if lower < c.min_clearance {
            return Err(fail(
                "certified lower clearance bound is below required minimum".into(),
                Some(lower),
                Some(upper),
            ));
        }
        reports.insert(
            c.id.clone(),
            ClearanceCheckReport {
                declaration: c.clone(),
                rotation_group: c.rotation_group.clone(),
                motion_proof,
                moving_endpoint,
                group_rotation_world,
                status: "passed".into(),
                assembly_content_hash: assembly.content_hash.clone(),
                left: left_witness,
                right: right_witness,
                query,
                placement_uncertainty_mm: uncertainty,
                sweep,
                right_translation_world,
                rotation_sweep,
                right_rotation_world,
                path_uncertainty_mm,
                lower_bound_mm: lower,
                upper_bound_mm: upper,
            },
        );
    }
    Ok(reports)
}
