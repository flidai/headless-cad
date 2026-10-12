use cad_build::{BuildEngine, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe};
use serde_json::json;

fn recipe(bore: f64, roof: f64, width: f64, start: f64, margin: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[
        {"op":"circle","id":"profile","radius":24.,"inner_radius":bore},
        {"op":"extrude","id":"stock","profile":"profile","height":14.},
        {"op":"rectangle","id":"tool_profile","width":roof-start,"depth":width},
        {"op":"extrude","id":"tool_blank","profile":"tool_profile","height":14.+2.*margin},
        {"op":"translate","id":"tool","body":"tool_blank","offset":[start,-width/2.,-margin]},
        {"op":"cut","id":"keyed","left":"stock","right":"tool"}
    ],"outputs":[{"name":"stock","feature":"stock"},{"name":"hub","feature":"keyed"}]}))
    .unwrap()
}
fn expected(bore: f64, roof: f64, width: f64) -> f64 {
    let b = width / 2.;
    let removed =
        2. * b * roof - b * (bore * bore - b * b).sqrt() - bore * bore * (b / bore).asin();
    (std::f64::consts::PI * (24. * 24. - bore * bore) - removed) * 14.
}
#[test]
fn hub_keyway_volume_lineage_and_cached_design_edits() {
    let mut engine = BuildEngine::default();
    let r = recipe(6.15, 8.05, 4.04, 0., 1.);
    let first = engine.build(&r).unwrap();
    assert!(
        first.outputs["hub"]
            .result
            .model
            .validate(Default::default())
            .valid
    );
    assert!((first.outputs["hub"].result.model.volume() - expected(6.15, 8.05, 4.04)).abs() < 1e-7);
    for (anchor, role, count) in [
        ("keyed", "hub_keyway_roof", 1),
        ("keyed", "hub_keyway_side_negative", 1),
        ("keyed", "hub_keyway_side_positive", 1),
        ("stock", "inner_wall", 2),
        ("stock", "wall", 1),
        ("stock", "start_cap", 1),
        ("stock", "end_cap", 1),
    ] {
        assert_eq!(
            first
                .resolve_faces(&FaceSelector {
                    anchor_feature: anchor.into(),
                    role: role.into(),
                    target_feature: "keyed".into(),
                    cardinality: FaceCardinality::Exactly { count },
                })
                .status,
            FaceResolutionStatus::Resolved
        );
    }
    assert_eq!(engine.build(&r).unwrap().stats.computed_solids, 0);
    let roof = engine.build(&recipe(6.15, 8.25, 4.04, 0., 1.)).unwrap();
    assert_eq!(
        first.outputs["stock"].content_hash,
        roof.outputs["stock"].content_hash
    );
    assert_ne!(
        first.outputs["hub"].content_hash,
        roof.outputs["hub"].content_hash
    );
    assert!((roof.outputs["hub"].result.model.volume() - expected(6.15, 8.25, 4.04)).abs() < 1e-7);
    let bore = engine.build(&recipe(7.15, 9.05, 4.04, 0., 1.)).unwrap();
    assert_ne!(
        first.outputs["stock"].content_hash,
        bore.outputs["stock"].content_hash
    );
    assert!((bore.outputs["hub"].result.model.volume() - expected(7.15, 9.05, 4.04)).abs() < 1e-7);
}
#[test]
fn hub_keyway_rejects_blind_slot_outer_breakthrough_and_disconnected_tool() {
    for r in [
        recipe(6.15, 8.05, 4.04, 0., -1.),
        recipe(6.15, 24., 4.04, 0., 1.),
        recipe(6.15, 8.05, 4.04, 6.2, 1.),
        recipe(6.15, 8.05, 13., 0., 1.),
        recipe(6.15, 8.05, 4.04, -1., 1.),
    ] {
        assert!(cad_build::build(&r).is_err());
    }
}
