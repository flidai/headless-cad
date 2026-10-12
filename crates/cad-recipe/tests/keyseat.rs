const SOURCE: &str = include_str!("../../../examples/keyed_shaft.star");
#[test]
fn keyed_shaft_source_roundtrips_with_native_cut_and_global_lineage_obligations() {
    let recipe = cad_recipe::from_starlark("keyed_shaft.star", SOURCE).unwrap();
    let encoded = serde_json::to_string(&recipe).unwrap();
    let decoded: cad_build::Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let mut engine = cad_build::BuildEngine::default();
    let b = engine.build_outputs(&decoded, &["stock".into()]).unwrap();
    assert_eq!(b.face_assertions.len(), 5);
    assert_eq!(b.property_assertions.len(), 1);
    assert!(b
        .face_assertions
        .values()
        .all(|r| r.status == cad_build::FaceResolutionStatus::Resolved));
    assert!(!b.outputs.contains_key("keyed_shaft"));
    assert_eq!(
        engine
            .build_outputs(&decoded, &["stock".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let changed = SOURCE.replace(
        "key_depth = param(name = \"key_depth\", default = 2)",
        "key_depth = param(name = \"key_depth\", default = 6)",
    );
    let invalid = cad_recipe::from_starlark("invalid.star", &changed).unwrap();
    assert!(engine.build_outputs(&invalid, &["stock".into()]).is_err());
}
#[test]
fn source_pose_preserves_keyseat_volume_and_semantic_faces() {
    let original =
        cad_build::build(&cad_recipe::from_starlark("base.star", SOURCE).unwrap()).unwrap();
    for pose in [1, 2] {
        let posed = SOURCE.replace(
            "pose = param(name = \"pose\", default = 0)",
            &format!("pose = param(name = \"pose\", default = {pose})"),
        );
        let b =
            cad_build::build(&cad_recipe::from_starlark("posed.star", &posed).unwrap()).unwrap();
        assert!(
            (b.outputs["keyed_shaft"].result.model.volume()
                - original.outputs["keyed_shaft"].result.model.volume())
            .abs()
                < 1e-7
        );
        assert!(b
            .face_assertions
            .values()
            .all(|r| r.status == cad_build::FaceResolutionStatus::Resolved));
    }
}
