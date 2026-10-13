const SOURCE: &str = include_str!("../../../examples/nominal_seating.star");

#[test]
fn isolated_native_seating_replays_and_survives_selected_build() {
    let recipe = cad_recipe::from_starlark("nominal_seating.star", SOURCE).unwrap();
    let replay: cad_build::Recipe =
        serde_json::from_value(serde_json::to_value(&recipe).unwrap()).unwrap();
    assert_eq!(recipe.nominal_seating_checks.len(), 1);
    let mut engine = cad_build::BuildEngine::default();
    let built = engine
        .build_outputs(&replay, &["seated_pair".into()])
        .unwrap();
    let seat = &built.nominal_seating_checks["flange_plate_seat"];
    assert_eq!(seat.status, "passed");
    assert_eq!(seat.left.instance_path, ["flange"]);
    assert_eq!(seat.right.instance_path, ["plate"]);
    assert!(seat.signed_gap_lower_mm <= 0. && seat.signed_gap_upper_mm >= 0.);
    assert!(seat.disk_radius_lower_mm >= 0.2);
    assert!(seat.disk_area_lower_mm2 > 0.);
    assert_eq!(
        seat.query.left_face,
        seat.left.semantic_datum.selection.witnesses[0].target_face
    );
    assert_eq!(
        seat.query.right_face,
        seat.right.semantic_datum.selection.witnesses[0].target_face
    );
    let warm = engine
        .build_outputs(&replay, &["seated_pair".into()])
        .unwrap();
    assert!(warm.stats.cache_hits > 0);
    assert_eq!(
        warm.nominal_seating_checks["flange_plate_seat"].disk_radius_lower_mm,
        seat.disk_radius_lower_mm
    );
}

#[test]
fn resolved_axial_gap_and_penetration_both_fail() {
    for offset in ["-1.9", "-2.1"] {
        let source = SOURCE.replace("offset = [0,0,-2]", &format!("offset = [0,0,{offset}]"));
        let recipe = cad_recipe::from_starlark("nominal_seating.star", &source).unwrap();
        let mut engine = cad_build::BuildEngine::default();
        let error = engine
            .build_outputs(&recipe, &["plate_body".into()])
            .unwrap_err();
        assert!(
            matches!(
                error,
                cad_build::BuildError::NominalSeatingCheckFailed { .. }
            ),
            "{error}"
        );
        let warm_error = engine
            .build_outputs(&recipe, &["plate_body".into()])
            .unwrap_err();
        assert!(
            matches!(
                warm_error,
                cad_build::BuildError::NominalSeatingCheckFailed { .. }
            ),
            "{warm_error}"
        );
    }
}

#[test]
fn cancelled_large_hierarchy_offsets_exhaust_seating_precision() {
    let source = SOURCE
        .replace("instance(id = \"flange\", part = flange)",
            "instance(id = \"flange\", part = flange, offset = [100000000,0,0])")
        .replace("offset = [0,0,-2]", "offset = [100000000,0,-2]")
        .replace("assembly_output(name = \"seated_pair\", assembly = pair)",
            "outer = assembly(id = \"outer\", instances = [assembly_instance(id = \"nested\", assembly = pair, offset = [-100000000,0,0])])\nassembly_output(name = \"seated_pair\", assembly = outer)")
        .replace("[\"flange\"]", "[\"nested\",\"flange\"]")
        .replace("[\"plate\"]", "[\"nested\",\"plate\"]");
    let recipe = cad_recipe::from_starlark("nominal_seating.star", &source).unwrap();
    let error = cad_build::build(&recipe).unwrap_err();
    assert!(
        matches!(
            error,
            cad_build::BuildError::NominalSeatingCheckFailed { .. }
        ),
        "{error}"
    );
}

#[test]
fn strict_schema_and_bounded_source_declarations() {
    let recipe = cad_recipe::from_starlark("nominal_seating.star", SOURCE).unwrap();
    let mut value = serde_json::to_value(recipe).unwrap();
    value["schema_version"] = 1.into();
    assert!(serde_json::from_value::<cad_build::Recipe>(value.clone()).is_err());
    value["schema_version"] = 2.into();
    value["nominal_seating_checks"][0]["unexpected"] = true.into();
    assert!(serde_json::from_value::<cad_build::Recipe>(value).is_err());
}
