//! Rigid placement and identity for assemblies of immutable kernel definitions.
//!
//! An assembly retains occurrences, including overlapping ones; it never performs
//! a Boolean union. Material volume is the sum per occurrence. Rotated geometry
//! remains local to its definition and does not expand the kernel's supported
//! modeling domain. Nested reusable definitions retain local placements and explicit
//! occurrence paths. Assembly constraints are not implemented.
mod bom;
mod hierarchy;
pub use bom::{BomItem, BomReport, Material, PartMetadata};
use cad_kernel::{Bounds, OperationResult, Tolerance};
pub use hierarchy::validate_definitions;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};
use thiserror::Error;

const ASSEMBLY_SCHEMA: &str = "headless-cad/assembly/v1";
const MAX_INSTANCES: usize = 100_000;
const MAX_PLACEMENT_ENTITIES: usize = 10_000_000;

/// Source/dependency fingerprint, independent of paths and timestamps.
/// Placement changes invalidate assembly artifacts without changing part hashes.
pub fn assembly_build_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        hash.update(include_bytes!("lib.rs"));
        hash.update(include_bytes!("hierarchy.rs"));
        hash.update(include_bytes!("bom.rs"));
        hash.update(include_bytes!("../Cargo.toml"));
        hash.update(include_bytes!("../../../Cargo.lock"));
        hash.update(cad_kernel::KERNEL_BUILD_ID.as_bytes());
        hash.finalize().to_hex().to_string()
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartDefinition {
    pub id: String,
    pub feature: String,
    #[serde(default)]
    pub metadata: PartMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: String,
    pub part: String,
    #[serde(default)]
    pub transform: RigidTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyDefinition {
    pub id: String,
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<AssemblyInstance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyInstance {
    pub id: String,
    pub assembly: String,
    #[serde(default)]
    pub transform: RigidTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyOutput {
    pub name: String,
    pub assembly: String,
}

fn identity_rotation() -> [f64; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

/// Millimetre translation and unit quaternion in `[x,y,z,w]` order.
/// Only proper rigid rotations are supported; no scaling or silent normalization.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidTransform {
    #[serde(default)]
    pub translation: [f64; 3],
    #[serde(default = "identity_rotation")]
    pub rotation: [f64; 4],
}

impl Default for RigidTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: identity_rotation(),
        }
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

impl RigidTransform {
    pub fn validate(&self) -> Result<(), AssemblyError> {
        if !self
            .translation
            .into_iter()
            .chain(self.rotation)
            .all(f64::is_finite)
        {
            return Err(AssemblyError::Transform(
                "transform components must be finite",
            ));
        }
        let norm_squared: f64 = self.rotation.into_iter().map(|q| q * q).sum();
        if !norm_squared.is_finite() || (norm_squared - 1.0).abs() > 32.0 * f64::EPSILON {
            return Err(AssemblyError::Transform(
                "rotation quaternion must be unit length; normalization is explicit",
            ));
        }
        Ok(())
    }

    fn matrix(&self) -> [[f64; 3]; 3] {
        let [x, y, z, w] = self.rotation;
        [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ]
    }

    /// Rotate a vector. Call `validate` before using externally supplied transforms.
    pub fn vector(&self, vector: [f64; 3]) -> [f64; 3] {
        self.matrix().map(|row| dot(row, vector))
    }

    /// Place a point in world space. Assembly validation checks range/precision.
    pub fn point(&self, point: [f64; 3]) -> [f64; 3] {
        let rotated = self.vector(point);
        std::array::from_fn(|i| rotated[i] + self.translation[i])
    }

    /// Conservative AABB of the source AABB's eight transformed corners.
    /// This can be larger than the tight AABB of a non-box part.
    pub fn transformed_bounds(&self, bounds: &Bounds) -> Result<Bounds, AssemblyError> {
        self.validate()?;
        if (0..3).any(|i| {
            !bounds.min[i].is_finite()
                || !bounds.max[i].is_finite()
                || bounds.min[i] > bounds.max[i]
        }) {
            return Err(AssemblyError::NumericRange);
        }
        let mut result = Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for corner in 0..8 {
            let point = self.point(std::array::from_fn(|axis| {
                if corner & (1 << axis) == 0 {
                    bounds.min[axis]
                } else {
                    bounds.max[axis]
                }
            }));
            if !point.into_iter().all(f64::is_finite) {
                return Err(AssemblyError::NumericRange);
            }
            for (axis, value) in point.into_iter().enumerate() {
                result.min[axis] = result.min[axis].min(value);
                result.max[axis] = result.max[axis].max(value);
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltPart {
    #[serde(default)]
    pub metadata: PartMetadata,
    pub feature_id: String,
    pub content_hash: String,
    pub result: Arc<OperationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltAssembly {
    pub assembly_id: String,
    pub content_hash: String,
    pub parts: BTreeMap<String, BuiltPart>,
    pub instances: Vec<Instance>,
    /// Reachable authoritative assembly definitions; root is `assembly_id`.
    pub hierarchy: BTreeMap<String, AssemblyDefinition>,
    /// Explicit occurrence path for every derived flattened instance ID.
    pub instance_paths: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Error)]
pub enum AssemblyError {
    #[error("invalid engineering metadata: {0}")]
    Metadata(&'static str),
    #[error("invalid assembly hierarchy: {0}")]
    Hierarchy(String),
    #[error("derived instances or paths do not match authoritative assembly hierarchy")]
    DerivedMismatch,
    #[error("{kind} identifier '{id}' must be nonempty with no surrounding whitespace")]
    Identifier { kind: &'static str, id: String },
    #[error("duplicate instance ID '{0}'")]
    DuplicateInstance(String),
    #[error("definition hash '{0}' identifies different part results")]
    DefinitionHashCollision(String),
    #[error("instance '{instance}' references missing part '{part}'")]
    MissingPart { instance: String, part: String },
    #[error("invalid rigid transform: {0}")]
    Transform(&'static str),
    #[error("linear tolerance must be finite and positive")]
    Tolerance,
    #[error("part '{part}' is not a valid kernel model: {message}")]
    InvalidPart { part: String, message: String },
    #[error("assembly placement exceeds numeric range or modeling precision")]
    NumericRange,
    #[error("assembly exceeds instance count or aggregate placement entity budget")]
    ResourceLimit,
    #[error("assembly content hash does not match its definitions and placements")]
    HashMismatch,
    #[error("could not encode assembly identity: {0}")]
    Serialization(#[from] serde_json::Error),
}

fn identifier(kind: &'static str, id: &str) -> Result<(), AssemblyError> {
    if id.is_empty() || id.trim() != id {
        return Err(AssemblyError::Identifier {
            kind,
            id: id.to_owned(),
        });
    }
    Ok(())
}

impl BuiltAssembly {
    /// Construct and validate an assembly while retaining shared part results.
    /// Supply the referenced definitions; definitions are never copied per instance.
    pub fn new(
        assembly_id: String,
        parts: BTreeMap<String, BuiltPart>,
        instances: Vec<Instance>,
        tolerance: Tolerance,
    ) -> Result<Self, AssemblyError> {
        let definition = AssemblyDefinition {
            id: assembly_id.clone(),
            instances,
            assemblies: vec![],
        };
        Self::from_definitions(
            assembly_id.clone(),
            parts,
            BTreeMap::from([(assembly_id, definition)]),
            tolerance,
        )
    }

    /// Build a reachable hierarchy of shared definitions, checking expansion
    /// limits before materializing leaf occurrences. Root-local leaf IDs retain
    /// their original spelling for flat assemblies. Nested hierarchies encode
    /// *all* flattened IDs as JSON arrays, while `instance_paths` keeps the
    /// original components without delimiter ambiguity.
    pub fn from_definitions(
        root: String,
        parts: BTreeMap<String, BuiltPart>,
        definitions: BTreeMap<String, AssemblyDefinition>,
        tolerance: Tolerance,
    ) -> Result<Self, AssemblyError> {
        let (hierarchy, used_parts, instances, instance_paths) =
            hierarchy::expand(&root, &parts, &definitions, tolerance)?;
        let parts = parts
            .into_iter()
            .filter(|(id, _)| used_parts.contains(id))
            .collect();
        let mut result = Self {
            assembly_id: root,
            content_hash: String::new(),
            parts,
            instances,
            hierarchy,
            instance_paths,
        };
        result.validate_structure(tolerance)?;
        result.content_hash = result.hash()?;
        Ok(result)
    }

    /// Validate externally supplied/deserialized assemblies, including identity.
    pub fn validate(&self, tolerance: Tolerance) -> Result<(), AssemblyError> {
        let (hierarchy, used_parts, instances, paths) =
            hierarchy::expand(&self.assembly_id, &self.parts, &self.hierarchy, tolerance)?;
        if hierarchy != self.hierarchy
            || used_parts.len() != self.parts.len()
            || instances != self.instances
            || paths != self.instance_paths
        {
            return Err(AssemblyError::DerivedMismatch);
        }
        self.validate_structure(tolerance)?;
        if self.content_hash != self.hash()? {
            return Err(AssemblyError::HashMismatch);
        }
        Ok(())
    }

    fn validate_structure(&self, tolerance: Tolerance) -> Result<(), AssemblyError> {
        if !tolerance.is_valid() {
            return Err(AssemblyError::Tolerance);
        }
        identifier("assembly", &self.assembly_id)?;
        if self.instances.len() > MAX_INSTANCES {
            return Err(AssemblyError::ResourceLimit);
        }
        let mut definition_digests = BTreeMap::new();
        for (id, part) in &self.parts {
            identifier("part", id)?;
            part.metadata.validate()?;
            identifier("feature", &part.feature_id)?;
            identifier("definition hash", &part.content_hash)?;
            let report = part.result.model.validate(tolerance);
            if !report.valid {
                return Err(AssemblyError::InvalidPart {
                    part: id.clone(),
                    message: format!("{:?}", report.issues),
                });
            }
            let digest = blake3::hash(&serde_json::to_vec(&*part.result)?);
            if definition_digests
                .insert(&part.content_hash, digest)
                .is_some_and(|previous| previous != digest)
            {
                return Err(AssemblyError::DefinitionHashCollision(
                    part.content_hash.clone(),
                ));
            }
        }
        // Preflight the entire placement workload before transforming vertices.
        // A future implementation can replace per-edge checks with certified
        // transform error bounds; this increment uses an explicit work budget.
        let placement_entities = self.instances.iter().try_fold(0usize, |total, instance| {
            let part =
                self.parts
                    .get(&instance.part)
                    .ok_or_else(|| AssemblyError::MissingPart {
                        instance: instance.id.clone(),
                        part: instance.part.clone(),
                    })?;
            total
                .checked_add(part.result.model.vertices.len())
                .and_then(|total| total.checked_add(part.result.model.edges.len()))
                .ok_or(AssemblyError::ResourceLimit)
        })?;
        if placement_entities > MAX_PLACEMENT_ENTITIES {
            return Err(AssemblyError::ResourceLimit);
        }
        let mut ids = BTreeSet::new();
        let mut solids = 0usize;
        for instance in &self.instances {
            identifier("instance", &instance.id)?;
            identifier("part reference", &instance.part)?;
            if !ids.insert(&instance.id) {
                return Err(AssemblyError::DuplicateInstance(instance.id.clone()));
            }
            instance.transform.validate()?;
            let part =
                self.parts
                    .get(&instance.part)
                    .ok_or_else(|| AssemblyError::MissingPart {
                        instance: instance.id.clone(),
                        part: instance.part.clone(),
                    })?;
            let model = &part.result.model;
            solids = solids
                .checked_add(model.solids.len())
                .ok_or(AssemblyError::ResourceLimit)?;
            if !model.vertices.is_empty() {
                instance.transform.transformed_bounds(&model.bounds())?;
            }
            let placed: Vec<_> = model
                .vertices
                .iter()
                .map(|v| instance.transform.point(v.position))
                .collect();
            if placed.iter().flatten().any(|n| !n.is_finite()) {
                return Err(AssemblyError::NumericRange);
            }
            for edge in &model.edges {
                let [a, b] = edge.vertices;
                let local = sub(model.vertices[b.0].position, model.vertices[a.0].position);
                let world = sub(placed[b.0], placed[a.0]);
                let expected = instance.transform.vector(local);
                // Detect cancellation after large translations as well as the
                // finite-precision error in the nominally rigid rotation.
                if norm(sub(world, expected)) > tolerance.linear
                    || (norm(world) - norm(local)).abs() > tolerance.linear
                {
                    return Err(AssemblyError::NumericRange);
                }
            }
        }
        let volume = self.material_volume();
        if !volume.is_finite() || volume < 0.0 {
            return Err(AssemblyError::NumericRange);
        }
        if let Some(bounds) = self.bounds() {
            if bounds
                .min
                .into_iter()
                .chain(bounds.max)
                .any(|v| !v.is_finite())
            {
                return Err(AssemblyError::NumericRange);
            }
        }
        self.calculate_bom()?;
        Ok(())
    }

    fn hash(&self) -> Result<String, AssemblyError> {
        let mut instances: Vec<_> = self.instances.iter().collect();
        instances.sort_by(|a, b| a.id.cmp(&b.id));
        // Bind both declared dependency hashes and their actual immutable data.
        // This prevents callers assigning one hash to two different definitions
        // from creating indistinguishable public assembly objects.
        let definitions: BTreeMap<_, _> = self
            .parts
            .iter()
            .map(|(id, part)| {
                let bytes = serde_json::to_vec(&*part.result)?;
                Ok((
                    id,
                    (
                        &part.feature_id,
                        &part.content_hash,
                        &part.metadata,
                        blake3::hash(&bytes).to_hex().to_string(),
                    ),
                ))
            })
            .collect::<Result<_, serde_json::Error>>()?;
        let bytes = serde_json::to_vec(&(
            ASSEMBLY_SCHEMA,
            assembly_build_id(),
            &self.assembly_id,
            definitions,
            self.hierarchy
                .iter()
                .map(|(id, definition)| {
                    let mut leaves: Vec<_> = definition.instances.iter().collect();
                    leaves.sort_by(|a, b| a.id.cmp(&b.id));
                    let mut children: Vec<_> = definition.assemblies.iter().collect();
                    children.sort_by(|a, b| a.id.cmp(&b.id));
                    (id, leaves, children)
                })
                .collect::<Vec<_>>(),
            &self.instance_paths,
            instances,
        ))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Total solid occurrences, not the number of solids after an assembly union.
    /// Validate before calling on externally modified structures.
    pub fn solid_count(&self) -> usize {
        self.instances
            .iter()
            .filter_map(|i| self.parts.get(&i.part))
            .map(|p| p.result.model.solids.len())
            .sum()
    }

    /// Sum of definition volume per occurrence, including overlapping occurrences.
    /// This is deliberately not the union volume. Validate external data first.
    pub fn material_volume(&self) -> f64 {
        let mut total = 0.0;
        let mut compensation = 0.0;
        for instance in &self.instances {
            let Some(part) = self.parts.get(&instance.part) else {
                return f64::NAN;
            };
            let value = part.result.model.volume() - compensation;
            let next = total + value;
            compensation = (next - total) - value;
            total = next;
        }
        total
    }

    /// Conservative world-space bounds; None for no nonempty occurrences.
    /// Validate external data first; invalid placements also return None.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut result: Option<Bounds> = None;
        for instance in &self.instances {
            let model = &self.parts.get(&instance.part)?.result.model;
            if model.vertices.is_empty() {
                continue;
            }
            let placed = instance
                .transform
                .transformed_bounds(&model.bounds())
                .ok()?;
            if let Some(bounds) = &mut result {
                for axis in 0..3 {
                    bounds.min[axis] = bounds.min[axis].min(placed.min[axis]);
                    bounds.max[axis] = bounds.max[axis].max(placed.max[axis]);
                }
            } else {
                result = Some(placed);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests;
