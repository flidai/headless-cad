use cad_build::*;
use serde_json::{json, Value};
fn source() -> Value {
    json!({"schema_version":2,"units":"mm","features":[
 {"op":"sketch","id":"layout","sketch":{"points":[{"id":"a","position":[0.2,-0.1]},{"id":"b","position":[9.5,0.2]},{"id":"c","position":[10.4,5.7]},{"id":"d","position":[-0.1,6.3]}],"constraints":[
 {"id":"anchor","kind":"fixed","point":"a","position":[0,0]},
 {"id":"bottom","kind":"horizontal","a":"a","b":"b"},
 {"id":"right","kind":"vertical","a":"b","b":"c"},
 {"id":"top","kind":"horizontal","a":"c","b":"d"},
 {"id":"left","kind":"vertical","a":"d","b":"a"},
 {"id":"width","kind":"distance","a":"a","b":"b","value":10},
 {"id":"depth","kind":"distance","a":"b","b":"c","value":6}
 ]},"loops":[["a","b","c","d"]]},
 {"op":"extrude","id":"body","profile":"layout","height":3}
 ],"outputs":[{"name":"part","feature":"body"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn solved_dimensions_reports_and_geometry_survive_memory_and_disk_caches() {
    let dir = tempfile::tempdir().unwrap();
    let r = recipe(source());
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let first = engine.build(&r).unwrap();
    assert!((first.outputs["part"].result.model.volume() - 180.).abs() < 1e-5);
    let report = &first.sketch_reports["layout"];
    assert!(report.is_fully_constrained());
    assert_eq!(report.degrees_of_freedom, 0);
    assert_eq!(report.solver_build_id, solver_build_id());
    assert!((report.points["b"][0] - 10.).abs() < 4e-7);
    assert!((report.points["c"][1] - 6.).abs() < 4e-7);
    let cached = engine.build(&r).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.stats.cache_hits, 1);
    assert_eq!(
        serde_json::to_value(&first.sketch_reports).unwrap(),
        serde_json::to_value(&cached.sketch_reports).unwrap()
    );
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    assert_eq!(disk.sketch_reports.len(), 1);
    let mut changed = source();
    changed["features"][0]["sketch"]["constraints"][5]["value"] = json!(14);
    let revised = engine.build(&recipe(changed)).unwrap();
    assert_ne!(
        first.feature_hashes["layout"],
        revised.feature_hashes["layout"]
    );
    assert_ne!(
        first.outputs["part"].content_hash,
        revised.outputs["part"].content_hash
    );
    assert!((revised.outputs["part"].result.model.volume() - 252.).abs() < 1e-5);
}
#[test]
fn selected_scope_solves_only_demanded_sketches_but_checks_all_structure() {
    let mut v = source();
    let mut free = v["features"][0].clone();
    free["id"] = json!("free");
    free["sketch"]["constraints"] = json!([]);
    v["features"].as_array_mut().unwrap().extend([
        free,
        json!({"op":"extrude","id":"free_body","profile":"free","height":3}),
    ]);
    v["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"free_part","feature":"free_body"}));
    let mut engine = BuildEngine::default();
    let built = engine
        .build_outputs(&recipe(v.clone()), &["part".into()])
        .unwrap();
    assert_eq!(built.sketch_reports.len(), 1);
    assert!(!built.feature_hashes.contains_key("free"));
    assert!(
        matches!(engine.build(&recipe(v.clone())),Err(BuildError::SketchSolveFailed{feature,report,..})if feature=="free"&&report.status==SolveStatus::Underconstrained)
    );
    v["features"][2]["loops"][0][0] = json!("missing");
    assert!(
        matches!(engine.build_outputs(&recipe(v),&["part".into()]),Err(BuildError::Sketch{feature,..})if feature=="free")
    );
}
#[test]
fn cache_does_not_bypass_changed_constraint_failure_or_profile_orientation() {
    let mut engine = BuildEngine::default();
    engine.build(&recipe(source())).unwrap();
    let mut v = source();
    v["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(matches!(
        engine.build(&recipe(v)),
        Err(BuildError::SketchSolveFailed { .. })
    ));
    let mut v = source();
    v["features"][0]["loops"] = json!([["d", "c", "b", "a"]]);
    assert!(matches!(
        engine.build(&recipe(v)),
        Err(BuildError::PolygonProfile { .. })
    ));
    let mut v = source();
    v["features"][0]["op_version"] = json!(2);
    assert!(matches!(
        engine.build(&recipe(v)),
        Err(BuildError::OperationVersion { .. })
    ));
}
#[test]
fn sketch_json_and_loop_references_are_strict() {
    let mut v = source();
    v["features"][0]["unknown"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut v = source();
    v["features"][0]["sketch"]["constraints"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
    let mut v = source();
    v["features"][0]["loops"] = json!([["a", "b", "c", "a"]]);
    assert!(matches!(build(&recipe(v)), Err(BuildError::Sketch { .. })));
    let mut v = source();
    v["features"][0]["loops"] = json!([]);
    assert!(matches!(build(&recipe(v)), Err(BuildError::Sketch { .. })));
    let mut v = source();
    v["features"][1] = json!({"op":"revolve","id":"body","profile":"layout"});
    assert!(matches!(
        build(&recipe(v)),
        Err(BuildError::InputType { .. })
    ));
    let text = serde_json::to_string(&source()).unwrap();
    let duplicate = text.replacen("\"loops\":", "\"loops\":[],\"loops\":", 1);
    assert!(serde_json::from_str::<Recipe>(&duplicate).is_err());
}
#[test]
fn aggregate_solver_work_is_reserved_before_any_solves() {
    let template = source();
    let mut features = vec![];
    let mut outputs = vec![];
    for i in 0..129 {
        let mut f = template["features"][0].clone();
        f["id"] = json!(format!("s{i}"));
        features.push(f);
        features.push(
            json!({"op":"extrude","id":format!("b{i}"),"profile":format!("s{i}"),"height":3}),
        );
        outputs.push(json!({"name":format!("o{i}"),"feature":format!("b{i}")}));
    }
    let r = recipe(json!({"schema_version":2,"units":"mm","features":features,"outputs":outputs}));
    assert!(matches!(
        build(&r),
        Err(BuildError::ResourceLimit {
            resource: "demanded sketches",
            limit: 128
        })
    ));
    assert_eq!(
        BuildEngine::default()
            .build_outputs(&r, &["o0".into()])
            .unwrap()
            .sketch_reports
            .len(),
        1
    );
}

#[test]
fn solved_clockwise_hole_retains_native_inner_boundaries() {
    let points = [
        ("a", [0., 0.]),
        ("b", [10., 0.]),
        ("c", [10., 6.]),
        ("d", [0., 6.]),
        ("e", [2., 2.]),
        ("f", [2., 4.]),
        ("g", [4., 4.]),
        ("h", [4., 2.]),
    ];
    let mut v = source();
    v["features"][0]["sketch"]["points"] = json!(points
        .iter()
        .map(|(id, p)| json!({"id":id,"position":p}))
        .collect::<Vec<_>>());
    v["features"][0]["sketch"]["constraints"] = json!(points
        .iter()
        .map(|(id, p)| json!({"id":format!("fix_{id}"),"kind":"fixed","point":id,"position":p}))
        .collect::<Vec<_>>());
    v["features"][0]["loops"] = json!([["a", "b", "c", "d"], ["e", "f", "g", "h"]]);
    let built = build(&recipe(v)).unwrap();
    let m = &built.outputs["part"].result.model;
    assert!((m.volume() - 168.).abs() < 1e-8);
    assert_eq!(m.faces.len(), 10);
    assert_eq!(m.faces[0].loops.len(), 2);
    assert_eq!(m.faces[1].loops.len(), 2);
    assert_eq!(built.sketch_reports["layout"].rank, 16);
}
