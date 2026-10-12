use cad_build::*;
use serde_json::{json, Value};
fn source(angle: f64) -> Value {
    json!({"schema_version":2,"units":"mm","features":[
 {"op":"sketch","id":"layout","sketch":{"points":[{"id":"a","position":[0,0]},{"id":"b","position":[20.5,0]},{"id":"c","position":[10.3,6]}],"constraints":[
 {"id":"anchor_a","kind":"fixed","point":"a","position":[0,0]},
 {"id":"anchor_b","kind":"fixed","point":"b","position":[20.5,0]},
 {"id":"equal_legs","kind":"equal_length","a":["a","c"],"b":["b","c"]},
 {"id":"slope","kind":"angle","a":["a","b"],"b":["a","c"],"degrees":angle}
 ]},"loops":[["a","b","c"]]},
 {"op":"extrude","id":"body","profile":"layout","height":3.25}
 ],"outputs":[{"name":"part","feature":"body"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
fn check(build: &BuildResult, angle: f64) {
    let report = &build.sketch_reports["layout"];
    assert!(report.is_fully_constrained());
    assert_eq!(report.schema_version, 3);
    assert_eq!(report.rank, 6);
    assert_eq!(report.degrees_of_freedom, 0);
    assert!(report.max_residual_mm <= report.tolerance_mm);
    assert!(report.max_angular_residual_radians <= report.angular_tolerance_radians);
    let x = 20.5 / 2.;
    let y = x * angle.to_radians().tan();
    let p = report.points["c"];
    assert!((p[0] - x).abs() < 1e-6);
    assert!((p[1] - y).abs() < 1e-6);
    let properties = build.outputs["part"]
        .result
        .model
        .mass_properties(cad_kernel::Tolerance::default())
        .unwrap();
    let volume = 20.5 * y / 2. * 3.25;
    assert!((properties.volume - volume).abs() < 1e-4);
    assert!((properties.centroid.unwrap()[1] - y / 3.).abs() < 1e-6);
    let evidence = serde_json::to_value(report).unwrap();
    for row in evidence["constraint_residuals"].as_array().unwrap() {
        if row["id"] == "slope" {
            assert!(row.get("residual_mm").is_none());
            assert!(row["residual_radians"].is_number());
        } else {
            assert!(row["residual_mm"].is_number());
            assert!(row.get("residual_radians").is_none());
        }
    }
    for residual in &report.constraint_residuals {
        if residual.id == "slope" {
            assert!(residual.residual_mm.is_none());
            assert!(residual.residual_radians.unwrap() <= 1e-9);
        } else {
            assert!(residual.residual_mm.unwrap() <= report.tolerance_mm);
            assert!(residual.residual_radians.is_none());
        }
    }
}
#[test]
fn angular_edits_change_native_geometry_and_cached_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let r = recipe(source(30.25));
    let first = engine.build(&r).unwrap();
    check(&first, 30.25);
    let cached = engine.build(&r).unwrap();
    check(&cached, 30.25);
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.stats.cache_hits, 1);
    assert_eq!(
        serde_json::to_value(&first.sketch_reports).unwrap(),
        serde_json::to_value(&cached.sketch_reports).unwrap()
    );
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    check(&disk, 30.25);
    let revised = engine.build(&recipe(source(45.125))).unwrap();
    check(&revised, 45.125);
    assert_ne!(
        first.feature_hashes["layout"],
        revised.feature_hashes["layout"]
    );
    assert_ne!(
        first.outputs["part"].content_hash,
        revised.outputs["part"].content_hash
    );
}
#[test]
fn parallel_and_perpendicular_drive_a_rectangle() {
    let mut v = source(30.);
    v["features"][0]["sketch"]["points"] = json!([{"id":"a","position":[0,0]},{"id":"b","position":[20.5,0]},{"id":"c","position":[20.7,6]},{"id":"d","position":[-0.1,5.9]}]);
    let c = v["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap();
    c.truncate(2);
    c.extend([
        json!({"id":"right","kind":"perpendicular","a":["a","b"],"b":["b","c"]}),
        json!({"id":"top","kind":"parallel","a":["a","b"],"b":["d","c"]}),
        json!({"id":"left","kind":"perpendicular","a":["a","b"],"b":["a","d"]}),
        json!({"id":"height","kind":"distance","a":"b","b":"c","value":6.125}),
    ]);
    v["features"][0]["loops"] = json!([["a", "b", "c", "d"]]);
    let built = build(&recipe(v)).unwrap();
    let report = &built.sketch_reports["layout"];
    assert!(report.is_fully_constrained());
    assert_eq!(report.rank, 8);
    assert!((report.points["c"][1] - 6.125).abs() < 1e-6);
    assert!((built.outputs["part"].result.model.volume() - 20.5 * 6.125 * 3.25).abs() < 1e-4);
    for r in &report.constraint_residuals {
        if ["left", "right", "top"].contains(&r.id.as_str()) {
            assert!(r.residual_mm.is_none());
            assert!(r.residual_radians.unwrap() <= 1e-9);
        }
    }
}
#[test]
fn angular_scope_and_unselected_structure_remain_distinct() {
    let mut v = source(30.25);
    let mut free = v["features"][0].clone();
    free["id"] = json!("free");
    free["sketch"]["constraints"].as_array_mut().unwrap().pop();
    v["features"].as_array_mut().unwrap().extend([
        free,
        json!({"op":"extrude","id":"free_body","profile":"free","height":2}),
    ]);
    v["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"free_part","feature":"free_body"}));
    let selected = BuildEngine::default()
        .build_outputs(&recipe(v.clone()), &["part".into()])
        .unwrap();
    assert_eq!(selected.sketch_reports.len(), 1);
    assert!(
        matches!(build(&recipe(v.clone())),Err(BuildError::SketchSolveFailed{feature,..})if feature=="free")
    );
    v["features"][2]["sketch"]["constraints"][2]["a"] = json!(["a", "missing"]);
    assert!(
        matches!(BuildEngine::default().build_outputs(&recipe(v),&["part".into()]),Err(BuildError::Sketch{feature,..})if feature=="free")
    );
}
#[test]
fn angular_json_and_invalid_segment_endpoints_are_strict() {
    for value in [json!("a"), json!(["a"]), json!(["a", "b", "c"])] {
        let mut v = source(30.25);
        v["features"][0]["sketch"]["constraints"][3]["a"] = value;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v = source(30.25);
    v["features"][0]["sketch"]["constraints"][3]["a"] = json!(["a", "a"]);
    assert!(matches!(build(&recipe(v)), Err(BuildError::Sketch { .. })));
    for angle in [-180., 180.01] {
        assert!(matches!(
            build(&recipe(source(angle))),
            Err(BuildError::Sketch { .. })
        ));
    }
    let mut v = source(30.25);
    v["features"][0]["sketch"]["constraints"][3]["value"] = json!(30.25);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
}
