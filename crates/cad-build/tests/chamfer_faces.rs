use cad_build::*;
use serde_json::{json, Value};
fn source() -> Value {
    json!({"schema_version":2,"units":"mm","features":[
 {"op":"rectangle","id":"p","width":20,"depth":12},
 {"op":"extrude","id":"stock","profile":"p","height":10},
 {"op":"chamfer_faces","id":"bevel","body":"stock","faces":[{"from_feature":"stock","role":"wall_x_max"},{"from_feature":"stock","role":"end_cap"}],"distance":2}
 ],"outputs":[{"name":"stock","feature":"stock"},{"name":"finished","feature":"bevel"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn semantic_chamfer_chains_roles_through_rotations_and_reuses_memory_and_disk_cache() {
    let dir = std::env::temp_dir().join(format!("cad-semantic-chamfer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut v = source();
    v["features"].as_array_mut().unwrap().push(json!({"op":"chamfer_faces","id":"second","body":"bevel","faces":[{"from_feature":"bevel","role":"chamfer"},{"from_feature":"stock","role":"wall_y_max"}],"distance":0.25}));
    v["outputs"][1]["feature"] = json!("second");
    let r = recipe(v.clone());
    let mut engine = BuildEngine::with_cache_dir(&dir);
    let first = engine.build(&r).unwrap();
    let selector = FaceSelector {
        anchor_feature: "bevel".into(),
        role: "chamfer".into(),
        target_feature: "second".into(),
        cardinality: FaceCardinality::Exactly { count: 1 },
    };
    assert_eq!(
        first.resolve_faces(&selector).status,
        FaceResolutionStatus::Resolved
    );
    assert_eq!(engine.build(&r).unwrap().stats.computed_solids, 0);
    let disk = BuildEngine::with_cache_dir(&dir).build(&r).unwrap();
    assert_eq!(disk.stats.computed_solids, 0);
    assert!(disk.stats.disk_cache_hits > 0);
    assert_eq!(
        disk.outputs["finished"].content_hash,
        first.outputs["finished"].content_hash
    );
    v["features"][2]["distance"] = json!(1.5);
    let changed = engine.build(&recipe(v)).unwrap();
    assert_eq!(changed.stats.computed_solids, 2);
    assert_eq!(
        changed.outputs["stock"].content_hash,
        first.outputs["stock"].content_hash
    );
    let mut rotated = source();
    rotated["features"].as_array_mut().unwrap().insert(2,json!({"op":"transform","id":"placed","body":"stock","offset":[7,-4,2],"rotation":[0.5,0.5,0,std::f64::consts::FRAC_1_SQRT_2]}));
    rotated["features"][3]["body"] = json!("placed");
    let moved = build(&recipe(rotated)).unwrap();
    assert!((moved.outputs["finished"].result.model.volume() - 2376.).abs() < 1e-7);
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn warm_geometry_alias_cannot_substitute_for_semantic_ancestry() {
    let mut v = source();
    v["features"].as_array_mut().unwrap().insert(
        2,
        json!({"op":"extrude","id":"alias","profile":"p","height":10}),
    );
    let dir = std::env::temp_dir().join(format!("cad-semantic-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut engine = BuildEngine::with_cache_dir(&dir);
    engine.build(&recipe(v.clone())).unwrap();
    v["features"][3]["body"] = json!("alias");
    let invalid = recipe(v);
    let error = engine.build(&invalid).unwrap_err().to_string();
    assert!(error.contains("ancestor"), "{error}");
    let error = BuildEngine::with_cache_dir(&dir)
        .build(&invalid)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ancestor"), "{error}");
    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn deletion_splits_nonplanar_faces_and_structural_cycles_are_rejected() {
    for (width, x, z, role) in [(2., 9., -1., "end_cap"), (22., -1., 5., "end_cap")] {
        let mut v = source();
        v["features"].as_array_mut().unwrap().splice(
            2..2,
            [
                json!({"op":"rectangle","id":"tp","width":width,"depth":14}),
                json!({"op":"extrude","id":"tool","profile":"tp","height":12}),
                json!({"op":"translate","id":"placed","body":"tool","offset":[x,-1,z]}),
                json!({"op":"cut","id":"cut","left":"stock","right":"placed"}),
            ],
        );
        v["features"][6]["body"] = json!("cut");
        v["features"][6]["faces"][0]["role"] = json!(role);
        let error = build(&recipe(v)).unwrap_err().to_string();
        assert!(
            error.contains("Ambiguous") || error.contains("Deleted"),
            "{error}"
        );
    }
    let mut v = source();
    v["features"][0] = json!({"op":"circle","id":"p","radius":10});
    v["features"][2]["faces"][0]["role"] = json!("wall");
    assert!(build(&recipe(v))
        .unwrap_err()
        .to_string()
        .contains("not planar"));
    let mut v = source();
    v["features"][2]["faces"][0]["from_feature"] = json!("bevel");
    assert!(build(&recipe(v)).is_err());
    let mut v = source();
    v["features"][2]["faces"][0]["from_feature"] = json!("missing");
    assert!(build(&recipe(v)).is_err());
    let mut v = source();
    v["features"][2]["faces"][0]["role"] = json!("bad_role");
    v["face_assertions"] = json!([{"id":"global","selector":{"anchor_feature":"bevel","role":"chamfer","target_feature":"bevel","cardinality":{"kind":"exactly","count":1}}}]);
    assert!(BuildEngine::default()
        .build_outputs(&recipe(v), &["stock".into()])
        .is_err());
}
#[test]
fn semantic_face_ir_is_strict_and_order_independent() {
    let v = source();
    let r = recipe(v.clone());
    let encoded = serde_json::to_string(&r).unwrap();
    assert_eq!(
        encoded,
        serde_json::to_string(&serde_json::from_str::<Recipe>(&encoded).unwrap()).unwrap()
    );
    let mut reordered = v.clone();
    reordered["features"].as_array_mut().unwrap().reverse();
    assert_eq!(
        build(&r).unwrap().outputs["finished"].content_hash,
        build(&recipe(reordered)).unwrap().outputs["finished"].content_hash
    );
    for face in [
        json!({"from_feature":"stock","role":"end_cap","face_index":1}),
        json!({"from_feature":"stock"}),
        json!({"from_feature":3,"role":"end_cap"}),
    ] {
        let mut invalid = v.clone();
        invalid["features"][2]["faces"][0] = face;
        assert!(serde_json::from_value::<Recipe>(invalid).is_err());
    }
    let mut invalid = v;
    invalid["features"][2]["faces"] = json!([]);
    assert!(serde_json::from_value::<Recipe>(invalid).is_err());
}
