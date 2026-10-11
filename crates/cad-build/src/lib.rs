//! A small, versioned recipe graph with deterministic geometry caching.
//!
//! Logical identity belongs to the recipe. Kernel results and their topology
//! history are independent of recipe IDs, so identical features can share them.

use cad_kernel::{
    boolean, extrude_rectangle, transform, translate, validate_transform, BooleanOp,
    OperationResult, Tolerance, KERNEL_BUILD_ID, KERNEL_VERSION,
};
pub use cad_kernel::{CircleHole, MAX_POLYGON_HOLES, MAX_POLYGON_POINTS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use thiserror::Error;

mod assembly;
mod demand;
pub use demand::{BuildMode, BuildScope};
mod face_datums;
pub use face_datums::{
    FaceDatumDefinition, FaceDatumResolution, ResolvedFaceDatums, MAX_FACE_DATUMS,
    MAX_FACE_DATUM_GEOMETRY_WORK,
};
mod selection;
pub use assembly::{MAX_ASSEMBLIES, MAX_FEATURES, MAX_INSTANCES, MAX_OUTPUTS, MAX_PARTS};
pub use cad_assembly::{
    validate_datums, AssemblyAttachment, AssemblyDefinition, AssemblyInstance, AssemblyOutput,
    DatumAttachment, Instance, Material, PartDefinition, PartMetadata, RigidTransform,
};
pub use selection::{
    FaceCardinality, FaceLineageStep, FaceResolution, FaceResolutionStatus, FaceSelector,
    FaceWitness, MAX_FACE_SELECTION_WORK,
};

#[cfg(test)]
mod tests;

/// Latest recipe schema. Version 1 remains supported for solid-only recipes.
pub const RECIPE_SCHEMA_VERSION: u32 = 2;

/// Fingerprint the executor/IR implementation independently of package semver.
/// Source bytes are embedded at compilation; no runtime source access is needed.
pub fn executor_build_id() -> &'static str {
    static BUILD_ID: OnceLock<String> = OnceLock::new();
    BUILD_ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        hash.update(b"headless-cad/executor-build/v1\0");
        for source in [
            include_bytes!("lib.rs").as_slice(),
            include_bytes!("assembly.rs").as_slice(),
            include_bytes!("demand.rs").as_slice(),
            include_bytes!("selection.rs").as_slice(),
            include_bytes!("face_datums.rs").as_slice(),
            include_bytes!("../Cargo.toml").as_slice(),
        ] {
            hash.update(&(source.len() as u64).to_le_bytes());
            hash.update(source);
        }
        hash.finalize().to_hex().to_string()
    })
}
/// Maximum number of persisted semantic face assertions in one recipe.
pub const MAX_FACE_ASSERTIONS: usize = 1_000;
/// Shared traversal, copied-handle and identifier-byte budget for all assertions
/// in one build, preventing successful reports from multiplying memory use.
pub const MAX_FACE_ASSERTION_WORK: usize = MAX_FACE_SELECTION_WORK;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceAssertion {
    pub id: String,
    pub selector: FaceSelector,
}

fn operation_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub schema_version: u32,
    pub units: String,
    #[serde(default)]
    pub tolerance: Option<Tolerance>,
    pub features: Vec<Feature>,
    pub outputs: Vec<Output>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<PartDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<AssemblyDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub assembly_outputs: Vec<AssemblyOutput>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub face_assertions: Vec<FaceAssertion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub face_datums: Vec<FaceDatumDefinition>,
}

