const SOURCE: &str = include_str!("../../../examples/rounded_bearing_cover.star");
#[test]
fn rounded_cover_source_preserves_four_native_blends_five_real_bores_and_replay() {
    let recipe = cad_recipe::from_starlark("rounded_bearing_cover.star", SOURCE).unwrap();
    assert_eq!(
        recipe
            .features
            .iter()
            .filter(|f| matches!(f, cad_build::Feature::Fillet { .. }))
            .count(),
        4
    );
    let json = serde_json::to_string(&recipe).unwrap();
    let replay: cad_build::Recipe = serde_json::from_str(&json).unwrap();
    let mut engine = cad_build::BuildEngine::default();
    let built = engine.build(&replay).unwrap();
    assert_eq!(built.face_assertions.len(), 4);
    for resolution in built.face_assertions.values() {
        assert_eq!(resolution.status, cad_build::FaceResolutionStatus::Resolved);
        assert_eq!(resolution.witnesses.len(), 1);
    }
    let expected = (2400. - (4. - std::f64::consts::PI) * 16. - 125. * std::f64::consts::PI) * 8.;
    assert!((built.outputs["bearing_cover"].result.model.volume() - expected).abs() < 1e-7);
    assert_eq!(engine.build(&replay).unwrap().stats.computed_solids, 0);
    let revision = SOURCE.replace("default = 4)", "default = 5)");
    let changed = engine
        .build(&cad_recipe::from_starlark("revision.star", &revision).unwrap())
        .unwrap();
    assert_ne!(
        changed.outputs["bearing_cover"].content_hash,
        built.outputs["bearing_cover"].content_hash
    );
}
#[test]
fn source_rejects_raw_handles_unknown_fields_and_invalid_radius() {
    for args in ["faces=[0,1],radius=1","faces=[{'from_feature':'b','role':'wall_x_min','face_id':0},{'from_feature':'b','role':'wall_y_min'}],radius=1","faces=[{'from_feature':'b','role':'wall_x_min'},{'from_feature':'b','role':'wall_y_min'}],radius=0"] {
  let source=format!("b=extrude(id='b',profile=rectangle(id='p',width=20,depth=12),height=10)\nf=fillet(id='f',body=b,{args})\noutput(name='body',feature=f)\n");
  let parsed=cad_recipe::from_starlark("invalid.star",&source);assert!(parsed.is_err()||cad_build::build(&parsed.unwrap()).is_err());
 }
}
