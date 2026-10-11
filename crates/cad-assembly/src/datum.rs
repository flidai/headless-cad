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

/// Attach a rigid child occurrence through a descendant part datum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyAttachment {
    pub instance: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_path: Vec<String>,
    /// Full nonempty path inside this child definition, ending at a part.
    pub source_path: Vec<String>,
    pub datum: String,
    pub target_datum: String,
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
fn walk<'a>(
    definition: &'a AssemblyDefinition,
    path: &[&str],
    definitions: &'a BTreeMap<String, AssemblyDefinition>,
    index: &OccurrenceIndex,
) -> Result<(Vec<&'a RigidTransform>, &'a Instance), AssemblyError> {
    if path.is_empty() {
        return Err(AssemblyError::Attachment(
            "datum path must end at a part".into(),
        ));
    }
    if path.len() > 64 {
        return Err(AssemblyError::ResourceLimit);
    }
    let mut current = definition;
    let mut frames = vec![];
    for (i, id) in path.iter().enumerate() {
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
                if i + 1 != path.len() {
                    return Err(AssemblyError::Attachment(format!(
                        "target path continues beyond leaf '{}'",
                        id
                    )));
                }
                let leaf = &current.instances[*k];
                frames.push(&leaf.transform);
                return Ok((frames, leaf));
            }
            Occurrence::Child(k) => {
                if i + 1 == path.len() {
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
    Err(AssemblyError::Attachment("empty datum path".into()))
}
fn target_path<'a>(instance: &'a str, tail: &'a [String]) -> Result<Vec<&'a str>, AssemblyError> {
    if tail.len() >= 64 {
        return Err(AssemblyError::ResourceLimit);
    }
    Ok(std::iter::once(instance)
        .chain(tail.iter().map(String::as_str))
        .collect())
}
fn dependency_order(dependencies: &[Option<usize>]) -> Result<Vec<usize>, AssemblyError> {
    let mut state = vec![0u8; dependencies.len()];
    let mut depth = vec![0usize; dependencies.len()];
    let mut order = vec![];
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
fn preflight<'a>(
    definitions: &BTreeMap<String, AssemblyDefinition>,
    index: &OccurrenceIndex,
    datums: impl Fn(&str) -> Option<&'a BTreeMap<String, RigidTransform>>,
) -> Result<BTreeMap<String, Vec<usize>>, AssemblyError> {
    let mut orders = BTreeMap::new();
    let mut work = 0usize;
    for (id, d) in definitions {
        let mut dependencies = vec![None; d.instances.len() + d.assemblies.len()];
        let mut check = |i: usize,
                         source_part: &str,
                         datum: &str,
                         root: &str,
                         tail: &[String],
                         target_datum: &str|
         -> Result<(), AssemblyError> {
            identifier("attachment datum", datum)?;
            identifier("attachment target datum", target_datum)?;
            let path = target_path(root, tail)?;
            work = work
                .checked_add(path.len())
                .ok_or(AssemblyError::ResourceLimit)?;
            if work > MAX_INSTANCES * 128 {
                return Err(AssemblyError::ResourceLimit);
            }
            let (_, target) = walk(d, &path, definitions, index)?;
            if !datums(source_part).is_some_and(|m| m.contains_key(datum))
                || !datums(&target.part).is_some_and(|m| m.contains_key(target_datum))
            {
                return Err(AssemblyError::Attachment(
                    "missing source or target datum".into(),
                ));
            }
            let target_index = match index[id][root] {
                Occurrence::Leaf(k) => k,
                Occurrence::Child(k) => d.instances.len() + k,
            };
            dependencies[i] = Some(target_index);
            Ok(())
        };
        for (i, leaf) in d.instances.iter().enumerate() {
            if let Some(a) = &leaf.attachment {
                check(
                    i,
                    &leaf.part,
                    &a.datum,
                    &a.instance,
                    &a.target_path,
                    &a.target_datum,
                )?;
            }
        }
        for (i, child) in d.assemblies.iter().enumerate() {
            if let Some(a) = &child.attachment {
                if a.source_path.len() > 64 {
                    return Err(AssemblyError::ResourceLimit);
                }
                let path: Vec<_> = a.source_path.iter().map(String::as_str).collect();
                let (_, source) = walk(&definitions[&child.assembly], &path, definitions, index)?;
                check(
                    d.instances.len() + i,
                    &source.part,
                    &a.datum,
                    &a.instance,
                    &a.target_path,
                    &a.target_datum,
                )?;
            }
        }
        orders.insert(id.clone(), dependency_order(&dependencies)?);
    }
    Ok(orders)
}
/// Validate all source paths and mixed leaf/child sibling DAGs, including unused
/// definitions, before geometry evaluation. Each path and dependency chain is
/// bounded at 64; path components are literal occurrence IDs.
pub fn validate_attachment_definitions(
    parts: &BTreeMap<String, PartDefinition>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<(), AssemblyError> {
    validate_definitions(&parts.keys().cloned().collect(), definitions)?;
    for p in parts.values() {
        validate_datums(&p.datums)?;
    }
    preflight(definitions, &occurrence_index(definitions), |id| {
        parts.get(id).map(|p| &p.datums)
    })?;
    Ok(())
}
pub(super) fn validate_built_attachments(
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<(), AssemblyError> {
    for p in parts.values() {
        validate_datums(&p.datums)?;
    }
    preflight(definitions, &occurrence_index(definitions), |id| {
        parts.get(id).map(|p| &p.datums)
    })?;
    Ok(())
}
fn datum_frame(
    definition: &AssemblyDefinition,
    path: &[&str],
    datum: &str,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    index: &OccurrenceIndex,
    parts: &BTreeMap<String, BuiltPart>,
    tolerance: Tolerance,
) -> Result<RigidTransform, AssemblyError> {
    let (frames, leaf) = walk(definition, path, definitions, index)?;
    let mut frame = RigidTransform::default();
    for f in frames {
        frame = frame.compose(f, tolerance)?;
    }
    frame.compose(&parts[&leaf.part].datums[datum], tolerance)
}
/// Resolve every reusable definition once, bottom-up; returned definitions
/// contain derived placements and no attachments. Input source intent is retained.
pub fn resolve_hierarchy_placements(
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, AssemblyDefinition>, AssemblyError> {
    if !tolerance.is_valid() {
        return Err(AssemblyError::Tolerance);
    }
    validate_definitions(&parts.keys().cloned().collect(), definitions)?;
    for p in parts.values() {
        validate_datums(&p.datums)?;
    }
    let index = occurrence_index(definitions);
    let orders = preflight(definitions, &index, |id| parts.get(id).map(|p| &p.datums))?;
    fn visit(
        id: &str,
        parts: &BTreeMap<String, BuiltPart>,
        definitions: &BTreeMap<String, AssemblyDefinition>,
        index: &OccurrenceIndex,
        orders: &BTreeMap<String, Vec<usize>>,
        tolerance: Tolerance,
        memo: &mut BTreeMap<String, AssemblyDefinition>,
    ) -> Result<(), AssemblyError> {
        if memo.contains_key(id) {
            return Ok(());
        }
        let source = &definitions[id];
        for child in &source.assemblies {
            visit(
                &child.assembly,
                parts,
                definitions,
                index,
                orders,
                tolerance,
                memo,
            )?;
        }
        let mut resolved = source.clone();
        for &i in &orders[id] {
            if i < source.instances.len() {
                let leaf = &source.instances[i];
                if let Some(a) = &leaf.attachment {
                    let path = target_path(&a.instance, &a.target_path)?;
                    let destination = datum_frame(
                        &resolved,
                        &path,
                        &a.target_datum,
                        memo,
                        index,
                        parts,
                        tolerance,
                    )?;
                    resolved.instances[i].transform =
                        destination.compose(&leaf.transform, tolerance)?.compose(
                            &parts[&leaf.part].datums[&a.datum].inverse(tolerance)?,
                            tolerance,
                        )?;
                }
                resolved.instances[i].attachment = None;
            } else {
                let j = i - source.instances.len();
                let child = &source.assemblies[j];
                if let Some(a) = &child.attachment {
                    let path = target_path(&a.instance, &a.target_path)?;
                    let destination = datum_frame(
                        &resolved,
                        &path,
                        &a.target_datum,
                        memo,
                        index,
                        parts,
                        tolerance,
                    )?;
                    let source_path: Vec<_> = a.source_path.iter().map(String::as_str).collect();
                    let source_frame = datum_frame(
                        &memo[&child.assembly],
                        &source_path,
                        &a.datum,
                        memo,
                        index,
                        parts,
                        tolerance,
                    )?;
                    resolved.assemblies[j].transform = destination
                        .compose(&child.transform, tolerance)?
                        .compose(&source_frame.inverse(tolerance)?, tolerance)?;
                }
                resolved.assemblies[j].attachment = None;
            }
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
            &index,
            &orders,
            tolerance,
            &mut memo,
        )?;
    }
    Ok(memo)
}
/// Compatibility view of derived local leaves. Consumers needing child placements
/// must use `resolve_hierarchy_placements` instead.
pub fn resolve_hierarchy_instances(
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    tolerance: Tolerance,
) -> Result<BTreeMap<String, Vec<Instance>>, AssemblyError> {
    Ok(resolve_hierarchy_placements(parts, definitions, tolerance)?
        .into_iter()
        .map(|(id, d)| {
            let mut leaves = d.instances;
            leaves.sort_by(|a, b| a.id.cmp(&b.id));
            (id, leaves)
        })
        .collect())
}
/// Legacy sibling-only local leaf resolver; descendant and child attachments
/// require the complete hierarchy and are rejected here explicitly.
pub fn resolve_definition_instances(
    definition: &AssemblyDefinition,
    parts: &BTreeMap<String, BuiltPart>,
    tolerance: Tolerance,
) -> Result<Vec<Instance>, AssemblyError> {
    if definition.instances.iter().any(|x| {
        x.attachment
            .as_ref()
            .is_some_and(|a| !a.target_path.is_empty())
    }) || definition.assemblies.iter().any(|x| x.attachment.is_some())
    {
        return Err(AssemblyError::Attachment(
            "attachment requires a complete assembly hierarchy".into(),
        ));
    }
    let mut local = definition.clone();
    local.assemblies.clear();
    let result = resolve_hierarchy_instances(
        parts,
        &BTreeMap::from([(local.id.clone(), local)]),
        tolerance,
    )?;
    Ok(result[&definition.id].clone())
}
