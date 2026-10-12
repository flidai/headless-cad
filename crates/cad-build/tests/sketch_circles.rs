use cad_build::*;
use serde_json::{json, Value};
fn source(cx: f64, radius: f64) -> Value {
    let points = [
        ("a", [0., 0.]),
        ("b", [30., 0.]),
        ("c", [30., 20.]),
        ("d", [0., 20.]),
        ("h0", [1., 1.]),
        ("h1", [1., 3.]),
        ("h2", [3., 3.]),
        ("h3", [3., 1.]),
        ("center", [cx, 10.]),
    ];
    let mut guesses: Vec<_> = points
        .iter()
        .map(|(id, p)| json!({"id":id,"position":p}))
        .collect();
    guesses.push(json!({"id":"rim","position":[cx+radius*0.9,10.1]}));
    let mut constraints: Vec<_> = points
        .iter()
        .map(|(id, p)| json!({"id":id,"kind":"fixed","point":id,"position":p}))
        .collect();
    constraints.extend([
        json!({"id":"radius","kind":"distance","a":"center","b":"rim","value":radius}),
        json!({"id":"gauge","kind":"horizontal","a":"center","b":"rim"}),
    ]);
    json!({"schema_version":2,"units":"mm","features":[
        {"op":"sketch","id":"profile","sketch":{"points":guesses,"constraints":constraints},"loops":[["a","b","c","d"],["h0","h1","h2","h3"]],"circular_holes":[{"center":"center","rim":"rim"}]},
        {"op":"extrude","id":"body","profile":"profile","height":5}
    ],"outputs":[{"name":"part","feature":"body"}]})
}
fn recipe(value: Value) -> Recipe {
    serde_json::from_value(value).unwrap()
}
fn check(b: &BuildResult, cx: f64, r: f64) {
    let m = &b.outputs["part"].result.model;
    let p = m.mass_properties(cad_kernel::Tolerance::default()).unwrap();
    let circle = std::f64::consts::PI * r * r * 5.;
    let volume = 3000. - 20. - circle;
    let center = [
        (3000. * 15. - 20. * 2. - circle * cx) / volume,
        (3000. * 10. - 20. * 2. - circle * 10.) / volume,
        2.5,
    ];
    assert!((p.volume - volume).abs() < 1e-4);
    for (actual, expected) in p.centroid.unwrap().iter().zip(center) {
        assert!((actual - expected).abs() < 1e-6);
    }
    let components = [
        (
            3000.,
            [15., 10., 2.5],
            [30. * 30. / 12., 20. * 20. / 12., 25. / 12.],
        ),
        (-20., [2., 2., 2.5], [4. / 12., 4. / 12., 25. / 12.]),
        (-circle, [cx, 10., 2.5], [r * r / 4., r * r / 4., 25. / 12.]),
    ];
    let mut covariance = [[0.; 3]; 3];
    for (v, c, variance) in components {
        for i in 0..3 {
            for j in 0..3 {
                covariance[i][j] += v
                    * ((c[i] - center[i]) * (c[j] - center[j])
                        + if i == j { variance[i] } else { 0. });
            }
        }
    }
    let trace = (0..3).map(|i| covariance[i][i]).sum::<f64>();
    let inertia = p.central_inertia.unwrap();
    for i in 0..3 {
        for j in 0..3 {
            let expected = if i == j {
                trace - covariance[i][i]
            } else {
                -covariance[i][j]
            };
            assert!(
                (inertia[i][j] - expected).abs() < 0.02,
                "{i},{j}: {} expected {expected}",
                inertia[i][j]
            );
        }
    }
    assert!(m
        .faces
        .iter()
        .any(|f| matches!(f.surface, cad_kernel::Surface::Cylinder(_))));
    assert!(b.sketch_reports["profile"].is_fully_constrained());
}
#[test]
fn circles_follow_solved_handles_with_exact_native_properties_and_cache_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let r = recipe(source(20., 3.));
    let first = engine.build(&r).unwrap();
    check(&first, 20., 3.);
    let cached = engine.build(&r).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.sketch_reports, first.sketch_reports);
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    assert_eq!(disk.sketch_reports, first.sketch_reports);
    for (cx, radius) in [(18.5, 3.), (20., 4.25)] {
        let changed = engine.build(&recipe(source(cx, radius))).unwrap();
        check(&changed, cx, radius);
        assert_eq!(changed.stats.computed_solids, 1);
        assert_ne!(
            first.feature_hashes["profile"],
            changed.feature_hashes["profile"]
        );
    }
    let mut changed = source(20., 3.);
    changed["features"][0]["circular_holes"] = json!([]);
    let removed = engine.build(&recipe(changed.clone())).unwrap();
    assert_ne!(
        first.feature_hashes["profile"],
        removed.feature_hashes["profile"]
    );
    changed["features"][0]
        .as_object_mut()
        .unwrap()
        .remove("circular_holes");
    let omitted = engine.build(&recipe(changed)).unwrap();
    assert_eq!(removed.feature_hashes, omitted.feature_hashes);
}
#[test]
fn native_validation_rejects_zero_outside_overlapping_holes_and_unsolved_handles() {
    let mut cases = vec![source(29., 3.), source(27., 3.)];
    let mut overlap = source(20., 3.);
    overlap["features"][0]["circular_holes"] =
        json!([{"center":"center","rim":"rim"},{"center":"center","rim":"rim"}]);
    cases.push(overlap);
    let mut crossing = source(20., 3.);
    for (id, position) in [("center2", [24., 10.]), ("rim2", [27., 10.])] {
        crossing["features"][0]["sketch"]["points"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"position":position}));
        crossing["features"][0]["sketch"]["constraints"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":id,"kind":"fixed","point":id,"position":position}));
    }
    crossing["features"][0]["circular_holes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"center":"center2","rim":"rim2"}));
    cases.push(crossing);
    let mut polygon_overlap = source(3., 2.);
    polygon_overlap["features"][0]["sketch"]["constraints"][8]["position"] = json!([3., 3.]);
    cases.push(polygon_overlap);

    let mut zero = source(20., 3.);
    zero["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap()
        .truncate(9);
    zero["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fixed_rim","kind":"fixed","point":"rim","position":[20,10]}));
    cases.push(zero);
    for v in cases {
        assert!(matches!(
            BuildEngine::default().build(&recipe(v)),
            Err(BuildError::PolygonProfile { .. })
        ));
    }
    let mut free = source(20., 3.);
    free["features"][0]["sketch"]["constraints"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(matches!(
        BuildEngine::default().build(&recipe(free)),
        Err(BuildError::SketchSolveFailed { .. })
    ));
}
#[test]
fn strict_schema_global_references_and_combined_hole_budget() {
    let good = source(20., 3.);
    for hole in [
        json!({"center":"center","rim":"rim","extra":1}),
        json!({"center":"center","rim":"rim","radius":null}),
        json!({"center":[20,10],"radius":3}),
        json!({"center":"center","rim":null}),
        json!({"center":"center"}),
    ] {
        let mut v = good.clone();
        v["features"][0]["circular_holes"] = json!([hole]);
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    for hole in [
        json!({"center":"absent","rim":"rim"}),
        json!({"center":"center","rim":"center"}),
    ] {
        let mut v = good.clone();
        let mut unused = v["features"][0].clone();
        unused["id"] = json!("unused");
        unused["circular_holes"] = json!([hole]);
        v["features"].as_array_mut().unwrap().push(unused);
        assert!(
            matches!(BuildEngine::default().build_outputs(&recipe(v),&["part".into()]),Err(BuildError::Sketch{feature,..})if feature=="unused")
        );
    }
    let mut v = good;
    v["features"][0]["circular_holes"] = json!(vec![json!({"center":"center","rim":"rim"}); 32]);
    assert!(matches!(
        BuildEngine::default().build(&recipe(v)),
        Err(BuildError::Sketch { .. })
    ));
    let duplicate = r#"{"op":"sketch","id":"s","sketch":{"points":[],"constraints":[]},"loops":[],"circular_holes":[{"center":"a","center":"b","rim":"c"}]}"#;
    assert!(serde_json::from_str::<Feature>(duplicate).is_err());
    let fractional = r#"{"circular_holes":[{"radius":1.25,"center":[2.5,3.75]}],"op":"polygon","id":"p","points":[[0,0],[10,0],[0,10]]}"#;
    assert!(serde_json::from_str::<Feature>(fractional).is_ok());
}
