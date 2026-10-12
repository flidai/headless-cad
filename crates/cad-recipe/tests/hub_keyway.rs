const SOURCE: &str = include_str!("../../../examples/keyed_hub.star");
#[test]
fn hub_source_roundtrips_and_selected_stock_enforces_global_shape_obligations() {
    let recipe = cad_recipe::from_starlark("keyed_hub.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&recipe).unwrap();
    let decoded: cad_build::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let mut engine = cad_build::BuildEngine::default();
    let b = engine.build_outputs(&decoded, &["stock".into()]).unwrap();
    assert_eq!(b.face_assertions.len(), 6);
    assert_eq!(b.property_assertions.len(), 1);
    assert!(b
        .face_assertions
        .values()
        .all(|r| r.status == cad_build::FaceResolutionStatus::Resolved));
    assert!(!b.outputs.contains_key("keyed_hub"));
    assert_eq!(
        engine
            .build_outputs(&decoded, &["stock".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let changed = SOURCE.replace(
        "tool_margin = param(name = \"tool_margin\", default = 1)",
        "tool_margin = param(name = \"tool_margin\", default = -1)",
    );
    let invalid = cad_recipe::from_starlark("invalid.star", &changed).unwrap();
    assert!(engine.build_outputs(&invalid, &["stock".into()]).is_err());
}
#[test]
fn common_operand_poses_and_scales_preserve_native_keyway_semantics() {
    let original =
        cad_build::build(&cad_recipe::from_starlark("base.star", SOURCE).unwrap()).unwrap();
    let original_volume = original.outputs["keyed_hub"].result.model.volume();
    for (pose, scale) in [(1, 1.), (2, 1.), (2, 0.001), (2, 100.)] {
        let source = SOURCE
            .replace(
                "pose = param(name = \"pose\", default = 0)",
                &format!("pose = param(name = \"pose\", default = {pose})"),
            )
            .replace(
                "scale = param(name = \"scale\", default = 1)",
                &format!("scale = param(name = \"scale\", default = {scale})"),
            );
        let b = cad_build::build(&cad_recipe::from_starlark("posed.star", &source).unwrap())
            .unwrap_or_else(|error| panic!("pose={pose} scale={scale}: {error}"));
        let expected = original_volume * scale * scale * scale;
        assert!((b.outputs["keyed_hub"].result.model.volume() - expected).abs() < expected * 1e-10);
        assert!(b
            .face_assertions
            .values()
            .all(|r| r.status == cad_build::FaceResolutionStatus::Resolved));
    }
}
