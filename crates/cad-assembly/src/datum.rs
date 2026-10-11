//! Deterministic sibling/descendant datum attachment; not a nonlinear solver.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatumAttachment {
    /// Target root occurrence ID in this definition.
    pub instance: String,
    /// Descendant occurrence IDs ending at a leaf; empty means a sibling leaf.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_path: Vec<String>,
    /// Datum name on this occurrence's part definition.
    pub datum: String,
    /// Datum name on the target occurrence's part definition.
    pub target_datum: String,
}

pub fn validate_datums(datums: &BTreeMap<String, RigidTransform>) -> Result<(), AssemblyError> {
    if datums.len() > 1024 {
        return Err(AssemblyError::ResourceLimit);
    }
    for (name, frame) in datums {
        identifier("datum", name)?;
        frame.validate()?;
    }
    Ok(())
}

impl RigidTransform {
    /// Invert a validated unit frame. The checked composition enforces the same
    /// numerical envelope as placement, including large rotated translations.
    pub fn inverse(&self, tolerance: Tolerance) -> Result<Self, AssemblyError> {
        self.validate()?;
        let [x, y, z, w] = self.rotation;
        let rotation = Self {
            translation: [0.0; 3],
            rotation: [-x, -y, -z, w],
        };
        let translation = Self {
            translation: self.translation.map(|v| -v),
            ..Default::default()
        };
        rotation.compose(&translation, tolerance)
    }
}

fn attachment_order<'a>(
    definition: &AssemblyDefinition,
    mut datums: impl FnMut(&str) -> Option<&'a BTreeMap<String, RigidTransform>>,
    external: &BTreeMap<String, String>,
) -> Result<Vec<usize>, AssemblyError> {
    if definition.instances.len() > MAX_INSTANCES {
        return Err(AssemblyError::ResourceLimit);
    }
    let mut index = BTreeMap::new();
    for (i, leaf) in definition.instances.iter().enumerate() {
        identifier("instance", &leaf.id)?;
        leaf.transform.validate()?;
        if index.insert(leaf.id.as_str(), i).is_some() {
            return Err(AssemblyError::DuplicateInstance(leaf.id.clone()));
        }
    }
    let mut dependencies = vec![None; definition.instances.len()];
    for (i, leaf) in definition.instances.iter().enumerate() {
        let source_datums = datums(&leaf.part).ok_or_else(|| AssemblyError::MissingPart {
            instance: leaf.id.clone(),
            part: leaf.part.clone(),
        })?;
        if let Some(attachment) = &leaf.attachment {
            for name in [
                &attachment.instance,
                &attachment.datum,
                &attachment.target_datum,
            ] {
                identifier("attachment reference", name)?;
            }
            let target = if attachment.target_path.is_empty() {
                Some(*index.get(attachment.instance.as_str()).ok_or_else(|| {
                    AssemblyError::Attachment(format!(
                        "'{}' must reference a sibling part occurrence '{}'",
                        leaf.id, attachment.instance
                    ))
                })?)
            } else {
                None
            };
            let target_part = if let Some(target) = target {
                &definition.instances[target].part
            } else {
                external.get(&leaf.id).ok_or_else(|| {
                    AssemblyError::Attachment(
                        "descendant target requires a validated assembly hierarchy".into(),
                    )
                })?
            };
            let target_datums = datums(target_part).ok_or_else(|| AssemblyError::MissingPart {
                instance: attachment.instance.clone(),
                part: target_part.clone(),
            })?;
            if !source_datums.contains_key(attachment.datum.as_str())
                || !target_datums.contains_key(attachment.target_datum.as_str())
            {
                return Err(AssemblyError::Attachment(format!(
                    "missing source or target datum on occurrence '{}'",
                    leaf.id
                )));
            }
            dependencies[i] = target;
        }
    }
    let mut state = vec![0u8; dependencies.len()];
    let mut depth = vec![0usize; dependencies.len()];
    let mut order = Vec::with_capacity(dependencies.len());
    for start in 0..dependencies.len() {
        if state[start] == 2 {
            continue;
        }
        let mut stack = vec![(start, false)];
        while let Some((i, finish)) = stack.pop() {
            if finish {
                depth[i] = dependencies[i].map_or(1, |j| depth[j] + 1);
                if depth[i] > 64 {
                    return Err(AssemblyError::ResourceLimit);
                }
                state[i] = 2;
                order.push(i);
                continue;
            }
            if state[i] == 2 {
                continue;
            }
            if state[i] == 1 {
                return Err(AssemblyError::Attachment("sibling attachment cycle".into()));
            }
            if stack.len() >= 64 {
                return Err(AssemblyError::ResourceLimit);
            }
            state[i] = 1;
            stack.push((i, true));
            if let Some(j) = dependencies[i] {
                stack.push((j, false));
            }
        }
    }
    Ok(order)
}

