const SOURCE: &str = include_str!("../../../examples/retained_shaft.star");
#[test]
fn retention_source_keeps_actual_stop_datums_clearances_and_replay() {
    let recipe = cad_recipe::from_starlark("retained_shaft.star", SOURCE).unwrap();
    let json = serde_json::to_string(&recipe).unwrap();
    let replay: cad_build::Recipe = serde_json::from_str(&json).unwrap();
    assert_eq!(json, serde_json::to_string(&replay).unwrap());
    let built = cad_build::build(&replay).unwrap();
    assert_eq!(built.datum_dimensions.len(), 4);
    let endplay = &built.tolerance_stacks["shaft_endplay"];
    assert!(endplay.measurement_interval_certified);
    assert!(endplay.measured_lower_bound_mm < 0.2 && endplay.measured_upper_bound_mm > 0.2);
    assert!(endplay.lower_limit_mm < 0.16 && endplay.upper_limit_mm > 0.24);
    assert!(endplay.terms.iter().all(|t| t.datum_dimension.is_some()
        && t.view.is_empty()
        && t.dimension.is_empty()
        && t.measurement_interval_certified));
    let stop = &built.face_datums["shaft"];
    assert_eq!(
        stop["shoulder"].selection.status,
        cad_build::FaceResolutionStatus::Resolved
    );
    assert_eq!(
        stop["loaded_groove_wall"].selection.status,
        cad_build::FaceResolutionStatus::Resolved
    );
    let stop_span =
        stop["loaded_groove_wall"].frame.translation[2] - stop["shoulder"].frame.translation[2];
    let washer = &built.face_datums["washer"];
    let ring = &built.face_datums["retaining_ring"];
    let thickness =
        |datums: &std::collections::BTreeMap<String, cad_build::FaceDatumResolution>| {
            datums["end"].frame.translation[2] - datums["start"].frame.translation[2]
        };
    let assembly = &built.assemblies["retained_shaft"];
    let outer_face = |id: &str| {
        let instance = assembly.instances.iter().find(|i| i.id == id).unwrap();
        let datum = assembly.parts[&instance.part].datums["outer_thrust"];
        instance.transform.point(datum.translation)[2]
    };
    let bush_span = outer_face("right_bush") - outer_face("left_bush");
    assert!((bush_span - 80.).abs() < 1e-9);
    assert!((stop_span - bush_span - thickness(washer) - thickness(ring) - 0.2).abs() < 1e-9);
    for (id, expected) in [
        ("left_bearing", 0.1),
        ("right_bearing", 0.15),
        ("washer_bearing", 0.1),
    ] {
        let q = &built.clearance_checks[id];
        assert!(
            q.lower_bound_mm < expected && q.upper_bound_mm > expected,
            "{id}: {q:?}"
        );
    }
}
#[test]
fn retention_selected_outputs_keep_global_checks_and_threshold_only_reuses_geometry() {
    let mut engine = cad_build::BuildEngine::default();
    let recipe = cad_recipe::from_starlark("retained_shaft.star", SOURCE).unwrap();
    let first = engine.build_outputs(&recipe, &["washer".into()]).unwrap();
    assert_eq!(first.clearance_checks.len(), 3);
    let source = SOURCE.replace(
        "required_clearance = param(name = \"required_clearance\", default = 0.09)",
        "required_clearance = param(name = \"required_clearance\", default = 0.11)",
    );
    let invalid = cad_recipe::from_starlark("fail.star", &source).unwrap();
    assert!(engine.build_outputs(&invalid, &["washer".into()]).is_err());
    let cached = engine.build_outputs(&recipe, &["washer".into()]).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.feature_hashes, first.feature_hashes);
}

#[test]
fn retained_source_directed_measurements_reject_reversed_order_and_invalid_direction() {
    let reversed = SOURCE.replace(
        "to_datum = \"loaded_groove_wall\", direction = 1",
        "to_datum = \"loaded_groove_wall\", direction = -1",
    );
    let r = cad_recipe::from_starlark("reversed.star", &reversed).unwrap();
    assert!(matches!(
        cad_build::build(&r),
        Err(cad_build::BuildError::DatumDimensionFailed { .. })
    ));
    let malformed = SOURCE.replace(
        "to_datum = \"loaded_groove_wall\", direction = 1",
        "to_datum = \"loaded_groove_wall\", direction = 0",
    );
    assert!(cad_recipe::from_starlark("invalid.star", &malformed).is_err());
}
