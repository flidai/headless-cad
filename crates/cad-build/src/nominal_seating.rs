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
                // Attached datums introduce additional frame operations and are not
                // admitted by this first local seating certificate.
                let mut definition = assembly
                    .hierarchy
                    .get(&assembly.assembly_id)
                    .ok_or_else(|| fail("seating hierarchy root absent".into()))?;
                let mut path_translation_scale = 0.;
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
                        if child.attachment.is_some() {
                            return Err(fail(
                                "seating does not admit attached occurrence frames".into(),
                            ));
                        }
                        child.transform
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
                }
                let depth = end.instance_path.len() as f64;
                let error = (((source_scale + placement_scale) * 1024.
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
