//! Source-owned local planar seating obligations for exact assembly leaves.
use super::*;
use cad_assembly::BuiltAssembly;

pub const MAX_NOMINAL_SEATING_CHECKS: usize = 32;
pub const MAX_NOMINAL_SEATING_WORK: usize = 10_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NominalSeatingCheck {
    pub id: String,
    pub output: String,
    pub left: DatumDimensionEndpoint,
    pub right: DatumDimensionEndpoint,
    pub min_shared_disk_radius_mm: f64,
}

impl<'de> Deserialize<'de> for NominalSeatingCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            output: String,
            left: DatumDimensionEndpoint,
            right: DatumDimensionEndpoint,
            min_shared_disk_radius_mm: f64,
        }
        let value = serde_json::Value::deserialize(deserializer)?;
        if !value.is_object() {
            return Err(serde::de::Error::custom(
                "nominal seating check must be an object",
            ));
        }
        let raw: Raw = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: raw.id,
            output: raw.output,
            left: raw.left,
            right: raw.right,
            min_shared_disk_radius_mm: raw.min_shared_disk_radius_mm,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NominalSeatingCheckReport {
    pub declaration: NominalSeatingCheck,
    pub status: String,
    pub assembly_content_hash: String,
    pub left: DatumDimensionWitness,
    pub right: DatumDimensionWitness,
    pub query: cad_kernel::PlanarSeating,
    /// Conservative additional uncertainty from source-to-world occurrence placement.
    pub placement_uncertainty_mm: f64,
    pub signed_gap_lower_mm: f64,
    pub signed_gap_upper_mm: f64,
    /// Opposed normal departure with the composed occurrence frame allowance.
    pub normal_opposition_bound: f64,
    /// Guaranteed shared disk after both occurrence placement budgets.
    pub disk_radius_lower_mm: f64,
    pub disk_area_lower_mm2: f64,
}

impl NominalSeatingCheck {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidNominalSeatingCheck {
            id: self.id.clone(),
            output: self.output.clone(),
            message: message.into(),
        };
        let portable = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        if !portable(&self.id) || self.output.is_empty() || self.output.len() > 256 {
            return Err(fail("bounded portable check ID and output required"));
        }
        for end in [&self.left, &self.right] {
            if end.datum.is_empty()
                || end.datum.len() > 128
                || end.instance_path.is_empty()
                || end.instance_path.len() > 64
                || end
                    .instance_path
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 256)
            {
                return Err(fail(
                    "bounded exact occurrence path and native face datum required",
                ));
            }
        }
        if self.left.instance_path == self.right.instance_path {
            return Err(fail("two distinct leaf occurrences required"));
        }
        if !self.min_shared_disk_radius_mm.is_finite()
            || self.min_shared_disk_radius_mm <= 0.
            || self.min_shared_disk_radius_mm > 1_000_000.
        {
            return Err(fail(
                "minimum shared disk radius must be finite and positive",
            ));
        }
        Ok(())
    }
}

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.nominal_seating_checks.len() > MAX_NOMINAL_SEATING_CHECKS {
        return Err(BuildError::ResourceLimit {
            resource: "nominal seating checks",
            limit: MAX_NOMINAL_SEATING_CHECKS,
        });
    }
    let mut ids = BTreeSet::new();
    for check in &recipe.nominal_seating_checks {
        check.validate()?;
        if recipe.schema_version != 2
            || !ids.insert(check.id.to_ascii_lowercase())
            || !recipe
                .assembly_outputs
                .iter()
                .any(|o| o.name == check.output)
        {
            return Err(BuildError::InvalidNominalSeatingCheck {
                id: check.id.clone(),
                output: check.output.clone(),
                message:
                    "require schema 2, unique case-insensitive ID and declared assembly output"
                        .into(),
            });
        }
    }
    Ok(())
}

