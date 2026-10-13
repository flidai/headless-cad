//! Authoring shapes for revision-bound engineering obligations.
use super::*;

pub(super) fn objects() -> Vec<ObjectSpec> {
    use ArgumentType as T;
    let optional_null = |name, kind| {
        optional(
            arg(
                name,
                T::Nullable {
                    value: Box::new(kind),
                },
            ),
            json!(null),
        )
    };
    let mut count = arg("count", T::Integer);
    count
        .constraints
        .push("positive usize; zero is rejected by semantic validation");
    let mut cardinality = object_spec("face_cardinality", vec![]);
    cardinality.discriminator = Some("kind");
    cardinality.variants = vec![
        ObjectVariant {
            tag_value: "exactly",
            arguments: vec![count],
        },
        ObjectVariant {
            tag_value: "all",
            arguments: vec![],
        },
    ];
    cardinality.obligations.push("exactly requires the stated positive survivor count; all accepts nonempty splits; deleted/ambiguous/unsupported selections are not certified references");
    let mut selector = object_spec(
        "face_selector",
        vec![
            arg("anchor_feature", reference("feature")),
            arg("role", T::String),
            arg("target_feature", reference("solid_feature")),
            arg("cardinality", object("face_cardinality")),
        ],
    );
    selector.obligations.extend([
        "role names an anchor operation's generated face role, not a transient face index",
        "lineage follows the dependency graph; no nearest-face or array-index fallback",
    ]);
    let mut assertion = object_spec(
        "face_assertion",
        vec![
            arg("id", T::String),
            arg("selector", object("face_selector")),
        ],
    );
    assertion.obligations.extend([
        "schema 2; unique nonempty assertion ID",
        "global build obligation even with selected outputs or cache hits; failed reference prevents publication",
    ]);
    let properties = vec![
        "volume_mm3",
        "mass_kg",
        "volume_centroid_x_mm",
        "volume_centroid_y_mm",
        "volume_centroid_z_mm",
        "mass_centroid_x_mm",
        "mass_centroid_y_mm",
        "mass_centroid_z_mm",
        "volume_inertia_xx_mm5",
        "volume_inertia_yy_mm5",
        "volume_inertia_zz_mm5",
        "volume_inertia_xy_mm5",
        "volume_inertia_xz_mm5",
        "volume_inertia_yz_mm5",
        "mass_inertia_xx_kg_mm2",
        "mass_inertia_yy_kg_mm2",
        "mass_inertia_zz_kg_mm2",
        "mass_inertia_xy_kg_mm2",
        "mass_inertia_xz_kg_mm2",
        "mass_inertia_yz_kg_mm2",
    ];
    let bound = |name| {
        let mut spec = optional(
            arg(
                name,
                T::Nullable {
                    value: Box::new(T::Number),
                },
            ),
            json!(null),
        );
        spec.units = Some("selected property units (encoded in property name)");
        spec.constraints.push("null/omitted means no bound; present number must be finite; inclusive comparison without added tolerance");
        spec
    };
    let mut property = object_spec(
        "property_assertion",
        vec![
            arg("id", T::String),
            arg("output", reference("output_name")),
            arg("property", T::Enum { values: properties }),
            bound("min"),
            bound("max"),
        ],
    );
    property.obligations.extend([
        "schema 2; unique nonempty ID; at least one non-null bound; min <= max when both present",
        "property applies to a named body or assembly output; mass properties require an assembly whose every occurrence has density",
        "volume centroid is geometric; mass centroid is density-weighted; inertia is central in output/root frame; off-diagonal entries are negative product moments",
        "assembly properties sum occurrences including overlaps; not collision, fit or union-volume evidence",
        "global build obligation, including selected-output builds and cache hits; unavailable results fail rather than becoming zero",
    ]);
    let mut direction = arg("x_direction", numbers(3));
    direction.constraints.push("finite nonzero orientation hint; projected tangent must be numerically resolvable on the selected planar face");
    let mut offset = optional(arg("offset", numbers(3)), json!([0, 0, 0]));
    offset.units = Some("mm");
    offset
        .constraints
        .push("finite displacement in resolved datum-local axes");
    let mut datum = object_spec(
        "face_datum",
        vec![
            arg("part", reference("part_definition")),
            arg("name", T::String),
            arg("anchor_feature", reference("feature")),
            arg("role", T::String),
            direction,
            offset,
        ],
    );
    datum.obligations.extend([
        "schema 2; name unique across fixed and associative datums of the owning part",
        "resolve anchor role through lineage to exactly one face of the owning part's solid feature; failure blocks publication",
        "supported planar face frame uses material area centroid and outward normal; ambiguous, nonplanar or unsupported faces fail",
        "rigid attachment datum, not a manufacturing GD&T datum system",
    ]);
    let mut exchange = object_spec(
        "exchange_requirement",
        vec![
            arg("id", T::String),
            arg("output", reference("output_name")),
            arg(
                "format",
                T::Enum {
                    values: vec!["step", "stl"],
                },
            ),
            arg(
                "aspect",
                T::Enum {
                    values: crate::EXCHANGE_ASPECTS.to_vec(),
                },
            ),
        ],
    );
    exchange.obligations.extend([
        "schema 2; unique nonempty ID; known output; at most 256 requirements",
        "publication obligation authored by Starlark require_exchange; native checks leave it pending",
        "full CLI publication requires supported fidelity status and a positive inspected source count",
        "global across selected-output builds and cache hits; unselected required outputs and display-only publication fail",
        "exporter declarations are not independent downstream fidelity evidence or manufacturing certification",
    ]);
    let mut anchor = object_spec(
        "section_anchor",
        vec![
            arg(
                "instance_path",
                array(reference("occurrence_path_component"), None),
            ),
            arg("datum", reference("part_datum")),
        ],
    );
    anchor.obligations.extend(["assembly output only; exact literal leaf occurrence path, no flattened-ID guessing or navigation", "path length 1..64; each component and datum name nonempty, no surrounding whitespace, at most 1024 UTF-8 bytes", "section plane interpreted relative to named part datum; resolved through occurrence placement times datum frame; missing path/datum fails globally", "Starlark section_view instance_path and datum must be supplied together; source local plane remains unchanged and result includes placement witness"]);
    let mut dimension = object_spec(
        "section_dimension",
        vec![
            arg("id", T::String),
            arg("from", object("section_anchor")),
            arg("to", object("section_anchor")),
            arg(
                "axis",
                T::Enum {
                    values: vec!["x", "y"],
                },
            ),
            arg("offset", T::Number),
            optional(
                arg(
                    "min",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
            optional(
                arg(
                    "max",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
        ],
    );
    dimension.obligations.extend(["projected datum-origin distance along resolved section X or Y, not 3D distance; endpoints need not lie in the plane", "value_mm is absolute axis delta; signed_delta_mm and projected endpoints retained; value <= modeling tolerance fails as degenerate", "offset is an absolute dimension-line coordinate on the perpendicular drawing axis in mm, not a measurement adjustment", "IDs case-insensitive unique within view; same safe identifier rules as section_view; finite offset and finite nonnegative ordered bounds", "Starlark section_dimension(view,id,from_instance_path,from_datum,to_instance_path,to_datum,offset,axis='x',min=None,max=None,tolerance=None) attaches to an already declared assembly section", "at most 32 per view and 1024 per recipe; all measurements are global native obligations including cache hits and selected builds; datum/face/placement witnesses retained"]);
    let mut circle_dimension = object_spec(
        "section_circle_dimension",
        vec![
            arg("id", T::String),
            arg("anchor_feature", reference("solid_feature")),
            arg("role", T::String),
            optional(
                arg(
                    "instance_path",
                    T::Nullable {
                        value: Box::new(array(reference("occurrence_path_component"), None)),
                    },
                ),
                json!(null),
            ),
            arg(
                "kind",
                T::Enum {
                    values: vec!["radius", "diameter"],
                },
            ),
            arg("label_offset", numbers(2)),
            optional(
                arg(
                    "min",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
            optional(
                arg(
                    "max",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
        ],
    );
    circle_dimension.arguments.push(optional(
        arg(
            "coverage",
            T::Enum {
                values: vec!["full_circle", "single_arc"],
            },
        ),
        json!("full_circle"),
    ));
    circle_dimension.obligations.extend(["Starlark section_circle_dimension attaches to an already declared view; from_feature maps to anchor_feature; default kind diameter", "nonempty semantic face lineage selection in the owning body or exact assembly leaf occurrence; path absent for body and required for assembly", "default full_circle coverage requires selected descendants cover exactly one complete native circular section loop; consistent center/radius/orientation, continuous full turn; no partial coverage, multiple loops, duplicate winding, nearest-face guesses or mesh fitting", "label_offset is relative to circle center in drawing mm and must place label strictly outside circle; bounds inclusive native radius/diameter in mm", "shares IDs and 32-perview/1024-global limits with projected dimensions, and shared face-assertion traversal budget; all source views validate globally on cache hits and selected builds"]);
    circle_dimension.obligations.extend(["explicit coverage single_arc requires kind radius, exactly one selected native face descendant and exactly one native Arc segment across all loops of the exact body or occurrence", "single_arc refuses zero, full-turn, duplicate, noncircular or unresolved angular extents; report arc retains face, loop/segment indices, angular span and midpoint tip; arithmetic_uncertainty_mm bounds support evaluation only, not physical geometry or manufacturing tolerance", "radius remains the native nominal analytic support value; world/native origins and occurrence placement enter arithmetic admission; existing native dimension bounds, tolerance and stack scalar semantics remain unchanged"]);
    let mut clearance = object_spec(
        "clearance_check",
        vec![
            arg("id", T::String),
            arg("output", reference("assembly_output")),
            arg(
                "left_instance_path",
                array(reference("occurrence_path_component"), None),
            ),
            arg(
                "right_instance_path",
                array(reference("occurrence_path_component"), None),
            ),
            arg("min_clearance", T::Number),
            optional(
                arg("right_translation", numbers(3)),
                serde_json::Value::Null,
            ),
            optional(
                arg("right_rotation", object("clearance_rotation_path")),
                serde_json::Value::Null,
            ),
        ],
    );
    clearance.arguments.push(optional(
        arg("rotation_group", reference("rigid_rotation_group")),
        json!(null),
    ));
    clearance.arguments.push(optional(
        arg("translation_group", reference("rigid_translation_group")),
        json!(null),
    ));
    clearance.obligations.extend(["rotation_group is mutually exclusive with individual right_translation/right_rotation; group and check must name the same output and at least one endpoint must be a member", "both group members use the initial native distance under exact common-rigid-map invariance, counted once; one moving endpoint uses the group's single reference-resolved world arc, counts actual kernel samples, and preserves authored left/right initial-query witnesses", "rotation proof_kind is lipschitz_subdivision with complete intervals, rotational_symmetry with a whole-stationary-boundary invariance certificate, or bore_witness_orbit with a pair-specific guarded-bore/material-witness orbit certificate; both special proofs carry full [0,1] coverage and three actual native queries; stationary symmetry admits canonical circular extrusions and orthogonal-meridian full-turn bodies only, including flanged bushes; bore_witness_orbit requires the static bore_trim_witness domain plus complete guarded bore/cap witness circles and uniformly bounded raw support, coaxial alignment and motion arithmetic; a rounded housing is not rotationally symmetric, and empty subdivision intervals alone are not coverage"]);
    let mut rotation_group = object_spec(
        "rigid_rotation_group",
        vec![
            arg("id", T::String),
            arg("output", reference("assembly_output")),
            arg("instance_paths", array(array(T::String, None), None)),
            arg("reference_instance_path", array(T::String, None)),
            arg("pivot", numbers(3)),
            arg("axis", numbers(3)),
            arg("angle_radians", T::Number),
        ],
    );
    rotation_group.obligations.extend(["schema-2 source-owned common rigid group: pivot and unit axis are in one declared reference member's initial body frame; every member receives the same world rigid map, not a rotation about its own origin", "at most 16 groups, 64 exact leaf paths per group and 256 members total; duplicate paths and overlapping groups on the same output rejected; reference must be a member; finite signed nonzero angle abs<=2pi and finite pivot length<=1000000 mm", "every group must have explicit clearance_check references; only these named pairs are certified, never all group or assembly collisions; all members and native bounds resolve globally even for selected outputs and cache hits", "group checks share the 24 motion-check and 4096 native-query budget; internal common-motion pair consumes one initial query; one-moving symmetry consumes three queries with initial query reused", "reports retain exact member paths/native hashes/initial frames and resolved common world path; inspector animation is illustrative and cannot author or certify motion"]);
    let mut translation_group = object_spec(
        "rigid_translation_group",
        vec![
            arg("id", T::String),
            arg("output", reference("assembly_output")),
            arg("instance_paths", array(array(T::String, None), None)),
            arg("reference_instance_path", array(T::String, None)),
            arg("translation", numbers(3)),
        ],
    );
    translation_group.obligations.extend(["schema-2 source-owned common translation: finite nonzero local vector of length at most 1000000 mm in one declared reference member's initial frame", "at most 16 groups, 64 exact leaf paths per group and 256 members total; duplicate or overlapping translation and rotation group paths on the same output rejected", "every group requires named clearance pairs; both moving endpoints preserve native distance under a common rigid map, while crossing pairs require complete native translation proof", "global path and geometry bounds resolve on selected outputs and cache hits; every authored ancestor translation and path depth contribute to arithmetic guards and attached group frames are refused; reports retain exact source paths, member hashes, initial frames, world translation, and numerical uncertainty; viewer motion is illustrative"]);
    clearance.arguments[4].units = Some("mm");
    clearance.arguments[5].units = Some("mm");
    clearance.obligations.extend(["schema-2 recipe.clearance_checks; Starlark clearance_check(id,output,left_instance_path,right_instance_path,min_clearance,right_translation=None,right_rotation=None,rotation_group=None,translation_group=None); strict object, unknown fields rejected", "at most 64 checks; case-insensitive unique ASCII letters/digits/underscore/hyphen IDs of length 1..128; finite strictly positive minimum", "two distinct exact leaf paths of length 1..64, nonempty trimmed components at most 1024 bytes; bounded 10000000 aggregate path-component resolution work", "assembly output only; global native dependency and check even on selected builds and cache hits; not a viewer or display-mesh query", "validated single strictly convex planar solids, hole-free convex line/arc right prisms (including sequential longitudinal fillets), finite solid cylinders and convex full-turn linear-meridian revolutions (cones, frusta and convex external multisegments), including mixed pairs; coaxial recognized cylinders/annuli and full-turn linear-meridian solids also support hollow and stepped nonconvex profiles by filled-meridian distance; parallel or opposed-axis recognized cylinder/annulus pairs additionally admit lateral displacement using analytic radial/axial interval distance; unsupported noncoaxial nonconvex families beyond these circular extrusions or other curved, multiple/empty/contact/overlap or unresolved inputs fail explicitly; convex_trim_witness additionally admits canonical convex curved line/arc prisms with circular through holes against a certified convex partner only when separated convex outer envelopes yield actual material witnesses outside every bore by a reserved arithmetic margin; overlapping envelopes or witnesses on/inside/too near holes refuse, never use filled-envelope witnesses as material evidence", "before placement, each operand is capped at 128 vertices, 256 edges, 128 faces and 512 loop-edge references; convex admission allows 128 loops with one per face, while coaxial meridian admission allows 256 loops with two per face; circular-holed curved-prism trim admission allows up to 32 bores and 256 total loops with canonical cap loops equal to one plus the hole count, and its stripped envelope must meet the convex limits; kernel planar candidate limit is 200000 and analytic support refinement limit is 256 iterations", "method-labelled native closest-witness bounds from convex support certification or coaxial filled-meridian region distance or parallel annular radial/axial distance or material-validated convex trim witnesses, widened by explicit occurrence-placement arithmetic envelope using full native analytic bounds; lower bound must meet minimum without tolerance slack", "report retains assembly/part/feature identity, exact paths and frames, raw kernel query and placement-adjusted bounds; query axis is a witness direction, not necessarily a separating plane for nested coaxial bodies; no penetration depth, general collision or manufacturing-tolerance inference"]);
    clearance.obligations.push("bore_trim_witness admits a recognized circular through bore in a convex curved line/arc prism against an arithmetic-coaxial solid cylindrical or recognized solid stepped/revolved shaft stock, including a finite external keyseat; the competing-radius disk must remain strictly inside the outer profile and outside every other bore, separated meridians must yield actual retained bore/cap and shaft witnesses on an untouched ray; unresolved alignment, contact/overlap and competing outer/cap/other-hole features fail explicitly; this is nominal native separation, not a manufactured fit or general holed-body distance");
    clearance.obligations.push("slot_trim_witness admits a canonical full-height capsule slot in a native arc prism against one arithmetic-parallel constant-radius cylindrical shank spanning the slab; other bolt sections must remain axially separated beyond the radial gap, and the typed translation proof bounds the complete affine path by its guarded endpoint centerline distances; no general holed-body distance or bolted clamp preload is implied");
    clearance.obligations.extend(["optional right_translation is a nonzero finite local body-axis vector with length <=1000000 mm; right orientation and left placement remain fixed over normalized time [0,1]", "whole-path translation uses midpoint Lipschitz bounds with complete dyadic interval coverage, or a typed slot_motion_proof that certifies the complete affine shank path inside one native capsule from guarded endpoint distances; the latter has two actual endpoint queries and empty subdivision intervals, and refuses any unresolved head, rim or axial interaction", "at most 24 combined translation/rotation checks per recipe, 1025 native samples per check and 4096 aggregate native sweep samples; all checks remain global on selected builds/cache hits", "query is the initial static certificate; optional sweep retains samples plus complete interval coverage or a typed whole-slot certificate; top-level bounds enclose the whole-path minimum and need not be tolerance-width; local-vector rotation uncertainty is reserved in the required threshold and widened report bounds"]);
    clearance.obligations.extend(["right_rotation is mutually exclusive with right_translation; strict {pivot,axis,angle_radians} object, pivot and unit axis in right body local coordinates, finite pivot length <=1000000 mm, nonzero signed angle abs<=2pi", "rotation_sweep certifies the complete fixed-pivot signed arc using radius/speed Lipschitz bounds and dyadic coverage; it shares the 24-sweep/4096-query recipe budget and 1025-query per-check cap", "right_rotation_world retains resolved pivot/axis; query remains initial pose, path_uncertainty_mm includes initial placement plus pivot/axis arithmetic and top-level interval bounds the full-path minimum"]);
    let rotation_path = object_spec(
        "clearance_rotation_path",
        vec![
            arg("pivot", numbers(3)),
            arg("axis", numbers(3)),
            arg("angle_radians", T::Number),
        ],
    );
    let mut datum_dimension = object_spec(
        "datum_dimension",
        vec![
            arg("id", T::String),
            arg("output", T::String),
            arg("from", object("section_anchor")),
            arg("to", object("section_anchor")),
            optional(arg("direction", T::Integer), json!(1)),
            optional_null("min", T::Number),
            optional_null("max", T::Number),
            optional_null("tolerance", object("dimension_tolerance")),
        ],
    );
    datum_dimension.obligations.extend(["schema 2 datum_dimensions; at most 128 globally unique portable IDs; bounded 10000000 aggregate path-component/byte comparison and native geometry work; exact leaf paths only",
        "Starlark datum_dimension(id,output,from_instance_path,from_datum,to_instance_path,to_datum,direction=1,min=None,max=None,tolerance=None)",
        "both datums must resolve uniquely to current native planar faces with zero authored offset; transformed native support-plane origins and normals are measured, not section projections or display geometry",
        "direction must be exactly -1 or 1; signed dot(to_origin-from_origin,direction*from_normal), no absolute value; nonpositive or unresolved measurements fail; normals must be arithmetic-parallel or opposed",
        "report retains native and world support planes, conservative native extents, semantic face selections, datum and occurrence frames, part hashes, value and explicit arithmetic bounds including admitted angular departure over native extents; global selected-output and warm-cache obligation",
        "native measurement bounds must entirely satisfy min/max without tolerance slack; authored manufacturing tolerance is separately nominal-checked and is not measurement arithmetic uncertainty"]);
    let mut nominal_seating = object_spec(
        "nominal_seating_check",
        vec![
            arg("id", T::String),
            arg("output", T::String),
            arg("left", object("section_anchor")),
            arg("right", object("section_anchor")),
            arg("min_shared_disk_radius_mm", T::Number),
        ],
    );
    nominal_seating.obligations.extend([
        "schema 2; at most 32 checks with unique portable IDs, an assembly output, distinct exact leaf paths and zero-offset native planar face datums",
        "Starlark nominal_seating_check(id,output,left_instance_path,left_datum,right_instance_path,right_datum,min_shared_disk_radius_mm)",
        "world-transformed canonical full-turn revolution annulus or convex polygon prism cap with separated circular throughbores; opposed supports coincident within bounded arithmetic error and a positive material disk common to their projected trims",
        "kernel arithmetic gap and normal guards, plus separate occurrence-placement gap and disk-radius uncertainty; full disk-radius lower bound must meet the declared minimum",
        "global selected-output and warm-cache obligation; reports bind source paths, native face IDs, part hashes and placement frames",
        "local nominal planar boundary coincidence only; attached occurrence paths unsupported; no exact three-dimensional face contact, full-body nonpenetration, preload, threaded engagement, manufacturing tolerance, force or rated contact claim",
    ]);
    let mut stack_term = object_spec(
        "tolerance_stack_term",
        vec![
            optional(arg("view", T::String), json!("")),
            optional(arg("dimension", T::String), json!("")),
            optional_null("datum_dimension", T::String),
            arg("coefficient", T::Number),
        ],
    );
    stack_term.obligations.extend(["exactly one reference form: nonempty view and dimension, or datum_dimension ID; referenced native dimension requires authored tolerance; duplicate references rejected",
        "finite nonzero coefficient with absolute value <=1000000; negative coefficients reverse interval bounds"]);
    let mut stack = object_spec(
        "tolerance_stack",
        vec![
            arg("id", T::String),
            arg("terms", array(object("tolerance_stack_term"), None)),
            optional(
                arg(
                    "min",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
            optional(
                arg(
                    "max",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
        ],
    );
    stack.obligations.extend(["schema-2 recipe.tolerance_stacks; Starlark tolerance_stack(id,terms,min=None,max=None); strict objects and unknown-field rejection", "case-insensitive unique ASCII identifiers of length 1..128; 64 stacks, 1..32 terms each and 1024 terms total", "at least one finite required bound; signed min/max permitted and min<=max", "independent bounded interval arithmetic: recompute nominal plus signed deviations with outward rounding, outward multiply and accumulate; never reuse rounded absolute report limits as exact inputs", "all resulting bounds must fit required envelope without modeling-tolerance slack; overflow/nonfinite intermediates fail explicitly; global obligation on cache hits and selected builds", "source owns relationships; not root-sum-square statistics, correlation inference, GD&T, fit classes or geometric collision certification", "reports contain declaration, passed status, nominal_mm, lower_limit_mm, upper_limit_mm and native source/tolerance witnesses for every term"]);
    stack.obligations.extend(["datum-only stacks propagate outward native measurement intervals separately from authored design intervals; both must satisfy required bounds, measurement_interval_certified=true", "section-only stacks preserve prior design-interval checks; measured fields represent outward scalar arithmetic only, measurement_interval_certified=false; mixed datum/section stacks are unsupported"]);
    let mut dimensional_tolerance = object_spec(
        "dimension_tolerance",
        vec![
            arg("nominal", T::Number),
            arg("lower", T::Number),
            arg("upper", T::Number),
        ],
    );
    for a in &mut dimensional_tolerance.arguments {
        a.units = Some("mm");
    }
    dimensional_tolerance.obligations.extend(["authored manufacturing limits, not measured manufactured conformance; nominal is the native design dimension", "finite positive nominal; finite signed lower < upper with finite positive width; nominal+lower and nominal+upper must be finite, positive and strictly distinct after arithmetic; deviation width and resulting limit width exceed 128 machine epsilons times maximum operand/limit magnitude", "both deviations may have the same sign; no named fit class or GD&T interpretation", "native measured value must match nominal within model linear tolerance, independently of manufacturing interval and separate min/max design assertions", "report tolerance contains nominal_mm, lower_deviation_mm, upper_deviation_mm, lower_limit_mm and upper_limit_mm"]);
    for spec in [&mut dimension, &mut circle_dimension] {
        spec.arguments.push(optional(
            arg(
                "tolerance",
                T::Nullable {
                    value: Box::new(object("dimension_tolerance")),
                },
            ),
            json!(null),
        ));
        spec.obligations.push("optional tolerance dict (or None in Starlark) is a global nominal-match obligation on selected and cached builds; omitted/null leaves existing semantics unchanged");
    }
    let mut section = object_spec(
        "section_view",
        vec![
            arg("id", T::String),
            arg("output", reference("output")),
            arg("origin", numbers(3)),
            optional(
                arg(
                    "circle_dimensions",
                    array(object("section_circle_dimension"), None),
                ),
                json!([]),
            ),
            optional(
                arg("dimensions", array(object("section_dimension"), None)),
                json!([]),
            ),
            optional(
                arg(
                    "anchor",
                    T::Nullable {
                        value: Box::new(object("section_anchor")),
                    },
                ),
                json!(null),
            ),
            optional(arg("normal", numbers(3)), json!([0, 0, 1])),
            optional(arg("x_direction", numbers(3)), json!([1, 0, 0])),
            optional(
                arg(
                    "min_area",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
            optional(
                arg(
                    "max_area",
                    T::Nullable {
                        value: Box::new(T::Number),
                    },
                ),
                json!(null),
            ),
        ],
    );
    section.obligations.extend(["recipe-level declaration in recipe.section_views, authored by Starlark section_view; not a feature operation", "native query domain query.planar_section; examples/countersink.star and examples/countersunk_bearing_cover.star", "schema 2; at most 128 declarations; IDs are case-insensitive unique ASCII letters/digits/underscore/hyphen, length 1..128", "finite unit orthogonal section frame; origin in mm; area bounds in mm2 finite nonnegative and ordered", "global native validation even on selected builds and cache hits; all declared targets demanded; failed section or bound blocks publication", "10,000,000 aggregate topology visits across all views, charging every occurrence; assembly area is a sum, not union"]);
    let mut paper_annotation_label = object_spec(
        "drawing_annotation_label",
        vec![arg("dimension", T::String), arg("tag", T::String)],
    );
    paper_annotation_label.obligations.extend(["exact section dimension or circle dimension ID on the referenced view", "tag is 1..12 ASCII letters/digits/underscore/hyphen, starts with a letter; unique case-insensitively per styled viewport", "tag is a short identifier only; renderer appends native measured value and kind"]);
    let mut viewport = object_spec(
        "drawing_viewport",
        vec![
            arg(
                "view",
                T::Reference {
                    target: "section_view",
                },
            ),
            arg("x", T::Number),
            arg("y", T::Number),
            arg("width", T::Number),
            arg("height", T::Number),
            arg("scale", T::Number),
            optional_null("paper_annotation_font_mm", T::Number),
            optional_null("paper_caption", T::String),
            optional(
                arg(
                    "annotation_labels",
                    array(object("drawing_annotation_label"), None),
                ),
                json!([]),
            ),
        ],
    );
    for field in &mut viewport.arguments[1..5] {
        field.units = Some("paper mm");
    }
    viewport.arguments[5].units = Some("paper mm / model mm");
    viewport.arguments[6].units = Some("paper mm");
    viewport.obligations.extend(["optional styled view fixes paper annotation font at 2.5..4 mm; all native section dimensions require one short tag, and optional caption is ≤64 UTF-8 bytes", "without style the legacy section SVG rendering is preserved; styled font/caption cannot silently auto-shrink or clip", "top-left paper origin; X right, Y down; finite coordinates and strictly positive sizes and explicit scale", "viewport wholly within sheet's 10 mm margins, additionally reserving 25 mm at bottom for title block; touching boundaries allowed, overlapping interiors rejected", "exact section_view ID, unique within sheet; repeated section across different sheets allowed"]);
    let mut dimension_row = object_spec(
        "drawing_dimension_row",
        vec![arg("dimension", T::String), arg("label", T::String)],
    );
    dimension_row.obligations.extend(["exact native datum_dimension ID on same table output; no source-authored numeric value", "nonempty trimmed label without controls, at most 96 UTF-8 bytes; each dimension once per table"]);
    let mut dimension_table = object_spec(
        "drawing_dimension_table",
        vec![
            arg("id", T::String),
            arg("title", T::String),
            arg("output", T::String),
            optional(arg("show_size_limits", T::Boolean), json!(false)),
            arg("x", T::Number),
            arg("y", T::Number),
            arg("width", T::Number),
            arg("height", T::Number),
            arg("rows", array(object("drawing_dimension_row"), None)),
        ],
    );
    for field in &mut dimension_table.arguments[4..8] {
        field.units = Some("paper mm");
    }
    dimension_table.obligations.extend(["at most 8 tables per sheet, 1..16 rows per table and 128 rows per recipe; unique portable table IDs per sheet", "rectangles lie within printable margins and do not overlap section viewports or other tables; minimum 80 mm width and 10+8 mm per row height", "output must be declared assembly output and equal every referenced native datum_dimension output; values and witnesses come only from evaluated native reports", "show_size_limits=true adds authored size-limit cells from each native datum dimension tolerance report; false preserves the nominal-only layout; three-column tables need width at least 191 mm, height at least 15+8 mm per row, and labels fitting 85 mm"]);
    let mut sheet = object_spec(
        "drawing_sheet",
        vec![
            arg("id", T::String),
            arg("title", T::String),
            arg("revision", T::String),
            arg("width", T::Number),
            arg("height", T::Number),
            arg("views", array(object("drawing_viewport"), None)),
            optional(
                arg(
                    "dimension_tables",
                    array(object("drawing_dimension_table"), None),
                ),
                json!([]),
            ),
        ],
    );
    sheet.arguments[3].units = Some("paper mm");
    sheet.arguments[4].units = Some("paper mm");
    sheet.obligations.extend(["recipe.drawing_sheets requires schema 2 even when empty; null and unknown fields rejected", "case-insensitive unique IDs: 1..128 ASCII letters/digits/underscore/hyphen, excluding reserved Windows device names", "nonempty trimmed title/revision without controls, maximum 256/64 UTF-8 bytes; finite positive paper dimensions at most 2000 mm", "at most 16 sheets, 1..32 viewports each, 256 total; all section references and layout validated before geometry, even for selected outputs", "Starlark drawing_sheet(id,title,revision,width,height,views,dimension_tables=None) references already declared section_view IDs; each viewport dictionary requires view,x,y,width,height,scale; table rows reference native datum_dimension IDs on the same declared assembly output", "report status layout_validated certifies layout and references only; geometry/annotation scale fit remains pending publication; report retains declaration and sorted unique section_view_ids/datum_dimension_ids; native witnesses remain in section_views and datum_dimensions; no projected hidden-line views or GD&T support implied"]);
    vec![
        sheet,
        viewport,
        paper_annotation_label,
        dimension_table,
        dimension_row,
        dimensional_tolerance,
        stack,
        stack_term,
        datum_dimension,
        nominal_seating,
        clearance,
        rotation_path,
        rotation_group,
        translation_group,
        cardinality,
        selector,
        assertion,
        property,
        datum,
        exchange,
        section,
        anchor,
        dimension,
        circle_dimension,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn property_names_and_nullable_bounds_match_native_authoring_types() {
        let specs = objects();
        let property = specs.iter().find(|s| s.id == "property_assertion").unwrap();
        let ArgumentType::Enum { values } = &property
            .arguments
            .iter()
            .find(|a| a.name == "property")
            .unwrap()
            .value_type
        else {
            panic!("property enum")
        };
        assert_eq!(values.len(), 20);
        for name in values {
            let parsed: crate::EngineeringProperty = serde_json::from_value(json!(name)).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), json!(name));
            let assertion: crate::PropertyAssertion = serde_json::from_value(
                json!({"id":"check","output":"part","property":name,"min":null,"max":1}),
            )
            .unwrap();
            assert_eq!(assertion.min, None);
            assert_eq!(assertion.max, Some(1.));
        }
        let datum:crate::FaceDatumDefinition=serde_json::from_value(json!({"part":"p","name":"top","anchor_feature":"f","role":"end_cap","x_direction":[1,0,0]})).unwrap();
        assert_eq!(datum.offset, [0.; 3]);
        let all: crate::FaceCardinality = serde_json::from_value(json!({"kind":"all"})).unwrap();
        assert_eq!(all, crate::FaceCardinality::All);
    }
}
