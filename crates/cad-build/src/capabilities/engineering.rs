//! Authoring shapes for revision-bound engineering obligations.
use super::*;

pub(super) fn objects() -> Vec<ObjectSpec> {
    use ArgumentType as T;
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
    dimension.obligations.extend(["projected datum-origin distance along resolved section X or Y, not 3D distance; endpoints need not lie in the plane", "value_mm is absolute axis delta; signed_delta_mm and projected endpoints retained; value <= modeling tolerance fails as degenerate", "offset is an absolute dimension-line coordinate on the perpendicular drawing axis in mm, not a measurement adjustment", "IDs case-insensitive unique within view; same safe identifier rules as section_view; finite offset and finite nonnegative ordered bounds", "Starlark section_dimension(view,id,from_instance_path,from_datum,to_instance_path,to_datum,offset,axis='x',min=None,max=None) attaches to an already declared assembly section", "at most 32 per view and 1024 per recipe; all measurements are global native obligations including cache hits and selected builds; datum/face/placement witnesses retained"]);
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
    circle_dimension.obligations.extend(["Starlark section_circle_dimension attaches to an already declared view; from_feature maps to anchor_feature; default kind diameter", "Exactly1 semantic face lineage selection in the owning body or exact assembly leaf occurrence; path absent for body and required for assembly", "only one complete native circular section on the selected face; consistent center/radius/orientation, continuous full turn; no lines, partial arcs, nearest-face guesses or mesh fitting", "label_offset is relative to circle center in drawing mm and must place label strictly outside circle; bounds inclusive native radius/diameter in mm", "shares IDs and 32-perview/1024-global limits with projected dimensions, and shared face-assertion traversal budget; all source views validate globally on cache hits and selected builds"]);
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
    vec![
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
