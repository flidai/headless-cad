use cad_build::{BuildEngine, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe};
use serde_json::json;
fn recipe(start: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[
        {"op":"polygon","id":"profile","points":[[0,0],[8,0],[8,12.9],[6,12.9],[6,94.55],[5.4,94.55],[5.4,95.8],[6,95.8],[6,130],[0,130]]},
        {"op":"revolve","id":"stock","profile":"profile"},
        {"op":"rectangle","id":"tool_profile","width":4,"depth":4.04},
        {"op":"extrude","id":"blank","profile":"tool_profile","height":13.5},
        {"op":"translate","id":"tool","body":"blank","offset":[4,-2.02,start]},
        {"op":"cut","id":"shaft","left":"stock","right":"tool"}
    ],"outputs":[{"name":"shaft","feature":"shaft"},{"name":"stock","feature":"stock"}]})).unwrap()
}
#[test]
fn integral_shoulder_groove_and_keyseat_keep_volume_and_native_lineage() {
    let mut engine = BuildEngine::default();
    let r = recipe(45.);
    let b = engine.build(&r).unwrap();
    let half = 2.02_f64;
    let removed = half * (36. - half * half).sqrt() + 36. * (half / 6.).asin() - 8. * half;
    let expected = std::f64::consts::PI
        * (64. * 12.9 + 36. * (130. - 12.9) - 1.25 * (36. - 5.4 * 5.4))
        - 13.5 * removed;
    assert!((b.outputs["shaft"].result.model.volume() - expected).abs() < 1e-7);
    for role in ["segment_2", "segment_6"] {
        let resolved = b.resolve_faces(&FaceSelector {
            anchor_feature: "stock".into(),
            role: role.into(),
            target_feature: "shaft".into(),
            cardinality: FaceCardinality::Exactly { count: 1 },
        });
        assert_eq!(resolved.status, FaceResolutionStatus::Resolved);
    }
    assert_eq!(engine.build(&r).unwrap().stats.computed_solids, 0);
    let changed = engine.build(&recipe(50.)).unwrap();
    assert_eq!(
        changed.outputs["stock"].content_hash,
        b.outputs["stock"].content_hash
    );
    assert_ne!(
        changed.outputs["shaft"].content_hash,
        b.outputs["shaft"].content_hash
    );
    assert!((changed.outputs["shaft"].result.model.volume() - expected).abs() < 1e-7);
}
#[test]
fn seat_cannot_cross_shoulder_or_groove_transition() {
    for start in [10., 85., 94.55] {
        assert!(cad_build::build(&recipe(start)).is_err(), "start={start}");
    }
}