pub(super) fn evaluate(
    recipe: &Recipe,
    assemblies: &BTreeMap<String, BuiltAssembly>,
    datums: &ResolvedFaceDatums,
    t: cad_kernel::Tolerance,
) -> Result<BTreeMap<String, NominalSeatingCheckReport>, BuildError> {
    let mut reports = BTreeMap::new();
    let mut work = 0;
    for check in &recipe.nominal_seating_checks {
        let fail = |message: String| BuildError::NominalSeatingCheckFailed {
            id: check.id.clone(),
            output: check.output.clone(),
            message,
        };
        let assembly = assemblies
            .get(&check.output)
            .ok_or_else(|| fail("assembly output not evaluated".into()))?;
        let left =
            datum_dimensions::resolve(&check.left, assembly, datums, t, &mut work).map_err(fail)?;
        let right = datum_dimensions::resolve(&check.right, assembly, datums, t, &mut work)
            .map_err(fail)?;
        let mut place =
            |end: &DatumDimensionWitness| -> Result<(cad_kernel::ModelSnapshot, f64), BuildError> {
                let instance = assembly
                    .instances
                    .iter()
                    .find(|i| i.id == end.instance_id)
                    .ok_or_else(|| fail("resolved occurrence absent".into()))?;
                let model = &assembly.parts[&instance.part].result.model;
                work = work
                    .saturating_add(model.vertices.len())
                    .saturating_add(model.edges.len())
                    .saturating_add(model.faces.len())
                    .saturating_add(model.loops.len());
                if work > MAX_NOMINAL_SEATING_WORK {
                    return Err(fail("native seating geometry work exhausted".into()));
                }
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
                // The flattened occurrence frame was composed through the authored
                // hierarchy. Account for every quaternion, rotation and translation
                // stage, including cancellation that may make the final offset small.
                // The narrow sibling attachment branch also charges every raw datum
                // operand before cancellation; attached ancestors remain unsupported.
                let mut definition = assembly
                    .hierarchy
                    .get(&assembly.assembly_id)
                    .ok_or_else(|| fail("seating hierarchy root absent".into()))?;
                let mut path_translation_scale = 0.;
                let mut attachment_scale = 0.;
                let mut expected_frame = RigidTransform::default();
                for (index, component) in end.instance_path.iter().enumerate() {
                    let last = index + 1 == end.instance_path.len();
                    let frame = if last {
                        let child = definition
                            .instances
                            .iter()
                            .find(|i| &i.id == component)
                            .ok_or_else(|| {
                                fail("seating leaf absent from authored hierarchy".into())
                            })?;
                        if let Some(a) = &child.attachment {
                            if !a.target_path.is_empty() || a.joint.is_some() {
                                return Err(fail("seating attachment requires a direct sibling target without a joint".into()));
                            }
                            let opposite = if end.instance_path == check.left.instance_path {
                                &check.right
                            } else {
                                &check.left
                            };
                            let mut target_path =
                                end.instance_path[..end.instance_path.len() - 1].to_vec();
                            target_path.push(a.instance.clone());
                            if opposite.instance_path != target_path
                                || opposite.datum != a.target_datum
                            {
                                return Err(fail("seating attachment target must be the opposite certified seating endpoint".into()));
                            }
                            let own = &assembly.parts[&child.part];
                            if own.datums.get(&a.datum) != Some(&RigidTransform::default())
                                || datums
                                    .get(&child.part)
                                    .is_some_and(|d| d.contains_key(&a.datum))
                            {
                                return Err(fail(
                                    "seating attachment requires an authored identity source datum"
                                        .into(),
                                ));
                            }
                            let target = definition
                                .instances
                                .iter()
                                .find(|v| v.id == a.instance)
                                .ok_or_else(|| {
                                fail("seating attachment target must be a sibling part".into())
                            })?;
                            if target.attachment.is_some() {
                                return Err(fail(
                                    "seating attachment chains are unsupported".into(),
                                ));
                            }
                            let part = &assembly.parts[&target.part];
                            let semantic = datums.get(&target.part).and_then(|d| d.get(&a.target_datum))
                                .ok_or_else(|| fail("seating attachment target requires a native semantic face datum".into()))?;
                            if semantic.target_content_hash != part.content_hash
                                || semantic.definition.offset != [0.; 3]
                                || semantic.selection.status != FaceResolutionStatus::Resolved
                                || semantic.selection.witnesses.len() != 1
                                || part.datums.get(&a.target_datum) != Some(&semantic.frame)
                            {
                                return Err(fail("seating attachment requires one current native face with zero datum offset".into()));
                            }
                            let plane = part
                                .result
                                .model
                                .faces
                                .get(semantic.selection.witnesses[0].target_face.0)
                                .and_then(|f| f.try_plane())
                                .ok_or_else(|| {
                                    fail("seating attachment target must be planar".into())
                                })?;
                            let hint = semantic.definition.x_direction;
                            let hint_norm = hint.into_iter().map(|v| v * v).sum::<f64>().sqrt();
                            let cosine = hint
                                .iter()
                                .zip(plane.normal)
                                .map(|(a, b)| a * b)
                                .sum::<f64>()
                                / hint_norm;
                            if !hint_norm.is_finite()
                                || hint_norm <= 0.
                                || !cosine.is_finite()
                                || cosine.abs() > 0.5
                            {
                                return Err(fail("seating attachment target datum hint must be well conditioned to the native plane".into()));
                            }
                            work = work
                                .saturating_add(part.result.model.vertices.len())
                                .saturating_add(part.result.model.edges.len())
                                .saturating_add(part.result.model.faces.len());
                            if work > MAX_NOMINAL_SEATING_WORK {
                                return Err(fail("native seating geometry work exhausted".into()));
                            }
                            let b = part.result.model.bounds();
                            attachment_scale = b
                                .min
                                .into_iter()
                                .chain(b.max)
                                .chain(plane.origin)
                                .chain(semantic.frame.translation)
                                .chain(target.transform.translation)
                                .chain(child.transform.translation)
                                .map(f64::abs)
                                .sum::<f64>()
                                + 1.;
                            target
                                .transform
                                .compose(&semantic.frame, t)
                                .and_then(|v| v.compose(&child.transform, t))
                                .map_err(|e| fail(e.to_string()))?
                        } else {
                            child.transform
                        }
                    } else {
                        let child = definition
                            .assemblies
                            .iter()
                            .find(|i| &i.id == component)
                            .ok_or_else(|| {
                                fail("seating child absent from authored hierarchy".into())
                            })?;
                        if child.attachment.is_some() {
                            return Err(fail(
                                "seating does not admit attached occurrence frames".into(),
                            ));
                        }
                        let frame = child.transform;
                        definition = assembly
                            .hierarchy
                            .get(&child.assembly)
                            .ok_or_else(|| fail("seating child definition absent".into()))?;
                        frame
                    };
                    path_translation_scale +=
                        frame.translation.into_iter().map(f64::abs).sum::<f64>();
                    expected_frame = expected_frame
                        .compose(&frame, t)
                        .map_err(|e| fail(e.to_string()))?;
                }
                // Bind the certificate to the actual flattened occurrence, not merely
                // a plausible attachment declaration with different derived placement.
                if expected_frame != instance.transform {
                    return Err(fail(
                        "seating occurrence frame differs from reconstructed authored placement"
                            .into(),
                    ));
                }
                let depth = end.instance_path.len() as f64;
                // With an identity own datum there is no inverse-frame conditioning.
                // Each attachment stage uses a bounded number (<128) of scalar
                // operations for normalization, projection and rigid composition.
                // The hint guard bounds projected-basis normalization by 2; 2048 eps
                // per stage reserves >8 times that amplified operation count. Raw
                // operand sums cover cancellation; four extra stages cover native
                // datum evaluation and the two local compositions. The target is
                // itself the other kernel-certified seating body, not an arbitrary
                // unchecked face-centroid construction.
                let error = (((source_scale + placement_scale) * 1024.
                    + attachment_scale * (depth + 4.) * 2048.
                    + (source_scale + path_translation_scale + placement_scale + 1.)
                        * depth
                        * 4096.)
                    * f64::EPSILON)
                    .next_up();
                if !error.is_finite() || error > t.linear / 32. {
                    return Err(fail(
                        "native occurrence placement exceeds seating precision budget".into(),
                    ));
                }
                let placed = cad_kernel::transform(
                    model,
                    instance.transform.translation,
                    instance.transform.rotation,
                    t,
                )
                .map_err(|e| fail(e.to_string()))?
                .model;
                Ok((placed, error))
            };
        let (left_model, left_error) = place(&left)?;
        let (right_model, right_error) = place(&right)?;
        let placement_uncertainty_mm = (left_error + right_error).next_up();
        if placement_uncertainty_mm > t.linear / 16. {
            return Err(fail(
                "combined placement uncertainty exceeds seating precision budget".into(),
            ));
        }
        let left_face = left.semantic_datum.selection.witnesses[0].target_face;
        let right_face = right.semantic_datum.selection.witnesses[0].target_face;
        let query = cad_kernel::certify_planar_seating(
            &left_model,
            left_face,
            &right_model,
            right_face,
            check.min_shared_disk_radius_mm,
            t,
        )
        .map_err(|e| fail(e.to_string()))?;
        let signed_gap_lower_mm =
            (query.signed_gap_lower_mm - placement_uncertainty_mm).next_down();
        let signed_gap_upper_mm = (query.signed_gap_upper_mm + placement_uncertainty_mm).next_up();
        let normal_opposition_bound = (query.normal_opposition_bound
            + 2. * placement_uncertainty_mm / check.min_shared_disk_radius_mm)
            .next_up();
        if !signed_gap_lower_mm.is_finite()
            || !signed_gap_upper_mm.is_finite()
            || !normal_opposition_bound.is_finite()
            || signed_gap_lower_mm > 0.
            || signed_gap_upper_mm < 0.
            || signed_gap_upper_mm - signed_gap_lower_mm > t.linear / 8.
        {
            return Err(fail(
                "nominal seating gap unresolved after placement uncertainty".into(),
            ));
        }
        let disk_radius_lower_mm =
            (query.disk_radius_lower_mm - placement_uncertainty_mm).next_down();
        if !disk_radius_lower_mm.is_finite()
            || disk_radius_lower_mm < check.min_shared_disk_radius_mm
        {
            return Err(fail(
                "shared material disk unresolved after placement uncertainty".into(),
            ));
        }
        let disk_area_lower_mm2 = (std::f64::consts::PI.next_down()
            * (disk_radius_lower_mm * disk_radius_lower_mm).next_down())
        .next_down();
        reports.insert(
            check.id.clone(),
            NominalSeatingCheckReport {
                declaration: check.clone(),
                status: "passed".into(),
                assembly_content_hash: assembly.content_hash.clone(),
                left,
                right,
                query,
                placement_uncertainty_mm,
                signed_gap_lower_mm,
                signed_gap_upper_mm,
                normal_opposition_bound,
                disk_radius_lower_mm,
                disk_area_lower_mm2,
            },
        );
    }
    Ok(reports)
}
