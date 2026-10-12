//! Discoverable contracts for the JSON recipe IR, not Starlark call signatures.
//! This catalog describes bounded implementations; every request still requires
//! structural, numeric and native-geometry validation by the executor.
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;
mod engineering;

pub const CAPABILITY_SCHEMA_VERSION: u32 = 2;

/// Identity of the discovery implementation and the implementations it describes.
/// This is not a hash of the serialized catalog document or a geometry cache key.
pub fn catalog_build_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = blake3::Hasher::new();
        for bytes in [
            b"headless-cad/capability-catalog-build/v1".as_slice(),
            include_bytes!("capabilities.rs").as_slice(),
            include_bytes!("capabilities/engineering.rs").as_slice(),
            cad_assembly::assembly_build_id().as_bytes(),
            cad_kernel::KERNEL_BUILD_ID.as_bytes(),
            crate::executor_build_id().as_bytes(),
            crate::solver_build_id().as_bytes(),
        ] {
            hash.update(&(bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        hash.finalize().to_hex().to_string()
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgumentType {
    Map {
        values: Box<ArgumentType>,
    },
    Nullable {
        value: Box<ArgumentType>,
    },
    String,
    Number,
    Integer,
    Boolean,
    Array {
        items: Box<ArgumentType>,
        #[serde(skip_serializing_if = "Option::is_none")]
        length: Option<usize>,
    },
    Object {
        type_id: &'static str,
    },
    Reference {
        target: &'static str,
    },
    Enum {
        values: Vec<&'static str>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArgumentSpec {
    pub name: &'static str,
    pub value_type: ArgumentType,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_schema_versions: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<&'static str>,
    pub constraints: Vec<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectVariant {
    pub tag_value: &'static str,
    pub arguments: Vec<ArgumentSpec>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectSpec {
    pub id: &'static str,
    pub arguments: Vec<ArgumentSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<&'static str>,
    pub variants: Vec<ObjectVariant>,
    pub obligations: Vec<&'static str>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureOutputKind {
    Profile,
    Solid,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationCapability {
    pub id: &'static str,
    pub json_op: &'static str,
    pub op_versions: Vec<u32>,
    pub output: FeatureOutputKind,
    pub arguments: Vec<ArgumentSpec>,
    pub domain_ids: Vec<&'static str>,
    pub exclusions: Vec<&'static str>,
    pub validation_obligations: Vec<&'static str>,
    pub examples: Vec<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DomainCapability {
    pub id: &'static str,
    pub operation_ids: Vec<&'static str>,
    pub requirements: Vec<&'static str>,
    pub exclusions: Vec<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImplementationIdentities {
    pub assembly_build_id: &'static str,
    pub catalog_build_id: &'static str,
    pub kernel_version: &'static str,
    pub kernel_build_id: &'static str,
    pub executor_build_id: &'static str,
    pub sketch_solver_build_id: &'static str,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SketchPolicy {
    pub report_schema_version: u32,
    pub angular_tolerance_radians: f64,
    pub max_condition_bound: f64,
    pub acceptance: &'static str,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CapabilityCatalog {
    pub schema_version: u32,
    pub interface: &'static str,
    pub recipe_schema_versions: Vec<u32>,
    pub latest_recipe_schema_version: u32,
    pub units: &'static str,
    pub default_linear_tolerance_mm: f64,
    pub identities: ImplementationIdentities,
    pub operations: Vec<OperationCapability>,
    pub object_types: Vec<ObjectSpec>,
    pub domains: Vec<DomainCapability>,
    pub resource_limits: BTreeMap<&'static str, usize>,
    pub sketch_policy: SketchPolicy,
    pub validation_obligations: Vec<&'static str>,
    pub unsupported_operation_ids: Vec<&'static str>,
}
fn array(items: ArgumentType, length: Option<usize>) -> ArgumentType {
    ArgumentType::Array {
        items: Box::new(items),
        length,
    }
}
fn object(type_id: &'static str) -> ArgumentType {
    ArgumentType::Object { type_id }
}
fn reference(target: &'static str) -> ArgumentType {
    ArgumentType::Reference { target }
}
fn numbers(n: usize) -> ArgumentType {
    array(ArgumentType::Number, Some(n))
}
fn arg(name: &'static str, value_type: ArgumentType) -> ArgumentSpec {
    ArgumentSpec {
        name,
        value_type,
        required: true,
        recipe_schema_versions: vec![],
        default: None,
        units: None,
        constraints: vec![],
    }
}
fn optional(mut a: ArgumentSpec, default: Value) -> ArgumentSpec {
    a.required = false;
    a.default = Some(default);
    a
}
fn dimension(name: &'static str) -> ArgumentSpec {
    let mut a = arg(name, ArgumentType::Number);
    a.units = Some("mm");
    a.constraints
        .push("finite; greater than modeling tolerance; arithmetic range validated");
    a
}
fn operation(
    id: &'static str,
    op: &'static str,
    output: FeatureOutputKind,
    args: Vec<ArgumentSpec>,
    domains: Vec<&'static str>,
    exclusions: Vec<&'static str>,
    examples: Vec<&'static str>,
) -> OperationCapability {
    let mut arguments = vec![
        arg("op", ArgumentType::Enum { values: vec![op] }),
        arg("id", ArgumentType::String),
        optional(arg("op_version", ArgumentType::Integer), json!(1)),
    ];
    arguments.extend(args);
    OperationCapability{id,json_op:op,op_versions:vec![1],output,arguments,domain_ids:domains,exclusions,validation_obligations:vec!["unique feature ID; typed dependency references; acyclic graph", "structural validation precedes demanded geometry; successful native validation required"],examples}
}
fn object_spec(id: &'static str, args: Vec<ArgumentSpec>) -> ObjectSpec {
    ObjectSpec {
        id,
        arguments: args,
        discriminator: None,
        variants: vec![],
        obligations: vec!["unknown and duplicate fields rejected"],
    }
}
/// Deterministic, versioned discovery of currently implemented JSON IR features.
/// This is a capability contract, not a replacement for recipe validation.
pub fn capability_catalog() -> CapabilityCatalog {
    use ArgumentType as T;
    use FeatureOutputKind::{Profile, Solid};
    let circles = || {
        optional(
            arg("circular_holes", array(object("circle_hole"), None)),
            json!([]),
        )
    };
    let pair = || array(reference("sketch_point"), Some(2));
    let mut loops = arg("loops", array(array(reference("sketch_point"), None), None));
    loops.required = false;
    loops.constraints = vec![
        "required when curve_outline is absent; must be empty when curve_outline is present",
        "first ring CCW; remaining rings CW; loops implicitly closed without duplicate endpoint",
    ];
    let mut outline = arg("curve_outline", object("sketch_path"));
    outline.required = false;
    let operations=vec![
        operation("feature.circle","circle",Profile,vec![dimension("radius"),optional(arg("inner_radius",T::Number),json!(0))],vec!["profile.circle"],vec!["inner_radius must be zero or positive and strictly below radius with resolved wall thickness"],vec!["examples/bearing_cartridge.star"]),
        operation("feature.rectangle","rectangle",Profile,vec![dimension("width"),dimension("depth")],vec!["profile.polygon"],vec![],vec!["examples/plate.star"]),
        operation("feature.polygon","polygon",Profile,vec![arg("points",array(numbers(2),None)),optional(arg("holes",array(array(numbers(2),None),None)),json!([])),circles()],vec!["profile.polygon"],vec!["self-intersections, crossing/touching/nested holes, automatic repair"],vec!["examples/serpentine_manifold.star"]),
        operation("feature.path","path",Profile,vec![arg("start",numbers(2)),arg("segments",array(object("path_segment"),None)),circles()],vec!["profile.path"],vec!["polygonal inner rings; arc sweep magnitude greater than pi; automatic repair"],vec!["examples/split_bearing_block.star"]),
        operation("feature.sketch","sketch",Profile,vec![arg("sketch",object("sketch")),loops,outline,optional(arg("circular_holes",array(object("sketch_circle_hole"),None)),json!([]))],vec!["profile.sketch","profile.polygon","profile.path"],vec!["underconstrained or unresolved solutions cannot produce a profile; no global uniqueness or inconsistency proof", "curve_outline cannot coexist with polygon loops; no general circle/circle or curve tangency solver"],vec!["examples/constrained_capsule_link.star","examples/tangent_bore_fixture.star"]),
        operation("feature.chamfer_faces","chamfer_faces",Solid,vec![arg("body",reference("solid_feature")),arg("faces",array(object("chamfer_face"),Some(2))),dimension("distance")],vec!["solid.chamfer"],vec!["ambiguous/deleted face lineage, nonplanar selected faces, nonadjacent faces; all native chamfer limits apply"],vec!["examples/chamfered_guide_block.star"]),
        operation("feature.chamfer","chamfer",Solid,vec![arg("body",reference("solid_feature")),arg("face_normals",ArgumentType::Array{items:Box::new(numbers(3)),length:Some(2)}),dimension("distance")],vec!["solid.chamfer"],vec!["curved or nonconvex bodies; unequal setbacks; vertex removal or topology events; fillets"],vec!["examples/chamfered_guide_block.star"]),
        operation("feature.extrude","extrude",Solid,vec![arg("profile",reference("profile_feature")),dimension("height")],vec!["solid.extrusion"],vec!["tapered extrusion, arbitrary sweep, mesh extrusion"],vec!["examples/rounded_clevis.star"]),
        operation("feature.revolve","revolve",Solid,vec![arg("profile",reference("polygon_feature_without_holes"))],vec!["solid.revolution"],vec!["partial turns, curved meridians, holed profile, sketch/path/circle input"],vec!["examples/centering_fixture.star","examples/tapered_nozzle.star"]),
        operation("feature.translate","translate",Solid,vec![arg("body",reference("solid_feature")),arg("offset",numbers(3))],vec!["solid.placement"],vec!["coordinate changes outside numeric certification"],vec!["examples/bracket.star"]),
        operation("feature.transform","transform",Solid,vec![arg("body",reference("solid_feature")),arg("offset",numbers(3)),arg("rotation",numbers(4))],vec!["solid.placement"],vec!["scaling, shear, quaternion normalization, general rotation of unsupported nonconvex/multisolid planar shells"],vec!["examples/tilted_jig.star"]),
        operation("feature.union","union",Solid,vec![arg("left",reference("solid_feature")),arg("right",reference("solid_feature"))],vec!["boolean.orthogonal","boolean.circular_cylinders"],vec!["general curved/planar union; disconnected curved union"],vec!["examples/twin_lobe_fixture.star"]),
        operation("feature.cut","cut",Solid,vec![arg("left",reference("solid_feature")),arg("right",reference("solid_feature"))],vec!["boolean.orthogonal","boolean.circular_cylinders","boolean.through_bore","boolean.blind_bore","boolean.stepped_bore","boolean.round_stock","boolean.countersink"],vec!["general curved Boolean; crossing outer profile or noncoaxial holes; enclosed cylindrical cavities"],vec!["examples/machined_bearing_support.star","examples/blind_bore_pallet.star","examples/counterbored_fixture.star","examples/round_bearing_cartridge.star","examples/countersunk_bearing_cover.star"]),
        operation("feature.intersect","intersect",Solid,vec![arg("left",reference("solid_feature")),arg("right",reference("solid_feature"))],vec!["boolean.orthogonal","boolean.convex_planar_intersection","boolean.circular_cylinders"],vec!["general concave planar or curved intersection"],vec!["examples/clipped_tools.star","examples/twin_lobe_fixture.star"]),
    ];
    let mut path_segment = object_spec("path_segment", vec![]);
    path_segment.discriminator = Some("kind");
    path_segment.variants = vec![
        ObjectVariant {
            tag_value: "line_to",
            arguments: vec![arg("end", numbers(2))],
        },
        ObjectVariant {
            tag_value: "arc_to",
            arguments: vec![
                arg("center", numbers(2)),
                arg("end", numbers(2)),
                optional(arg("clockwise", T::Boolean), json!(false)),
            ],
        },
    ];
    let mut sketch_segment = object_spec("sketch_path_segment", vec![]);
    sketch_segment.discriminator = Some("kind");
    sketch_segment.variants = vec![
        ObjectVariant {
            tag_value: "line_to",
            arguments: vec![arg("end", reference("sketch_point"))],
        },
        ObjectVariant {
            tag_value: "arc_to",
            arguments: vec![
                arg("center", reference("sketch_point")),
                arg("end", reference("sketch_point")),
                optional(arg("clockwise", T::Boolean), json!(false)),
            ],
        },
    ];
    let mut constraint = object_spec("sketch_constraint", vec![arg("id", T::String)]);
    constraint.discriminator = Some("kind");
    constraint.variants.push(ObjectVariant {
        tag_value: "fixed",
        arguments: vec![
            arg("point", reference("sketch_point")),
            arg("position", numbers(2)),
        ],
    });
    for kind in ["coincident", "horizontal", "vertical", "distance"] {
        let mut arguments = vec![
            arg("a", reference("sketch_point")),
            arg("b", reference("sketch_point")),
        ];
        if kind == "distance" {
            let mut value = arg("value", T::Number);
            value.units = Some("mm");
            value
                .constraints
                .push("finite and positive; solver numeric domain validated");
            arguments.push(value);
        }
        constraint.variants.push(ObjectVariant {
            tag_value: kind,
            arguments,
        });
    }
    for kind in ["equal_length", "parallel", "perpendicular", "angle"] {
        let mut arguments = vec![arg("a", pair()), arg("b", pair())];
        if kind == "angle" {
            let mut degrees = arg("degrees", T::Number);
            degrees.units = Some("degrees");
            degrees
                .constraints
                .push("finite and in (-180,180]; directed from segment a to b");
            arguments.push(degrees);
        }
        constraint.variants.push(ObjectVariant {
            tag_value: kind,
            arguments,
        });
    }
    constraint.variants.push(ObjectVariant {
        tag_value: "circle_line_tangent",
        arguments: vec![
            arg("circle", pair()),
            arg("line", pair()),
            arg(
                "side",
                T::Enum {
                    values: vec!["left", "right"],
                },
            ),
        ],
    });
    constraint.obligations.extend(["circle pair is [center,rim]; line pair is directed [start,end]; tangency is to infinite support, not guaranteed finite-segment contact", "shared point Jacobian terms accumulate; nonzero resolved segment/radius directions required", "parallel/perpendicular are unoriented modulo pi; directed angle branch and wrap-cut uncertainty are explicit"]);
    let mut object_types = vec![
        object_spec(
            "circle_hole",
            vec![arg("center", numbers(2)), dimension("radius")],
        ),
        path_segment,
        object_spec(
            "sketch_point",
            vec![arg("id", T::String), arg("position", numbers(2))],
        ),
        constraint,
        object_spec(
            "sketch",
            vec![
                arg("points", array(object("sketch_point"), None)),
                arg("constraints", array(object("sketch_constraint"), None)),
            ],
        ),
        object_spec(
            "sketch_circle_hole",
            vec![
                arg("center", reference("sketch_point")),
                arg("rim", reference("sketch_point")),
            ],
        ),
        object_spec(
            "sketch_path",
            vec![
                arg("start", reference("sketch_point")),
                arg("segments", array(object("sketch_path_segment"), None)),
            ],
        ),
        sketch_segment,
    ];
    object_types.extend(envelope_objects(&operations));
    object_types.extend(engineering::objects());
    for spec in &mut object_types {
        match spec.id {
            "motion_study_definition" => spec.obligations.extend(["example: examples/motion_four_bar.star", "artifact-safe IDs: 1..128 ASCII alphanumeric/underscore/hyphen, unique ignoring case", "global selected-output obligation, no part geometry evaluation per motion sample"]),
            "motion_study" => spec.obligations.extend(["targets contain every drive exactly once and no unknown IDs; first target equals source drive coordinates", "2..128 requested targets, finite coordinates and positive resolved step bounds; bounded deterministic subdivision", "accepted samples do not certify continuous motion, collisions, loads or dynamics"]),
            "mechanism_definition" => spec.obligations.extend(["example: examples/solved_four_bar.star", "all declared mechanisms solve globally before solids, including selected and display-only builds; immutable rigid geometry"]),
            "mechanism_binding" => spec.obligations.extend(["exact local occurrence ID in named assembly definition; leaf or rigid child", "one binding per occurrence; cannot coexist with datum attachment; placement = mechanism frame * solved link frame * existing transform"]),
            "planar_mechanism" => spec.obligations.extend(["1..30 pins, 1..64 links, at most 16 drives and 32 branch witnesses; generated solver constraints bounded", "fully constrained, converged, well-conditioned solution and branch witnesses required; initial coordinates are guesses, not authoritative placements", "no general 3D, collision, dynamics or global branch uniqueness claim"]),
            _ => {}
        }
        for argument in &mut spec.arguments {
            match (spec.id, argument.name) {
                ("mechanism_pin", "initial" | "ground") | ("mechanism_link", "length") => {
                    argument.units = Some("mm")
                }
                ("motion_study", "max_drive_step_radians" | "targets") => {
                    argument.units = Some("rad")
                }
                ("motion_study", "max_pin_step_mm") => argument.units = Some("mm"),
                ("mechanism_drive", "angle_radians") => {
                    argument.units = Some("rad");
                    argument.constraints.push("finite and in (-pi, pi]");
                }
                ("mechanism_branch", "side") => argument
                    .constraints
                    .push("left or right: signed orientation of the three ordered pins"),
                _ => {}
            }
        }
    }

    let domain = |id, operation_ids, requirements, exclusions| DomainCapability {
        id,
        operation_ids,
        requirements,
        exclusions,
    };
    let domains=vec![
        domain("assembly.motion_studies",vec![],vec!["source-owned sequences of complete angle-drive coordinates over an unchanged planar pin/bar graph", "warm-started deterministic subdivision bounded by angular and pin displacement, attempt count and recursion depth", "all studies are global obligations; accepted sample evidence and structured failure prefixes retained; static assembly pose remains unchanged", "sampled local continuation only; arbitrary mechanism plane frame remains source-owned"],vec!["proof of continuous collision-free motion or branch safety between samples; automatic singularity crossing; time/velocity/dynamics; interpolation as exact geometry"]),
        domain("assembly.planar_mechanisms",vec![],vec!["bounded planar pin/bar graphs with fixed lengths, grounded pins, directed angle drives and explicit branch-side witnesses", "global deterministic numerical closure before geometry evaluation; all accepted links become rigid local XY frames", "separate arbitrary rigid mechanism frame; binding composes link frame then existing occurrence offset, for leaf parts or rigid children", "solver reports, rank/conditioning, branch witnesses and source context identity retained; selected builds cannot bypass failures"],vec!["general 3D or flexible-body solving; dynamics and collision certification; global uniqueness or automatic branch continuation"]),
        domain("assembly.prescribed_joints",vec![],vec!["schema-2 datum attachments optionally prescribe revolute or prismatic motion; finite position and inclusive min/max limits", "motion is target datum times existing offset times joint motion about/along local +Z times inverse source datum", "part or rigid-module attachments form an acyclic dependency graph; immutable shared geometry; placement-only motion changes assembly identity", "structural joint limits are global even for selected body builds"],vec!["nonlinear or closed-loop solving, free joints, dynamics, collision/clearance certification"]),
        domain("query.planar_section",vec![],vec!["exact directed line/arc loops with native face provenance, area and centroid", "general planar solids; axis-transverse circular, annular, line/arc prism, blind/stepped/countersunk and revolution solids", "body and assembly occurrence frames; assembly areas sum occurrences, not union; declared views are global obligations"],vec!["oblique curved intersections; tangent/coplanar topology-event planes; implicit perturbation; manufacturing PMI"]),
        domain("profile.circle",vec!["feature.circle"],vec!["disk or concentric annulus, finite radii with resolved positive material thickness"],vec!["arbitrary circular-edge graphs"]),
        domain("profile.polygon",vec!["feature.rectangle","feature.polygon","feature.sketch"],vec!["simple CCW outer ring; simple CW polygon holes; circular holes strictly interior and disjoint", "rings implicitly closed without repeated endpoint; bounded predicate/integration arithmetic"],vec!["overlap, nesting, contacts, repair"]),
        domain("profile.path",vec!["feature.path","feature.sketch"],vec!["simple CCW line/circular-arc outer loop, including concavity and clockwise inward arcs", "2..=256 segments; explicit exact closure; resolved circle support; each nonzero arc magnitude <= pi; disjoint circular holes", "line-only paths lower to native polygon extrusion"],vec!["polygon holes in curved paths; splines; major arcs as a single segment"]),
        domain("profile.sketch",vec!["feature.sketch"],vec!["all points solved fully constrained; initial guesses select local branch", "independent constraint islands; every demanded island must converge with finite certified residuals", "curve profiles additionally require strict native circle compatibility after bounded solver refinement"],vec!["assembly joints; global branch uniqueness; automatic snapping; proof that nonconvergence means inconsistency"]),
        domain("solid.chamfer",vec!["feature.chamfer","feature.chamfer_faces"],vec!["one strictly convex planar solid; two exact outward unit normals select one face each in the input model frame", "selected faces share one straight edge; positive equal setback along both faces", "at most 48 supporting planes including the generated chamfer face", "chamfer_faces resolves two from_feature/role references against body with exactly-one cardinality before cache lookup; no nearest-face fallback; shared build-wide selection budget", "all original faces retained with complete body lineage; generated face role chamfer; no unrelated vertex removed or topology event"],vec!["curved/nonconvex/multisolid input, ambiguous/nonadjacent face criteria, unequal setbacks, fillets"]),
        domain("solid.extrusion",vec!["feature.extrude"],vec!["supported profile extruded along +Z through positive height; native planar/cylindrical boundary"],vec!["draft, sweep, loft"]),
        domain("solid.revolution",vec!["feature.revolve"],vec!["full turn about Z of simple polygon [radius,z] meridian, nonnegative radius", "bounded piecewise-linear supports: planes, cylinders, cones, including certified apex singularities", "axis-contact and manifold constraints are validated; arbitrary meridian rings are not guaranteed supported"],vec!["partial turn; curved meridian; arbitrary self-contact"]),
        domain("solid.placement",vec!["feature.translate","feature.transform"],vec!["offset in mm; rotation unit quaternion [x,y,z,w]; no implicit normalization", "general rotation: recognized analytic families, right prisms, or one strictly convex planar solid; exact signed permutations additionally support orthogonal nonconvex/multisolid models", "native and source-support numeric validation after placement"],vec!["scaling/shear; arbitrary nonconvex shell rotation"]),
        domain("boolean.orthogonal",vec!["feature.union","feature.cut","feature.intersect"],vec!["axis-aligned planar rectangular-face solids; exact input-coordinate arrangement", "bounded cells/classification work; disjoint components and through-holes supported"],vec!["cavities; nonmanifold/tangent contacts; unresolved close coordinate planes"]),
        domain("boolean.convex_planar_intersection",vec!["feature.intersect"],vec!["two single strictly convex planar solids; at most 48 combined input planes", "analytic plane reconstruction; resolved triples and positive-volume intersection"],vec!["general planar union/difference; lower-dimensional contacts"]),
        domain("boolean.circular_cylinders",vec!["feature.union","feature.cut","feature.intersect"],vec!["two recognized solid circular cylinders with parallel or opposite axes", "union requires matching axial intervals; cut requires tool coverage of left height; intersection may trim common axial interval", "native trimmed circle boundaries, major portions split into <= pi arcs; supported containment and empty cases", "world support reconstruction certified within arithmetic envelope; modeling tolerance never snaps support planes"],vec!["annular operands; disconnected curved union; skew axes; unresolved tangency; general curved operands"]),
        domain("boolean.through_bore",vec!["feature.cut"],vec!["canonical polygon/simple-hole/mixed-hole or line/arc prism stock; solid cylindrical tool", "tool covers whole stock height; circle lies strictly inside material and clear of all boundaries/holes", "sequential bores supported; existing stock faces retained; new inward tool wall attributed; tool caps deleted"],vec!["partial-depth pocket; crossing/touching/outside bore; cutter inside existing hole; skew or uncertain alignment"]),
        domain("boolean.blind_bore",vec!["feature.cut"],vec!["canonical polygon/simple-hole/mixed-hole or line/arc prism stock; solid parallel or opposite-axis cylinder", "flat-bottom blind cut opens exactly one stock cap with resolved depth and remaining floor thickness", "sequential top/bottom blind and through cuts with disjoint circular projections; original faces retained and new inward wall/floor attributed", "native planar floor, exact signed cylinder mass integrals, shared boundary tessellation and rigid placements"],vec!["enclosed cavities; intersecting or coaxial projected holes; crossing/touching outer profile or existing holes; skew axes; unresolved cap separation; drill-tip cones and general pockets"]),
        domain("boolean.stepped_bore",vec!["feature.cut"],vec!["canonical polygon or line/arc right prism with existing circular through/blind holes", "coaxial cylindrical cuts normalize to contiguous disjoint axial sections with resolved radial steps", "through-then-counterbore and blind-then-through, opposed counterbores and repeated steps; disjoint hole groups", "native annular shoulders with semantic wall/shoulder overlap lineage, analytic union-volume moments and datum attachment"],vec!["noncoaxial overlapping groups; outer-boundary contact; fully enclosed cutter intervals; skew or uncertain alignment; automatic healing"]),
        domain("boolean.countersink",vec!["feature.cut"],vec!["contained conical countersinks in canonical polygon/line-arc prism or adapted circular/annular stock", "parallel or opposite-axis solid frustum tools composed with coaxial cylindrical bores; analytical radius-equality section boundaries", "native inward cones, shared continuous boundary rings, explicit radial-step shoulders and full analytic mass properties", "semantic wall lineage, conservative tessellation and common rigid placements"],vec!["general curved Boolean closure; skew axes; noncoaxial overlapping hole groups; boundary contact; enclosed cutters; unresolved narrow sections; conical apex cuts"]),
        domain("boolean.round_stock",vec!["feature.cut"],vec!["canonical solid circular or annular stock with a solid parallel or opposite-axis cylindrical tool", "contained circular through/blind drilling followed by separated or coaxial stepped cuts", "exact two-semicircle outer boundary adaptation; original outer-wall lineage splits into two faces, caps and untouched central bore retain lineage", "native circular support, analytic mass properties and common rigid placements"],vec!["tangent/crossing stock boundaries or existing bore; noncoaxial overlapping holes; enclosed cutter; skew or unresolved placement; general curved Boolean closure"]),
    ];
    CapabilityCatalog{schema_version:CAPABILITY_SCHEMA_VERSION,interface:"json_recipe_ir",recipe_schema_versions:vec![1,2],latest_recipe_schema_version:crate::RECIPE_SCHEMA_VERSION,units:"mm",default_linear_tolerance_mm:cad_kernel::Tolerance::default().linear,identities:ImplementationIdentities{assembly_build_id:cad_assembly::assembly_build_id(),catalog_build_id:catalog_build_id(),kernel_version:cad_kernel::KERNEL_VERSION,kernel_build_id:cad_kernel::KERNEL_BUILD_ID,executor_build_id:crate::executor_build_id(),sketch_solver_build_id:crate::solver_build_id()},operations,object_types,domains,
        resource_limits:BTreeMap::from([("motion_studies_per_recipe",crate::MAX_MOTION_STUDIES),("motion_solve_attempts_per_build",crate::MAX_MOTION_ATTEMPTS),("mechanisms_per_recipe",crate::MAX_MECHANISMS),("mechanism_bindings_per_recipe",crate::MAX_MECHANISM_BINDINGS),("prescribed_joints_per_recipe",cad_assembly::MAX_PRESCRIBED_JOINTS),("features_per_recipe",crate::MAX_FEATURES),("section_views_per_recipe",crate::MAX_SECTION_VIEWS),("drawing_sheets_per_recipe",crate::MAX_DRAWING_SHEETS),("tolerance_stacks_per_recipe",crate::MAX_TOLERANCE_STACKS),("clearance_checks_per_recipe",crate::MAX_CLEARANCE_CHECKS),("clearance_path_resolution_work",crate::MAX_CLEARANCE_RESOLVE_WORK),("tolerance_stack_terms_per_stack",crate::MAX_TOLERANCE_STACK_TERMS),("tolerance_stack_terms_per_recipe",crate::MAX_TOLERANCE_STACK_TERMS_TOTAL),("drawing_viewports_per_sheet",crate::MAX_DRAWING_VIEWPORTS_PER_SHEET),("drawing_viewports_per_recipe",crate::MAX_DRAWING_VIEWPORTS),("section_dimensions_per_view",crate::MAX_SECTION_DIMENSIONS_PER_VIEW),("section_dimensions_per_recipe",crate::MAX_SECTION_DIMENSIONS),("section_view_geometry_work",crate::MAX_SECTION_VIEW_WORK),("exchange_requirements_per_recipe",crate::MAX_EXCHANGE_REQUIREMENTS),("demanded_sketches_per_build",128),("polygon_points_total",crate::MAX_POLYGON_POINTS),("stepped_bore_axial_sections",cad_kernel::MAX_STEPPED_BORE_SEGMENTS),("profile_holes_total",crate::MAX_POLYGON_HOLES),("sketch_points_total",cad_sketch::MAX_POINTS),("sketch_constraints_total",cad_sketch::MAX_CONSTRAINTS),("sketch_points_per_island",cad_sketch::MAX_ISLAND_POINTS),("sketch_constraints_per_island",cad_sketch::MAX_ISLAND_CONSTRAINTS),("solver_iterations_per_island",cad_sketch::MAX_ITERATIONS),("parts_per_recipe",crate::MAX_PARTS),("assemblies_per_recipe",crate::MAX_ASSEMBLIES),("instances_per_recipe",crate::MAX_INSTANCES),("outputs_per_recipe",crate::MAX_OUTPUTS)]),
        sketch_policy:SketchPolicy{report_schema_version:3,angular_tolerance_radians:cad_sketch::ANGULAR_TOLERANCE_RADIANS,max_condition_bound:cad_sketch::MAX_CONDITION_BOUND,acceptance:"fully_constrained only; linear residuals in mm and angular residuals in radians checked separately; condition bound is worst local normalized Jacobian condition, not coordinate accuracy"},
        validation_obligations:vec!["recipe units must be mm; schemas 1 and 2 accepted; assembly/assertion/datum fields require schema 2", "strict structural fields, unique identifiers, valid references, acyclic dependencies and aggregate resource budgets", "default tolerance is native Tolerance::default(); requested tolerance must be finite and positive", "numeric bounds, minimum feature clearances, canonical native topology and analytic mass properties validated; a declared family does not guarantee every parameter combination", "semantic face selections and property assertions must resolve; display meshes are derived, never authoritative", "unsupported operations fail explicitly; no implicit repair or dimensional snapping", "example paths are repository fixtures illustrating the domain, not a guarantee that arbitrary parameter edits succeed"],unsupported_operation_ids:vec!["feature.fillet","feature.loft","feature.sweep","feature.shell","feature.import_step","feature.general_boolean","assembly.solve_joints"]}
}

fn nullable(value: ArgumentType) -> ArgumentType {
    ArgumentType::Nullable {
        value: Box::new(value),
    }
}
fn map(values: ArgumentType) -> ArgumentType {
    ArgumentType::Map {
        values: Box::new(values),
    }
}
fn envelope_objects(operations: &[OperationCapability]) -> Vec<ObjectSpec> {
    use ArgumentType as T;
    let optional_null = |name, kind| optional(arg(name, nullable(kind)), Value::Null);
    let array_default = |name, kind| optional(arg(name, array(kind, None)), json!([]));
    let identity = json!({"translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]});
    let transform_args = || {
        vec![
            optional(arg("translation", numbers(3)), json!([0., 0., 0.])),
            optional(arg("rotation", numbers(4)), json!([0., 0., 0., 1.])),
        ]
    };
    let mut recipe = object_spec(
        "recipe",
        vec![
            arg("schema_version", T::Integer),
            arg("units", T::Enum { values: vec!["mm"] }),
            optional_null("tolerance", object("tolerance")),
            arg("features", array(object("feature"), None)),
            array_default("outputs", object("output")),
        ],
    );
    recipe.arguments[0]
        .constraints
        .push("supported values: 1, 2");
    for (name, kind) in [
        ("parts", "part_definition"),
        ("assemblies", "assembly_definition"),
        ("assembly_outputs", "assembly_output"),
        ("face_assertions", "face_assertion"),
        ("property_assertions", "property_assertion"),
        ("exchange_requirements", "exchange_requirement"),
        ("section_views", "section_view"),
        ("drawing_sheets", "drawing_sheet"),
        ("tolerance_stacks", "tolerance_stack"),
        ("clearance_checks", "clearance_check"),
        ("face_datums", "face_datum"),
        ("mechanisms", "mechanism_definition"),
        ("motion_studies", "motion_study_definition"),
        ("mechanism_bindings", "mechanism_binding"),
    ] {
        let mut a = array_default(name, object(kind));
        a.recipe_schema_versions = vec![2];
        a.constraints
            .push("schema 1 rejects field presence, even an empty array; null is invalid");
        recipe.arguments.push(a);
    }
    recipe.obligations.extend(["unique definition/output IDs, typed references and bounded acyclic graphs", "default tolerance is used for omitted or null tolerance; accepted recipe geometry is always validated"]);
    let feature = ObjectSpec {
        id: "feature",
        arguments: vec![],
        discriminator: Some("op"),
        variants: operations
            .iter()
            .filter(|op| op.id.starts_with("feature."))
            .map(|op| ObjectVariant {
                tag_value: op.json_op,
                arguments: op
                    .arguments
                    .iter()
                    .filter(|a| a.name != "op")
                    .cloned()
                    .collect(),
            })
            .collect(),
        obligations: vec![
            "operation-specific fields only; native validation remains authoritative",
        ],
    };
    let mut tolerance = object_spec("tolerance", vec![arg("linear", T::Number)]);
    tolerance.arguments[0].units = Some("mm");
    tolerance.arguments[0]
        .constraints
        .push("finite and strictly positive");
    let mut material = object_spec(
        "material",
        vec![arg("name", T::String), arg("density_kg_per_m3", T::Number)],
    );
    material.arguments[0]
        .constraints
        .push("nonempty; no surrounding whitespace");
    material.arguments[1].units = Some("kg/m^3");
    material.arguments[1]
        .constraints
        .push("finite and positive");
    let mut metadata = object_spec(
        "part_metadata",
        vec![
            optional_null("part_number", T::String),
            optional_null("revision", T::String),
            optional_null("description", T::String),
            optional_null("material", object("material")),
        ],
    );
    metadata.obligations.extend([
        "part_number and revision, when present, must be nonempty without surrounding whitespace",
        "description, when present, must contain non-whitespace text; no arbitrary attributes map",
    ]);
    let mut part = object_spec(
        "part_definition",
        vec![
            arg("id", T::String),
            arg("feature", reference("solid_feature")),
            optional(arg("metadata", object("part_metadata")), json!({})),
            optional(arg("datums", map(object("datum"))), json!({})),
        ],
    );
    part.obligations.extend(["datum map keys are literal datum names; at most 1024 combined explicit and derived datums per part", "part geometry remains immutable and shared across occurrences"]);
    let mut frame = object_spec("rigid_transform", transform_args());
    frame.obligations.extend(["translation in mm; unit quaternion [x,y,z,w], finite values; no normalization/scaling/shear"]);
    let mut datum = object_spec("datum", transform_args());
    datum.obligations.extend([
        "local part rigid frame stored as the value of the datums map; the map key names the datum",
        "same finite/unit-quaternion validation as rigid_transform",
    ]);
    let mut attachment = object_spec(
        "datum_attachment",
        vec![
            arg("instance", reference("sibling_occurrence")),
            array_default("target_path", reference("occurrence_path_component")),
            arg("datum", reference("source_part_datum")),
            arg("target_datum", reference("target_part_datum")),
            optional_null("joint", object("prescribed_joint")),
        ],
    );
    attachment.obligations.extend(["instance identifies target root occurrence within this assembly definition; target_path contains literal descendant occurrence IDs and must end at a part", "empty target_path targets a sibling leaf; bounded acyclic attachment dependencies, not a nonlinear assembly solver"]);
    let mut assembly_attachment = object_spec(
        "assembly_attachment",
        vec![
            arg("instance", reference("sibling_occurrence")),
            array_default("target_path", reference("occurrence_path_component")),
            arg(
                "source_path",
                array(reference("occurrence_path_component"), None),
            ),
            arg("datum", reference("source_part_datum")),
            arg("target_datum", reference("target_part_datum")),
            optional_null("joint", object("prescribed_joint")),
        ],
    );
    assembly_attachment.obligations.extend(["source_path is nonempty and ends at a part inside this child assembly; target_path resolves relative to the target root occurrence", "paths use literal occurrence IDs, not slash-encoded strings; depth/work/resource limits apply"]);
    let mut joint = object_spec("prescribed_joint", vec![]);
    joint.discriminator = Some("kind");
    joint.variants = [("revolute", "rad"), ("prismatic", "mm")]
        .into_iter()
        .map(|(kind, unit)| ObjectVariant {
            tag_value: kind,
            arguments: ["position", "min", "max"]
                .into_iter()
                .map(|name| {
                    let mut a = arg(name, T::Number);
                    a.units = Some(unit);
                    a.constraints.push("finite; min <= position <= max");
                    a
                })
                .collect(),
        })
        .collect();
    joint.obligations.extend(["only valid inside complete datum attachments; prescribed motion, not a solved degree of freedom", "axis +Z in the target datum after the existing offset transform; angular position and limits in radians, translation in millimetres", "unknown fields, nonfinite values and out-of-range positions reject globally"]);
    attachment.obligations.push("optional prescribed joint composes after local offset and before inverse source datum; position does not change part geometry");
    assembly_attachment.obligations.push("optional prescribed joint moves the child rigidly; repeated definition geometry remains shared");
    let mut part_instance = object_spec(
        "part_instance",
        vec![
            arg("id", T::String),
            arg("part", reference("part_definition")),
            optional(
                arg("transform", object("rigid_transform")),
                identity.clone(),
            ),
            optional_null("attachment", object("datum_attachment")),
        ],
    );
    part_instance.obligations.push("transform is local placement; with attachment it is the offset composed between target datum and inverse source datum");
    let mut assembly_instance = object_spec(
        "assembly_instance",
        vec![
            arg("id", T::String),
            arg("assembly", reference("assembly_definition")),
            optional(arg("transform", object("rigid_transform")), identity),
            optional_null("attachment", object("assembly_attachment")),
        ],
    );
    assembly_instance.obligations.push("child definition is instantiated rigidly, with optional descendant datum attachment and local offset; no joint solving");
    let mut assembly = object_spec(
        "assembly_definition",
        vec![
            arg("id", T::String),
            array_default("instances", object("part_instance")),
            array_default("assemblies", object("assembly_instance")),
        ],
    );
    assembly.obligations.extend(["part and child occurrence IDs share one namespace per definition; nesting must be acyclic and within expansion budgets", "occurrences may overlap; assembly material volume/mass sums occurrences rather than Boolean union; placement is not collision certification"]);
    vec![
        recipe,
        feature,
        joint,
        object_spec(
            "motion_study_definition",
            vec![
                arg("id", T::String),
                arg("mechanism", reference("mechanism_definition")),
                arg("input", object("motion_study")),
            ],
        ),
        object_spec(
            "motion_study",
            vec![
                arg("targets", array(map(T::Number), None)),
                arg("max_drive_step_radians", T::Number),
                arg("max_pin_step_mm", T::Number),
            ],
        ),
        object_spec(
            "mechanism_definition",
            vec![
                arg("id", T::String),
                optional(
                    arg("frame", object("rigid_transform")),
                    json!({"translation":[0,0,0],"rotation":[0,0,0,1]}),
                ),
                arg("input", object("planar_mechanism")),
            ],
        ),
        object_spec(
            "planar_mechanism",
            vec![
                arg("pins", array(object("mechanism_pin"), None)),
                arg("links", array(object("mechanism_link"), None)),
                array_default("drives", object("mechanism_drive")),
                array_default("branches", object("mechanism_branch")),
            ],
        ),
        object_spec(
            "mechanism_pin",
            vec![
                arg("id", T::String),
                arg("initial", array(T::Number, Some(2))),
                optional_null("ground", array(T::Number, Some(2))),
            ],
        ),
        object_spec(
            "mechanism_link",
            vec![
                arg("id", T::String),
                arg("from", reference("mechanism_pin")),
                arg("to", reference("mechanism_pin")),
                arg("length", T::Number),
            ],
        ),
        object_spec(
            "mechanism_drive",
            vec![
                arg("id", T::String),
                arg("link", reference("mechanism_link")),
                arg("angle_radians", T::Number),
            ],
        ),
        object_spec(
            "mechanism_branch",
            vec![
                arg("id", T::String),
                arg("pins", array(reference("mechanism_pin"), Some(3))),
                arg("side", T::String),
            ],
        ),
        object_spec(
            "mechanism_binding",
            vec![
                arg("assembly", reference("assembly_definition")),
                arg("instance", reference("local_occurrence")),
                arg("mechanism", reference("mechanism_definition")),
                arg("link", reference("mechanism_link")),
            ],
        ),
        object_spec(
            "chamfer_face",
            vec![
                arg("from_feature", reference("solid_feature")),
                arg("role", T::String),
            ],
        ),
        tolerance,
        object_spec(
            "output",
            vec![
                arg("name", T::String),
                arg("feature", reference("solid_feature")),
            ],
        ),
        part,
        metadata,
        material,
        frame,
        datum,
        assembly,
        part_instance,
        assembly_instance,
        attachment,
        assembly_attachment,
        object_spec(
            "assembly_output",
            vec![
                arg("name", T::String),
                arg("assembly", reference("assembly_definition")),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Feature;
    use std::collections::BTreeSet;
    fn tag(feature: &Feature) -> &'static str {
        match feature {
            Feature::Sketch { .. } => "sketch",
            Feature::Path { .. } => "path",
            Feature::Circle { .. } => "circle",
            Feature::Rectangle { .. } => "rectangle",
            Feature::Polygon { .. } => "polygon",
            Feature::Revolve { .. } => "revolve",
            Feature::Extrude { .. } => "extrude",
            Feature::ChamferFaces { .. } => "chamfer_faces",
            Feature::Chamfer { .. } => "chamfer",
            Feature::Translate { .. } => "translate",
            Feature::Transform { .. } => "transform",
            Feature::Union { .. } => "union",
            Feature::Cut { .. } => "cut",
            Feature::Intersect { .. } => "intersect",
        }
    }
    #[test]
    fn every_feature_contract_matches_real_ir_fields_and_defaults() {
        let catalog = capability_catalog();
        let samples = [
            json!({"op":"circle","id":"f","radius":2.5}),
            json!({"op":"rectangle","id":"f","width":2.5,"depth":3.5}),
            json!({"op":"polygon","id":"f","points":[[0,0],[3,0],[0,3]]}),
            json!({"op":"path","id":"f","start":[0,0],"segments":[{"kind":"line_to","end":[3,0]},{"kind":"line_to","end":[0,3]},{"kind":"line_to","end":[0,0]}]}),
            json!({"op":"sketch","id":"f","sketch":{"points":[],"constraints":[]},"loops":[["a","b","c"]]}),
            json!({"op":"extrude","id":"f","profile":"p","height":2.5}),
            json!({"op":"revolve","id":"f","profile":"p"}),
            json!({"op":"chamfer_faces","id":"f","body":"b","faces":[{"from_feature":"b","role":"end_cap"},{"from_feature":"b","role":"wall_y_min"}],"distance":1}),
            json!({"op":"chamfer","id":"f","body":"b","face_normals":[[1,0,0],[0,0,1]],"distance":1}),
            json!({"op":"translate","id":"f","body":"b","offset":[1,2,3]}),
            json!({"op":"transform","id":"f","body":"b","offset":[1,2,3],"rotation":[0,0,0,1]}),
            json!({"op":"union","id":"f","left":"a","right":"b"}),
            json!({"op":"cut","id":"f","left":"a","right":"b"}),
            json!({"op":"intersect","id":"f","left":"a","right":"b"}),
        ];
        assert_eq!(
            catalog
                .operations
                .iter()
                .filter(|op| op.id.starts_with("feature."))
                .count(),
            samples.len()
        );
        let mut seen = BTreeSet::new();
        for sample in samples {
            let feature: Feature = serde_json::from_value(sample.clone()).unwrap();
            let op = catalog
                .operations
                .iter()
                .find(|op| op.json_op == tag(&feature))
                .unwrap();
            assert!(seen.insert(op.json_op));
            let serialized = serde_json::to_value(feature).unwrap();
            let fields: BTreeSet<_> = op.arguments.iter().map(|arg| arg.name).collect();
            for field in serialized.as_object().unwrap().keys() {
                assert!(
                    fields.contains(field.as_str()),
                    "undocumented IR field {field}"
                );
            }
            for arg in &op.arguments {
                if arg.required {
                    assert!(
                        sample.get(arg.name).is_some(),
                        "required field mismatch {}",
                        arg.name
                    );
                }
                if let Some(value) = &arg.default {
                    if sample.get(arg.name).is_none() {
                        if let Some(actual) = serialized.get(arg.name) {
                            assert_eq!(actual, value);
                        }
                    }
                }
            }
            let mut unknown = sample;
            unknown["unknown"] = json!(1);
            assert!(serde_json::from_value::<Feature>(unknown).is_err());
        }
    }
    #[test]
    fn recipe_envelope_presence_and_assembly_defaults_match_serde() {
        let catalog = capability_catalog();
        let recipe_spec = catalog
            .object_types
            .iter()
            .find(|o| o.id == "recipe")
            .unwrap();
        let base = json!({"schema_version":1,"units":"mm","features":[]});
        let recipe: crate::Recipe = serde_json::from_value(base.clone()).unwrap();
        assert!(recipe.outputs.is_empty());
        assert!(recipe.tolerance.is_none());
        for arg in recipe_spec
            .arguments
            .iter()
            .filter(|a| a.recipe_schema_versions == [2])
        {
            let mut source = base.clone();
            source[arg.name] = json!([]);
            assert!(
                serde_json::from_value::<crate::Recipe>(source.clone()).is_err(),
                "schema1 presence {}",
                arg.name
            );
            source["schema_version"] = json!(2);
            assert!(serde_json::from_value::<crate::Recipe>(source.clone()).is_ok());
            source[arg.name] = Value::Null;
            assert!(
                serde_json::from_value::<crate::Recipe>(source).is_err(),
                "null array {}",
                arg.name
            );
        }
        let source = json!({"schema_version":2,"units":"mm","tolerance":null,"features":[],"parts":[{"id":"part","feature":"body","metadata":{"part_number":"PN-1","revision":null,"description":"A part","material":{"name":"steel","density_kg_per_m3":7800}},"datums":{"mount":{}}}],"assemblies":[{"id":"root","instances":[{"id":"base","part":"part","attachment":null},{"id":"follower","part":"part","attachment":{"instance":"base","datum":"mount","target_datum":"mount"}}],"assemblies":[{"id":"child","assembly":"nested","attachment":{"instance":"base","source_path":["leaf"],"datum":"mount","target_datum":"mount"}}]},{"id":"nested","instances":[{"id":"leaf","part":"part"}]}],"assembly_outputs":[{"name":"root","assembly":"root"}]});
        let recipe: crate::Recipe = serde_json::from_value(source).unwrap();
        assert!(recipe.tolerance.is_none());
        assert_eq!(
            recipe.parts[0].datums["mount"],
            crate::RigidTransform::default()
        );
        assert_eq!(
            recipe.assemblies[0].instances[0].transform,
            crate::RigidTransform::default()
        );
        assert!(recipe.assemblies[0].instances[1]
            .attachment
            .as_ref()
            .unwrap()
            .target_path
            .is_empty());
        assert_eq!(
            recipe.assemblies[0].assemblies[0]
                .attachment
                .as_ref()
                .unwrap()
                .source_path,
            ["leaf"]
        );
        assert!(recipe.assemblies[0].assemblies[0]
            .attachment
            .as_ref()
            .unwrap()
            .target_path
            .is_empty());
        assert_eq!(
            recipe.parts[0]
                .metadata
                .material
                .as_ref()
                .unwrap()
                .density_kg_per_m3,
            7800.
        );
        let part: crate::PartDefinition =
            serde_json::from_value(json!({"id":"p","feature":"body"})).unwrap();
        assert_eq!(part.metadata, crate::PartMetadata::default());
        assert!(part.datums.is_empty());
        assert!(serde_json::from_value::<crate::PartMetadata>(
            json!({"attributes":{"custom":"x"}})
        )
        .is_err());
        assert!(serde_json::from_value::<crate::PartDefinition>(
            json!({"id":"p","feature":"body","metadata":null})
        )
        .is_err());
        assert!(serde_json::from_value::<crate::Instance>(
            json!({"id":"i","part":"p","transform":null})
        )
        .is_err());
        for id in [
            "recipe",
            "part_definition",
            "part_metadata",
            "material",
            "rigid_transform",
            "datum",
            "assembly_definition",
            "part_instance",
            "assembly_instance",
            "datum_attachment",
            "assembly_attachment",
            "prescribed_joint",
            "assembly_output",
        ] {
            assert!(catalog.object_types.iter().any(|o| o.id == id));
        }
    }
    #[test]
    fn discovery_is_deterministic_closed_and_backed_by_repository_examples() {
        let a = capability_catalog();
        let b = capability_catalog();
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(a.schema_version, CAPABILITY_SCHEMA_VERSION);
        assert_eq!(a.identities.catalog_build_id, catalog_build_id());
        assert_eq!(catalog_build_id().len(), 64);
        assert!(catalog_build_id().bytes().all(|v| v.is_ascii_hexdigit()));
        assert_eq!(
            serde_json::to_value(&a).unwrap()["identities"]["catalog_build_id"],
            catalog_build_id()
        );
        assert_eq!(
            a.default_linear_tolerance_mm,
            cad_kernel::Tolerance::default().linear
        );
        let ids: BTreeSet<_> = a.operations.iter().map(|op| op.id).collect();
        assert_eq!(ids.len(), a.operations.len());
        let domains: BTreeSet<_> = a.domains.iter().map(|d| d.id).collect();
        let objects: BTreeSet<_> = a.object_types.iter().map(|o| o.id).collect();
        fn check_type(t: &ArgumentType, objects: &BTreeSet<&str>) {
            match t {
                ArgumentType::Object { type_id } => assert!(objects.contains(type_id)),
                ArgumentType::Array { items, .. } => check_type(items, objects),
                ArgumentType::Map { values } => check_type(values, objects),
                ArgumentType::Nullable { value } => check_type(value, objects),
                _ => {}
            }
        }
        for op in &a.operations {
            for domain in &op.domain_ids {
                assert!(domains.contains(domain));
            }
            for arg in &op.arguments {
                check_type(&arg.value_type, &objects);
            }
            for example in &op.examples {
                assert!(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../..")
                        .join(example)
                        .is_file(),
                    "missing {example}"
                );
            }
        }
        for domain in &a.domains {
            for id in &domain.operation_ids {
                assert!(ids.contains(id));
            }
        }
        for object in &a.object_types {
            for arg in object
                .arguments
                .iter()
                .chain(object.variants.iter().flat_map(|v| &v.arguments))
            {
                check_type(&arg.value_type, &objects);
            }
        }
        assert_eq!(
            a.resource_limits["sketch_points_per_island"],
            cad_sketch::MAX_ISLAND_POINTS
        );
        assert_eq!(
            a.resource_limits["sketch_points_total"],
            cad_sketch::MAX_POINTS
        );
        assert_eq!(
            a.object_types
                .iter()
                .find(|o| o.id == "sketch_constraint")
                .unwrap()
                .variants
                .len(),
            10
        );
        assert!(!ids.contains("feature.fillet"));
    }
}
