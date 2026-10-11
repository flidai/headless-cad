//! Deterministic sibling datum attachment; not a nonlinear constraint solver.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatumAttachment {
    /// Target sibling leaf occurrence ID, not an assembly occurrence or path.
    pub instance: String,
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
            let target = *index.get(attachment.instance.as_str()).ok_or_else(|| {
                AssemblyError::Attachment(format!(
                    "'{}' must reference a sibling part occurrence '{}'",
                    leaf.id, attachment.instance
                ))
            })?;
            let target_datums = datums(&definition.instances[target].part).ok_or_else(|| {
                AssemblyError::MissingPart {
                    instance: attachment.instance.clone(),
                    part: definition.instances[target].part.clone(),
                }
            })?;
            if !source_datums.contains_key(attachment.datum.as_str())
                || !target_datums.contains_key(attachment.target_datum.as_str())
            {
                return Err(AssemblyError::Attachment(format!(
                    "missing source or target datum on occurrence '{}'",
                    leaf.id
                )));
            }
            dependencies[i] = Some(target);
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
/// Attachments reference sibling leaf parts; child assemblies and occurrence
/// paths are not attachment targets. Dependency chains are bounded at 64 leaves.
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
    let mut count = 0usize;
    for definition in definitions.values() {
        count = count
            .checked_add(definition.instances.len())
            .ok_or(AssemblyError::ResourceLimit)?;
        if count > MAX_INSTANCES {
            return Err(AssemblyError::ResourceLimit);
        }
        attachment_order(definition, |part| parts.get(part).map(|p| &p.datums))?;
    }
    Ok(())
}

/// Resolve one definition's local leaf placements in deterministic dependency
/// order, retaining IDs but clearing attachments in the returned derived leaves.
/// Exporters must use these placements instead of source attachment offsets.
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
    let order = attachment_order(definition, |part| parts.get(part).map(|p| &p.datums))?;
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
    for definition in definitions.values() {
        attachment_order(definition, |part| parts.get(part).map(|p| &p.datums))?;
    }
    Ok(())
}
