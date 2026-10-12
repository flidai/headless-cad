//! Source-owned planar mechanisms; rigid placement never modifies part geometry.
use super::*;

pub const MAX_MECHANISMS: usize = 64;
pub const MAX_MECHANISM_BINDINGS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismDefinition {
    pub id: String,
    #[serde(default)]
    pub frame: RigidTransform,
    pub input: cad_assembly::PlanarMechanism,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechanismBinding {
    pub assembly: String,
    pub instance: String,
    pub mechanism: String,
    pub link: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct MechanismReport {
    pub definition: MechanismDefinition,
    pub solver_id: String,
    pub input_hash: String,
    pub solution: cad_assembly::MechanismReport,
}
#[derive(Debug, Clone, Serialize)]
pub struct MechanismBindingReport {
    pub binding: MechanismBinding,
    pub local_transform: RigidTransform,
    pub resolved_transform: RigidTransform,
}
pub(super) struct Evaluation<'a> {
    pub recipe: std::borrow::Cow<'a, Recipe>,
    pub reports: BTreeMap<String, MechanismReport>,
    pub bindings: Vec<MechanismBindingReport>,
    pub context_hash: String,
}
fn invalid(id: &str, message: impl ToString) -> BuildError {
    BuildError::MechanismFailed {
        id: id.into(),
        message: message.to_string(),
        report: None,
    }
}
pub(super) fn preflight(recipe: &Recipe) -> Result<(), BuildError> {
    if recipe.schema_version != 2
        && (!recipe.mechanisms.is_empty() || !recipe.mechanism_bindings.is_empty())
    {
        return Err(invalid("", "mechanisms require recipe schema version 2"));
    }
    for (resource, count, limit) in [
        ("mechanisms", recipe.mechanisms.len(), MAX_MECHANISMS),
        (
            "mechanism bindings",
            recipe.mechanism_bindings.len(),
            MAX_MECHANISM_BINDINGS,
        ),
    ] {
        if count > limit {
            return Err(BuildError::ResourceLimit { resource, limit });
        }
    }
    let mut definitions = BTreeMap::new();
    for definition in &recipe.mechanisms {
        identifier("mechanism", &definition.id)?;
        if definitions.insert(&definition.id, definition).is_some() {
            return Err(invalid(&definition.id, "duplicate mechanism ID"));
        }
        definition
            .frame
            .validate()
            .map_err(|e| invalid(&definition.id, e))?;
        cad_assembly::validate_planar_mechanism(&definition.input)
            .map_err(|e| invalid(&definition.id, e))?;
    }
    let mut seen = std::collections::BTreeSet::new();
    for binding in &recipe.mechanism_bindings {
        for (kind, id) in [
            ("assembly", &binding.assembly),
            ("instance", &binding.instance),
            ("mechanism", &binding.mechanism),
            ("link", &binding.link),
        ] {
            identifier(kind, id)?;
        }
        if !seen.insert((&binding.assembly, &binding.instance)) {
            return Err(invalid(
                &binding.mechanism,
                "duplicate occurrence mechanism binding",
            ));
        }
        let definition = definitions
            .get(&binding.mechanism)
            .ok_or_else(|| invalid(&binding.mechanism, "missing mechanism"))?;
        if !definition.input.links.iter().any(|l| l.id == binding.link) {
            return Err(invalid(&binding.mechanism, "missing mechanism link"));
        }
        let assembly = recipe
            .assemblies
            .iter()
            .find(|a| a.id == binding.assembly)
            .ok_or_else(|| invalid(&binding.mechanism, "missing binding assembly"))?;
        let (transform, attached) =
            if let Some(instance) = assembly.instances.iter().find(|i| i.id == binding.instance) {
                (&instance.transform, instance.attachment.is_some())
            } else if let Some(instance) = assembly
                .assemblies
                .iter()
                .find(|i| i.id == binding.instance)
            {
                (&instance.transform, instance.attachment.is_some())
            } else {
                return Err(invalid(&binding.mechanism, "missing binding occurrence"));
            };
        if attached {
            return Err(invalid(
                &binding.mechanism,
                "mechanism binding and datum attachment cannot own the same occurrence",
            ));
        }
        transform
            .validate()
            .map_err(|e| invalid(&binding.mechanism, e))?;
    }
    Ok(())
}
pub(super) fn evaluate(
    recipe: &Recipe,
    tolerance: Tolerance,
) -> Result<Evaluation<'_>, BuildError> {
    let mut reports = BTreeMap::new();
    for definition in &recipe.mechanisms {
        identifier("mechanism", &definition.id)?;
        if reports.contains_key(&definition.id) {
            return Err(invalid(&definition.id, "duplicate mechanism ID"));
        }
        definition
            .frame
            .validate()
            .map_err(|e| invalid(&definition.id, e))?;
        let solution =
            cad_assembly::solve_planar_mechanism(&definition.input, tolerance).map_err(|e| {
                BuildError::MechanismFailed {
                    id: definition.id.clone(),
                    message: e.to_string(),
                    report: e.solver_report().cloned().map(Box::new),
                }
            })?;
        let input_hash =
            blake3::hash(&serde_json::to_vec(definition).expect("validated finite mechanism"))
                .to_hex()
                .to_string();
        reports.insert(
            definition.id.clone(),
            MechanismReport {
                definition: definition.clone(),
                solver_id: cad_assembly::mechanism_build_id().into(),
                input_hash,
                solution,
            },
        );
    }
    let mut resolved = if recipe.mechanism_bindings.is_empty() {
        std::borrow::Cow::Borrowed(recipe)
    } else {
        std::borrow::Cow::Owned(recipe.clone())
    };
    let mut bindings = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for binding in &recipe.mechanism_bindings {
        if !seen.insert((&binding.assembly, &binding.instance)) {
            return Err(invalid(
                &binding.mechanism,
                "duplicate occurrence mechanism binding",
            ));
        }
        let report = reports
            .get(&binding.mechanism)
            .ok_or_else(|| invalid(&binding.mechanism, "missing mechanism"))?;
        let link = report
            .solution
            .link_frames
            .get(&binding.link)
            .ok_or_else(|| invalid(&binding.mechanism, "missing mechanism link"))?;
        let assembly = resolved
            .to_mut()
            .assemblies
            .iter_mut()
            .find(|a| a.id == binding.assembly)
            .ok_or_else(|| invalid(&binding.mechanism, "missing binding assembly"))?;
        let (transform, attached) = if let Some(instance) = assembly
            .instances
            .iter_mut()
            .find(|i| i.id == binding.instance)
        {
            (&mut instance.transform, instance.attachment.is_some())
        } else if let Some(instance) = assembly
            .assemblies
            .iter_mut()
            .find(|i| i.id == binding.instance)
        {
            (&mut instance.transform, instance.attachment.is_some())
        } else {
            return Err(invalid(&binding.mechanism, "missing binding occurrence"));
        };
        if attached {
            return Err(invalid(
                &binding.mechanism,
                "mechanism binding and datum attachment cannot own the same occurrence",
            ));
        }
        let local_transform = *transform;
        *transform = report
            .definition
            .frame
            .compose(link, tolerance)
            .and_then(|frame| frame.compose(&local_transform, tolerance))
            .map_err(|e| invalid(&binding.mechanism, e))?;
        bindings.push(MechanismBindingReport {
            binding: binding.clone(),
            local_transform,
            resolved_transform: *transform,
        });
    }
    bindings.sort_by(|a, b| {
        (&a.binding.assembly, &a.binding.instance).cmp(&(&b.binding.assembly, &b.binding.instance))
    });
    let context_hash = blake3::hash(
        &serde_json::to_vec(&(&reports, &bindings)).expect("validated finite mechanism reports"),
    )
    .to_hex()
    .to_string();
    Ok(Evaluation {
        recipe: resolved,
        reports,
        bindings,
        context_hash,
    })
}
