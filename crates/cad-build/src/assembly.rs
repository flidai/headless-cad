//! Assembly graph validation and materialization are independent of the feature
//! DAG. Instances share immutable part results; placement never edits a B-rep.
use super::*;
use cad_assembly::{BuiltAssembly, BuiltPart};
use std::collections::HashSet;

pub const MAX_FEATURES: usize = 10_000;
pub const MAX_OUTPUTS: usize = 1_000;
pub const MAX_PARTS: usize = 10_000;
pub const MAX_ASSEMBLIES: usize = 1_000;
pub const MAX_INSTANCES: usize = 100_000;

pub(super) fn resource_limits(recipe: &Recipe) -> Result<(), BuildError> {
    for (resource, count, limit) in [
        ("features", recipe.features.len(), MAX_FEATURES),
        ("parts", recipe.parts.len(), MAX_PARTS),
        ("assemblies", recipe.assemblies.len(), MAX_ASSEMBLIES),
        (
            "outputs",
            recipe
                .outputs
                .len()
                .saturating_add(recipe.assembly_outputs.len()),
            MAX_OUTPUTS,
        ),
    ] {
        if count > limit {
            return Err(BuildError::ResourceLimit { resource, limit });
        }
    }
    let instances = recipe.assemblies.iter().fold(0usize, |total, assembly| {
        total.saturating_add(assembly.instances.len())
    });
    if instances > MAX_INSTANCES {
        return Err(BuildError::ResourceLimit {
            resource: "assembly instances",
            limit: MAX_INSTANCES,
        });
    }
    Ok(())
}

pub(super) fn validate<'a>(
    recipe: &'a Recipe,
    feature_indices: &HashMap<&str, usize>,
    output_names: &mut HashSet<&'a String>,
) -> Result<(), BuildError> {
    let mut parts = HashSet::new();
    for part in &recipe.parts {
        identifier("part", &part.id)?;
        if !parts.insert(&part.id) {
            return Err(BuildError::AssemblyGraph(format!(
                "duplicate part ID '{}'",
                part.id
            )));
        }
        let index = feature_indices.get(part.feature.as_str()).ok_or_else(|| {
            BuildError::AssemblyGraph(format!(
                "part '{}' references missing feature '{}'",
                part.id, part.feature
            ))
        })?;
        if !recipe.features[*index].is_solid() {
            return Err(BuildError::AssemblyGraph(format!(
                "part '{}' must reference a solid feature; '{}' is a profile",
                part.id, part.feature
            )));
        }
    }
    let mut assemblies = HashSet::new();
    for assembly in &recipe.assemblies {
        identifier("assembly", &assembly.id)?;
        if !assemblies.insert(&assembly.id) {
            return Err(BuildError::AssemblyGraph(format!(
                "duplicate assembly ID '{}'",
                assembly.id
            )));
        }
        let mut instances = HashSet::new();
        for instance in &assembly.instances {
            identifier("instance", &instance.id)?;
            if !instances.insert(&instance.id) {
                return Err(BuildError::AssemblyGraph(format!(
                    "assembly '{}' repeats instance ID '{}'",
                    assembly.id, instance.id
                )));
            }
            if !parts.contains(&instance.part) {
                return Err(BuildError::AssemblyGraph(format!(
                    "assembly '{}' instance '{}' references missing part '{}'",
                    assembly.id, instance.id, instance.part
                )));
            }
            instance
                .transform
                .validate()
                .map_err(|error| BuildError::Assembly {
                    assembly: assembly.id.clone(),
                    message: format!("instance '{}': {error}", instance.id),
                })?;
        }
    }
    for output in &recipe.assembly_outputs {
        identifier("output", &output.name)?;
        if !output_names.insert(&output.name) {
            return Err(BuildError::DuplicateOutput(output.name.clone()));
        }
        if !assemblies.contains(&output.assembly) {
            return Err(BuildError::AssemblyGraph(format!(
                "output '{}' references missing assembly '{}'",
                output.name, output.assembly
            )));
        }
    }
    Ok(())
}

pub(super) fn evaluate(
    recipe: &Recipe,
    hashes: &BTreeMap<String, String>,
    evaluated: &HashMap<String, Arc<OperationResult>>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, BuiltAssembly>, BuildError> {
    let definitions: HashMap<_, _> = recipe
        .parts
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect();
    let mut built = HashMap::new();
    for assembly in &recipe.assemblies {
        let mut parts = BTreeMap::new();
        for instance in &assembly.instances {
            parts.entry(instance.part.clone()).or_insert_with(|| {
                let definition = definitions[instance.part.as_str()];
                BuiltPart {
                    feature_id: definition.feature.clone(),
                    content_hash: hashes[&definition.feature].clone(),
                    result: Arc::clone(&evaluated[&definition.feature]),
                }
            });
        }
        let result = BuiltAssembly::new(
            assembly.id.clone(),
            parts,
            assembly.instances.clone(),
            tolerance,
        )
        .map_err(|error| BuildError::Assembly {
            assembly: assembly.id.clone(),
            message: error.to_string(),
        })?;
        built.insert(assembly.id.as_str(), result);
    }
    Ok(recipe
        .assembly_outputs
        .iter()
        .map(|output| (output.name.clone(), built[output.assembly.as_str()].clone()))
        .collect())
}
