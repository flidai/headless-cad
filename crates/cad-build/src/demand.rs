//! Demand planning follows full-recipe structural validation; it never prunes
//! source intent or suppresses global semantic validation obligations.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    All,
    Selected,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildScope {
    pub mode: BuildMode,
    /// Canonical output names; for eager builds this lists every output.
    pub requested_outputs: Vec<String>,
    pub evaluated_features: Vec<String>,
    pub skipped_features: Vec<String>,
    /// All global assertion and datum anchor/target feature IDs, including
    /// unresolved names (which subsequently produce semantic failures).
    pub validation_roots: Vec<String>,
}

pub(super) fn assembly_closure<'a>(
    recipe: &'a Recipe,
    roots: impl IntoIterator<Item = &'a str>,
) -> (BTreeSet<&'a str>, BTreeSet<&'a str>) {
    let definitions: BTreeMap<_, _> = recipe
        .assemblies
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    let mut pending: Vec<_> = roots.into_iter().collect();
    let mut assemblies = BTreeSet::new();
    let mut parts = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !assemblies.insert(id) {
            continue;
        }
        let definition = definitions[id];
        parts.extend(definition.instances.iter().map(|i| i.part.as_str()));
        pending.extend(definition.assemblies.iter().map(|i| i.assembly.as_str()));
    }
    (assemblies, parts)
}

pub(super) fn scope(
    recipe: &Recipe,
    requested: Option<&[String]>,
    indices: &HashMap<&str, usize>,
) -> Result<BuildScope, BuildError> {
    let all_outputs: BTreeSet<_> = recipe
        .outputs
        .iter()
        .map(|o| o.name.clone())
        .chain(recipe.assembly_outputs.iter().map(|o| o.name.clone()))
        .collect();
    let names = match requested {
        None => all_outputs.clone(),
        Some([]) => return Err(BuildError::EmptyOutputSelection),
        Some(names) => {
            for name in names {
                if !all_outputs.contains(name) {
                    return Err(BuildError::UnknownSelectedOutput(name.clone()));
                }
            }
            names.iter().cloned().collect()
        }
    };
    let parts: BTreeMap<_, _> = recipe.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut validation_roots = BTreeSet::new();
    for assertion in &recipe.face_assertions {
        validation_roots.insert(assertion.selector.anchor_feature.clone());
        validation_roots.insert(assertion.selector.target_feature.clone());
    }
    for view in &recipe.section_views {
        for d in &view.circle_dimensions {
            validation_roots.insert(d.anchor_feature.clone());
        }
        for d in &view.slot_dimensions {
            validation_roots.insert(d.anchor_feature.clone());
        }
    }
    for datum in &recipe.face_datums {
        validation_roots.insert(datum.anchor_feature.clone());
        validation_roots.insert(parts[datum.part.as_str()].feature.clone());
    }
    let asserted: BTreeSet<_> = recipe
        .property_assertions
        .iter()
        .map(|a| a.output.as_str())
        .chain(recipe.section_views.iter().map(|a| a.output.as_str()))
        .chain(recipe.clearance_checks.iter().map(|a| a.output.as_str()))
        .chain(recipe.datum_dimensions.iter().map(|a| a.output.as_str()))
        .chain(
            recipe
                .nominal_seating_checks
                .iter()
                .map(|a| a.output.as_str()),
        )
        .chain(
            recipe
                .rigid_rotation_groups
                .iter()
                .map(|a| a.output.as_str()),
        )
        .chain(
            recipe
                .rigid_translation_groups
                .iter()
                .map(|a| a.output.as_str()),
        )
        .collect();
    for output in recipe
        .outputs
        .iter()
        .filter(|o| asserted.contains(o.name.as_str()))
    {
        validation_roots.insert(output.feature.clone());
    }
    let (_, assertion_parts) = assembly_closure(
        recipe,
        recipe
            .assembly_outputs
            .iter()
            .filter(|o| asserted.contains(o.name.as_str()))
            .map(|o| o.assembly.as_str()),
    );
    for part in assertion_parts {
        validation_roots.insert(parts[part].feature.clone());
    }
    let mut needed = BTreeSet::new();
    if requested.is_none() {
        needed.extend(recipe.features.iter().map(|f| f.id().to_owned()));
    } else {
        let (_, used_parts) = assembly_closure(
            recipe,
            recipe
                .assembly_outputs
                .iter()
                .filter(|o| names.contains(&o.name))
                .map(|o| o.assembly.as_str()),
        );
        let mut pending: Vec<_> = validation_roots
            .iter()
            .map(String::as_str)
            .chain(
                recipe
                    .outputs
                    .iter()
                    .filter(|o| names.contains(&o.name))
                    .map(|o| o.feature.as_str()),
            )
            .chain(used_parts.iter().map(|p| parts[p].feature.as_str()))
            .collect();
        while let Some(id) = pending.pop() {
            // Missing semantic anchors/targets retain the established structured
            // selection failure; they cannot hide a structural graph error.
            let Some(index) = indices.get(id) else {
                continue;
            };
            if needed.insert(id.to_owned()) {
                pending.extend(recipe.features[*index].dependencies());
            }
        }
    }
    Ok(BuildScope {
        mode: if requested.is_none() {
            BuildMode::All
        } else {
            BuildMode::Selected
        },
        requested_outputs: names.into_iter().collect(),
        skipped_features: recipe
            .features
            .iter()
            .map(|f| f.id().to_owned())
            .filter(|id| !needed.contains(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        evaluated_features: needed.into_iter().collect(),
        validation_roots: validation_roots.into_iter().collect(),
    })
}
