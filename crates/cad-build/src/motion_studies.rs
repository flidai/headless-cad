//! Global geometry-free continuation obligations; samples never change static poses.
use super::*;

pub const MAX_MOTION_STUDIES: usize = 16;
pub const MAX_MOTION_ATTEMPTS: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionStudyDefinition {
    pub id: String,
    pub mechanism: String,
    pub input: cad_assembly::MotionStudy,
}
#[derive(Debug, Clone, Serialize)]
pub struct MotionStudyReport {
    pub definition: MotionStudyDefinition,
    pub mechanism_input_hash: String,
    pub solver_id: String,
    pub solution: cad_assembly::MotionReport,
}

pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version != 2 && !recipe.motion_studies.is_empty() {
        return Err(BuildError::MotionStudyFailed {
            id: String::new(),
            message: "motion studies require recipe schema version 2".into(),
            report: None,
        });
    }
    if recipe.motion_studies.len() > MAX_MOTION_STUDIES {
        return Err(BuildError::ResourceLimit {
            resource: "motion studies",
            limit: MAX_MOTION_STUDIES,
        });
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut minimum_attempts = 0usize;
    for definition in &recipe.motion_studies {
        let fail = |message: String| BuildError::MotionStudyFailed {
            id: definition.id.clone(),
            message,
            report: None,
        };
        if definition.id.is_empty()
            || definition.id.len() > 128
            || !definition
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(fail(
                "motion study ID requires 1..128 ASCII letters, digits, underscore or hyphen"
                    .into(),
            ));
        }
        if !ids.insert(definition.id.to_ascii_lowercase()) {
            return Err(fail("case-insensitive duplicate motion study ID".into()));
        }
        let mechanism = recipe
            .mechanisms
            .iter()
            .find(|m| m.id == definition.mechanism)
            .ok_or_else(|| fail("missing motion study mechanism".into()))?;
        cad_assembly::validate_motion_study(&mechanism.input, &definition.input)
            .map_err(|e| fail(e.to_string()))?;
        minimum_attempts = minimum_attempts.saturating_add(definition.input.targets.len());
        if minimum_attempts > MAX_MOTION_ATTEMPTS {
            return Err(BuildError::ResourceLimit {
                resource: "motion study minimum solve attempts",
                limit: MAX_MOTION_ATTEMPTS,
            });
        }
    }
    Ok(())
}

pub(super) fn evaluate(
    recipe: &Recipe,
    mechanisms: &BTreeMap<String, MechanismReport>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, MotionStudyReport>, BuildError> {
    let mut reports = BTreeMap::new();
    let mut remaining_attempts = MAX_MOTION_ATTEMPTS;
    // Sorting makes shared-budget allocation deterministic under source declaration reorder.
    let definitions: BTreeMap<_, _> = recipe.motion_studies.iter().map(|d| (&d.id, d)).collect();
    for definition in definitions.values() {
        let mechanism = &mechanisms[&definition.mechanism];
        let solution = cad_assembly::continue_planar_mechanism(
            &mechanism.definition.input,
            &definition.input,
            tolerance,
            &mut remaining_attempts,
        )
        .map_err(|error| BuildError::MotionStudyFailed {
            id: definition.id.clone(),
            message: error.to_string(),
            report: error.failure_report().cloned().map(Box::new),
        })?;
        reports.insert(
            definition.id.clone(),
            MotionStudyReport {
                definition: (*definition).clone(),
                mechanism_input_hash: mechanism.input_hash.clone(),
                solver_id: cad_assembly::motion_build_id().into(),
                solution,
            },
        );
    }
    Ok(reports)
}
