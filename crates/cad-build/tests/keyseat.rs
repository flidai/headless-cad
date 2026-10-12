use cad_build::{BuildEngine, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe};
use serde_json::json;
fn recipe(radius: f64, depth: f64, width: f64, start: f64, length: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[
        {"op":"circle","id":"profile","radius":radius},
        {"op":"extrude","id":"stock","profile":"profile","height":40},
        {"op":"rectangle","id":"tool_profile","width":depth+2.,"depth":width},
        {"op":"extrude","id":"tool_blank","profile":"tool_profile","height":length},
        {"op":"translate","id":"tool","body":"tool_blank","offset":[radius-depth,-width/2.,start]},
        {"op":"cut","id":"keyed","left":"stock","right":"tool"}
    ],"outputs":[{"name":"stock","feature":"stock"},{"name":"shaft","feature":"keyed"}]}))
    .unwrap()
}
fn expected(radius: f64, depth: f64, width: f64, length: f64) -> f64 {
    let b = width / 2.;
    let a = radius - depth;
    std::f64::consts::PI * radius * radius * 40.
        - length
            * (b * (radius * radius - b * b).sqrt() + radius * radius * (b / radius).asin()
                - 2. * a * b)
}
#[test]
fn finite_keyseat_native_volume_lineage_and_cache_regeneration() {
    let mut engine = BuildEngine::default();
    let r = recipe(6., 2., 4., 10., 14.);
    let first = engine.build(&r).unwrap();
    let shaft = &first.outputs["shaft"].result.model;
    assert!(shaft.validate(Default::default()).valid);
    assert!((shaft.volume() - expected(6., 2., 4., 14.)).abs() < 1e-7);
    for (anchor, role) in [
        ("keyed", "keyseat_floor"),
        ("keyed", "keyseat_start"),
        ("keyed", "keyseat_end"),
        ("stock", "start_cap"),
        ("stock", "end_cap"),
    ] {
        let selector = FaceSelector {
            anchor_feature: anchor.into(),
            role: role.into(),
            target_feature: "keyed".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        };
        assert_eq!(
            first.resolve_faces(&selector).status,
            FaceResolutionStatus::Resolved
        );
    }
    assert_eq!(engine.build(&r).unwrap().stats.computed_solids, 0);
    let edited = engine.build(&recipe(6., 1.5, 4., 10., 14.)).unwrap();
    assert_eq!(
        first.outputs["stock"].content_hash,
        edited.outputs["stock"].content_hash
    );
    assert_ne!(
        first.outputs["shaft"].content_hash,
        edited.outputs["shaft"].content_hash
    );
    assert!(
        (edited.outputs["shaft"].result.model.volume() - expected(6., 1.5, 4., 14.)).abs() < 1e-7
    );
    let diameter = engine.build(&recipe(7., 2., 4., 10., 14.)).unwrap();
    assert_ne!(
        first.outputs["stock"].content_hash,
        diameter.outputs["stock"].content_hash
    );
    assert!(
        (diameter.outputs["shaft"].result.model.volume() - expected(7., 2., 4., 14.)).abs() < 1e-7
    );
}
#[test]
fn keyseat_domain_rejects_lost_journals_axis_crossing_and_interior_tool() {
    for r in [
        recipe(6., 2., 4., 0., 14.),
        recipe(6., 2., 4., 30., 10.),
        recipe(6., 6., 4., 10., 14.),
        recipe(6., 1., 10., 10., 14.),
    ] {
        assert!(cad_build::build(&r).is_err());
    }
    let mut v = serde_json::to_value(recipe(6., 2., 4., 10., 14.)).unwrap();
    v["features"][2]["width"] = json!(0.5);
    let r: Recipe = serde_json::from_value(v).unwrap();
    assert!(cad_build::build(&r).is_err());
}
