use cad_build::{
    BuildEngine, BuildError, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe,
};
use serde_json::{json, Value};

fn value(relief_depth: f64) -> Value {
    json!({
        "schema_version":2,"units":"mm",
        "features":[
            {"op":"rectangle","id":"outline","width":120,"depth":80},
            {"op":"extrude","id":"stock","profile":"outline","height":12},
            {"op":"slot_pattern","id":"through","body":"stock","frame_origin":[0,0,0],
                "pattern_slots":[
                    {"id":"near","center":[20,15],"direction":[1,0],"centerline_length":12,"radius":3},
                    {"id":"far","center":[100,65],"direction":[1,0],"centerline_length":12,"radius":3}
                ]},
            {"op":"blind_slot_pattern","id":"locating","body":"through",
                "face":{"from_feature":"stock","role":"end_cap"},"depth":3,
                "frame_origin":[0,0,12],"pattern_slots":[
                    {"id":"locating","center":[38,40],"direction":[0,1],"centerline_length":18,"radius":4}
                ]},
            {"op":"blind_slot_pattern","id":"relief","body":"locating",
                "face":{"from_feature":"stock","role":"end_cap"},"depth":relief_depth,
                "frame_origin":[0,0,12],"pattern_slots":[
                    {"id":"relief","center":[82,40],"direction":[0,1],"centerline_length":20,"radius":5}
                ]}
        ],
        "outputs":[{"name":"through","feature":"through"},{"name":"final","feature":"relief"}],
        "parts":[{"id":"machined","feature":"relief"}],
        "slot_center_datums":[{"part":"machined","name":"near_center",
            "anchor_feature":"through","role":"slot_near_walls","cap_role":"start_cap"}],
        "face_datums":[
            {"part":"machined","name":"opening","anchor_feature":"stock","role":"end_cap","x_direction":[1,0,0]},
            {"part":"machined","name":"first_floor","anchor_feature":"locating","role":"slot_locating_floor","x_direction":[1,0,0]},
            {"part":"machined","name":"second_floor","anchor_feature":"relief","role":"slot_relief_floor","x_direction":[1,0,0]}
        ],
        "assemblies":[{"id":"fixture","instances":[{"id":"body","part":"machined"}]}],
        "assembly_outputs":[{"name":"fixture","assembly":"fixture"}],
        "datum_dimensions":[
            {"id":"first_depth","output":"fixture","from":{"instance_path":["body"],"datum":"opening"},
                "to":{"instance_path":["body"],"datum":"first_floor"},"direction":-1,"min":2.999999,"max":3.000001},
            {"id":"second_depth","output":"fixture","from":{"instance_path":["body"],"datum":"opening"},
                "to":{"instance_path":["body"],"datum":"second_floor"},"direction":-1,
                "min":relief_depth-0.000001,"max":relief_depth+0.000001}
        ],
        "section_views":[{"id":"through_section","output":"final","origin":[0,0,2],
            "slot_dimensions":[
                {"id":"near_width","anchor_feature":"through","role":"slot_near_walls","kind":"width",
                    "label_offset":[0,12],"min":5.999999,"max":6.000001},
                {"id":"far_length","anchor_feature":"through","role":"slot_far_walls","kind":"overall_length",
                    "label_offset":[0,-12],"min":17.999999,"max":18.000001}
            ]}]
    })
}

fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}

#[test]
fn mixed_through_and_serial_blind_cuts_keep_actual_through_dimensions_and_first_floor() {
    let mut engine = BuildEngine::default();
    let first = engine
        .build_outputs(&recipe(value(7.)), &["final".into()])
        .unwrap();
    let body = &first.outputs["final"].result.model;
    let through_area = 12. * 6. + std::f64::consts::PI * 9.;
    let first_area = 18. * 8. + std::f64::consts::PI * 16.;
    let second_area = 20. * 10. + std::f64::consts::PI * 25.;
    let expected_volume =
        120. * 80. * 12. - 2. * through_area * 12. - first_area * 3. - second_area * 7.;
    assert!((body.volume() - expected_volume).abs() < 1e-6);
    let floor = first.resolve_faces(&FaceSelector {
        anchor_feature: "locating".into(),
        role: "slot_locating_floor".into(),
        target_feature: "relief".into(),
        cardinality: FaceCardinality::Exactly { count: 1 },
    });
    assert_eq!(floor.status, FaceResolutionStatus::Resolved);
    assert!(
        (body.faces[floor.witnesses[0].target_face.0]
            .try_plane()
            .unwrap()
            .origin[2]
            - 9.)
            .abs()
            < 1e-8
    );
    assert_eq!(first.datum_dimensions["first_depth"].value_mm, 3.);
    assert_eq!(first.datum_dimensions["second_depth"].value_mm, 7.);
    let dimensions = &first.section_views["through_section"].slot_dimensions;
    assert_eq!(dimensions.len(), 2);
    assert!((dimensions[0].value_mm - 6.).abs() < 1e-8);
    assert!((dimensions[1].value_mm - 18.).abs() < 1e-8);
    assert_eq!(dimensions[0].selection.witnesses.len(), 4);
    assert_eq!(
        first.slot_center_datums["machined"]["near_center"]
            .walls
            .witnesses
            .len(),
        4
    );
    let warm = engine
        .build_outputs(&recipe(value(7.)), &["through".into()])
        .unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(
        warm.section_views["through_section"].slot_dimensions.len(),
        2
    );
    let changed = engine
        .build_outputs(&recipe(value(8.)), &["through".into()])
        .unwrap();
    assert_eq!(
        changed.feature_hashes["through"],
        first.feature_hashes["through"]
    );
    assert_eq!(
        changed.feature_hashes["locating"],
        first.feature_hashes["locating"]
    );
    assert_ne!(
        changed.feature_hashes["relief"],
        first.feature_hashes["relief"]
    );
    assert_eq!(changed.datum_dimensions["first_depth"].value_mm, 3.);
    assert_eq!(changed.datum_dimensions["second_depth"].value_mm, 8.);
    assert_eq!(
        changed.section_views["through_section"]
            .slot_dimensions
            .len(),
        2
    );
}

#[test]
fn selected_cached_mixed_model_rechecks_native_through_and_first_floor_obligations() {
    let mut engine = BuildEngine::default();
    engine
        .build_outputs(&recipe(value(7.)), &["through".into()])
        .unwrap();
    let mut wrong_width = value(7.);
    wrong_width["section_views"][0]["slot_dimensions"][0]["min"] = json!(6.1);
    wrong_width["section_views"][0]["slot_dimensions"][0]["max"] = json!(6.2);
    assert!(matches!(
        engine.build_outputs(&recipe(wrong_width), &["through".into()]),
        Err(BuildError::SectionDimensionFailed { .. })
    ));
    let mut wrong_floor = value(7.);
    wrong_floor["datum_dimensions"][0]["min"] = json!(3.1);
    wrong_floor["datum_dimensions"][0]["max"] = json!(3.2);
    assert!(matches!(
        engine.build_outputs(&recipe(wrong_floor), &["through".into()]),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
    let mut absent = value(7.);
    absent["slot_center_datums"][0]["role"] = json!("slot_missing_walls");
    assert!(matches!(
        engine.build_outputs(&recipe(absent), &["through".into()]),
        Err(BuildError::SlotCenterDatumFailed { .. })
    ));
}
