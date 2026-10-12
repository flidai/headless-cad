use cad_build::*;
use serde_json::{json, Value};
fn source(angle: f64) -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":10,"depth":2},{"op":"extrude","id":"body","profile":"p","height":1}],"outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"part","feature":"body","datums":{"origin":{}}}],"assemblies":[
        {"id":"child","instances":[{"id":"leaf","part":"part","transform":{"translation":[0,0,2]}}]},
        {"id":"module","instances":[{"id":"bar","part":"part","transform":{"translation":[1,0,0]}},{"id":"follower","part":"part","attachment":{"instance":"bar","datum":"origin","target_datum":"origin"}}],"assemblies":[{"id":"rigid","assembly":"child"}]},
        {"id":"root","assemblies":[{"id":"left","assembly":"module"},{"id":"right","assembly":"module","transform":{"translation":[30,0,0]}}]}],"assembly_outputs":[{"name":"scene","assembly":"root"}],
        "mechanisms":[{"id":"motion","frame":{"translation":[3,4,5]},"input":{"pins":[{"id":"a","initial":[0,0],"ground":[0,0]},{"id":"b","initial":[8,6]}],"links":[{"id":"bar","from":"a","to":"b","length":10}],"drives":[{"id":"drive","link":"bar","angle_radians":angle}]}}],
        "mechanism_bindings":[{"assembly":"module","instance":"bar","mechanism":"motion","link":"bar"},{"assembly":"module","instance":"rigid","mechanism":"motion","link":"bar"}]})
}
fn recipe(value: Value) -> Recipe {
    serde_json::from_value(value).unwrap()
}
#[test]
fn solved_link_placements_reuse_geometry_and_compose_nested_definitions() {
    let mut engine = BuildEngine::default();
    let r = recipe(source(0.6));
    let first = engine.build(&r).unwrap();
    let a = &first.assemblies["scene"];
    assert_eq!(a.instances.len(), 6);
    assert_eq!(a.parts.len(), 1);
    for (side, shift) in [("left", 0.), ("right", 30.)] {
        for name in ["bar", "follower"] {
            let i = a
                .instances
                .iter()
                .find(|i| a.instance_paths[&i.id] == [side, name])
                .unwrap();
            for (actual, expected) in i.transform.translation.into_iter().zip([
                3. + shift + 0.6_f64.cos(),
                4. + 0.6_f64.sin(),
                5.,
            ]) {
                assert!((actual - expected).abs() < 1e-7);
            }
        }
        let i = a
            .instances
            .iter()
            .find(|i| a.instance_paths[&i.id] == [side, "rigid", "leaf"])
            .unwrap();
        assert_eq!(i.transform.translation, [3. + shift, 4., 7.]);
    }
    let second = engine.build(&recipe(source(0.9))).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(first.feature_hashes, second.feature_hashes);
    assert_ne!(a.content_hash, second.assemblies["scene"].content_hash);
    assert_ne!(first.mechanism_context_hash, second.mechanism_context_hash);
    let replay: Recipe = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
    assert_eq!(
        first.mechanism_context_hash,
        engine.build(&replay).unwrap().mechanism_context_hash
    );
    assert_eq!(first.mechanism_bindings.len(), 2);
    assert_eq!(first.mechanism_reports["motion"].definition.id, "motion");
}
#[test]
fn source_intent_remains_distinct_when_solved_pose_is_identical() {
    let mut a = source(0.6);
    let first = build(&recipe(a.clone())).unwrap();
    a["mechanisms"][0]["input"]["drives"][0]["id"] = json!("renamed_source_drive");
    let second = build(&recipe(a)).unwrap();
    assert_eq!(
        first.assemblies["scene"].content_hash,
        second.assemblies["scene"].content_hash
    );
    assert_ne!(first.mechanism_context_hash, second.mechanism_context_hash);
}
#[test]
fn global_failures_cannot_be_bypassed_by_selected_body_or_cached_geometry() {
    let mut engine = BuildEngine::default();
    engine.build(&recipe(source(0.6))).unwrap();
    for mutation in 0..6 {
        let mut value = source(0.6);
        match mutation {
            0 => value["mechanisms"][0]["input"]["drives"] = json!([]),
            1 => value["mechanism_bindings"][0]["instance"] = json!("missing"),
            2 => value["mechanism_bindings"][0]["link"] = json!("missing"),
            3 => value["mechanism_bindings"][0]["instance"] = json!("follower"),
            4 => {
                let binding = value["mechanism_bindings"][0].clone();
                value["mechanism_bindings"]
                    .as_array_mut()
                    .unwrap()
                    .push(binding);
            }
            _ => value["mechanisms"][0]["input"]["links"][0]["length"] = json!(-1),
        }
        assert!(
            engine
                .build_outputs(&recipe(value), &["body".into()])
                .is_err(),
            "mutation {mutation}"
        );
    }
    let mut value = source(0.6);
    value["mechanisms"][0]["input"]["drives"] = json!([]);
    match engine.build(&recipe(value)).unwrap_err() {
        BuildError::MechanismFailed { report, .. } => assert!(report.is_some()),
        other => panic!("{other}"),
    }
}
#[test]
fn strict_schema_rejects_unknown_fields_and_legacy_presence() {
    for field in ["mechanisms", "mechanism_bindings"] {
        assert!(serde_json::from_value::<Recipe>(
            json!({"schema_version":1,"units":"mm","features":[],field:[]})
        )
        .is_err());
    }
    let mut value = source(0.6);
    value["mechanisms"][0]["input"]["links"][0]["typo"] = json!(1);
    assert!(serde_json::from_value::<Recipe>(value).is_err());
}

#[test]
fn malformed_binding_preflight_precedes_iterative_solution() {
    let mut value = source(0.6);
    value["mechanisms"][0]["input"]["drives"] = json!([]); // Would fail rank if solved.
    value["mechanism_bindings"][0]["instance"] = json!("missing");
    match build(&recipe(value)).unwrap_err() {
        BuildError::MechanismFailed {
            message, report, ..
        } => {
            assert!(message.contains("missing binding occurrence"));
            assert!(report.is_none());
        }
        other => panic!("{other}"),
    }
}
