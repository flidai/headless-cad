//! Global source-owned complete capsule footprint and axial pocket-fit obligations.
use super::*;
use cad_assembly::BuiltAssembly;
pub const MAX_POCKET_FIT_CHECKS: usize = 32;
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PocketFitCheck {
    pub id: String,
    pub output: String,
    pub pocket: DatumDimensionEndpoint,
    pub insert: DatumDimensionEndpoint,
    pub min_side_clearance_mm: f64,
}
impl<'de> Deserialize<'de> for PocketFitCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            id: String,
            output: String,
            pocket: DatumDimensionEndpoint,
            insert: DatumDimensionEndpoint,
            min_side_clearance_mm: f64,
        }
        let value = serde_json::Value::deserialize(deserializer)?;
        if !value.is_object() {
            return Err(serde::de::Error::custom(
                "pocket fit check must be an object",
            ));
        }
        let raw: Raw = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: raw.id,
            output: raw.output,
            pocket: raw.pocket,
            insert: raw.insert,
            min_side_clearance_mm: raw.min_side_clearance_mm,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct PocketFitCheckReport {
    pub declaration: PocketFitCheck,
    pub status: String,
    pub assembly_content_hash: String,
    pub pocket: DatumDimensionWitness,
    pub insert: DatumDimensionWitness,
    pub query: cad_kernel::CapsulePocketFit,
    pub placement_uncertainty_mm: f64,
    pub side_clearance_lower_mm: f64,
    pub side_clearance_upper_mm: f64,
    pub signed_floor_gap_lower_mm: f64,
    pub signed_floor_gap_upper_mm: f64,
    pub axial_overlap_lower_mm: f64,
    pub axial_overlap_upper_mm: f64,
}
impl PocketFitCheck {
    pub fn validate(&self) -> Result<(), BuildError> {
        let fail = |message: &str| BuildError::InvalidPocketFitCheck {
            id: self.id.clone(),
            output: self.output.clone(),
            message: message.into(),
        };
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || self.output.is_empty()
            || self.output.len() > 256
        {
            return Err(fail(
                "bounded portable fit check ID and assembly output required",
            ));
        }
        for endpoint in [&self.pocket, &self.insert] {
            if endpoint.datum.is_empty()
                || endpoint.datum.len() > 128
                || endpoint.instance_path.is_empty()
                || endpoint.instance_path.len() > 64
                || endpoint
                    .instance_path
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 256)
            {
                return Err(fail(
                    "bounded exact pocket/insert occurrence path and native face datum required",
                ));
            }
        }
        if self.pocket.instance_path == self.insert.instance_path {
            return Err(fail("distinct pocket and insert leaf occurrences required"));
        }
        if !self.min_side_clearance_mm.is_finite()
            || self.min_side_clearance_mm <= 0.
            || self.min_side_clearance_mm > 1_000_000.
        {
            return Err(fail(
                "minimum side clearance must be finite, positive and at most 1000000 mm",
            ));
        }
        Ok(())
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.pocket_fit_checks.len() > MAX_POCKET_FIT_CHECKS {
        return Err(BuildError::ResourceLimit {
            resource: "pocket fit checks",
            limit: MAX_POCKET_FIT_CHECKS,
        });
    }
    let mut ids = BTreeSet::new();
    for check in &recipe.pocket_fit_checks {
        check.validate()?;
        if recipe.schema_version != 2
            || !ids.insert(check.id.to_ascii_lowercase())
            || !recipe
                .assembly_outputs
                .iter()
                .any(|o| o.name == check.output)
        {
            return Err(BuildError::InvalidPocketFitCheck {
                id: check.id.clone(),
                output: check.output.clone(),
                message: "schema 2, unique portable ID and declared assembly output required"
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
) -> Result<BTreeMap<String, PocketFitCheckReport>, BuildError> {
    let mut reports = BTreeMap::new();
    let mut work = 0;
    for check in &recipe.pocket_fit_checks {
        let fail = |message: String| BuildError::PocketFitCheckFailed {
            id: check.id.clone(),
            output: check.output.clone(),
            message,
        };
        let assembly = assemblies
            .get(&check.output)
            .ok_or_else(|| fail("declared assembly was not evaluated".into()))?;
        let pocket = datum_dimensions::resolve(&check.pocket, assembly, datums, t, &mut work)
            .map_err(fail)?;
        let insert = datum_dimensions::resolve(&check.insert, assembly, datums, t, &mut work)
            .map_err(fail)?;
        let (pocket_model, pe) =
            nominal_seating::placed(&pocket, &check.insert, assembly, datums, t, &mut work)
                .map_err(fail)?;
        let (insert_model, ie) =
            nominal_seating::placed(&insert, &check.pocket, assembly, datums, t, &mut work)
                .map_err(fail)?;
        let placement_uncertainty_mm = (pe + ie).next_up();
        if !placement_uncertainty_mm.is_finite() || placement_uncertainty_mm > t.linear / 16. {
            return Err(fail(
                "combined native placement exceeds pocket-fit precision budget".into(),
            ));
        }
        let query = cad_kernel::certify_capsule_pocket_fit(
            &pocket_model,
            pocket.semantic_datum.selection.witnesses[0].target_face,
            &insert_model,
            t,
        )
        .map_err(|e| fail(e.to_string()))?;
        if query.insert_floor_face != insert.semantic_datum.selection.witnesses[0].target_face {
            return Err(fail(
                "insert datum must identify the actual floor-facing native cap".into(),
            ));
        }
        let side_clearance_lower_mm =
            (query.side_clearance_lower_mm - placement_uncertainty_mm).next_down();
        let side_clearance_upper_mm =
            (query.side_clearance_upper_mm + placement_uncertainty_mm).next_up();
        let signed_floor_gap_lower_mm =
            (query.signed_floor_gap_lower_mm - placement_uncertainty_mm).next_down();
        let signed_floor_gap_upper_mm =
            (query.signed_floor_gap_upper_mm + placement_uncertainty_mm).next_up();
        let axial_overlap_lower_mm =
            (query.axial_overlap_lower_mm - placement_uncertainty_mm).next_down();
        let axial_overlap_upper_mm =
            (query.axial_overlap_upper_mm + placement_uncertainty_mm).next_up();
        if !side_clearance_lower_mm.is_finite()
            || side_clearance_lower_mm < check.min_side_clearance_mm
            || axial_overlap_lower_mm <= 0.
        {
            return Err(fail(format!("guarded side clearance {side_clearance_lower_mm} mm does not satisfy {} mm or positive axial wall overlap is unresolved",check.min_side_clearance_mm)));
        }
        reports.insert(
            check.id.clone(),
            PocketFitCheckReport {
                declaration: check.clone(),
                status: "passed".into(),
                assembly_content_hash: assembly.content_hash.clone(),
                pocket,
                insert,
                query,
                placement_uncertainty_mm,
                side_clearance_lower_mm,
                side_clearance_upper_mm,
                signed_floor_gap_lower_mm,
                signed_floor_gap_upper_mm,
                axial_overlap_lower_mm,
                axial_overlap_upper_mm,
            },
        );
    }
    Ok(reports)
}
