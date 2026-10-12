use cad_build::{BuildEngine, BuildError, FaceResolutionStatus, Recipe};
use serde_json::json;
fn recipe(z: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[
        {"op":"circle","id":"profile","radius":6},
        {"op":"extrude","id":"stock","profile":"profile","height":40},
        {"op":"rectangle","id":"tool_profile","width":4,"depth":4.04},
        {"op":"extrude","id":"tool_blank","profile":"tool_profile","height":13.5},
        {"op":"translate","id":"tool","body":"tool_blank","offset":[4,-2.02,10]},
        {"op":"cut","id":"shaft","left":"stock","right":"tool"}
    ],"outputs":[{"name":"shaft","feature":"shaft"},{"name":"stock","feature":"stock"}],
    "section_views":[{"id":"journal","output":"shaft","origin":[0,0,z],"circle_dimensions":[
        {"id":"diameter","anchor_feature":"stock","role":"wall","kind":"diameter","label_offset":[10,10],"min":11.99,"max":12.01}
    ]}]})).unwrap()
}
#[test]
fn journal_dimension_keeps_all_split_face_provenance_and_global_obligations() {
    let mut engine = BuildEngine::default();
    let r = recipe(5.);
    let result = engine.build_outputs(&r, &["stock".into()]).unwrap();
    let d = &result.section_views["journal"].circle_dimensions[0];
    assert_eq!(d.value_mm, 12.);
    assert_eq!(d.selection.status, FaceResolutionStatus::Resolved);
    assert_eq!(d.selection.witnesses.len(), 4);
    assert_eq!(
        engine
            .build_outputs(&r, &["stock".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let rear = engine.build(&recipe(30.)).unwrap();
    assert_eq!(
        rear.section_views["journal"].circle_dimensions[0].value_mm,
        12.
    );
    assert!(matches!(
        engine.build_outputs(&recipe(15.), &["stock".into()]),
        Err(BuildError::SectionDimensionFailed { .. })
    ));
}
