use cad_build::*;
use serde_json::{json, Value};
fn source() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":10,"depth":2},{"op":"extrude","id":"body","profile":"p","height":1}],"outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"part","feature":"body"}],"assemblies":[{"id":"root","instances":[{"id":"bar","part":"part"}]}],"assembly_outputs":[{"name":"scene","assembly":"root"}],
        "mechanisms":[{"id":"motion","input":{"pins":[{"id":"a","initial":[0,0],"ground":[0,0]},{"id":"b","initial":[8,6]}],"links":[{"id":"bar","from":"a","to":"b","length":10}],"drives":[{"id":"drive","link":"bar","angle_radians":0.6}]}}],
        "mechanism_bindings":[{"assembly":"root","instance":"bar","mechanism":"motion","link":"bar"}],
        "motion_studies":[{"id":"cycle","mechanism":"motion","input":{"targets":[{"drive":0.6},{"drive":0.9},{"drive":0.3}],"max_drive_step_radians":0.1,"max_pin_step_mm":2.0}}]})
}
fn recipe(value: Value) -> Recipe {
    serde_json::from_value(value).unwrap()
}
#[test]
fn sampling_preserves_static_pose_reuses_geometry_and_captures_source() {
    let mut engine = BuildEngine::default();
    let r = recipe(source());
    let first = engine.build(&r).unwrap();
    let study = &first.motion_studies["cycle"];
    assert_eq!(study.solution.completed_requested_frames, 3);
    assert!(study.solution.frames.len() > 3);
    for frame in &study.solution.frames {
        let angle = frame.coordinates["drive"];
        let pin = frame.pins["b"];
        assert!((pin[0] - 10. * angle.cos()).abs() < 1e-6);
        assert!((pin[1] - 10. * angle.sin()).abs() < 1e-6);
        assert!(frame.max_drive_step_radians <= 0.1 + 1e-12);
        assert!(frame.max_pin_step_mm <= 2.0 + 1e-12);
    }
    let mut value = source();
    value["motion_studies"][0]["input"]["targets"][1]["drive"] = json!(1.1);
    let second = engine.build(&recipe(value)).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(
        first.assemblies["scene"].content_hash,
        second.assemblies["scene"].content_hash
    );
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert_ne!(first.mechanism_context_hash, second.mechanism_context_hash);
    let replay: Recipe = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
    assert_eq!(
        first.mechanism_context_hash,
        engine.build(&replay).unwrap().mechanism_context_hash
    );
    let selected = engine.build_outputs(&r, &["body".into()]).unwrap();
    assert_eq!(
        selected.motion_studies["cycle"].solution.frames.len(),
        study.solution.frames.len()
    );
    assert!(selected.assemblies.is_empty());
}
#[test]
fn malformed_global_studies_fail_before_solving_and_ignore_output_selection() {
    for mutation in 0..6 {
        let mut value = source();
        match mutation {
            0 => value["motion_studies"][0]["mechanism"] = json!("missing"),
            1 => value["motion_studies"][0]["input"]["targets"][0]["drive"] = json!(0.5),
            2 => value["motion_studies"][0]["input"]["targets"][1] = json!({"unknown":0.9}),
            3 => value["motion_studies"][0]["input"]["max_pin_step_mm"] = json!(0),
            4 => {
                let item = value["motion_studies"][0].clone();
                value["motion_studies"].as_array_mut().unwrap().push(item);
            }
            _ => value["motion_studies"][0]["input"]["targets"] = json!([]),
        }
        let error = BuildEngine::default()
            .build_outputs(&recipe(value), &["body".into()])
            .unwrap_err();
        assert!(
            matches!(error, BuildError::MotionStudyFailed { report: None, .. }),
            "{error}"
        );
    }
}
#[test]
fn failed_sampling_retains_prefix_and_budget_evidence() {
    let mut value = source();
    value["motion_studies"][0]["input"]["max_drive_step_radians"] = json!(1e-6);
    let error = BuildEngine::default()
        .build_outputs(&recipe(value), &["body".into()])
        .unwrap_err();
    match error {
        BuildError::MotionStudyFailed {
            report: Some(report),
            ..
        } => {
            assert!(!report.accepted.frames.is_empty());
            assert!(report.accepted.attempted_solves > 0);
            assert!(!report.failed_coordinates.is_empty());
        }
        other => panic!("{other}"),
    }
}
#[test]
fn study_count_and_ir_schema_are_strict() {
    let mut value = source();
    let template = value["motion_studies"][0].clone();
    value["motion_studies"] = json!((0..=MAX_MOTION_STUDIES)
        .map(|i| {
            let mut d = template.clone();
            d["id"] = json!(format!("s{i}"));
            d
        })
        .collect::<Vec<_>>());
    assert!(matches!(
        build(&recipe(value)),
        Err(BuildError::ResourceLimit {
            resource: "motion studies",
            ..
        })
    ));
    assert!(serde_json::from_value::<Recipe>(
        json!({"schema_version":1,"units":"mm","features":[],"motion_studies":[]})
    )
    .is_err());
    let mut value = source();
    value["motion_studies"][0]["input"]["typo"] = json!(1);
    assert!(serde_json::from_value::<Recipe>(value).is_err());
}

#[test]
fn publication_ids_are_safe_and_unique_ignoring_case() {
    for id in ["../outside", "a/b", "", "name.html"] {
        let mut value = source();
        value["motion_studies"][0]["id"] = json!(id);
        assert!(build(&recipe(value)).is_err());
    }
    let mut value = source();
    let mut duplicate = value["motion_studies"][0].clone();
    duplicate["id"] = json!("CYCLE");
    value["motion_studies"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(build(&recipe(value)).is_err());
}

#[test]
fn shared_attempt_budget_applies_across_individually_bounded_studies() {
    let mut value = source();
    let mut template = value["motion_studies"][0].clone();
    template["input"]["targets"] = json!([{"drive":0.6},{"drive":0.7}]);
    template["input"]["max_drive_step_radians"] = json!(0.001);
    value["motion_studies"] = json!((0..MAX_MOTION_STUDIES)
        .map(|i| {
            let mut d = template.clone();
            d["id"] = json!(format!("study_{i:02}"));
            d
        })
        .collect::<Vec<_>>());
    match build(&recipe(value)).unwrap_err() {
        BuildError::MotionStudyFailed {
            id,
            report: Some(report),
            ..
        } => {
            assert_ne!(id, "study_00", "one study should fit its local budget");
            assert!(
                report.accepted.attempted_solves < 512,
                "aggregate budget must fail before the local cap"
            );
        }
        other => panic!("{other}"),
    }
}
