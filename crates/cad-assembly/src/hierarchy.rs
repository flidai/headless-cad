//! Bounded DAG validation and deterministic placement expansion.
use super::*;

const MAX_DEPTH: usize = 64;
const MAX_DEFINITIONS: usize = 10_000;
const MAX_PATH_BYTES: usize = 10_000_000;

#[derive(Clone, Copy)]
struct Size {
    leaves: usize,
    nodes: usize,
    depth: usize,
    path_bytes: usize,
}

fn add(a: usize, b: usize, limit: usize) -> Result<usize, AssemblyError> {
    a.checked_add(b)
        .filter(|&n| n <= limit)
        .ok_or(AssemblyError::ResourceLimit)
}

/// Validate all supplied definitions, including unreachable ones, before any
/// geometry evaluation/expansion. Limits: 64 definition levels, 100,000 expanded
/// leaves and assembly nodes per root, and 10 MB of unescaped leaf path text.
/// Leaf and child occurrence IDs share a namespace inside each definition.
pub fn validate_definitions(
    parts: &BTreeSet<String>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
) -> Result<(), AssemblyError> {
    if definitions.len() > MAX_DEFINITIONS || parts.len() > MAX_DEFINITIONS {
        return Err(AssemblyError::ResourceLimit);
    }
    let mut declared = 0;
    for (key, definition) in definitions {
        identifier("assembly", key)?;
        if key != &definition.id {
            return Err(AssemblyError::Hierarchy(
                "definition key differs from ID".into(),
            ));
        }
        declared = add(declared, definition.instances.len(), MAX_INSTANCES)?;
        declared = add(declared, definition.assemblies.len(), MAX_INSTANCES)?;
        let mut ids = BTreeSet::new();
        for instance in &definition.instances {
            identifier("instance", &instance.id)?;
            identifier("part reference", &instance.part)?;
            if !ids.insert(&instance.id) {
                return Err(AssemblyError::DuplicateInstance(instance.id.clone()));
            }
            instance.transform.validate()?;
            if !parts.contains(&instance.part) {
                return Err(AssemblyError::MissingPart {
                    instance: instance.id.clone(),
                    part: instance.part.clone(),
                });
            }
        }
        for child in &definition.assemblies {
            identifier("assembly instance", &child.id)?;
            identifier("assembly reference", &child.assembly)?;
            if !ids.insert(&child.id) {
                return Err(AssemblyError::DuplicateInstance(child.id.clone()));
            }
            child.transform.validate()?;
            if !definitions.contains_key(&child.assembly) {
                return Err(AssemblyError::Hierarchy(format!(
                    "missing assembly '{}'",
                    child.assembly
                )));
            }
        }
    }
    fn visit(
        id: &str,
        definitions: &BTreeMap<String, AssemblyDefinition>,
        active: &mut BTreeSet<String>,
        sizes: &mut BTreeMap<String, Size>,
    ) -> Result<Size, AssemblyError> {
        if let Some(size) = sizes.get(id) {
            return Ok(*size);
        }
        if active.len() >= MAX_DEPTH {
            return Err(AssemblyError::ResourceLimit);
        }
        if !active.insert(id.to_owned()) {
            return Err(AssemblyError::Hierarchy(format!(
                "assembly cycle at '{id}'"
            )));
        }
        let definition = &definitions[id];
        let mut size = Size {
            leaves: definition.instances.len(),
            nodes: 1,
            depth: 1,
            path_bytes: 0,
        };
        for leaf in &definition.instances {
            size.path_bytes = add(
                size.path_bytes,
                leaf.id.len().saturating_add(leaf.part.len()),
                MAX_PATH_BYTES,
            )?;
        }
        for child in &definition.assemblies {
            let nested = visit(&child.assembly, definitions, active, sizes)?;
            size.leaves = add(size.leaves, nested.leaves, MAX_INSTANCES)?;
            size.nodes = add(size.nodes, nested.nodes, MAX_INSTANCES)?;
            size.depth = size.depth.max(add(1, nested.depth, MAX_DEPTH)?);
            let prefix_bytes = child
                .id
                .len()
                .checked_mul(nested.leaves.saturating_add(nested.nodes))
                .ok_or(AssemblyError::ResourceLimit)?;
            size.path_bytes = add(size.path_bytes, nested.path_bytes, MAX_PATH_BYTES)?;
            size.path_bytes = add(size.path_bytes, prefix_bytes, MAX_PATH_BYTES)?;
        }
        active.remove(id);
        sizes.insert(id.to_owned(), size);
        Ok(size)
    }
    let mut sizes = BTreeMap::new();
    for id in definitions.keys() {
        visit(id, definitions, &mut BTreeSet::new(), &mut sizes)?;
    }
    Ok(())
}

