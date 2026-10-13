//! A small, versioned recipe graph with deterministic geometry caching.
//!
//! Logical identity belongs to the recipe. Kernel results and their topology
//! history are independent of recipe IDs, so identical features can share them.

use cad_kernel::{
    boolean, extrude_rectangle, transform, translate, validate_transform, BooleanOp,
    OperationResult, Tolerance, KERNEL_BUILD_ID, KERNEL_VERSION,
};
pub use cad_kernel::{CircleHole, PathSegment, PlanarPath, MAX_POLYGON_HOLES, MAX_POLYGON_POINTS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use thiserror::Error;

pub mod capabilities;
pub use capabilities::{capability_catalog, CapabilityCatalog, CAPABILITY_SCHEMA_VERSION};

mod sketch_profiles;
pub use cad_sketch::{
    Constraint, ConstraintKind, IslandReport, LineSide, Sketch, SketchPoint, SolveReport,
    SolveStatus,
};
pub fn solver_build_id() -> &'static str {
    cad_sketch::SOLVER_BUILD_ID
}

mod assembly;
mod demand;
mod tolerance_stacks;
pub use tolerance_stacks::{
    ToleranceStack, ToleranceStackReport, ToleranceStackTerm, ToleranceStackTermReport,
    MAX_TOLERANCE_STACKS, MAX_TOLERANCE_STACK_TERMS, MAX_TOLERANCE_STACK_TERMS_TOTAL,
};
mod rotation_groups;
pub use rotation_groups::{
    RigidRotationGroup, RigidRotationGroupReport, MAX_ROTATION_GROUPS, MAX_ROTATION_GROUP_MEMBERS,
    MAX_ROTATION_GROUP_MEMBERS_TOTAL,
};
mod datum_dimensions;
pub use datum_dimensions::{
    DatumDimension, DatumDimensionEndpoint, DatumDimensionReport, DatumDimensionWitness,
    MAX_DATUM_DIMENSIONS, MAX_DATUM_DIMENSION_WORK,
};
mod clearance_checks;
pub use clearance_checks::{
    ClearanceCheck, ClearanceCheckReport, ClearanceOccurrenceWitness, RotationPath,
    MAX_CLEARANCE_CHECKS, MAX_CLEARANCE_RESOLVE_WORK, MAX_CLEARANCE_SWEEPS,
    MAX_CLEARANCE_SWEEP_QUERIES, MAX_CLEARANCE_TRANSLATION_MM,
};
mod dimension_tolerance;
pub use dimension_tolerance::{DimensionTolerance, DimensionToleranceReport};
mod drawing_sheets;
mod exchange_requirements;
pub use drawing_sheets::{
    DrawingDimensionRow, DrawingDimensionTable, DrawingSheet, DrawingSheetReport, DrawingViewport,
    DRAWING_MARGIN_MM, DRAWING_TITLE_BLOCK_MM, MAX_DRAWING_DIMENSION_ROWS,
    MAX_DRAWING_DIMENSION_ROWS_PER_TABLE, MAX_DRAWING_DIMENSION_TABLES_PER_SHEET,
    MAX_DRAWING_SHEETS, MAX_DRAWING_VIEWPORTS, MAX_DRAWING_VIEWPORTS_PER_SHEET,
};
mod section_views;
pub use section_views::{
    query_section, SectionAnchor, SectionAnchorWitness, SectionArcWitness, SectionCircleCoverage,
    SectionCircleDimension, SectionCircleDimensionKind, SectionCircleDimensionReport,
    SectionDimension, SectionDimensionAxis, SectionDimensionReport, SectionOccurrence,
    SectionQuery, SectionSource, SectionSummary, SectionView, SectionViewReport,
    MAX_SECTION_DIMENSIONS, MAX_SECTION_DIMENSIONS_PER_VIEW, MAX_SECTION_VIEWS,
    MAX_SECTION_VIEW_WORK,
};
mod property_assertions;
pub use demand::{BuildMode, BuildScope};
pub use exchange_requirements::{ExchangeRequirement, EXCHANGE_ASPECTS, MAX_EXCHANGE_REQUIREMENTS};
pub use property_assertions::{
    EngineeringProperty, PropertyAssertion, PropertyAssertionReport, PropertyAssertionStatus,
    MAX_PROPERTY_ASSERTIONS, MAX_PROPERTY_ASSERTION_WORK,
};
mod motion_studies;
pub use motion_studies::{
    MotionStudyDefinition, MotionStudyReport, MAX_MOTION_ATTEMPTS, MAX_MOTION_STUDIES,
};
mod mechanisms;
pub use mechanisms::{
    MechanismBinding, MechanismBindingReport, MechanismDefinition, MechanismReport, MAX_MECHANISMS,
    MAX_MECHANISM_BINDINGS,
};
mod face_datums;
pub use face_datums::{
    FaceDatumDefinition, FaceDatumResolution, ResolvedFaceDatums, MAX_FACE_DATUMS,
    MAX_FACE_DATUM_GEOMETRY_WORK,
};
mod selection;
pub use assembly::{MAX_ASSEMBLIES, MAX_FEATURES, MAX_INSTANCES, MAX_OUTPUTS, MAX_PARTS};
pub use cad_assembly::{
    validate_datums, AssemblyAttachment, AssemblyDefinition, AssemblyInstance, AssemblyOutput,
    DatumAttachment, Instance, JointReport, Material, PartDefinition, PartMetadata,
    PrescribedJoint, RigidTransform, MAX_PRESCRIBED_JOINTS,
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
            include_bytes!("sketch_profiles.rs").as_slice(),
            include_bytes!("demand.rs").as_slice(),
            include_bytes!("selection.rs").as_slice(),
            include_bytes!("face_datums.rs").as_slice(),
            include_bytes!("mechanisms.rs").as_slice(),
            include_bytes!("motion_studies.rs").as_slice(),
            include_bytes!("property_assertions.rs").as_slice(),
            include_bytes!("exchange_requirements.rs").as_slice(),
            include_bytes!("section_views.rs").as_slice(),
            include_bytes!("drawing_sheets.rs").as_slice(),
            include_bytes!("dimension_tolerance.rs").as_slice(),
            include_bytes!("tolerance_stacks.rs").as_slice(),
            include_bytes!("clearance_checks.rs").as_slice(),
            include_bytes!("datum_dimensions.rs").as_slice(),
            include_bytes!("rotation_groups.rs").as_slice(),
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
    pub property_assertions: Vec<PropertyAssertion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exchange_requirements: Vec<ExchangeRequirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub section_views: Vec<SectionView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drawing_sheets: Vec<DrawingSheet>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tolerance_stacks: Vec<ToleranceStack>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clearance_checks: Vec<ClearanceCheck>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub datum_dimensions: Vec<DatumDimension>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rigid_rotation_groups: Vec<RigidRotationGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub face_datums: Vec<FaceDatumDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mechanisms: Vec<MechanismDefinition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mechanism_bindings: Vec<MechanismBinding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub motion_studies: Vec<MotionStudyDefinition>,
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
            property_assertions: Option<Vec<PropertyAssertion>>,
            #[serde(default, deserialize_with = "present_vec")]
            exchange_requirements: Option<Vec<ExchangeRequirement>>,
            #[serde(default, deserialize_with = "present_vec")]
            section_views: Option<Vec<SectionView>>,
            #[serde(default, deserialize_with = "present_vec")]
            drawing_sheets: Option<Vec<DrawingSheet>>,
            #[serde(default, deserialize_with = "present_vec")]
            tolerance_stacks: Option<Vec<ToleranceStack>>,
            #[serde(default, deserialize_with = "present_vec")]
            clearance_checks: Option<Vec<ClearanceCheck>>,
            #[serde(default, deserialize_with = "present_vec")]
            datum_dimensions: Option<Vec<DatumDimension>>,
            #[serde(default, deserialize_with = "present_vec")]
            rigid_rotation_groups: Option<Vec<RigidRotationGroup>>,
            #[serde(default, deserialize_with = "present_vec")]
            face_datums: Option<Vec<FaceDatumDefinition>>,
            #[serde(default, deserialize_with = "present_vec")]
            mechanisms: Option<Vec<MechanismDefinition>>,
            #[serde(default, deserialize_with = "present_vec")]
            mechanism_bindings: Option<Vec<MechanismBinding>>,
            #[serde(default, deserialize_with = "present_vec")]
            motion_studies: Option<Vec<MotionStudyDefinition>>,
        }
        let raw = RawRecipe::deserialize(deserializer)?;
        if raw.schema_version == 1 && raw.motion_studies.is_some() {
            return Err(serde::de::Error::custom(
                "motion studies require schema version 2",
            ));
        }
        if raw.schema_version == 1 && (raw.mechanisms.is_some() || raw.mechanism_bindings.is_some())
        {
            return Err(serde::de::Error::custom(
                "mechanisms require schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.rigid_rotation_groups.is_some() {
            return Err(serde::de::Error::custom(
                "rigid rotation groups require schema2",
            ));
        }
        if raw.schema_version == 1 && raw.datum_dimensions.is_some() {
            return Err(serde::de::Error::custom(
                "datum dimensions require schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.clearance_checks.is_some() {
            return Err(serde::de::Error::custom(
                "clearance checks require schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.tolerance_stacks.is_some() {
            return Err(serde::de::Error::custom(
                "tolerance stacks require schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.drawing_sheets.is_some() {
            return Err(serde::de::Error::custom(
                "drawing sheets require schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.section_views.is_some() {
            return Err(serde::de::Error::custom(
                "section views require schema version 2",
            ));
        }
        if raw.schema_version == 1
            && (raw.parts.is_some() || raw.assemblies.is_some() || raw.assembly_outputs.is_some())
        {
            return Err(serde::de::Error::custom(
                "assembly fields require recipe schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.exchange_requirements.is_some() {
            return Err(serde::de::Error::custom(
                "exchange requirements require recipe schema version 2",
            ));
        }
        if raw.schema_version == 1 && raw.property_assertions.is_some() {
            return Err(serde::de::Error::custom(
                "property assertions require recipe schema version 2",
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
            property_assertions: raw.property_assertions.unwrap_or_default(),
            exchange_requirements: raw.exchange_requirements.unwrap_or_default(),
            section_views: raw.section_views.unwrap_or_default(),
            drawing_sheets: raw.drawing_sheets.unwrap_or_default(),
            tolerance_stacks: raw.tolerance_stacks.unwrap_or_default(),
            clearance_checks: raw.clearance_checks.unwrap_or_default(),
            datum_dimensions: raw.datum_dimensions.unwrap_or_default(),
            rigid_rotation_groups: raw.rigid_rotation_groups.unwrap_or_default(),
            face_datums: raw.face_datums.unwrap_or_default(),
            mechanisms: raw.mechanisms.unwrap_or_default(),
            mechanism_bindings: raw.mechanism_bindings.unwrap_or_default(),
            motion_studies: raw.motion_studies.unwrap_or_default(),
        })
    }
}

/// A solved center point and radius handle define a native circular hole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchCircleHole {
    pub center: String,
    pub rim: String,
}

/// One named blind circular cut positioned by solved face-local sketch points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlindPatternHole {
    pub id: String,
    pub center: String,
    pub rim: String,
    pub depth: f64,
}
pub const MAX_BLIND_PATTERN_HOLES: usize = 16;

/// A closed outer path whose coordinates come from solved sketch points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchPath {
    pub start: String,
    pub segments: Vec<SketchPathSegment>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SketchPathSegment {
    LineTo {
        end: String,
    },
    ArcTo {
        center: String,
        end: String,
        #[serde(default)]
        clockwise: bool,
    },
}

/// Source-owned semantic face intent; snapshot-local handles are never authoring input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChamferFace {
    pub from_feature: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Feature {
    Sketch {
        id: String,
        sketch: Sketch,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        circular_holes: Vec<SketchCircleHole>,
        loops: Vec<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        curve_outline: Option<SketchPath>,
        op_version: u32,
    },
    Path {
        id: String,
        #[serde(flatten)]
        path: PlanarPath,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        circular_holes: Vec<CircleHole>,
        op_version: u32,
    },
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
    Fillet {
        id: String,
        body: String,
        faces: [ChamferFace; 2],
        radius: f64,
        op_version: u32,
    },
    HolePattern {
        id: String,
        body: String,
        face: ChamferFace,
        x_direction_hint: [f64; 3],
        origin_offset: [f64; 2],
        sketch: Sketch,
        #[serde(rename = "pattern_holes")]
        holes: Vec<BlindPatternHole>,
        op_version: u32,
    },
    ChamferFaces {
        id: String,
        body: String,
        faces: [ChamferFace; 2],
        distance: f64,
        op_version: u32,
    },
    Chamfer {
        id: String,
        body: String,
        face_normals: [[f64; 3]; 2],
        distance: f64,
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
                let mut start = None;
                let mut segments = None;
                let mut sketch = None;
                let mut loops = None;
                let mut curve_outline = None;
                let mut holes = None;
                let mut circular_holes = None;
                let mut height = None;
                let mut body = None;
                let mut face_normals = None;
                let mut face = None;
                let mut x_direction_hint = None;
                let mut origin_offset = None;
                let mut pattern_holes = None;
                let mut faces = None;
                let mut distance = None;
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
                        "start" => read_field!(start, [f64; 2]),
                        "segments" => read_field!(segments, Vec<PathSegment>),
                        "sketch" => read_field!(sketch, Sketch),
                        "loops" => read_field!(loops, Vec<Vec<String>>),
                        "curve_outline" => read_field!(curve_outline, SketchPath),
                        "holes" => read_field!(holes, Vec<Vec<[f64; 2]>>),
                        "circular_holes" => {
                            read_field!(circular_holes, Vec<sketch_profiles::InputHole>)
                        }
                        "height" => read_field!(height, f64),
                        "body" => read_field!(body, String),
                        "faces" => read_field!(faces, [ChamferFace; 2]),
                        "face_normals" => read_field!(face_normals, [[f64; 3]; 2]),
                        "face" => read_field!(face, ChamferFace),
                        "x_direction_hint" => read_field!(x_direction_hint, [f64; 3]),
                        "origin_offset" => read_field!(origin_offset, [f64; 2]),
                        "pattern_holes" => read_field!(pattern_holes, Vec<BlindPatternHole>),
                        "distance" => read_field!(distance, f64),
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
                                    "start",
                                    "segments",
                                    "sketch",
                                    "loops",
                                    "curve_outline",
                                    "holes",
                                    "circular_holes",
                                    "height",
                                    "body",
                                    "faces",
                                    "face_normals",
                                    "face",
                                    "x_direction_hint",
                                    "origin_offset",
                                    "pattern_holes",
                                    "distance",
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
                    "sketch" => &[
                        "op",
                        "id",
                        "op_version",
                        "sketch",
                        "loops",
                        "curve_outline",
                        "circular_holes",
                    ],
                    "path" => &[
                        "op",
                        "id",
                        "op_version",
                        "start",
                        "segments",
                        "circular_holes",
                    ],
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
                    "fillet" => &["op", "id", "op_version", "body", "faces", "radius"],
                    "hole_pattern" => &[
                        "op",
                        "id",
                        "op_version",
                        "body",
                        "face",
                        "x_direction_hint",
                        "origin_offset",
                        "sketch",
                        "pattern_holes",
                    ],
                    "chamfer_faces" => &["op", "id", "op_version", "body", "faces", "distance"],
                    "chamfer" => &["op", "id", "op_version", "body", "face_normals", "distance"],
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
                                "path",
                                "sketch",
                                "extrude",
                                "revolve",
                                "fillet",
                                "hole_pattern",
                                "chamfer_faces",
                                "chamfer",
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
                    "sketch" => Feature::Sketch {
                        id,
                        op_version,
                        sketch: sketch.ok_or_else(|| M::Error::missing_field("sketch"))?,
                        circular_holes: circular_holes
                            .unwrap_or_default()
                            .into_iter()
                            .map(sketch_profiles::InputHole::into_sketch)
                            .collect::<Result<_, M::Error>>()?,
                        loops: match loops {
                            Some(loops) => loops,
                            None if curve_outline.is_some() => vec![],
                            None => return Err(M::Error::missing_field("loops")),
                        },
                        curve_outline,
                    },
                    "path" => Feature::Path {
                        id,
                        op_version,
                        path: PlanarPath {
                            start: start.ok_or_else(|| M::Error::missing_field("start"))?,
                            segments: segments
                                .ok_or_else(|| M::Error::missing_field("segments"))?,
                        },
                        circular_holes: circular_holes
                            .unwrap_or_default()
                            .into_iter()
                            .map(sketch_profiles::InputHole::into_circle)
                            .collect::<Result<_, M::Error>>()?,
                    },
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
                        circular_holes: circular_holes
                            .unwrap_or_default()
                            .into_iter()
                            .map(sketch_profiles::InputHole::into_circle)
                            .collect::<Result<_, M::Error>>()?,
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
                    "fillet" => Feature::Fillet {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        faces: faces.ok_or_else(|| M::Error::missing_field("faces"))?,
                        radius: radius.ok_or_else(|| M::Error::missing_field("radius"))?,
                    },
                    "hole_pattern" => Feature::HolePattern {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        face: face.ok_or_else(|| M::Error::missing_field("face"))?,
                        x_direction_hint: x_direction_hint
                            .ok_or_else(|| M::Error::missing_field("x_direction_hint"))?,
                        origin_offset: origin_offset.unwrap_or([0.; 2]),
                        sketch: sketch.ok_or_else(|| M::Error::missing_field("sketch"))?,
                        holes: pattern_holes
                            .ok_or_else(|| M::Error::missing_field("pattern_holes"))?,
                    },
                    "chamfer_faces" => Feature::ChamferFaces {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        faces: faces.ok_or_else(|| M::Error::missing_field("faces"))?,
                        distance: distance.ok_or_else(|| M::Error::missing_field("distance"))?,
                    },
                    "chamfer" => Feature::Chamfer {
                        id,
                        op_version,
                        body: body.ok_or_else(|| M::Error::missing_field("body"))?,
                        face_normals: face_normals
                            .ok_or_else(|| M::Error::missing_field("face_normals"))?,
                        distance: distance.ok_or_else(|| M::Error::missing_field("distance"))?,
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
            | Self::Sketch { id, .. }
            | Self::Path { id, .. }
            | Self::Extrude { id, .. }
            | Self::Revolve { id, .. }
            | Self::Fillet { id, .. }
            | Self::HolePattern { id, .. }
            | Self::ChamferFaces { id, .. }
            | Self::Chamfer { id, .. }
            | Self::Translate { id, .. }
            | Self::Transform { id, .. }
            | Self::Union { id, .. }
            | Self::Cut { id, .. }
            | Self::Intersect { id, .. } => id,
        }
    }
    pub fn dependencies(&self) -> Vec<&str> {
        match self {
            Self::Circle { .. }
            | Self::Rectangle { .. }
            | Self::Polygon { .. }
            | Self::Sketch { .. }
            | Self::Path { .. } => vec![],
            Self::Extrude { profile, .. } | Self::Revolve { profile, .. } => vec![profile],
            Self::ChamferFaces { body, faces, .. } | Self::Fillet { body, faces, .. } => {
                let mut dependencies = vec![body.as_str()];
                for face in faces {
                    if !dependencies.contains(&face.from_feature.as_str()) {
                        dependencies.push(&face.from_feature);
                    }
                }
                dependencies
            }
            Self::HolePattern { body, face, .. } => {
                let mut dependencies = vec![body.as_str()];
                if face.from_feature != *body {
                    dependencies.push(&face.from_feature);
                }
                dependencies
            }
            Self::Chamfer { body, .. }
            | Self::Translate { body, .. }
            | Self::Transform { body, .. } => vec![body],
            Self::Union { left, right, .. }
            | Self::Cut { left, right, .. }
            | Self::Intersect { left, right, .. } => vec![left, right],
        }
    }
    pub fn is_solid(&self) -> bool {
        !matches!(
            self,
            Self::Sketch { .. }
                | Self::Circle { .. }
                | Self::Rectangle { .. }
                | Self::Polygon { .. }
                | Self::Path { .. }
        )
    }
    fn version(&self) -> u32 {
        match self {
            Self::Circle { op_version, .. }
            | Self::Rectangle { op_version, .. }
            | Self::Polygon { op_version, .. }
            | Self::Sketch { op_version, .. }
            | Self::Path { op_version, .. }
            | Self::Extrude { op_version, .. }
            | Self::Revolve { op_version, .. }
            | Self::Fillet { op_version, .. }
            | Self::HolePattern { op_version, .. }
            | Self::ChamferFaces { op_version, .. }
            | Self::Chamfer { op_version, .. }
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
            Self::Sketch { .. } => "sketch",
            Self::Path { .. } => "path",
            Self::Extrude { .. } => "extrude",
            Self::Revolve { .. } => "revolve",
            Self::Fillet { .. } => "fillet",
            Self::HolePattern { .. } => "hole_pattern",
            Self::ChamferFaces { .. } => "chamfer_faces",
            Self::Chamfer { .. } => "chamfer",
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
    pub sketch_reports: BTreeMap<String, SolveReport>,
    pub scope: BuildScope,
    pub outputs: BTreeMap<String, BuiltOutput>,
    pub assemblies: BTreeMap<String, cad_assembly::BuiltAssembly>,
    pub feature_hashes: BTreeMap<String, String>,
    pub stats: BuildStats,
    pub face_assertions: BTreeMap<String, FaceResolution>,
    pub property_assertions: BTreeMap<String, PropertyAssertionReport>,
    /// Unenforced publication obligations; native geometry success does not satisfy them.
    pub pending_exchange_requirements: Vec<ExchangeRequirement>,
    pub section_views: BTreeMap<String, SectionViewReport>,
    pub drawing_sheets: BTreeMap<String, DrawingSheetReport>,
    pub tolerance_stacks: BTreeMap<String, ToleranceStackReport>,
    pub clearance_checks: BTreeMap<String, ClearanceCheckReport>,
    pub datum_dimensions: BTreeMap<String, DatumDimensionReport>,
    pub rigid_rotation_groups: BTreeMap<String, RigidRotationGroupReport>,
    pub face_datums: ResolvedFaceDatums,
    pub mechanism_reports: BTreeMap<String, MechanismReport>,
    pub mechanism_bindings: Vec<MechanismBindingReport>,
    pub mechanism_context_hash: String,
    pub motion_studies: BTreeMap<String, MotionStudyReport>,
    selection_graph: selection::SelectionGraph,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("invalid rotation group '{id}' on '{output}': {message}")]
    InvalidRotationGroup {
        id: String,
        output: String,
        message: String,
    },
    #[error("rotation group '{id}' on '{output}' failed: {message}")]
    RotationGroupFailed {
        id: String,
        output: String,
        message: String,
    },
    #[error("invalid datum dimension '{id}' on '{output}': {message}")]
    InvalidDatumDimension {
        id: String,
        output: String,
        message: String,
    },
    #[error("datum dimension '{id}' on '{output}' failed: {message}")]
    DatumDimensionFailed {
        id: String,
        output: String,
        message: String,
        actual_value: Option<f64>,
    },
    #[error("motion study '{id}' failed: {message}")]
    MotionStudyFailed {
        id: String,
        message: String,
        report: Option<Box<cad_assembly::MotionFailure>>,
    },
    #[error("mechanism '{id}' failed: {message}")]
    MechanismFailed {
        id: String,
        message: String,
        report: Option<Box<SolveReport>>,
    },
    #[error("section dimension '{id}' in view '{view}' on output '{output}' failed: {message}")]
    SectionDimensionFailed {
        view: String,
        id: String,
        output: String,
        message: String,
        actual_value: Option<f64>,
    },
    #[error("section views require recipe schema version 2")]
    SectionViewSchema,
    #[error("drawing sheets require recipe schema version 2")]
    DrawingSheetSchema,
    #[error("invalid clearance check '{id}' on '{output}': {message}")]
    InvalidClearanceCheck {
        id: String,
        output: String,
        message: String,
    },
    #[error("clearance check '{id}' on '{output}' failed: {message}")]
    ClearanceCheckFailed {
        id: String,
        output: String,
        message: String,
        lower_bound_mm: Option<f64>,
        upper_bound_mm: Option<f64>,
    },
    #[error("invalid tolerance stack '{id}': {message}")]
    InvalidToleranceStack { id: String, message: String },
    #[error("tolerance stack '{id}' failed: {message}; interval [{lower_limit_mm:?}, {upper_limit_mm:?}]")]
    ToleranceStackFailed {
        id: String,
        message: String,
        lower_limit_mm: Option<f64>,
        upper_limit_mm: Option<f64>,
    },
    #[error("invalid drawing sheet '{id}': {message}")]
    InvalidDrawingSheet { id: String, message: String },
    #[error("invalid section view '{id}': {message}")]
    InvalidSectionView { id: String, message: String },
    #[error("section view '{id}' on output '{output}' failed: {message}")]
    SectionViewFailed {
        id: String,
        output: String,
        message: String,
        actual_area: Option<f64>,
    },
    #[error("section query failed: {message}")]
    SectionQuery { message: String },
    #[error("exchange requirements require recipe schema version 2")]
    ExchangeRequirementSchema,
    #[error("invalid exchange requirement '{id}': {message}")]
    InvalidExchangeRequirement { id: String, message: String },
    #[error("invalid sketch feature '{feature}': {message}")]
    Sketch { feature: String, message: String },
    #[error("hole_pattern requires recipe schema version 2")]
    HolePatternSchema,
    #[error("sketch feature '{feature}' did not solve completely")]
    SketchSolveFailed {
        feature: String,
        report: Box<SolveReport>,
        content_hash: String,
    },
    #[error("property assertions require recipe schema version 2")]
    PropertyAssertionSchema,
    #[error("invalid property assertion '{id}': {message}")]
    InvalidPropertyAssertion { id: String, message: String },
    #[error("property assertion '{assertion_id}' failed")]
    PropertyAssertionFailed {
        assertion_id: String,
        report: Box<PropertyAssertionReport>,
    },
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

/// Persistent in-memory cache limits. Logical bytes are the serialized result
/// length, not allocated bytes or a process memory limit.
pub const DEFAULT_CACHE_MAX_ENTRIES: usize = 1_024;
pub const DEFAULT_CACHE_MAX_LOGICAL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CacheUsage {
    pub max_entries: usize,
    pub max_logical_bytes: u64,
    pub entries: usize,
    pub logical_bytes: u64,
    pub pinned_entries: usize,
    pub pinned_logical_bytes: u64,
    /// Possible only after lowering limits while external results pin entries.
    pub over_budget_entries: usize,
    pub over_budget_logical_bytes: u64,
    pub evictions: u64,
    pub skipped_retention: u64,
}

struct CachedSolid {
    result: Arc<OperationResult>,
    logical_bytes: u64,
    last_used: u64,
}

#[derive(Default)]
struct LogicalByteCounter(u64);

impl std::io::Write for LogicalByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("serialized result length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn logical_result_bytes(result: &OperationResult) -> Option<u64> {
    let mut counter = LogicalByteCounter::default();
    serde_json::to_writer(&mut counter, result).ok()?;
    Some(counter.0)
}

/// Geometry cache. Disk caching is optional and best effort: corrupt, stale, or
/// inaccessible cache entries are misses and never prevent a geometry build.
pub struct BuildEngine {
    solids: HashMap<String, CachedSolid>,
    cache_dir: Option<PathBuf>,
    cache_max_entries: usize,
    cache_max_logical_bytes: u64,
    cache_logical_bytes: u64,
    cache_clock: u64,
    cache_evictions: u64,
    cache_skipped_retention: u64,
}

impl Default for BuildEngine {
    fn default() -> Self {
        Self {
            solids: HashMap::new(),
            cache_dir: None,
            cache_max_entries: DEFAULT_CACHE_MAX_ENTRIES,
            cache_max_logical_bytes: DEFAULT_CACHE_MAX_LOGICAL_BYTES,
            cache_logical_bytes: 0,
            cache_clock: 0,
            cache_evictions: 0,
            cache_skipped_retention: 0,
        }
    }
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
        Feature::Sketch {
            sketch,
            loops,
            curve_outline,
            circular_holes,
            ..
        } => {
            hash.update(cad_sketch::SOLVER_BUILD_ID.as_bytes());
            hash.update(
                &serde_json::to_vec(&(sketch, loops)).expect("validated sketch serializes"),
            );
            if let Some(path) = curve_outline {
                hash.update(b"sketch_curve_outline");
                hash.update(
                    &serde_json::to_vec(path).expect("validated path references serialize"),
                );
            }
            if !circular_holes.is_empty() {
                hash.update(b"sketch_circular_holes");
                hash.update(
                    &serde_json::to_vec(circular_holes)
                        .expect("validated circle references serialize"),
                );
            }
        }
        Feature::Path {
            path,
            circular_holes,
            ..
        } => {
            for coordinate in path.start {
                hash.update(&coordinate.to_le_bytes());
            }
            hash.update(&(path.segments.len() as u64).to_le_bytes());
            for segment in &path.segments {
                match segment {
                    PathSegment::LineTo { end } => {
                        hash.update(b"line_to");
                        for coordinate in end {
                            hash.update(&coordinate.to_le_bytes());
                        }
                    }
                    PathSegment::ArcTo {
                        center,
                        end,
                        clockwise,
                    } => {
                        hash.update(b"arc_to");
                        for coordinate in center.iter().chain(end.iter()) {
                            hash.update(&coordinate.to_le_bytes());
                        }
                        hash.update(&[u8::from(*clockwise)]);
                    }
                }
            }
            hash.update(&(circular_holes.len() as u64).to_le_bytes());
            for hole in circular_holes {
                for coordinate in hole.center.into_iter().chain([hole.radius]) {
                    hash.update(&coordinate.to_le_bytes());
                }
            }
        }

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
        Feature::Fillet {
            body,
            faces,
            radius,
            ..
        } => {
            hash.update(&serde_json::to_vec(&(body, faces)).expect("face intent serializes"));
            hash.update(&radius.to_le_bytes());
        }
        Feature::HolePattern {
            face,
            x_direction_hint,
            origin_offset,
            sketch,
            holes,
            ..
        } => {
            hash.update(cad_sketch::SOLVER_BUILD_ID.as_bytes());
            hash.update(
                &serde_json::to_vec(&(face, sketch, holes)).expect("pattern intent serializes"),
            );
            for value in x_direction_hint.iter().chain(origin_offset.iter()) {
                hash.update(&value.to_le_bytes());
            }
        }
        Feature::ChamferFaces {
            body,
            faces,
            distance,
            ..
        } => {
            hash.update(&serde_json::to_vec(&(body, faces)).expect("face intent serializes"));
            hash.update(&distance.to_le_bytes());
        }
        Feature::Chamfer {
            face_normals,
            distance,
            ..
        } => {
            for value in face_normals.iter().flatten().chain([distance]) {
                hash.update(&value.to_le_bytes());
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
    /// Configure persistent in-memory reuse. Zero disables new retention,
    /// while builds and the optional disk cache continue normally. Lowering
    /// limits on an existing engine may temporarily leave pinned entries over
    /// budget; they are evicted after external references are dropped.
    pub fn with_cache_limits(mut self, max_entries: usize, max_logical_bytes: u64) -> Self {
        self.cache_max_entries = max_entries;
        self.cache_max_logical_bytes = max_logical_bytes;
        self.trim_cache(0, 0);
        self
    }

    pub fn cache_usage(&self) -> CacheUsage {
        let mut pinned_entries = 0;
        let mut pinned_logical_bytes = 0u64;
        for cached in self.solids.values() {
            if Arc::strong_count(&cached.result) > 1 {
                pinned_entries += 1;
                pinned_logical_bytes = pinned_logical_bytes.saturating_add(cached.logical_bytes);
            }
        }
        CacheUsage {
            max_entries: self.cache_max_entries,
            max_logical_bytes: self.cache_max_logical_bytes,
            entries: self.solids.len(),
            logical_bytes: self.cache_logical_bytes,
            pinned_entries,
            pinned_logical_bytes,
            over_budget_entries: self.solids.len().saturating_sub(self.cache_max_entries),
            over_budget_logical_bytes: self
                .cache_logical_bytes
                .saturating_sub(self.cache_max_logical_bytes),
            evictions: self.cache_evictions,
            skipped_retention: self.cache_skipped_retention,
        }
    }

    fn trim_cache(&mut self, incoming_bytes: u64, incoming_entries: usize) -> bool {
        while self
            .solids
            .len()
            .checked_add(incoming_entries)
            .is_none_or(|total| total > self.cache_max_entries)
            || self
                .cache_logical_bytes
                .checked_add(incoming_bytes)
                .is_none_or(|total| total > self.cache_max_logical_bytes)
        {
            let oldest = self
                .solids
                .iter()
                .filter(|(_, cached)| Arc::strong_count(&cached.result) == 1)
                .min_by(|(left_hash, left), (right_hash, right)| {
                    left.last_used
                        .cmp(&right.last_used)
                        .then_with(|| left_hash.cmp(right_hash))
                })
                .map(|(hash, _)| hash.clone());
            let Some(oldest) = oldest else {
                return false;
            };
            let removed = self.solids.remove(&oldest).expect("selected cached solid");
            self.cache_logical_bytes -= removed.logical_bytes;
            self.cache_evictions += 1;
        }
        true
    }

    fn retain_solid(&mut self, hash: String, result: &Arc<OperationResult>) -> bool {
        if self.cache_max_entries == 0 || self.cache_max_logical_bytes == 0 {
            self.cache_skipped_retention += 1;
            return false;
        }
        let Some(bytes) = logical_result_bytes(result) else {
            self.cache_skipped_retention += 1;
            return false;
        };
        if bytes > self.cache_max_logical_bytes || !self.trim_cache(bytes, 1) {
            self.cache_skipped_retention += 1;
            return false;
        }
        self.cache_clock = self.cache_clock.saturating_add(1);
        self.cache_logical_bytes += bytes;
        self.solids.insert(
            hash,
            CachedSolid {
                result: Arc::clone(result),
                logical_bytes: bytes,
                last_used: self.cache_clock,
            },
        );
        true
    }

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
        // A caller may have dropped prior BuildResults since the last build.
        // Revisit deferred evictions before any cache lookup.
        self.trim_cache(0, 0);
        if !matches!(recipe.schema_version, 1 | 2) {
            return Err(BuildError::SchemaVersion(recipe.schema_version));
        }
        assembly::resource_limits(recipe)?;
        property_assertions::preflight(recipe)?;
        exchange_requirements::preflight(recipe)?;
        section_views::preflight(recipe)?;
        drawing_sheets::preflight(recipe)?;
        datum_dimensions::preflight(recipe)?;
        tolerance_stacks::preflight(recipe)?;
        clearance_checks::preflight(recipe)?;
        rotation_groups::preflight(recipe)?;
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
        mechanisms::preflight(recipe)?;
        motion_studies::preflight(recipe)?;
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
                Feature::Sketch {
                    id,
                    sketch,
                    loops,
                    curve_outline,
                    circular_holes,
                    ..
                } => sketch_profiles::validate(
                    id,
                    sketch,
                    loops,
                    curve_outline.as_ref(),
                    circular_holes,
                )?,
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
                Feature::Path {
                    id,
                    path,
                    circular_holes,
                    ..
                } => {
                    cad_kernel::validate_path(path, circular_holes, tolerance).map_err(|error| {
                        BuildError::PolygonProfile {
                            feature: id.clone(),
                            message: error.to_string(),
                        }
                    })?
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
                Feature::Fillet {
                    id, faces, radius, ..
                } => {
                    dimension(id, "radius", *radius)?;
                    for face in faces {
                        identifier("fillet anchor", &face.from_feature)?;
                        identifier("fillet role", &face.role)?;
                    }
                }
                Feature::HolePattern {
                    id,
                    face,
                    x_direction_hint,
                    origin_offset,
                    sketch,
                    holes,
                    ..
                } => {
                    if recipe.schema_version != 2 {
                        return Err(BuildError::HolePatternSchema);
                    }
                    identifier("hole pattern anchor", &face.from_feature)?;
                    identifier("hole pattern face role", &face.role)?;
                    if x_direction_hint
                        .iter()
                        .chain(origin_offset.iter())
                        .any(|x| !x.is_finite())
                    {
                        return Err(BuildError::Sketch {
                            feature: id.clone(),
                            message: "hole pattern face frame must be finite".into(),
                        });
                    }
                    if holes.is_empty() || holes.len() > MAX_BLIND_PATTERN_HOLES {
                        return Err(BuildError::ResourceLimit {
                            resource: "blind holes per pattern",
                            limit: MAX_BLIND_PATTERN_HOLES,
                        });
                    }
                    cad_sketch::validate(sketch).map_err(|e| BuildError::Sketch {
                        feature: id.clone(),
                        message: e.to_string(),
                    })?;
                    let points: BTreeSet<_> = sketch
                        .points
                        .iter()
                        .map(|point| point.id.as_str())
                        .collect();
                    let mut names = BTreeSet::new();
                    for hole in holes {
                        if hole.id.is_empty()
                            || hole.id.len() > 64
                            || !hole
                                .id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                            || !names.insert(&hole.id)
                        {
                            return Err(BuildError::Sketch {
                                feature: id.clone(),
                                message: format!(
                                    "invalid or duplicate blind hole ID '{}'",
                                    hole.id
                                ),
                            });
                        }
                        if hole.center == hole.rim
                            || !points.contains(hole.center.as_str())
                            || !points.contains(hole.rim.as_str())
                        {
                            return Err(BuildError::Sketch {
                                feature: id.clone(),
                                message: format!(
                                    "blind hole '{}' needs distinct defined center and rim points",
                                    hole.id
                                ),
                            });
                        }
                        dimension(id, "blind hole depth", hole.depth)?;
                    }
                }
                Feature::ChamferFaces {
                    id,
                    faces,
                    distance,
                    ..
                } => {
                    dimension(id, "distance", *distance)?;
                    for face in faces {
                        identifier("chamfer anchor", &face.from_feature)?;
                        identifier("chamfer role", &face.role)?;
                    }
                }
                Feature::Chamfer {
                    id,
                    face_normals,
                    distance,
                    ..
                } => {
                    dimension(id, "distance", *distance)?;
                    for normal in face_normals {
                        let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                        if normal.iter().any(|v| !v.is_finite())
                            || (length - 1.).abs() > 128. * f64::EPSILON
                        {
                            return Err(BuildError::Kernel {
                                feature: id.clone(),
                                message: "chamfer face normals must be finite unit vectors".into(),
                            });
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
        let mechanism_evaluation = mechanisms::evaluate(recipe, tolerance)?;
        let recipe = &mechanism_evaluation.recipe;
        let scope = demand::scope(recipe, requested, &indices)?;
        let wanted: BTreeSet<_> = scope
            .evaluated_features
            .iter()
            .map(String::as_str)
            .collect();
        sketch_profiles::budget(recipe, &wanted)?;
        let mut sketch_reports = BTreeMap::new();
        let mut solved_profiles = BTreeMap::new();
        let mut hashes = BTreeMap::new();
        let mut evaluated: HashMap<String, Arc<OperationResult>> = HashMap::new();
        // Results skipped by the persistent cache still share geometry among
        // equal hashes within this one build. Evaluated features already own
        // these Arcs; this map only provides hash lookup.
        let mut transient_results: HashMap<String, Arc<OperationResult>> = HashMap::new();
        let mut stats = BuildStats::default();
        let mut selection_graph =
            selection::SelectionGraph::new(recipe, order.clone(), HashMap::new());
        let mut assertion_work_remaining = MAX_FACE_ASSERTION_WORK;
        for &index in &order {
            let feature = &recipe.features[index];
            if !wanted.contains(feature.id()) {
                continue;
            }
            let mut hash = hash_feature(recipe, tolerance, feature, &hashes);
            // Logical ancestry is always resolved before either cache lookup.
            // Equal geometric hashes do not establish a semantic face reference.
            let fillet_faces = if let Feature::Fillet {
                id, body, faces, ..
            } = feature
            {
                let mut selected = [cad_kernel::FaceId(0); 2];
                let mut resolutions = Vec::new();
                for (i, face) in faces.iter().enumerate() {
                    let resolution = selection_graph.resolve_faces_with_remaining(
                        &FaceSelector {
                            anchor_feature: face.from_feature.clone(),
                            role: face.role.clone(),
                            target_feature: body.clone(),
                            cardinality: FaceCardinality::Exactly { count: 1 },
                        },
                        &mut assertion_work_remaining,
                    );
                    if resolution.status != FaceResolutionStatus::Resolved
                        || resolution.witnesses.len() != 1
                    {
                        return Err(BuildError::Kernel {
                            feature: id.clone(),
                            message: format!(
                                "fillet face {i} selection {:?}: {}",
                                resolution.status, resolution.diagnostic
                            ),
                        });
                    }
                    selected[i] = resolution.witnesses[0].target_face;
                    resolutions.push(resolution);
                }
                let mut binding = blake3::Hasher::new();
                binding.update(hash.as_bytes());
                binding
                    .update(&serde_json::to_vec(&resolutions).expect("face witnesses serialize"));
                hash = binding.finalize().to_hex().to_string();
                Some(selected)
            } else {
                None
            };
            let chamfer_normals = if let Feature::ChamferFaces {
                id, body, faces, ..
            } = feature
            {
                let mut normals = [[0.; 3]; 2];
                let mut resolutions = Vec::new();
                for (i, face) in faces.iter().enumerate() {
                    let resolution = selection_graph.resolve_faces_with_remaining(
                        &FaceSelector {
                            anchor_feature: face.from_feature.clone(),
                            role: face.role.clone(),
                            target_feature: body.clone(),
                            cardinality: FaceCardinality::Exactly { count: 1 },
                        },
                        &mut assertion_work_remaining,
                    );
                    if resolution.status != FaceResolutionStatus::Resolved
                        || resolution.witnesses.len() != 1
                    {
                        return Err(BuildError::Kernel {
                            feature: id.clone(),
                            message: format!(
                                "chamfer face {} selection {:?}: {}",
                                i, resolution.status, resolution.diagnostic
                            ),
                        });
                    }
                    let model = &evaluated[body].model;
                    let plane = model.faces[resolution.witnesses[0].target_face.0]
                        .try_plane()
                        .ok_or_else(|| BuildError::Kernel {
                            feature: id.clone(),
                            message: format!("chamfer face {i} is not planar"),
                        })?;
                    normals[i] = plane.normal;
                    resolutions.push(resolution);
                }
                let mut binding = blake3::Hasher::new();
                binding.update(hash.as_bytes());
                binding.update(
                    &serde_json::to_vec(&(&normals, &resolutions))
                        .expect("finite face witnesses serialize"),
                );
                hash = binding.finalize().to_hex().to_string();
                Some(normals)
            } else {
                None
            };
            let mut pattern_cap = None;
            let mut pattern_holes = None;
            if let Feature::HolePattern {
                id,
                body,
                face,
                x_direction_hint,
                origin_offset,
                sketch,
                holes,
                ..
            } = feature
            {
                let resolution = selection_graph.resolve_faces_with_remaining(
                    &FaceSelector {
                        anchor_feature: face.from_feature.clone(),
                        role: face.role.clone(),
                        target_feature: body.clone(),
                        cardinality: FaceCardinality::Exactly { count: 1 },
                    },
                    &mut assertion_work_remaining,
                );
                if resolution.status != FaceResolutionStatus::Resolved
                    || resolution.witnesses.len() != 1
                {
                    return Err(BuildError::Kernel {
                        feature: id.clone(),
                        message: format!(
                            "hole pattern cap selection {:?}: {}",
                            resolution.status, resolution.diagnostic
                        ),
                    });
                }
                let cap = resolution.witnesses[0].target_face;
                let frame = evaluated[body]
                    .model
                    .face_frame(
                        cap,
                        *x_direction_hint,
                        [origin_offset[0], origin_offset[1], 0.],
                        tolerance,
                    )
                    .map_err(|e| BuildError::Kernel {
                        feature: id.clone(),
                        message: e.to_string(),
                    })?;
                let mut binding = blake3::Hasher::new();
                binding.update(hash.as_bytes());
                binding.update(
                    &serde_json::to_vec(&(&resolution, &frame)).expect("face frame serializes"),
                );
                hash = binding.finalize().to_hex().to_string();
                let report = cad_sketch::solve_refined(sketch, tolerance.linear).map_err(|e| {
                    BuildError::Sketch {
                        feature: id.clone(),
                        message: e.to_string(),
                    }
                })?;
                if !report.is_fully_constrained() {
                    return Err(BuildError::SketchSolveFailed {
                        feature: id.clone(),
                        report: Box::new(report),
                        content_hash: hash,
                    });
                }
                let solved = holes
                    .iter()
                    .map(|hole| {
                        let center = report.points[&hole.center];
                        let rim = report.points[&hole.rim];
                        cad_kernel::CapBlindHole {
                            id: hole.id.clone(),
                            center,
                            radius: (rim[0] - center[0]).hypot(rim[1] - center[1]),
                            depth: hole.depth,
                        }
                    })
                    .collect::<Vec<_>>();
                sketch_reports.insert(id.clone(), report);
                pattern_cap = Some(cap);
                pattern_holes = Some(solved);
            }
            if let Feature::Sketch {
                id,
                sketch,
                loops,
                curve_outline,
                circular_holes,
                ..
            } = feature
            {
                let (report, rings) = sketch_profiles::solve(
                    id,
                    sketch,
                    loops,
                    curve_outline.as_ref(),
                    circular_holes,
                    tolerance,
                    &hash,
                )?;
                sketch_reports.insert(id.clone(), report);
                solved_profiles.insert(id.clone(), rings);
            }
            if feature.is_solid() {
                self.cache_clock = self.cache_clock.saturating_add(1);
                let result = if let Some(cached) = self.solids.get_mut(&hash) {
                    stats.cache_hits += 1;
                    cached.last_used = self.cache_clock;
                    Arc::clone(&cached.result)
                } else if let Some(cached) = transient_results.get(&hash) {
                    stats.cache_hits += 1;
                    Arc::clone(cached)
                } else if let Some(cached) = self.disk_read(&hash, tolerance) {
                    stats.cache_hits += 1;
                    stats.disk_cache_hits += 1;
                    let result = Arc::new(cached);
                    if !self.retain_solid(hash.clone(), &result) {
                        transient_results.insert(hash.clone(), Arc::clone(&result));
                    }
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
                            Feature::Sketch { id, .. } => match &solved_profiles[id] {
                                sketch_profiles::SolvedProfile::Polygon { rings, circles } => {
                                    cad_kernel::extrude_polygon_with_circular_holes(
                                        &rings[0],
                                        &rings[1..],
                                        circles,
                                        *height,
                                        tolerance,
                                    )
                                }
                                sketch_profiles::SolvedProfile::Path { path, circles } => {
                                    cad_kernel::extrude_path(path, circles, *height, tolerance)
                                }
                            },
                            Feature::Path {
                                path,
                                circular_holes,
                                ..
                            } => cad_kernel::extrude_path(path, circular_holes, *height, tolerance),
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
                        Feature::Fillet { body, radius, .. } => cad_kernel::fillet_longitudinal(
                            &evaluated[body].model,
                            fillet_faces.expect("semantic faces resolved before cache"),
                            *radius,
                            tolerance,
                        ),
                        Feature::HolePattern {
                            body,
                            x_direction_hint,
                            origin_offset,
                            ..
                        } => cad_kernel::drill_cap_pattern(
                            &evaluated[body].model,
                            pattern_cap.expect("hole pattern cap resolved before cache"),
                            *x_direction_hint,
                            *origin_offset,
                            pattern_holes
                                .as_deref()
                                .expect("hole pattern solved before cache"),
                            tolerance,
                        ),
                        Feature::ChamferFaces { body, distance, .. } => cad_kernel::chamfer(
                            &evaluated[body].model,
                            chamfer_normals.expect("semantic faces resolved before cache"),
                            *distance,
                            tolerance,
                        ),
                        Feature::Chamfer {
                            body,
                            face_normals,
                            distance,
                            ..
                        } => cad_kernel::chamfer(
                            &evaluated[body].model,
                            *face_normals,
                            *distance,
                            tolerance,
                        ),
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
                        | Feature::Polygon { .. }
                        | Feature::Sketch { .. }
                        | Feature::Path { .. } => unreachable!(),
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
                    if !self.retain_solid(hash.clone(), &result) {
                        transient_results.insert(hash.clone(), Arc::clone(&result));
                    }
                    stats.computed_solids += 1;
                    result
                };
                selection_graph.insert(feature.id().into(), Arc::clone(&result));
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
        let mut result = BuildResult {
            sketch_reports,
            scope,
            outputs,
            assemblies: BTreeMap::new(),
            feature_hashes: hashes,
            stats,
            face_assertions: BTreeMap::new(),
            property_assertions: BTreeMap::new(),
            pending_exchange_requirements: recipe.exchange_requirements.clone(),
            section_views: BTreeMap::new(),
            drawing_sheets: BTreeMap::new(),
            tolerance_stacks: BTreeMap::new(),
            clearance_checks: BTreeMap::new(),
            datum_dimensions: BTreeMap::new(),
            rigid_rotation_groups: BTreeMap::new(),
            face_datums: BTreeMap::new(),
            motion_studies: mechanism_evaluation.motion_studies,
            mechanism_reports: mechanism_evaluation.reports,
            mechanism_bindings: mechanism_evaluation.bindings,
            mechanism_context_hash: mechanism_evaluation.context_hash,
            selection_graph,
        };
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
        let assembly_requests: Vec<_> = result
            .scope
            .requested_outputs
            .iter()
            .cloned()
            .chain(recipe.property_assertions.iter().map(|a| a.output.clone()))
            .chain(recipe.section_views.iter().map(|a| a.output.clone()))
            .chain(recipe.clearance_checks.iter().map(|a| a.output.clone()))
            .chain(recipe.datum_dimensions.iter().map(|a| a.output.clone()))
            .chain(
                recipe
                    .rigid_rotation_groups
                    .iter()
                    .map(|a| a.output.clone()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let assertion_assemblies = assembly::evaluate(
            recipe,
            &result.feature_hashes,
            &evaluated,
            tolerance,
            &result.face_datums,
            &assembly_requests,
        )?;
        result.property_assertions = property_assertions::evaluate(
            recipe,
            &result.feature_hashes,
            &evaluated,
            &assertion_assemblies,
            tolerance,
        )?;
        result.section_views = section_views::evaluate(
            recipe,
            &result,
            &evaluated,
            &assertion_assemblies,
            tolerance,
            &mut assertion_work_remaining,
        )?;
        result.rigid_rotation_groups =
            rotation_groups::evaluate(recipe, &assertion_assemblies, tolerance)?;
        result.clearance_checks = clearance_checks::evaluate(
            recipe,
            &assertion_assemblies,
            &result.rigid_rotation_groups,
            tolerance,
        )?;
        result.datum_dimensions = datum_dimensions::evaluate(
            recipe,
            &assertion_assemblies,
            &result.face_datums,
            tolerance,
        )?;
        result.tolerance_stacks =
            tolerance_stacks::evaluate(recipe, &result.section_views, &result.datum_dimensions)?;
        result.drawing_sheets =
            drawing_sheets::evaluate(recipe, &result.section_views, &result.datum_dimensions)?;
        result.assemblies = assertion_assemblies
            .into_iter()
            .filter(|(name, _)| result.scope.requested_outputs.binary_search(name).is_ok())
            .collect();
        Ok(result)
    }
}

#[cfg(test)]
mod hole_pattern_tests {
    use super::*;

    #[test]
    fn same_geometry_unrelated_face_anchor_refuses_before_warm_cache_lookup() {
        let recipe = |anchor: &str| -> Recipe {
            serde_json::from_value(serde_json::json!({
                "schema_version":2,"units":"mm",
                "features":[
                    {"op":"rectangle","id":"profile","width":40,"depth":30},
                    {"op":"extrude","id":"a","profile":"profile","height":12},
                    {"op":"extrude","id":"b","profile":"profile","height":12},
                    {"op":"hole_pattern","id":"drilled","body":"b",
                     "face":{"from_feature":anchor,"role":"end_cap"},
                     "x_direction_hint":[1,0,0],"origin_offset":[0,0],
                     "sketch":{"points":[{"id":"c","position":[0,0]},{"id":"r","position":[2,0]}],
                         "constraints":[{"id":"fix_c","kind":"fixed","point":"c","position":[0,0]},
                                        {"id":"fix_r","kind":"fixed","point":"r","position":[2,0]}]},
                     "pattern_holes":[{"id":"pilot","center":"c","rim":"r","depth":5}]}
                ],"outputs":[{"name":"drilled","feature":"drilled"}]
            })).unwrap()
        };
        let mut engine = BuildEngine::default();
        let valid = engine.build(&recipe("b")).unwrap();
        assert!(valid.sketch_reports["drilled"].is_fully_constrained());
        let error = engine.build(&recipe("a")).unwrap_err().to_string();
        assert!(error.contains("hole pattern cap selection"), "{error}");
        let mut wrong_body = serde_json::to_value(recipe("b")).unwrap();
        wrong_body["features"][3]["body"] = serde_json::json!("profile");
        let wrong_body: Recipe = serde_json::from_value(wrong_body).unwrap();
        assert!(matches!(
            engine.build(&wrong_body),
            Err(BuildError::InputType { .. })
        ));
        let mut old_schema = recipe("b");
        old_schema.schema_version = 1;
        assert!(matches!(
            engine.build(&old_schema),
            Err(BuildError::HolePatternSchema)
        ));
    }
}
