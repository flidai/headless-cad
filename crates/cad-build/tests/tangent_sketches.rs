use cad_build::*;
use serde_json::{json, Value};
fn source(side: &str) -> Value {
    let fixed = [
        ("a", [0., 0.]),
        ("b", [100., 0.]),
        ("c", [100., 80.]),
        ("d", [0., 80.]),
        ("l0", [10., 40.]),
        ("l1", [90., 40.]),
        ("anchor", [40., 40.]),
    ];
    let mut points: Vec<_> = fixed
        .iter()
        .map(|(id, p)| json!({"id":id,"position":p}))
        .collect();
    points.extend([
        json!({"id":"center","position":[40.1,43.5]}),
        json!({"id":"rim","position":[44.1,43.5]}),
    ]);
    let mut constraints: Vec<_> = fixed
        .iter()
        .map(|(id, p)| json!({"id":id,"kind":"fixed","point":id,"position":p}))
        .collect();
    constraints.extend([
        json!({"id":"x","kind":"vertical","a":"anchor","b":"center"}),
        json!({"id":"gauge","kind":"horizontal","a":"center","b":"rim"}),
        json!({"id":"radius","kind":"distance","a":"center","b":"rim","value":4.}),
        json!({"id":"tangent","kind":"circle_line_tangent","circle":["center","rim"],"line":["l0","l1"],"side":side})]);
    json!({"schema_version":2,"units":"mm","features":[{"op":"sketch","id":"layout","sketch":{"points":points,"constraints":constraints},"loops":[["a","b","c","d"]],"circular_holes":[{"center":"center","rim":"rim"}]},{"op":"extrude","id":"body","profile":"layout","height":8}],"outputs":[{"name":"part","feature":"body"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
fn check(b: &BuildResult, y: f64) {
    let report = &b.sketch_reports["layout"];
    assert!(report.is_fully_constrained());
    assert_eq!(report.rank, 18);
    assert!((report.points["center"][0] - 40.).abs() < 1e-6);
    assert!((report.points["center"][1] - y).abs() < 1e-6);
    let row = report
        .constraint_residuals
        .iter()
        .find(|r| r.id == "tangent")
        .unwrap();
    assert!(row.residual_mm.unwrap() <= report.tolerance_mm);
    assert!(row.residual_radians.is_none());
    let p = b.outputs["part"]
        .result
        .model
        .mass_properties(cad_kernel::Tolerance::default())
        .unwrap();
    let removed = 128. * std::f64::consts::PI;
    let v = 64000. - removed;
    assert!((p.volume - v).abs() < 1e-4);
    assert!((p.centroid.unwrap()[1] - (64000. * 40. - removed * y) / v).abs() < 1e-6);
}
#[test]
fn tangent_side_edits_move_exact_holes_and_invalidate_cached_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let r = recipe(source("left"));
    let first = engine.build(&r).unwrap();
    check(&first, 44.);
    let cached = engine.build(&r).unwrap();
    assert_eq!(cached.stats.cache_hits, 1);
    assert_eq!(first.sketch_reports, cached.sketch_reports);
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    check(&disk, 44.);
    let flipped = engine.build(&recipe(source("right"))).unwrap();
    check(&flipped, 36.);
    assert_ne!(
        first.feature_hashes["layout"],
        flipped.feature_hashes["layout"]
    );
    assert_ne!(
        first.outputs["part"].content_hash,
        flipped.outputs["part"].content_hash
    );
}
#[test]
fn conflicting_tangency_fails_with_hash_and_linear_evidence() {
    let mut v = source("left");
    v["features"][0]["sketch"]["constraints"].as_array_mut().unwrap().push(json!({"id":"conflict","kind":"circle_line_tangent","circle":["center","rim"],"line":["l0","l1"],"side":"right"}));
    let error = BuildEngine::default().build(&recipe(v)).unwrap_err();
    let BuildError::SketchSolveFailed {
        feature,
        content_hash,
        report,
    } = error
    else {
        panic!("unexpected {error}")
    };
    assert_eq!(feature, "layout");
    assert!(!content_hash.is_empty());
    assert!(!report.is_fully_constrained());
    assert!(report
        .constraint_residuals
        .iter()
        .any(|r| r.residual_mm.unwrap_or(0.) > report.tolerance_mm));
}
#[test]
fn tangent_json_rejects_ambiguous_or_mistyped_sides_and_references() {
    for patch in [
        json!({"side":"either"}),
        json!({"side":null}),
        json!({"circle":["center"]}),
        json!({"line":"l0"}),
        json!({"degrees":1}),
    ] {
        let mut v = source("left");
        let row = v["features"][0]["sketch"]["constraints"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()
            .as_object_mut()
            .unwrap();
        for (key, value) in patch.as_object().unwrap() {
            row.insert(key.clone(), value.clone());
        }
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
}