impl RigidTransform {
    /// Compose parent placement with a child's local placement, in that order.
    /// Quaternion products are validated, never silently normalized. Reject
    /// translation cancellation beyond the supplied modeling tolerance. For
    /// non-permutation rotations, reject offsets whose conservative arithmetic
    /// roundoff envelope (64 eps times largest coordinate) exceeds tolerance.
    pub fn compose(&self, child: &Self, tolerance: Tolerance) -> Result<Self, AssemblyError> {
        if !tolerance.is_valid() {
            return Err(AssemblyError::Tolerance);
        }
        self.validate()?;
        child.validate()?;
        let matrix = self.matrix();
        let exact_permutation = matrix.iter().all(|row| {
            row.iter().filter(|&&v| v != 0.0).count() == 1
                && row.iter().all(|v| matches!(*v, -1.0 | 0.0 | 1.0))
        });
        let offset_scale = child
            .translation
            .into_iter()
            .map(f64::abs)
            .fold(0.0, f64::max);
        if !exact_permutation && offset_scale * (64.0 * f64::EPSILON) > tolerance.linear {
            return Err(AssemblyError::NumericRange);
        }
        let [x, y, z, w] = self.rotation;
        let [a, b, c, d] = child.rotation;
        let rotated = self.vector(child.translation);
        let mut translation = [0.0; 3];
        for i in 0..3 {
            translation[i] = self.translation[i] + rotated[i];
            // Error-free TwoSum residual for the translation addition.
            let recovered = translation[i] - self.translation[i];
            let residual =
                (self.translation[i] - (translation[i] - recovered)) + (rotated[i] - recovered);
            if !residual.is_finite() || residual.abs() > tolerance.linear {
                return Err(AssemblyError::NumericRange);
            }
        }
        let result = Self {
            translation,
            rotation: [
                w * a + x * d + y * c - z * b,
                w * b - x * c + y * d + z * a,
                w * c + x * b - y * a + z * d,
                w * d - x * a - y * b - z * c,
            ],
        };
        result.validate()?;
        Ok(result)
    }
}

type Expanded = (
    BTreeMap<String, AssemblyDefinition>,
    BTreeSet<String>,
    Vec<Instance>,
    BTreeMap<String, Vec<String>>,
);

pub(super) fn expand(
    root: &str,
    parts: &BTreeMap<String, BuiltPart>,
    definitions: &BTreeMap<String, AssemblyDefinition>,
    tolerance: Tolerance,
) -> Result<Expanded, AssemblyError> {
    if !tolerance.is_valid() {
        return Err(AssemblyError::Tolerance);
    }
    validate_definitions(&parts.keys().cloned().collect(), definitions)?;
    crate::datum::validate_built_attachments(parts, definitions)?;
    let root_definition = definitions
        .get(root)
        .ok_or_else(|| AssemblyError::Hierarchy(format!("missing root '{root}'")))?;
    let nested = !root_definition.assemblies.is_empty();
    let mut hierarchy = BTreeMap::new();
    let mut used_parts = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if hierarchy.contains_key(id) {
            continue;
        }
        let definition = &definitions[id];
        used_parts.extend(definition.instances.iter().map(|i| i.part.clone()));
        pending.extend(definition.assemblies.iter().map(|i| i.assembly.as_str()));
        hierarchy.insert(id.to_owned(), definition.clone());
    }
    // Preflight expanded placement work without allocating a leaf list.
    fn placement_work(
        id: &str,
        parts: &BTreeMap<String, BuiltPart>,
        defs: &BTreeMap<String, AssemblyDefinition>,
        memo: &mut BTreeMap<String, usize>,
    ) -> Result<usize, AssemblyError> {
        if let Some(&n) = memo.get(id) {
            return Ok(n);
        }
        let mut total = 0;
        for leaf in &defs[id].instances {
            let model = &parts[&leaf.part].result.model;
            total = add(total, model.vertices.len(), MAX_PLACEMENT_ENTITIES)?;
            total = add(total, model.edges.len(), MAX_PLACEMENT_ENTITIES)?;
        }
        for child in &defs[id].assemblies {
            total = add(
                total,
                placement_work(&child.assembly, parts, defs, memo)?,
                MAX_PLACEMENT_ENTITIES,
            )?;
        }
        memo.insert(id.to_owned(), total);
        Ok(total)
    }
    placement_work(root, parts, definitions, &mut BTreeMap::new())?;
    let resolved = resolve_hierarchy_instances(parts, &hierarchy, tolerance)?;
    let mut leaves = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut pending = vec![(root, Vec::<String>::new(), RigidTransform::default())];
    while let Some((id, prefix, transform)) = pending.pop() {
        for leaf in &resolved[id] {
            let mut path = prefix.clone();
            path.push(leaf.id.clone());
            let leaf_id = if nested {
                serde_json::to_string(&path)?
            } else {
                leaf.id.clone()
            };
            leaves.insert(
                leaf_id.clone(),
                Instance {
                    attachment: None,
                    id: leaf_id.clone(),
                    part: leaf.part.clone(),
                    transform: transform.compose(&leaf.transform, tolerance)?,
                },
            );
            paths.insert(leaf_id, path);
        }
        for child in &definitions[id].assemblies {
            let mut path = prefix.clone();
            path.push(child.id.clone());
            pending.push((
                child.assembly.as_str(),
                path,
                transform.compose(&child.transform, tolerance)?,
            ));
        }
    }
    Ok((hierarchy, used_parts, leaves.into_values().collect(), paths))
}
