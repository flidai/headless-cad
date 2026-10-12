use cad_build::*;
use serde_json::{json, Value};
fn source() -> Value {
    let corners = [
        ("a", [0., 0.]),
        ("b", [180., 0.]),
        ("c", [180., 150.]),
        ("d", [0., 150.]),
    ];
    let mut points: Vec<_> = corners
        .iter()
        .map(|(id, p)| json!({"id":id,"position":p}))
        .collect();
    let mut constraints: Vec<_> = corners
        .iter()
        .map(|(id, p)| json!({"id":id,"kind":"fixed","point":id,"position":p}))
        .collect();
    let mut holes = vec![];
    for i in 0..30 {
        let center = format!("center_{i:02}");
        let rim = format!("rim_{i:02}");
        let p = [
            90. + (i % 6) as f64 * 25. - 62.5,
            75. + (i / 6) as f64 * 25. - 50.,
        ];
        points.extend([
            json!({"id":center,"position":p}),
            json!({"id":rim,"position":[p[0]+3.5,p[1]+0.2]}),
        ]);
        constraints.extend([
            json!({"id":format!("anchor_{i:02}"),"kind":"fixed","point":center,"position":p}),
            json!({"id":format!("gauge_{i:02}"),"kind":"horizontal","a":center,"b":rim}),
            json!({"id":format!("radius_{i:02}"),"kind":"distance","a":center,"b":rim,"value":4}),
        ]);
        holes.push(json!({"center":center,"rim":rim}));
    }
    json!({"schema_version":2,"units":"mm","features":[{"op":"sketch","id":"layout","sketch":{"points":points,"constraints":constraints},"loops":[["a","b","c","d"]],"circular_holes":holes},{"op":"extrude","id":"body","profile":"layout","height":8}],"outputs":[{"name":"plate","feature":"body"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn sixty_four_points_build_native_holes_with_stable_island_reports_and_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let r = recipe(source());
    let first = engine.build(&r).unwrap();
    let report = &first.sketch_reports["layout"];
    assert_eq!(report.schema_version, 3);
    assert!(report.is_fully_constrained());
    assert_eq!(report.rank, 128);
    assert_eq!(report.islands.len(), 34);
    assert!(report.islands.windows(2).all(|w| w[0].id < w[1].id));
    for island in &report.islands {
        assert_eq!(island.degrees_of_freedom, 0);
        assert!(island.rank == 2 || island.rank == 4);
    }
    let expected = (27000. - 30. * std::f64::consts::PI * 16.) * 8.;
    assert!((first.outputs["plate"].result.model.volume() - expected).abs() < 1e-3);
    let second = engine.build(&r).unwrap();
    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(first.sketch_reports, second.sketch_reports);
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    assert_eq!(first.sketch_reports, disk.sketch_reports);
}
#[test]
fn failing_island_remains_scoped_and_reports_healthy_neighbors() {
    let mut v = source();
    let mut bad = v["features"][0].clone();
    bad["id"] = json!("bad");
    bad["sketch"]["constraints"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["id"] != "radius_07");
    v["features"].as_array_mut().unwrap().extend([
        bad,
        json!({"op":"extrude","id":"bad_body","profile":"bad","height":8}),
    ]);
    v["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"bad_plate","feature":"bad_body"}));
    let r = recipe(v);
    let mut engine = BuildEngine::default();
    let selected = engine.build_outputs(&r, &["plate".into()]).unwrap();
    assert_eq!(selected.sketch_reports.len(), 1);
    let error = engine.build(&r).unwrap_err();
    let BuildError::SketchSolveFailed {
        report, feature, ..
    } = error
    else {
        panic!("{error}")
    };
    assert_eq!(feature, "bad");
    assert_eq!(report.islands.len(), 34);
    assert_eq!(report.degrees_of_freedom, 1);
    assert_eq!(
        report
            .islands
            .iter()
            .filter(|i| i.degrees_of_freedom > 0)
            .count(),
        1
    );
    assert_eq!(
        report
            .islands
            .iter()
            .find(|i| i.degrees_of_freedom > 0)
            .unwrap()
            .id,
        "center_07"
    );
}
#[test]
fn oversized_connected_island_fails_structural_preflight_even_unselected() {
    let mut v = source();
    let constraints = v["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap();
    for i in 1..17 {
        constraints.push(json!({"id":format!("connect_{i}"),"kind":"horizontal","a":format!("center_{:02}",i-1),"b":format!("center_{i:02}")}));
    }
    v["features"].as_array_mut().unwrap().extend([
        json!({"op":"rectangle","id":"plain_profile","width":1,"depth":1}),
        json!({"op":"extrude","id":"plain","profile":"plain_profile","height":1}),
    ]);
    v["outputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"plain","feature":"plain"}));
    // Validate all sketch structure even when no sketch output is demanded.
    assert!(matches!(
        BuildEngine::default().build_outputs(&recipe(v), &["plain".into()]),
        Err(BuildError::Sketch { .. })
    ));
}