impl<'de> Deserialize<'de> for Recipe {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        fn present_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
        where
            D: serde::Deserializer<'de>,
            T: Deserialize<'de>,
        {
            Vec::deserialize(deserializer).map(Some)
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawRecipe {
            schema_version: u32,
            units: String,
            #[serde(default)]
            tolerance: Option<Tolerance>,
            features: Vec<Feature>,
            #[serde(default)]
            outputs: Vec<Output>,
            #[serde(default, deserialize_with = "present_vec")]
            parts: Option<Vec<PartDefinition>>,
            #[serde(default, deserialize_with = "present_vec")]
            assemblies: Option<Vec<AssemblyDefinition>>,
            #[serde(default, deserialize_with = "present_vec")]
            assembly_outputs: Option<Vec<AssemblyOutput>>,
            #[serde(default, deserialize_with = "present_vec")]
            face_assertions: Option<Vec<FaceAssertion>>,
            #[serde(default, deserialize_with = "present_vec")]
            face_datums: Option<Vec<FaceDatumDefinition>>,
        }
        let raw = RawRecipe::deserialize(deserializer)?;
        if raw.schema_version == 1
            && (raw.parts.is_some() || raw.assemblies.is_some() || raw.assembly_outputs.is_some())
        {
            return Err(serde::de::Error::custom(
                "assembly fields require recipe schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.face_assertions.is_some() {
            return Err(serde::de::Error::custom(
                "face assertions require recipe schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.face_datums.is_some() {
            return Err(serde::de::Error::custom(
                "face datums require recipe schema version 2",
            ));
        }
        Ok(Self {
            schema_version: raw.schema_version,
            units: raw.units,
            tolerance: raw.tolerance,
            features: raw.features,
            outputs: raw.outputs,
            parts: raw.parts.unwrap_or_default(),
            assemblies: raw.assemblies.unwrap_or_default(),
            assembly_outputs: raw.assembly_outputs.unwrap_or_default(),
            face_assertions: raw.face_assertions.unwrap_or_default(),
            face_datums: raw.face_datums.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Feature {
    Circle {
        id: String,
        radius: f64,
        #[serde(skip_serializing_if = "is_zero")]
        inner_radius: f64,
        op_version: u32,
    },
    Rectangle {
        id: String,
        width: f64,
        depth: f64,
        op_version: u32,
    },
    Polygon {
        id: String,
        points: Vec<[f64; 2]>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        holes: Vec<Vec<[f64; 2]>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        circular_holes: Vec<cad_kernel::CircleHole>,
        op_version: u32,
    },
    Revolve {
        id: String,
        profile: String,
        op_version: u32,
    },
    Extrude {
        id: String,
        profile: String,
        height: f64,
        op_version: u32,
    },
    Translate {
        id: String,
        body: String,
        offset: [f64; 3],
        op_version: u32,
    },
    Transform {
        id: String,
        body: String,
        offset: [f64; 3],
        rotation: [f64; 4],
        op_version: u32,
    },
    Union {
        id: String,
        left: String,
        right: String,
        op_version: u32,
    },
    Cut {
        id: String,
        left: String,
        right: String,
        op_version: u32,
    },
    Intersect {
        id: String,
        left: String,
        right: String,
        op_version: u32,
    },
}

// Read typed values directly: buffering internally tagged enums through Serde
// Content breaks floats when Starlark enables serde_json/arbitrary_precision.
impl<'de> Deserialize<'de> for Feature {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FeatureVisitor;
        impl<'de> serde::de::Visitor<'de> for FeatureVisitor {
            type Value = Feature;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a supported CAD feature")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Feature, M::Error> {
                use serde::de::Error;
                let mut op = None;
                let mut id = None;
                let mut op_version = None;
                let mut radius = None;
                let mut inner_radius = None;
                let mut width = None;
                let mut depth = None;
                let mut profile = None;
                let mut points = None;
                let mut holes = None;
                let mut circular_holes = None;
                let mut height = None;
                let mut body = None;
                let mut offset = None;
                let mut rotation = None;
                let mut left = None;
                let mut right = None;
                let mut seen = Vec::new();
                macro_rules! read_field {
                    ($field:ident,$ty:ty) => {{
                        if $field.is_some() {
                            return Err(M::Error::duplicate_field(stringify!($field)));
                        }
                        $field = Some(map.next_value::<$ty>()?);
                    }};
                }
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "op" => read_field!(op, String),
                        "id" => read_field!(id, String),
                        "op_version" => read_field!(op_version, u32),
                        "radius" => read_field!(radius, f64),
                        "inner_radius" => read_field!(inner_radius, f64),
                        "width" => read_field!(width, f64),
                        "depth" => read_field!(depth, f64),
                        "profile" => read_field!(profile, String),
                        "points" => read_field!(points, Vec<[f64; 2]>),
                        "holes" => read_field!(holes, Vec<Vec<[f64; 2]>>),
                        "circular_holes" => {
                            read_field!(circular_holes, Vec<cad_kernel::CircleHole>)
                        }
                        "height" => read_field!(height, f64),
                        "body" => read_field!(body, String),
                        "offset" => read_field!(offset, [f64; 3]),
                        "rotation" => read_field!(rotation, [f64; 4]),
                        "left" => read_field!(left, String),
                        "right" => read_field!(right, String),
                        _ => {
                            return Err(M::Error::unknown_field(
                                &key,
                                &[
                                    "op",
                                    "id",
                                    "op_version",
                                    "radius",
                                    "inner_radius",
                                    "width",
                                    "depth",
                                    "profile",
                                    "points",
                                    "holes",
                                    "circular_holes",
                                    "height",
                                    "body",
                                    "offset",
                                    "rotation",
                                    "left",
                                    "right",
                                ],
                            ))
                        }
                    }
                    seen.push(key);
                }
                let op = op.ok_or_else(|| M::Error::missing_field("op"))?;
                let id = id.ok_or_else(|| M::Error::missing_field("id"))?;
                let op_version = op_version.unwrap_or_else(operation_version);
                let allowed: &[&str] = match op.as_str() {
                    "circle" => &["op", "id", "op_version", "radius", "inner_radius"],
                    "rectangle" => &["op", "id", "op_version", "width", "depth"],
                    "polygon" => &[
                        "op",
                        "id",
                        "op_version",
                        "points",
                        "holes",
                        "circular_holes",
                    ],
                    "revolve" => &["op", "id", "op_version", "profile"],
                    "extrude" => &["op", "id", "op_version", "profile", "height"],
                    "translate" => &["op", "id", "op_version", "body", "offset"],
                    "transform" => &["op", "id", "op_version", "body", "offset", "rotation"],
                    "union" => &["op", "id", "op_version", "left", "right"],
                    "cut" => &["op", "id", "op_version", "left", "right"],
                    "intersect" => &["op", "id", "op_version", "left", "right"],
                    _ => {
                        return Err(M::Error::unknown_variant(
                            &op,
                            &[
                                "circle",
                                "rectangle",
                                "polygon",
                                "extrude",
                                "revolve",
                                "translate",
                                "transform",
                                "union",
                                "cut",
                                "intersect",
                            ],
                        ))
                    }
                };
                for field in &seen {
                    if !allowed.contains(&field.as_str()) {
                        return Err(M::Error::unknown_field(field, allowed));
                    }
                }
                Ok(match op.as_str() {
                    "circle" => Feature::Circle {
                        id,
                        op_version,
                        radius: radius.ok_or_else(|| M::Error::missing_field("radius"))?,
                        inner_radius: inner_radius.unwrap_or(0.),
                    },
                    "rectangle" => Feature::Rectangle {
                        id,
                        op_version,
                        width: width.ok_or_else(|| M::Error::missing_field("width"))?,
                        depth: depth.ok_or_else(|| M::Error::missing_field("depth"))?,
                    },
                    "polygon" => Feature::Polygon {
                        id,
                        op_version,
                        points: points.ok_or_else(|| M::Error::missing_field("points"))?,
                        holes: holes.unwrap_or_default(),
                        circular_holes: circular_holes.unwrap_or_default(),
                    },
                    "revolve" => Feature::Revolve {
                        id,
                        op_version,
                        profile: profile.ok_or_else(|| M::Error::missing_field("profile"))?,
                    },
                    "extrude" => Feature::Extrude {
                        id,
                        op_version,
                        profile: profile.ok_or_else(|| M::Error::missing_field("profile"))?,
                        height: height.ok_or_else(|| M::Error::missing_field("height"))?,
                    },
                    "translate" => Feature::Translate {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        offset: offset.ok_or_else(|| M::Error::missing_field("offset"))?,
                    },
                    "transform" => Feature::Transform {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        offset: offset.ok_or_else(|| M::Error::missing_field("offset"))?,
                        rotation: rotation.ok_or_else(|| M::Error::missing_field("rotation"))?,
                    },
                    "union" => Feature::Union {
                        id,
                        op_version,
                        left: left.ok_or_else(|| M::Error::missing_field("left"))?,
                        right: right.ok_or_else(|| M::Error::missing_field("right"))?,
                    },
                    "cut" => Feature::Cut {
                        id,
                        op_version,
                        left: left.ok_or_else(|| M::Error::missing_field("left"))?,
                        right: right.ok_or_else(|| M::Error::missing_field("right"))?,
                    },
                    "intersect" => Feature::Intersect {
                        id,
                        op_version,
                        left: left.ok_or_else(|| M::Error::missing_field("left"))?,
                        right: right.ok_or_else(|| M::Error::missing_field("right"))?,
                    },
                    _ => unreachable!("operation checked above"),
                })
            }
        }
        deserializer.deserialize_map(FeatureVisitor)
    }
}

fn is_zero(value: &f64) -> bool {
    *value == 0.
}

impl Feature {
    pub fn id(&self) -> &str {
        match self {
            Self::Circle { id, .. }
            | Self::Rectangle { id, .. }
            | Self::Polygon { id, .. }
            | Self::Extrude { id, .. }
            | Self::Revolve { id, .. }
            | Self::Translate { id, .. }
            | Self::Transform { id, .. }
            | Self::Union { id, .. }
            | Self::Cut { id, .. }
            | Self::Intersect { id, .. } => id,
        }
    }
    pub fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Circle { .. } | Self::Rectangle { .. } | Self::Polygon { .. } => vec![],
            Self::Extrude { profile, .. } | Self::Revolve { profile, .. } => vec![profile],
            Self::Translate { body, .. } | Self::Transform { body, .. } => vec![body],
            Self::Union { left, right, .. }
            | Self::Cut { left, right, .. }
            | Self::Intersect { left, right, .. } => vec![left, right],
        }
    }
    pub fn is_solid(&self) -> bool {
        !matches!(
            self,
            Self::Circle { .. } | Self::Rectangle { .. } | Self::Polygon { .. }
        )
    }
    fn version(&self) -> u32 {
        match self {
            Self::Circle { op_version, .. }
            | Self::Rectangle { op_version, .. }
            | Self::Polygon { op_version, .. }
            | Self::Extrude { op_version, .. }
            | Self::Revolve { op_version, .. }
            | Self::Translate { op_version, .. }
            | Self::Transform { op_version, .. }
            | Self::Union { op_version, .. }
            | Self::Cut { op_version, .. }
            | Self::Intersect { op_version, .. } => *op_version,
        }
    }
    fn operation(&self) -> &'static str {
        match self {
            Self::Circle { .. } => "circle",
            Self::Rectangle { .. } => "rectangle",
            Self::Polygon { .. } => "polygon",
            Self::Extrude { .. } => "extrude",
            Self::Revolve { .. } => "revolve",
            Self::Translate { .. } => "translate",
            Self::Transform { .. } => "transform",
            Self::Union { .. } => "union",
            Self::Cut { .. } => "cut",
            Self::Intersect { .. } => "intersect",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub name: String,
    pub feature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogicalProvenance {
    pub feature_id: String,
    pub input_features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BuiltOutput {
    pub feature_id: String,
    pub content_hash: String,
    pub result: Arc<OperationResult>,
    pub provenance: LogicalProvenance,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct BuildStats {
    /// All cache hits, including the disk-cache subset.
    pub cache_hits: usize,
    pub disk_cache_hits: usize,
    pub computed_solids: usize,
}

#[derive(Debug)]
pub struct BuildResult {
    pub scope: BuildScope,
    pub outputs: BTreeMap<String, BuiltOutput>,
    pub assemblies: BTreeMap<String, cad_assembly::BuiltAssembly>,
    pub feature_hashes: BTreeMap<String, String>,
    pub stats: BuildStats,
    pub face_assertions: BTreeMap<String, FaceResolution>,
    pub face_datums: ResolvedFaceDatums,
    selection_graph: selection::SelectionGraph,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("output selection must contain at least one output name")]
    EmptyOutputSelection,
    #[error("unknown selected output '{0}'")]
    UnknownSelectedOutput(String),
    #[error("face datums require recipe schema version 2")]
    FaceDatumSchema,
    #[error("invalid face datum '{part}/{name}': {message}")]
    InvalidFaceDatum {
        part: String,
        name: String,
        message: String,
    },
    #[error("face datum '{part}/{name}' did not resolve to exactly one face")]
    FaceDatumFailed {
        part: String,
        name: String,
        resolution: Box<FaceResolution>,
        target_content_hash: Option<String>,
        anchor_content_hash: Option<String>,
    },
    #[error("face datum '{part}/{name}' could not form a rigid frame: {message}")]
    FaceDatumFrame {
        part: String,
        name: String,
        message: String,
    },
    #[error("invalid polygon profile '{feature}': {message}")]
    PolygonProfile { feature: String, message: String },
    #[error("face assertions require recipe schema version 2")]
    FaceAssertionSchema,
    #[error("invalid face assertion '{id}': {message}")]
    InvalidFaceAssertion { id: String, message: String },
    #[error("face assertion '{assertion_id}' failed: {diagnostic}", diagnostic = resolution.diagnostic)]
    FaceAssertionFailed {
        assertion_id: String,
        resolution: Box<FaceResolution>,
        anchor_content_hash: Option<String>,
        target_content_hash: Option<String>,
    },
    #[error("unsupported recipe schema version {0}; expected 1 or 2")]
    SchemaVersion(u32),
    #[error("assembly fields require recipe schema version 2")]
    AssemblySchema,
    #[error("recipe exceeds {limit} {resource}")]
    ResourceLimit {
        resource: &'static str,
        limit: usize,
    },
    #[error("invalid assembly graph: {0}")]
    AssemblyGraph(String),
    #[error("assembly '{assembly}' failed: {message}")]
    Assembly { assembly: String, message: String },
    #[error("unsupported units '{0}'; expected 'mm'")]
    Units(String),
    #[error("tolerance.linear must be finite and positive")]
    Tolerance,
    #[error("{kind} identifier '{id}' must be nonempty with no surrounding whitespace")]
    Identifier { kind: &'static str, id: String },
    #[error("duplicate feature ID '{0}'")]
    DuplicateFeature(String),
    #[error("duplicate output name '{0}'")]
    DuplicateOutput(String),
    #[error("feature '{feature}' uses unsupported operation version {version}; expected 1")]
    OperationVersion { feature: String, version: u32 },
    #[error("feature '{feature}' references missing feature '{dependency}'")]
    MissingDependency { feature: String, dependency: String },
    #[error("dependency cycle: {0}")]
    Cycle(String),
    #[error("feature '{feature}' needs a {expected}; '{dependency}' has the wrong type")]
    InputType {
        feature: String,
        dependency: String,
        expected: &'static str,
    },
    #[error("feature '{feature}' offset must contain finite lengths")]
    Offset { feature: String },
    #[error("feature '{feature}' has an invalid rigid transform: {message}")]
    TransformFrame { feature: String, message: String },
    #[error("feature '{feature}' parameter '{parameter}' must be finite and positive")]
    Dimension {
        feature: String,
        parameter: &'static str,
    },
    #[error("output '{output}' references missing feature '{feature}'")]
    MissingOutput { output: String, feature: String },
    #[error("output '{output}' must reference a solid; '{feature}' is a profile")]
    OutputType { output: String, feature: String },
    #[error("feature '{feature}' failed: {message}")]
    Kernel { feature: String, message: String },
}

/// Geometry cache. Disk caching is optional and best effort: corrupt, stale, or
/// inaccessible cache entries are misses and never prevent a geometry build.
#[derive(Default)]
pub struct BuildEngine {
    solids: HashMap<String, Arc<OperationResult>>,
    cache_dir: Option<PathBuf>,
}

pub fn build(recipe: &Recipe) -> Result<BuildResult, BuildError> {
    BuildEngine::default().build(recipe)
}

fn identifier(kind: &'static str, id: &str) -> Result<(), BuildError> {
    if id.is_empty() || id.trim() != id {
        return Err(BuildError::Identifier {
            kind,
            id: id.into(),
        });
    }
    Ok(())
}

fn dimension(feature: &str, parameter: &'static str, value: f64) -> Result<(), BuildError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(BuildError::Dimension {
            feature: feature.into(),
            parameter,
        });
    }
    Ok(())
}

// Iterative DFS supports shared inputs and multiple dependencies without making
// the Rust call stack proportional to user-controlled graph depth.
fn ordered_features(
    recipe: &Recipe,
    indices: &HashMap<&str, usize>,
) -> Result<Vec<usize>, BuildError> {
    let mut state = vec![0u8; recipe.features.len()];
    let mut order = Vec::new();
    for start in 0..recipe.features.len() {
        if state[start] == 2 {
            continue;
        }
        state[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some(&(index, next_dependency)) = stack.last() {
            let feature = &recipe.features[index];
            let dependencies = feature.dependencies();
            if next_dependency == dependencies.len() {
                stack.pop();
                state[index] = 2;
                order.push(index);
                continue;
            }
            stack.last_mut().unwrap().1 += 1;
            let dependency = dependencies[next_dependency];
            let target = *indices
                .get(dependency)
                .ok_or_else(|| BuildError::MissingDependency {
                    feature: feature.id().into(),
                    dependency: dependency.into(),
                })?;
            match state[target] {
                0 => {
                    state[target] = 1;
                    stack.push((target, 0));
                }
                1 => {
                    let first = stack.iter().position(|&(i, _)| i == target).unwrap();
                    let mut path: Vec<_> = stack[first..]
                        .iter()
                        .map(|&(i, _)| recipe.features[i].id())
                        .collect();
                    path.push(recipe.features[target].id());
                    return Err(BuildError::Cycle(path.join(" -> ")));
                }
                _ => {}
            }
        }
    }
    Ok(order)
}

fn hash_feature(
    recipe: &Recipe,
    tolerance: Tolerance,
    feature: &Feature,
    hashes: &BTreeMap<String, String>,
) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(b"headless-cad/feature/v3\0");
    hash.update(env!("CARGO_PKG_VERSION").as_bytes());
    hash.update(b"\0");
    hash.update(KERNEL_VERSION.as_bytes());
    hash.update(b"\0");
    hash.update(KERNEL_BUILD_ID.as_bytes());
    hash.update(b"\0");
    hash.update(executor_build_id().as_bytes());
    hash.update(b"\0mm\0");
    hash.update(&recipe.schema_version.to_le_bytes());
    hash.update(&feature.version().to_le_bytes());
    hash.update(&tolerance.linear.to_le_bytes());
    hash.update(feature.operation().as_bytes());
    hash.update(b"\0");
    for dependency in feature.dependencies() {
        hash.update(hashes[dependency].as_bytes());
    }
    match feature {
        Feature::Circle {
            radius,
            inner_radius,
            ..
        } => {
            hash.update(&radius.to_le_bytes());
            if *inner_radius != 0. {
                hash.update(b"inner_radius");
                hash.update(&inner_radius.to_le_bytes());
            }
        }
        Feature::Rectangle { width, depth, .. } => {
            hash.update(&width.to_le_bytes());
            hash.update(&depth.to_le_bytes());
        }
        Feature::Polygon {
            points,
            holes,
            circular_holes,
            ..
        } => {
            hash.update(&(points.len() as u64).to_le_bytes());
            for point in points {
                for value in point {
                    hash.update(&value.to_le_bytes());
                }
            }
            hash.update(&(holes.len() as u64).to_le_bytes());
            for hole in holes {
                hash.update(&(hole.len() as u64).to_le_bytes());
                for coordinate in hole.iter().flatten() {
                    hash.update(&coordinate.to_le_bytes());
                }
            }
            if !circular_holes.is_empty() {
                hash.update(b"circular_holes");
                hash.update(&(circular_holes.len() as u64).to_le_bytes());
                for hole in circular_holes {
                    for value in hole.center.into_iter().chain([hole.radius]) {
                        hash.update(&value.to_le_bytes());
                    }
                }
            }
        }
        Feature::Extrude { height, .. } => {
            hash.update(&height.to_le_bytes());
        }
        Feature::Transform {
            offset, rotation, ..
        } => {
            for value in offset.iter().chain(rotation.iter()) {
                hash.update(&value.to_le_bytes());
            }
        }
        Feature::Translate { offset, .. } => {
            for value in offset {
                hash.update(&value.to_le_bytes());
            }
        }
        _ => {}
    }
    hash.finalize().to_hex().to_string()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEntry {
    cache_version: u32,
    kernel_version: String,
    kernel_build_id: String,
    executor_build_id: String,
    feature_hash: String,
    result_digest: String,
    result: OperationResult,
}

impl BuildEngine {
    pub fn with_cache_dir(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: Some(cache_dir.into()),
            ..Self::default()
        }
    }

    fn cache_path(&self, hash: &str) -> Option<PathBuf> {
        Some(
            self.cache_dir
                .as_ref()?
                .join("geometry-v3")
                .join(KERNEL_BUILD_ID)
                .join(executor_build_id())
                .join(format!("{hash}.json")),
        )
    }

    fn disk_read(&self, hash: &str, tolerance: Tolerance) -> Option<OperationResult> {
        let path = self.cache_path(hash)?;
        // Bound allocations from accidentally corrupted or unrelated cache files.
        if std::fs::metadata(&path).ok()?.len() > 64 * 1024 * 1024 {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let entry: CacheEntry = serde_json::from_slice(&bytes).ok()?;
        if entry.cache_version != 3
            || entry.kernel_version != KERNEL_VERSION
            || entry.kernel_build_id != KERNEL_BUILD_ID
            || entry.executor_build_id != executor_build_id()
            || entry.feature_hash != hash
        {
            return None;
        }
        let serialized = serde_json::to_vec(&entry.result).ok()?;
        if blake3::hash(&serialized).to_hex().as_str() != entry.result_digest {
            return None;
        }
        if !entry.result.model.validate(tolerance).valid {
            return None;
        }
        if entry
            .result
            .history
            .generated_faces
            .values()
            .flatten()
            .any(|face| face.0 >= entry.result.model.faces.len())
            || entry
                .result
                .history
                .input_faces
                .values()
                .flat_map(|faces| faces.values())
                .flatten()
                .any(|face| face.0 >= entry.result.model.faces.len())
        {
            return None;
        }
        Some(entry.result)
    }

    fn disk_write(&self, hash: &str, result: &OperationResult) {
        let Some(path) = self.cache_path(hash) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(serialized) = serde_json::to_vec(result) else {
            return;
        };
        let entry = CacheEntry {
            cache_version: 3,
            kernel_version: KERNEL_VERSION.into(),
            kernel_build_id: KERNEL_BUILD_ID.into(),
            executor_build_id: executor_build_id().into(),
            feature_hash: hash.into(),
            result_digest: blake3::hash(&serialized).to_hex().to_string(),
            result: result.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&entry) else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let temporary = path.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        else {
            return;
        };
        let written = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        if written.is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
        let _ = std::fs::remove_file(temporary);
    }

    pub fn build(&mut self, recipe: &Recipe) -> Result<BuildResult, BuildError> {
        self.build_scoped(recipe, None)
    }

    /// Evaluate selected outputs plus all global semantic validation obligations.
    /// Structural validation still covers the entire recipe.
    pub fn build_outputs(
        &mut self,
        recipe: &Recipe,
        outputs: &[String],
    ) -> Result<BuildResult, BuildError> {
        self.build_scoped(recipe, Some(outputs))
    }

    fn build_scoped(
        &mut self,
        recipe: &Recipe,
        requested: Option<&[String]>,
    ) -> Result<BuildResult, BuildError> {
        if !matches!(recipe.schema_version, 1 | 2) {
            return Err(BuildError::SchemaVersion(recipe.schema_version));
        }
        assembly::resource_limits(recipe)?;
        face_datums::preflight(recipe)?;
        if recipe.schema_version == 1 && !recipe.face_assertions.is_empty() {
            return Err(BuildError::FaceAssertionSchema);
        }
        if recipe.face_assertions.len() > MAX_FACE_ASSERTIONS {
            return Err(BuildError::ResourceLimit {
                resource: "face assertions",
                limit: MAX_FACE_ASSERTIONS,
            });
        }
        let mut assertion_ids = std::collections::BTreeSet::new();
        for assertion in &recipe.face_assertions {
            identifier("face assertion", &assertion.id)?;
            if !assertion_ids.insert(&assertion.id) {
                return Err(BuildError::InvalidFaceAssertion {
                    id: assertion.id.clone(),
                    message: "duplicate assertion ID".into(),
                });
            }
            if matches!(
                assertion.selector.cardinality,
                FaceCardinality::Exactly { count: 0 }
            ) {
                return Err(BuildError::InvalidFaceAssertion {
                    id: assertion.id.clone(),
                    message: "exact cardinality must be positive".into(),
                });
            }
        }
        if recipe.schema_version == 1
            && (!recipe.parts.is_empty()
                || !recipe.assemblies.is_empty()
                || !recipe.assembly_outputs.is_empty())
        {
            return Err(BuildError::AssemblySchema);
        }
        if recipe.units != "mm" {
            return Err(BuildError::Units(recipe.units.clone()));
        }
        let tolerance = recipe.tolerance.unwrap_or_default();
        if !tolerance.linear.is_finite() || tolerance.linear <= 0.0 {
            return Err(BuildError::Tolerance);
        }
        let mut indices = HashMap::new();
        for (index, feature) in recipe.features.iter().enumerate() {
            identifier("feature", feature.id())?;
            if indices.insert(feature.id(), index).is_some() {
                return Err(BuildError::DuplicateFeature(feature.id().into()));
            }
            if feature.version() != 1 {
                return Err(BuildError::OperationVersion {
                    feature: feature.id().into(),
                    version: feature.version(),
                });
            }
        }
        let order = ordered_features(recipe, &indices)?;
        for feature in &recipe.features {
            match feature {
                Feature::Circle {
                    id,
                    radius,
                    inner_radius,
                    ..
                } => cad_kernel::validate_annulus(*radius, *inner_radius, tolerance).map_err(
                    |error| BuildError::Kernel {
                        feature: id.clone(),
                        message: error.to_string(),
                    },
                )?,
                Feature::Rectangle {
                    id, width, depth, ..
                } => {
                    dimension(id, "width", *width)?;
                    dimension(id, "depth", *depth)?;
                }
                Feature::Polygon {
                    id,
                    points,
                    holes,
                    circular_holes,
                    ..
                } => cad_kernel::validate_polygon_with_circular_holes(
                    points,
                    holes,
                    circular_holes,
                    tolerance,
                )
                .map_err(|error| BuildError::PolygonProfile {
                    feature: id.clone(),
                    message: error.to_string(),
                })?,
                Feature::Revolve { id, profile, .. } => {
                    match &recipe.features[indices[profile.as_str()]] {
                        Feature::Polygon {
                            points,
                            holes,
                            circular_holes,
                            ..
                        } if holes.is_empty() && circular_holes.is_empty() => {
                            cad_kernel::validate_revolve_profile(points, tolerance).map_err(
                                |error| BuildError::Kernel {
                                    feature: id.clone(),
                                    message: error.to_string(),
                                },
                            )?;
                        }
                        _ => {
                            return Err(BuildError::InputType {
                                feature: id.clone(),
                                dependency: profile.clone(),
                                expected: "polygon profile without holes",
                            })
                        }
                    }
                }
                Feature::Extrude { id, height, .. } => dimension(id, "height", *height)?,
                Feature::Transform {
                    id,
                    offset,
                    rotation,
                    ..
                } => validate_transform(*offset, *rotation).map_err(|error| {
                    BuildError::TransformFrame {
                        feature: id.clone(),
                        message: error.to_string(),
                    }
                })?,
                Feature::Translate { id, offset, .. } if offset.iter().any(|x| !x.is_finite()) => {
                    return Err(BuildError::Offset {
                        feature: id.clone(),
                    })
                }
                _ => {}
            }
            for dependency in feature.dependencies() {
                let needs_profile =
                    matches!(feature, Feature::Extrude { .. } | Feature::Revolve { .. });
                if recipe.features[indices[dependency]].is_solid() == needs_profile {
                    return Err(BuildError::InputType {
                        feature: feature.id().into(),
                        dependency: dependency.into(),
                        expected: if needs_profile { "profile" } else { "solid" },
                    });
                }
            }
        }
        let mut output_names = std::collections::HashSet::new();
        for output in &recipe.outputs {
            identifier("output", &output.name)?;
            if !output_names.insert(&output.name) {
                return Err(BuildError::DuplicateOutput(output.name.clone()));
            }
            let index =
                indices
                    .get(output.feature.as_str())
                    .ok_or_else(|| BuildError::MissingOutput {
                        output: output.name.clone(),
                        feature: output.feature.clone(),
                    })?;
            if !recipe.features[*index].is_solid() {
                return Err(BuildError::OutputType {
                    output: output.name.clone(),
                    feature: output.feature.clone(),
                });
            }
        }
        assembly::validate(recipe, &indices, &mut output_names)?;
        let scope = demand::scope(recipe, requested, &indices)?;
        let wanted: BTreeSet<_> = scope
            .evaluated_features
            .iter()
            .map(String::as_str)
            .collect();
        let mut hashes = BTreeMap::new();
        let mut evaluated: HashMap<String, Arc<OperationResult>> = HashMap::new();
        let mut stats = BuildStats::default();
        for &index in &order {
            let feature = &recipe.features[index];
            if !wanted.contains(feature.id()) {
                continue;
            }
            let hash = hash_feature(recipe, tolerance, feature, &hashes);
            if feature.is_solid() {
                let result = if let Some(cached) = self.solids.get(&hash) {
                    stats.cache_hits += 1;
                    Arc::clone(cached)
                } else if let Some(cached) = self.disk_read(&hash, tolerance) {
                    stats.cache_hits += 1;
                    stats.disk_cache_hits += 1;
                    let result = Arc::new(cached);
                    self.solids.insert(hash.clone(), Arc::clone(&result));
                    result
                } else {
                    let operation = match feature {
                        Feature::Revolve { profile, .. } => {
                            match &recipe.features[indices[profile.as_str()]] {
                                Feature::Polygon { points, .. } => {
                                    cad_kernel::revolve_polygon(points, tolerance)
                                }
                                _ => unreachable!("revolve profile validated before execution"),
                            }
                        }
                        Feature::Extrude {
                            profile, height, ..
                        } => match &recipe.features[indices[profile.as_str()]] {
                            Feature::Circle {
                                radius,
                                inner_radius,
                                ..
                            } => cad_kernel::extrude_annulus(
                                *radius,
                                *inner_radius,
                                *height,
                                tolerance,
                            ),
                            Feature::Rectangle { width, depth, .. } => {
                                extrude_rectangle(*width, *depth, *height, tolerance)
                            }
                            Feature::Polygon {
                                points,
                                holes,
                                circular_holes,
                                ..
                            } => cad_kernel::extrude_polygon_with_circular_holes(
                                points,
                                holes,
                                circular_holes,
                                *height,
                                tolerance,
                            ),
                            _ => unreachable!("profile types validated before execution"),
                        },
                        Feature::Transform {
                            body,
                            offset,
                            rotation,
                            ..
                        } => transform(&evaluated[body].model, *offset, *rotation, tolerance),
                        Feature::Translate { body, offset, .. } => {
                            translate(&evaluated[body].model, *offset, tolerance)
                        }
                        Feature::Union { left, right, .. }
                        | Feature::Cut { left, right, .. }
                        | Feature::Intersect { left, right, .. } => {
                            let op = match feature {
                                Feature::Union { .. } => BooleanOp::Union,
                                Feature::Cut { .. } => BooleanOp::Difference,
                                _ => BooleanOp::Intersection,
                            };
                            boolean(
                                &evaluated[left].model,
                                &evaluated[right].model,
                                op,
                                tolerance,
                            )
                        }
                        Feature::Circle { .. }
                        | Feature::Rectangle { .. }
                        | Feature::Polygon { .. } => unreachable!(),
                    };
                    let result = operation.map_err(|e| BuildError::Kernel {
                        feature: feature.id().into(),
                        message: e.to_string(),
                    })?;
                    if !result.model.validate(tolerance).valid {
                        return Err(BuildError::Kernel {
                            feature: feature.id().into(),
                            message: "kernel returned a model that failed validation".into(),
                        });
                    }
                    self.disk_write(&hash, &result);
                    let result = Arc::new(result);
                    self.solids.insert(hash.clone(), Arc::clone(&result));
                    stats.computed_solids += 1;
                    result
                };
                evaluated.insert(feature.id().into(), result);
            }
            hashes.insert(feature.id().into(), hash);
        }
        let outputs = recipe
            .outputs
            .iter()
            .filter(|output| scope.requested_outputs.binary_search(&output.name).is_ok())
            .map(|output| {
                let feature = &recipe.features[indices[output.feature.as_str()]];
                (
                    output.name.clone(),
                    BuiltOutput {
                        feature_id: feature.id().into(),
                        content_hash: hashes[feature.id()].clone(),
                        result: Arc::clone(&evaluated[feature.id()]),
                        provenance: LogicalProvenance {
                            feature_id: feature.id().into(),
                            input_features: feature
                                .dependencies()
                                .into_iter()
                                .map(str::to_owned)
                                .collect(),
                        },
                    },
                )
            })
            .collect();
        let selection_graph = selection::SelectionGraph::new(recipe, order, evaluated.clone());
        let mut result = BuildResult {
            scope,
            outputs,
            assemblies: BTreeMap::new(),
            feature_hashes: hashes,
            stats,
            face_assertions: BTreeMap::new(),
            face_datums: BTreeMap::new(),
            selection_graph,
        };
        let mut assertion_work_remaining = MAX_FACE_ASSERTION_WORK;
        for assertion in &recipe.face_assertions {
            let resolution = result
                .resolve_faces_with_remaining(&assertion.selector, &mut assertion_work_remaining);
            if resolution.status != FaceResolutionStatus::Resolved {
                return Err(BuildError::FaceAssertionFailed {
                    assertion_id: assertion.id.clone(),
                    anchor_content_hash: result
                        .feature_hashes
                        .get(&assertion.selector.anchor_feature)
                        .cloned(),
                    target_content_hash: result
                        .feature_hashes
                        .get(&assertion.selector.target_feature)
                        .cloned(),
                    resolution: Box::new(resolution),
                });
            }
            result
                .face_assertions
                .insert(assertion.id.clone(), resolution);
        }
        result.face_datums = face_datums::resolve(
            recipe,
            &result,
            &evaluated,
            tolerance,
            &mut assertion_work_remaining,
        )?;
        result.assemblies = assembly::evaluate(
            recipe,
            &result.feature_hashes,
            &evaluated,
            tolerance,
            &result.face_datums,
            &result.scope.requested_outputs,
        )?;
        Ok(result)
    }
}