/// Source-only preflight for all declared part frames and assembly attachments.
/// Targets are sibling leaves or explicit descendant occurrence paths ending
/// at a part. Target paths and sibling dependency chains are bounded at 64.
pub fn validate_attachment_definitions(
    parts: &BTreeMap<String, PartDefinition>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<(), AssemblyError> {
    if parts.len() > 10_000 || definitions.len() > 10_000 {
        return Err(AssemblyError::ResourceLimit);
    }
    validate_definitions(&parts.keys().cloned().collect(), definitions)?;
    for part in parts.values() {
        validate_datums(&part.datums)?;
    }
    let targets = descendant_targets(definitions)?;
    let mut count = 0usize;
    for definition in definitions.values() {
        count = count
            .checked_add(definition.instances.len())
            .ok_or(AssemblyError::ResourceLimit)?;
        if count > MAX_INSTANCES {
            return Err(AssemblyError::ResourceLimit);
        }
        attachment_order(
            definition,
            |part| parts.get(part).map(|p| &p.datums),
            &targets[&definition.id],
        )?;
    }
    Ok(())
}

/// Resolve one definition's local leaf placements in deterministic dependency
/// order, retaining IDs but clearing attachments in the returned derived leaves.
/// Descendant targets explicitly require `resolve_hierarchy_instances`; this
/// legacy entry point has no hierarchy with which to resolve them.
pub fn resolve_definition_instances(
    definition: &AssemblyDefinition,
    parts: &BTreeMap<String, BuiltPart>,
    tolerance: Tolerance,
) -> Result<Vec<Instance>, AssemblyError> {
    if !tolerance.is_valid() {
        return Err(AssemblyError::Tolerance);
    }
    for part in definition
        .instances
        .iter()
        .map(|leaf| &leaf.part)
        .collect::<BTreeSet<_>>()
    {
        let definition = parts.get(part).ok_or_else(|| AssemblyError::MissingPart {
            instance: "attachment".into(),
            part: part.clone(),
        })?;
        validate_datums(&definition.datums)?;
    }
    let order = attachment_order(
        definition,
        |part| parts.get(part).map(|p| &p.datums),
        &BTreeMap::new(),
    )?;
    let mut resolved = BTreeMap::<String, Instance>::new();
    for i in order {
        let source = &definition.instances[i];
        let mut instance = source.clone();
        if let Some(attachment) = &source.attachment {
            let target = &resolved[&attachment.instance];
            let target_datum = &parts[&target.part].datums[&attachment.target_datum];
            let own_datum = &parts[&source.part].datums[&attachment.datum];
            instance.transform = target
                .transform
                .compose(target_datum, tolerance)?
                .compose(&source.transform, tolerance)?
                .compose(&own_datum.inverse(tolerance)?, tolerance)?;
        }
        instance.attachment = None;
        resolved.insert(instance.id.clone(), instance);
    }
    Ok(resolved.into_values().collect())
}

pub(super) fn validate_built_attachments(
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<(), AssemblyError> {
    for part in parts.values() {
        validate_datums(&part.datums)?;
    }
    let targets = descendant_targets(definitions)?;
    for definition in definitions.values() {
        attachment_order(
            definition,
            |part| parts.get(part).map(|p| &p.datums),
            &targets[&definition.id],
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Occurrence {
    Leaf(usize),
    Child(usize),
}
type OccurrenceIndex = BTreeMap<String, BTreeMap<String, Occurrence>>;
fn occurrence_index(definitions: &BTreeMap<String, AssemblyDefinition>) -> OccurrenceIndex {
    definitions
        .iter()
        .map(|(id, d)| {
            let mut map = BTreeMap::new();
            for (i, x) in d.instances.iter().enumerate() {
                map.insert(x.id.clone(), Occurrence::Leaf(i));
            }
            for (i, x) in d.assemblies.iter().enumerate() {
                map.insert(x.id.clone(), Occurrence::Child(i));
            }
            (id.clone(), map)
        })
        .collect()
}
fn target_walk<'a>(
    definition: &'a AssemblyDefinition,
    attachment: &DatumAttachment,
    definitions: &'a BTreeMap<String, AssemblyDefinition>,
    index: &OccurrenceIndex,
) -> Result<(Vec<&'a RigidTransform>, &'a str, &'a Instance), AssemblyError> {
    if attachment.target_path.len() >= 64 {
        return Err(AssemblyError::ResourceLimit);
    }
    let components: Vec<_> = std::iter::once(&attachment.instance)
        .chain(&attachment.target_path)
        .collect();
    let mut current = definition;
    let mut frames = vec![];
    for (i, id) in components.iter().enumerate() {
        identifier("attachment path component", id)?;
        let occurrence = index
            .get(&current.id)
            .and_then(|x| x.get(*id))
            .ok_or_else(|| {
                AssemblyError::Attachment(format!(
                    "missing target occurrence '{}' in definition '{}'",
                    id, current.id
                ))
            })?;
        match occurrence {
            Occurrence::Leaf(k) => {
                if i + 1 != components.len() {
                    return Err(AssemblyError::Attachment(format!(
                        "target path continues beyond leaf '{}'",
                        id
                    )));
                }
                return Ok((frames, current.id.as_str(), &current.instances[*k]));
            }
            Occurrence::Child(k) => {
                if i + 1 == components.len() {
                    return Err(AssemblyError::Attachment(format!(
                        "target path ends at assembly occurrence '{}' instead of a part",
                        id
                    )));
                }
                let child = &current.assemblies[*k];
                frames.push(&child.transform);
                current = definitions.get(&child.assembly).ok_or_else(|| {
                    AssemblyError::Hierarchy(format!("missing assembly '{}'", child.assembly))
                })?;
            }
        }
    }
    Err(AssemblyError::Attachment("empty target path".into()))
}
fn descendant_targets(
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, AssemblyError> {
    let index = occurrence_index(definitions);
    let mut result = BTreeMap::new();
    let mut work = 0usize;
    for (id, definition) in definitions {
        let mut targets = BTreeMap::new();
        for leaf in &definition.instances {
            if let Some(a) = &leaf.attachment {
                if !a.target_path.is_empty() {
                    work = work
                        .checked_add(a.target_path.len() + 1)
                        .ok_or(AssemblyError::ResourceLimit)?;
                    if work > MAX_INSTANCES * 64 {
                        return Err(AssemblyError::ResourceLimit);
                    }
                    let (_, _, target) = target_walk(definition, a, definitions, &index)?;
                    targets.insert(leaf.id.clone(), target.part.clone());
                }
            }
        }
        result.insert(id.clone(), targets);
    }
    Ok(result)
}
/// Resolve each definition once, bottom-up, without expanding repeated child
/// occurrences. A target path traverses occurrence IDs and ends at a part; all
/// transforms on that path precede the resolved target leaf and its named datum.
pub fn resolve_hierarchy_instances(
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, Vec<Instance>>, AssemblyError> {
    if !tolerance.is_valid() {
        return Err(AssemblyError::Tolerance);
    }
    validate_definitions(&parts.keys().cloned().collect(), definitions)?;
    validate_built_attachments(parts, definitions)?;
    let targets = descendant_targets(definitions)?;
    let index = occurrence_index(definitions);
    fn visit(
        id: &str,
        parts: &BTreeMap<String, BuiltPart>,
        definitions: &BTreeMap<String, AssemblyDefinition>,
        targets: &BTreeMap<String, BTreeMap<String, String>>,
        index: &OccurrenceIndex,
        tolerance: Tolerance,
        memo: &mut BTreeMap<String, BTreeMap<String, Instance>>,
    ) -> Result<(), AssemblyError> {
        if memo.contains_key(id) {
            return Ok(());
        }
        let definition = &definitions[id];
        for child in &definition.assemblies {
            visit(
                &child.assembly,
                parts,
                definitions,
                targets,
                index,
                tolerance,
                memo,
            )?;
        }
        let order = attachment_order(
            definition,
            |part| parts.get(part).map(|p| &p.datums),
            &targets[id],
        )?;
        let mut resolved: BTreeMap<String, Instance> = BTreeMap::new();
        for i in order {
            let source = &definition.instances[i];
            let mut instance = source.clone();
            if let Some(a) = &source.attachment {
                let (target_frame, target_part) = if a.target_path.is_empty() {
                    let target = &resolved[&a.instance];
                    (target.transform, target.part.as_str())
                } else {
                    let (frames, target_definition, target) =
                        target_walk(definition, a, definitions, index)?;
                    let mut frame = RigidTransform::default();
                    for child_frame in frames {
                        frame = frame.compose(child_frame, tolerance)?;
                    }
                    let target = &memo[target_definition][&target.id];
                    (
                        frame.compose(&target.transform, tolerance)?,
                        target.part.as_str(),
                    )
                };
                instance.transform = target_frame
                    .compose(&parts[target_part].datums[&a.target_datum], tolerance)?
                    .compose(&source.transform, tolerance)?
                    .compose(
                        &parts[&source.part].datums[&a.datum].inverse(tolerance)?,
                        tolerance,
                    )?;
            }
            instance.attachment = None;
            resolved.insert(instance.id.clone(), instance);
        }
        memo.insert(id.to_owned(), resolved);
        Ok(())
    }
    let mut memo = BTreeMap::new();
    for id in definitions.keys() {
        visit(
            id,
            parts,
            definitions,
            &targets,
            &index,
            tolerance,
            &mut memo,
        )?;
    }
    Ok(memo
        .into_iter()
        .map(|(id, instances)| (id, instances.into_values().collect()))
        .collect())
}
