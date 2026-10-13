use cad_build::{
    BuildEngine, BuildError, FaceCardinality, FaceResolutionStatus, FaceSelector, Recipe,
};
use serde_json::{json, Value};

fn serial_value(second_depth: f64) -> Value {
    json!({
        "schema_version":2,"units":"mm",
        "features":[
            {"op":"rectangle","id":"outline","width":100,"depth":80},
            {"op":"extrude","id":"stock","profile":"outline","height":12},
            {"op":"blind_slot_pattern","id":"first","body":"stock",
                "face":{"from_feature":"stock","role":"end_cap"},
                "depth":4,"frame_origin":[0,0,12],
                "pattern_slots":[{"id":"left","center":[25,40],"direction":[0,1],
                    "centerline_length":20,"radius":4}]},
            {"op":"blind_slot_pattern","id":"second","body":"first",
                "face":{"from_feature":"stock","role":"end_cap"},
                "depth":second_depth,"frame_origin":[0,0,12],
                "pattern_slots":[{"id":"right","center":[75,40],"direction":[0,1],
                    "centerline_length":20,"radius":4}]}
        ],
        "outputs":[{"name":"first","feature":"first"},{"name":"final","feature":"second"}],
        "parts":[{"id":"machined","feature":"second"}],
        "face_datums":[
            {"part":"machined","name":"opening","anchor_feature":"stock","role":"end_cap","x_direction":[1,0,0]},
            {"part":"machined","name":"first_floor","anchor_feature":"first","role":"slot_left_floor","x_direction":[1,0,0]},
            {"part":"machined","name":"second_floor","anchor_feature":"second","role":"slot_right_floor","x_direction":[1,0,0]}
        ],
        "assemblies":[{"id":"fixture","instances":[{"id":"body","part":"machined"}]}],
        "assembly_outputs":[{"name":"fixture","assembly":"fixture"}],
        "datum_dimensions":[
            {"id":"first_depth","output":"fixture","from":{"instance_path":["body"],"datum":"opening"},
                "to":{"instance_path":["body"],"datum":"first_floor"},"direction":-1,
                "min":3.999999,"max":4.000001},
            {"id":"second_depth","output":"fixture","from":{"instance_path":["body"],"datum":"opening"},
                "to":{"instance_path":["body"],"datum":"second_floor"},"direction":-1,
                "min":second_depth-0.000001,"max":second_depth+0.000001}
        ]
    })
}

fn recipe(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}

fn face(build: &cad_build::BuildResult, anchor: &str, role: &str) -> usize {
    let resolution = build.resolve_faces(&FaceSelector {
        anchor_feature: anchor.into(),
        role: role.into(),
        target_feature: "second".into(),
        cardinality: FaceCardinality::Exactly { count: 1 },
    });
    assert_eq!(
        resolution.status,
        FaceResolutionStatus::Resolved,
        "{anchor}: {role}"
    );
    resolution.witnesses[0].target_face.0
}

#[test]
fn serial_different_depths_keep_first_floor_and_source_lineage() {
    let mut engine = BuildEngine::default();
    let first = engine
        .build_outputs(&recipe(serial_value(6.)), &["final".into()])
        .unwrap();
    let final_body = &first.outputs["final"].result.model;
    let area = 20. * 8. + std::f64::consts::PI * 16.;
    assert!((final_body.volume() - (100. * 80. * 12. - area * 10.)).abs() < 1e-6);
    for (anchor, role, floor_z) in [
        ("first", "slot_left_floor", 8.),
        ("second", "slot_right_floor", 6.),
    ] {
        let id = face(&first, anchor, role);
        assert!((final_body.faces[id].try_plane().unwrap().origin[2] - floor_z).abs() < 1e-8);
    }
    assert_eq!(first.datum_dimensions["first_depth"].value_mm, 4.);
    assert_eq!(first.datum_dimensions["second_depth"].value_mm, 6.);
    let warm = engine
        .build_outputs(&recipe(serial_value(6.)), &["final".into()])
        .unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(
        face(&warm, "first", "slot_left_floor"),
        face(&first, "first", "slot_left_floor")
    );
    let revised = engine
        .build_outputs(&recipe(serial_value(7.)), &["final".into()])
        .unwrap();
    assert_eq!(
        revised.feature_hashes["first"],
        first.feature_hashes["first"]
    );
    assert_ne!(
        revised.feature_hashes["second"],
        first.feature_hashes["second"]
    );
    assert_eq!(revised.datum_dimensions["first_depth"].value_mm, 4.);
    assert_eq!(revised.datum_dimensions["second_depth"].value_mm, 7.);
    let revised_body = &revised.outputs["final"].result.model;
    assert!(
        (revised_body.faces[face(&revised, "first", "slot_left_floor")]
            .try_plane()
            .unwrap()
            .origin[2]
            - 8.)
            .abs()
            < 1e-8
    );
}

#[test]
fn selected_warm_build_still_checks_first_floor_measurement() {
    let mut engine = BuildEngine::default();
    engine
        .build_outputs(&recipe(serial_value(6.)), &["first".into()])
        .unwrap();
    let mut changed = serial_value(6.);
    changed["datum_dimensions"][0]["min"] = json!(4.1);
    changed["datum_dimensions"][0]["max"] = json!(4.2);
    let error = engine
        .build_outputs(&recipe(changed), &["first".into()])
        .unwrap_err();
    assert!(
        matches!(error, BuildError::DatumDimensionFailed { .. }),
        "{error:?}"
    );
    let mut missing = serial_value(6.);
    missing["face_datums"][1]["role"] = json!("slot_missing_floor");
    assert!(engine
        .build_outputs(&recipe(missing), &["first".into()])
        .is_err());
}

#[test]
fn serial_cuts_refuse_overlap_opposite_cap_and_fifth_pocket() {
    for center in [[25, 40], [32, 40]] {
        let mut overlap = serial_value(6.);
        overlap["features"][3]["pattern_slots"][0]["center"] = json!(center);
        let error = BuildEngine::default().build(&recipe(overlap)).unwrap_err();
        assert!(
            matches!(error, BuildError::Kernel { .. }),
            "{center:?}: {error:?}"
        );
    }
    let mut opposite = serial_value(6.);
    opposite["features"][3]["face"] = json!({"from_feature":"stock","role":"start_cap"});
    opposite["features"][3]["frame_origin"] = json!([0, 0, 0]);
    assert!(matches!(
        BuildEngine::default().build(&recipe(opposite)),
        Err(BuildError::Kernel { .. })
    ));
    let mut fifth = serial_value(6.);
    fifth["features"][2]["pattern_slots"] = json!([
        {"id":"left","center":[25,40],"direction":[0,1],"centerline_length":20,"radius":4},
        {"id":"a","center":[50,20],"direction":[0,1],"centerline_length":10,"radius":3},
        {"id":"b","center":[50,60],"direction":[0,1],"centerline_length":10,"radius":3}
    ]);
    let four = BuildEngine::default()
        .build(&recipe(fifth.clone()))
        .unwrap();
    for (anchor, name) in [
        ("first", "left"),
        ("first", "a"),
        ("first", "b"),
        ("second", "right"),
    ] {
        let id = face(&four, anchor, &format!("slot_{name}_floor"));
        assert!(four.outputs["final"].result.model.faces[id]
            .try_plane()
            .is_some());
    }
    fifth["features"][3]["pattern_slots"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"e","center":[50,40],"direction":[0,1],"centerline_length":10,"radius":3
        }));
    assert!(matches!(
        BuildEngine::default().build(&recipe(fifth)),
        Err(BuildError::Kernel { .. })
    ));
}
