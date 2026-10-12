const SOURCE: &str = include_str!("../../../examples/keyed_clearance.star");
fn source(pose: usize, turn: usize, scale: f64) -> String {
    SOURCE
        .replace(
            "pose = param(name = \"pose\", default = 0)",
            &format!("pose = param(name = \"pose\", default = {pose})"),
        )
        .replace(
            "hub_turn = param(name = \"hub_turn\", default = 0)",
            &format!("hub_turn = param(name = \"hub_turn\", default = {turn})"),
        )
        .replace(
            "scale = param(name = \"scale\", default = 1)",
            &format!("scale = param(name = \"scale\", default = {scale})"),
        )
}
#[test]
fn source_native_clearance_preserves_rotated_keyways_and_common_poses() {
    for (pose, turn, scale) in [
        (0, 0, 1.),
        (1, 1, 1.),
        (2, 2, 1.),
        (2, 1, 0.001),
        (0, 2, 2.),
    ] {
        let recipe =
            cad_recipe::from_starlark("keyed_clearance.star", &source(pose, turn, scale)).unwrap();
        let built = cad_build::build(&recipe)
            .unwrap_or_else(|e| panic!("pose={pose},turn={turn},scale={scale}: {e}"));
        assert_eq!(built.clearance_checks.len(), 5);
        for (id, report) in &built.clearance_checks {
            let expected = if id == "axial_gap" {
                2. * scale
            } else {
                0.15 * scale
            };
            assert!(
                report.lower_bound_mm < expected && report.upper_bound_mm > expected,
                "{id}: {report:?}"
            );
            assert_eq!(
                serde_json::to_value(report.query.method).unwrap(),
                serde_json::json!("native_trim_witness")
            );
        }
    }
}
#[test]
fn source_roundtrip_global_failure_and_cached_threshold_edit() {
    let recipe = cad_recipe::from_starlark("keyed_clearance.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&recipe).unwrap();
    let decoded: cad_build::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let mut engine = cad_build::BuildEngine::default();
    engine.build_outputs(&decoded, &["sleeve".into()]).unwrap();
    let edited = SOURCE.replace(
        "required_clearance = param(name = \"required_clearance\", default = 0.1)",
        "required_clearance = param(name = \"required_clearance\", default = 0.2)",
    );
    let failed = cad_recipe::from_starlark("fail.star", &edited).unwrap();
    assert!(engine.build_outputs(&failed, &["sleeve".into()]).is_err());
    assert_eq!(
        engine
            .build_outputs(&decoded, &["sleeve".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}

#[test]
fn separated_fixture_at_extreme_scale_explicitly_exhausts_placement_precision() {
    let recipe = cad_recipe::from_starlark("large.star", &source(0, 2, 100.)).unwrap();
    let error = cad_build::build(&recipe).unwrap_err().to_string();
    assert!(
        error.contains("numeric range") || error.contains("precision"),
        "{error}"
    );
}
