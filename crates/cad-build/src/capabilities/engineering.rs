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
    vec![cardinality, selector, assertion, property, datum, exchange]
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
