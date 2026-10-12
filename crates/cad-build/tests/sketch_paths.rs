use cad_build::*;
use serde_json::{json, Value};
fn source(d: f64, r: f64) -> Value {
    let points = [
        ("a", [-d, -r]),
        ("b", [d, -r]),
        ("c", [d, r]),
        ("d", [-d, r]),
        ("left", [-d, 0.]),
        ("right", [d, 0.]),
        ("center", [0., 0.]),
        ("rim", [2., 0.]),
    ];
    let guesses: Vec<_> = points
        .iter()
        .map(|(id, p)| json!({"id":id,"position":p}))
        .collect();
    let constraints: Vec<_> = points
        .iter()
        .map(|(id, p)| json!({"id":id,"kind":"fixed","point":id,"position":p}))
        .collect();
    json!({"schema_version":2,"units":"mm","features":[{"op":"sketch","id":"profile","sketch":{"points":guesses,"constraints":constraints},"curve_outline":{"start":"a","segments":[{"kind":"line_to","end":"b"},{"kind":"arc_to","center":"right","end":"c"},{"kind":"line_to","end":"d"},{"kind":"arc_to","center":"left","end":"a"}]},"circular_holes":[{"center":"center","rim":"rim"}]},{"op":"extrude","id":"body","profile":"profile","height":8}],"outputs":[{"name":"part","feature":"body"}]})
}
fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
fn check(b: &BuildResult, d: f64, r: f64) {
    let m = &b.outputs["part"].result.model;
    let p = m.mass_properties(cad_kernel::Tolerance::default()).unwrap();
    let pi = std::f64::consts::PI;
    let area = 4. * d * r + pi * (r * r - 4.);
    let xx = 4. * r * d.powi(3) / 3.
        + pi * r.powi(4) / 4.
        + pi * r * r * d * d
        + 8. * d * r.powi(3) / 3.
        - 4. * pi;
    let yy = 4. * d * r.powi(3) / 3. + pi * r.powi(4) / 4. - 4. * pi;
    assert!((p.volume - area * 8.).abs() < 1e-7);
    let inertia = p.central_inertia.unwrap();
    let expected = [
        8. * yy + area * 512. / 12.,
        8. * xx + area * 512. / 12.,
        8. * (xx + yy),
    ];
    for i in 0..3 {
        assert!((inertia[i][i] - expected[i]).abs() < 1e-6);
        for (j, value) in inertia[i].iter().enumerate() {
            if i != j {
                assert!(value.abs() < 1e-6);
            }
        }
    }
    for (a, b) in p.centroid.unwrap().iter().zip([0., 0., 4.]) {
        assert!((a - b).abs() < 1e-10);
    }
    let bounds = m.bounds();
    for (a, b) in bounds.min.iter().zip([-d - r, -r, 0.]) {
        assert!((a - b).abs() < 1e-10);
    }
    for (a, b) in bounds.max.iter().zip([d + r, r, 8.]) {
        assert!((a - b).abs() < 1e-10);
    }
    assert_eq!(
        m.edges
            .iter()
            .filter(|e| matches!(e.curve, cad_kernel::Curve::CircularArc(_)))
            .count(),
        4
    );
    assert_eq!(
        m.faces
            .iter()
            .filter(|f| matches!(f.surface, cad_kernel::Surface::Cylinder(_)))
            .count(),
        3
    );
    assert!(b.sketch_reports["profile"].is_fully_constrained());
}
#[test]
fn solved_capsule_native_moments_bounds_edits_and_cache_reports() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = BuildEngine::with_cache_dir(dir.path());
    let r = recipe(source(10., 5.));
    let first = engine.build(&r).unwrap();
    check(&first, 10., 5.);
    let cached = engine.build(&r).unwrap();
    assert_eq!(cached.stats.computed_solids, 0);
    assert_eq!(cached.sketch_reports, first.sketch_reports);
    let disk = BuildEngine::with_cache_dir(dir.path()).build(&r).unwrap();
    assert_eq!(disk.stats.disk_cache_hits, 1);
    assert_eq!(disk.sketch_reports, first.sketch_reports);
    for (d, r) in [(12., 5.), (10., 6.)] {
        let changed = engine.build(&recipe(source(d, r))).unwrap();
        check(&changed, d, r);
        assert_eq!(changed.stats.computed_solids, 1);
        assert_ne!(
            first.feature_hashes["profile"],
            changed.feature_hashes["profile"]
        );
    }
    let mut explicit = source(10., 5.);
    explicit["features"][0]["loops"] = json!([]);
    let same = engine.build(&recipe(explicit)).unwrap();
    assert_eq!(same.feature_hashes, first.feature_hashes);
}
#[test]
fn curve_structure_and_json_are_strict_even_when_unselected() {
    for (field, value) in [
        ("start", json!("missing")),
        ("segments", json!([])),
        ("segments", json!([{"kind":"line_to","end":"b"}])),
        (
            "segments",
            json!([{"kind":"arc_to","center":"missing","end":"a"}]),
        ),
    ] {
        let mut v = source(10., 5.);
        let mut unused = v["features"][0].clone();
        unused["id"] = json!("unused");
        unused["curve_outline"][field] = value;
        v["features"].as_array_mut().unwrap().push(unused);
        assert!(
            matches!(BuildEngine::default().build_outputs(&recipe(v),&["part".into()]),Err(BuildError::Sketch{feature,..})if feature=="unused")
        );
    }
    let mut ambiguous = source(10., 5.);
    ambiguous["features"][0]["loops"] = json!([["a", "b", "c", "d"]]);
    assert!(matches!(
        BuildEngine::default().build(&recipe(ambiguous)),
        Err(BuildError::Sketch { .. })
    ));
    for segment in [
        json!({"kind":"line_to","end":"b","center":"right"}),
        json!({"kind":"arc_to","end":"c","center":"right","clockwise":null}),
        json!({"kind":"arc_to","end":"c","center":"right","extra":1}),
    ] {
        let mut v = source(10., 5.);
        v["features"][0]["curve_outline"]["segments"][0] = segment;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    assert!(serde_json::from_str::<SketchPathSegment>(
        r#"{"kind":"arc_to","end":"a","end":"b","center":"c"}"#
    )
    .is_err());
    let mut empty = source(10., 5.);
    empty["features"][0]
        .as_object_mut()
        .unwrap()
        .remove("curve_outline");
    assert!(serde_json::from_value::<Recipe>(empty.clone()).is_err());
    empty["features"][0]["loops"] = json!([]);
    assert!(BuildEngine::default().build(&recipe(empty)).is_err());
    let mut excess = source(10., 5.);
    excess["features"][0]["curve_outline"]["segments"] =
        json!(vec![json!({"kind":"line_to","end":"a"}); 257]);
    assert!(matches!(
        BuildEngine::default().build(&recipe(excess)),
        Err(BuildError::Sketch { .. })
    ));
}
#[test]
fn solved_but_invalid_native_arcs_fail_without_snapping() {
    let mut cases = vec![];
    let mut clockwise = source(10., 5.);
    clockwise["features"][0]["curve_outline"]["segments"][1]["clockwise"] = json!(true);
    cases.push(clockwise);
    // This difference is below solve tolerance but above native circle-support
    // arithmetic consistency. A successful sketch cannot authorize projection.
    let mut mismatch = source(10., 5.);
    mismatch["features"][0]["sketch"]["points"][2]["position"] = json!([10., 5. + 1e-8]);
    mismatch["features"][0]["sketch"]["constraints"][2]["position"] = json!([10., 5. + 1e-8]);
    cases.push(mismatch);
    let mut zero = source(10., 5.);
    zero["features"][0]["curve_outline"]["segments"][1]["center"] = json!("b");
    cases.push(zero);
    for v in cases {
        assert!(matches!(
            BuildEngine::default().build(&recipe(v)),
            Err(BuildError::PolygonProfile { .. })
        ));
    }
    let mut free = source(10., 5.);
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
fn unsupported_major_arcs_and_concavity_fail_native_validation() {
    for (points, segments) in [
        (
            vec![("a", [0., -5.]), ("b", [5., 0.]), ("o", [0., 0.])],
            json!([{"kind":"line_to","end":"b"},{"kind":"arc_to","center":"o","end":"a"}]),
        ),
        (
            vec![
                ("a", [0., 0.]),
                ("b", [10., 0.]),
                ("c", [5., 3.]),
                ("d", [10., 6.]),
                ("e", [0., 6.]),
            ],
            json!([{"kind":"line_to","end":"b"},{"kind":"line_to","end":"c"},{"kind":"line_to","end":"d"},{"kind":"line_to","end":"e"},{"kind":"line_to","end":"a"}]),
        ),
    ] {
        let mut v = source(10., 5.);
        v["features"][0]["sketch"]["points"] = json!(points
            .iter()
            .map(|(id, p)| json!({"id":id,"position":p}))
            .collect::<Vec<_>>());
        v["features"][0]["sketch"]["constraints"] = json!(points
            .iter()
            .map(|(id, p)| json!({"id":id,"kind":"fixed","point":id,"position":p}))
            .collect::<Vec<_>>());
        v["features"][0]["curve_outline"]["segments"] = segments;
        v["features"][0]["circular_holes"] = json!([]);
        assert!(matches!(
            BuildEngine::default().build(&recipe(v)),
            Err(BuildError::PolygonProfile { .. })
        ));
    }
}
