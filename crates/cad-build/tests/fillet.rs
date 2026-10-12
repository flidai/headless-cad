use cad_build::{BuildEngine, Recipe};
use serde_json::{json, Value};
fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":20,"depth":12},{"op":"extrude","id":"stock","profile":"p","height":10},{"op":"fillet","id":"round","body":"stock","faces":[{"from_feature":"stock","role":"wall_x_min"},{"from_feature":"stock","role":"wall_y_min"}],"radius":2}],"outputs":[{"name":"stock","feature":"stock"},{"name":"finished","feature":"round"}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn semantic_fillet_measures_actual_removed_material_and_tracks_cache_binding() {
    let mut recipe = parse(value());
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let a = engine.build(&recipe).unwrap();
    let m = &a.outputs["finished"].result.model;
    let expected = 2400. - (4. - std::f64::consts::PI) * 10.;
    assert!((m.volume() - expected).abs() < 1e-8);
    assert_eq!(engine.build(&recipe).unwrap().stats.computed_solids, 0);
    assert_eq!(
        BuildEngine::with_cache_dir(dir.path())
            .build(&recipe)
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    if let cad_build::Feature::Fillet { radius, .. } = &mut recipe.features[2] {
        *radius = 3.;
    }
    let b = engine.build(&recipe).unwrap();
    assert_eq!(b.stats.computed_solids, 1);
    assert_eq!(
        a.outputs["stock"].content_hash,
        b.outputs["stock"].content_hash
    );
}
#[test]
fn semantic_alias_and_invalid_edge_domains_never_reuse_a_warm_geometry_result() {
    let mut v = value();
    v["features"].as_array_mut().unwrap().insert(
        2,
        json!({"op":"extrude","id":"alias","profile":"p","height":10}),
    );
    let mut engine = BuildEngine::default();
    engine.build(&parse(v.clone())).unwrap();
    v["features"][3]["body"] = json!("alias");
    assert!(engine
        .build(&parse(v))
        .unwrap_err()
        .to_string()
        .contains("ancestor"));
    for role in ["end_cap", "wall_x_min", "missing"] {
        let mut v = value();
        v["features"][2]["faces"][1]["role"] = json!(role);
        assert!(cad_build::build(&parse(v)).is_err());
    }
    let mut v = value();
    v["features"][2]["radius"] = json!(20);
    assert!(cad_build::build(&parse(v)).is_err());
    let mut v = value();
    v["features"][2]["edge_id"] = json!(0);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
}
#[test]
fn semantic_corner_is_rigid_pose_invariant_and_warm_edits_recheck_radius() {
    let mut v = value();
    v["features"].as_array_mut().unwrap().insert(2,json!({"op":"transform","id":"posed","body":"stock","offset":[7,-4,2],"rotation":[0.5,0.5,0.5,0.5]}));
    v["features"][3]["body"] = json!("posed");
    let mut recipe = parse(v);
    let mut engine = BuildEngine::default();
    let a = engine.build(&recipe).unwrap();
    let expected = 2400. - (4. - std::f64::consts::PI) * 10.;
    assert!((a.outputs["finished"].result.model.volume() - expected).abs() < 1e-8);
    if let cad_build::Feature::Fillet { radius, .. } = &mut recipe.features[3] {
        *radius = 12.;
    }
    // A selected stock output does not demand unrelated optional modeling features;
    // selecting the edited body must still refuse the topology event on a warm cache.
    assert!(engine.build_outputs(&recipe, &["finished".into()]).is_err());
}
