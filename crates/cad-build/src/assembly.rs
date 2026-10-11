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
        total
            .saturating_add(assembly.instances.len())
            .saturating_add(assembly.assemblies.len())
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
        validate_datums(&part.datums)
            .map_err(|error| BuildError::AssemblyGraph(format!("part '{}': {error}", part.id)))?;
        part.metadata
            .validate()
            .map_err(|error| BuildError::AssemblyGraph(format!("part '{}': {error}", part.id)))?;
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
    let definitions = recipe
        .assemblies
        .iter()
        .map(|definition| (definition.id.clone(), definition.clone()))
        .collect();
    let part_ids = recipe.parts.iter().map(|part| part.id.clone()).collect();
    cad_assembly::validate_definitions(&part_ids, &definitions)
        .map_err(|error| BuildError::AssemblyGraph(error.to_string()))?;
    let mut part_definitions = recipe
        .parts
        .iter()
        .map(|part| (part.id.clone(), part.clone()))
        .collect();
    face_datums::add_placeholders(recipe, &mut part_definitions);
    cad_assembly::validate_attachment_definitions(&part_definitions, &definitions)
        .map_err(|error| BuildError::AssemblyGraph(error.to_string()))?;
    let mut exported_leaves = 0usize;
    let mut exported_nodes = 0usize;
    let mut counts = BTreeMap::new();
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
        let (leaves, nodes) = expanded_counts(&output.assembly, &definitions, &mut counts);
        exported_leaves = exported_leaves.saturating_add(leaves);
        exported_nodes = exported_nodes.saturating_add(nodes);
        if exported_leaves > MAX_INSTANCES || exported_nodes > MAX_INSTANCES {
            return Err(BuildError::ResourceLimit {
                resource: "expanded assembly outputs",
                limit: MAX_INSTANCES,
            });
        }
    }
    Ok(())
}

// Called only after validate_definitions proves acyclicity and depth <=64.
// Memoize counts without materializing the exponentially expanded occurrences.
fn expanded_counts(
    id: &str,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    memo: &mut BTreeMap<String, (usize, usize)>,
) -> (usize, usize) {
    if let Some(count) = memo.get(id) {
        return *count;
    }
    let definition = &definitions[id];
    let mut count = (definition.instances.len(), 1usize);
    for child in &definition.assemblies {
        let nested = expanded_counts(&child.assembly, definitions, memo);
        count.0 = count.0.saturating_add(nested.0);
        count.1 = count.1.saturating_add(nested.1);
    }
    memo.insert(id.to_owned(), count);
    count
}

pub(super) fn evaluate(
    recipe: &Recipe,
    hashes: &BTreeMap<String, String>,
    evaluated: &HashMap<String, Arc<OperationResult>>,
    tolerance: Tolerance,
    face_datums: &ResolvedFaceDatums,
    requested_outputs: &[String],
) -> Result<BTreeMap<String, BuiltAssembly>, BuildError> {
    let selected: Vec<_> = recipe
        .assembly_outputs
        .iter()
        .filter(|o| requested_outputs.binary_search(&o.name).is_ok())
        .collect();
    let (needed_assemblies, needed_parts) =
        demand::assembly_closure(recipe, selected.iter().map(|o| o.assembly.as_str()));
    let parts: BTreeMap<_, _> = recipe
        .parts
        .iter()
        .filter(|definition| needed_parts.contains(definition.id.as_str()))
        .map(|definition| {
            (
                definition.id.clone(),
                BuiltPart {
                    feature_id: definition.feature.clone(),
                    metadata: definition.metadata.clone(),
                    datums: definition
                        .datums
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .chain(
                            face_datums
                                .get(&definition.id)
                                .into_iter()
                                .flat_map(|frames| {
                                    frames
                                        .iter()
                                        .map(|(name, resolution)| (name.clone(), resolution.frame))
                                }),
                        )
                        .collect(),
                    content_hash: hashes[&definition.feature].clone(),
                    result: Arc::clone(&evaluated[&definition.feature]),
                },
            )
        })
        .collect();
    let definitions: BTreeMap<_, _> = recipe
        .assemblies
        .iter()
        .filter(|definition| needed_assemblies.contains(definition.id.as_str()))
        .map(|definition| (definition.id.clone(), definition.clone()))
        .collect();
    let mut built = HashMap::new();
    let mut outputs = BTreeMap::new();
    for output in selected {
        if !built.contains_key(&output.assembly) {
            let assembly = BuiltAssembly::from_definitions(
                output.assembly.clone(),
                parts.clone(),
                definitions.clone(),
                tolerance,
            )
            .map_err(|error| BuildError::Assembly {
                assembly: output.assembly.clone(),
                message: error.to_string(),
            })?;
            built.insert(output.assembly.clone(), assembly);
        }
        outputs.insert(output.name.clone(), built[&output.assembly].clone());
    }
    Ok(outputs)
}
