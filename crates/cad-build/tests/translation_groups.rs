use cad_build::{BuildEngine, Recipe};
use serde_json::{json, Value};

fn value() -> Value {
    json!({"schema_version":2,"units":"mm",
        "features":[{"op":"rectangle","id":"p","width":2,"depth":3},
                    {"op":"extrude","id":"body","profile":"p","height":4}],
        "outputs":[{"name":"body","feature":"body"}],
        "parts":[{"id":"block","feature":"body"}],
        "assemblies":[{"id":"root","instances":[
            {"id":"a","part":"block"},
            {"id":"b","part":"block","transform":{"translation":[5,7,0]}}]}],
        "assembly_outputs":[{"name":"pair","assembly":"root"}],
        "rigid_translation_groups":[{"id":"slide","output":"pair",
            "instance_paths":[["a"],["b"]],"reference_instance_path":["a"],
            "translation":[2,0,0]}],
        "clearance_checks":[{"id":"gap","output":"pair","left_instance_path":["a"],
            "right_instance_path":["b"],"min_clearance":4.9,"translation_group":"slide"}]
    })
}

fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}

#[test]
fn common_translation_preserves_native_distance_on_cached_selected_build() {
    let mut recipe = parse(value());
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&recipe, &["body".into()]).unwrap();
    let check = &first.clearance_checks["gap"];
    assert_eq!(check.motion_proof.as_deref(), Some("common_rigid_motion"));
    assert_eq!(check.moving_endpoint.as_deref(), Some("both"));
    assert!(check.sweep.is_none());
    assert_eq!(first.rigid_translation_groups["slide"].members.len(), 2);
    assert_eq!(
        first.rigid_translation_groups["slide"].translation_world,
        [2., 0., 0.]
    );
    recipe.rigid_translation_groups[0].translation = [-2., 0., 0.];
    let second = engine.build_outputs(&recipe, &["body".into()]).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(
        first.clearance_checks["gap"].lower_bound_mm,
        second.clearance_checks["gap"].lower_bound_mm
    );
    recipe.clearance_checks[0].min_clearance = 5.1;
    assert!(engine.build_outputs(&recipe, &["body".into()]).is_err());
}

#[test]
fn invalid_translation_authority_fails_before_geometry() {
    let mut cases = Vec::new();
    let mut v = value();
    v["rigid_translation_groups"][0]["reference_instance_path"] = json!(["missing"]);
    cases.push(v);
    let mut v = value();
    v["rigid_translation_groups"][0]["instance_paths"] = json!([["a"], ["a"]]);
    cases.push(v);
    let mut v = value();
    v["rigid_translation_groups"][0]["translation"] = json!([0, 0, 0]);
    cases.push(v);
    let mut v = value();
    v["clearance_checks"] = json!([]);
    cases.push(v);
    let mut v = value();
    v["clearance_checks"][0]["translation_group"] = json!("missing");
    cases.push(v);
    let mut v = value();
    v["clearance_checks"][0]["right_translation"] = json!([1, 0, 0]);
    cases.push(v);
    let mut v = value();
    v["rigid_translation_groups"][0]["instance_paths"] = json!([["a"]]);
    v["clearance_checks"][0]["left_instance_path"] = json!(["missing"]);
    cases.push(v);
    for v in cases {
        assert!(cad_build::build(&parse(v)).is_err());
    }
    for malformed in [json!(null), json!({}), json!([{"unexpected":true}])] {
        let mut v = value();
        v["rigid_translation_groups"] = malformed;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v = value();
    v["schema_version"] = json!(1);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
}

#[test]
fn reference_frame_rotates_one_world_vector_for_all_members() {
    let mut v = value();
    let q = [
        0.,
        0.,
        std::f64::consts::FRAC_1_SQRT_2,
        std::f64::consts::FRAC_1_SQRT_2,
    ];
    v["assemblies"][0]["instances"][0]["transform"] =
        json!({"translation":[10,-20,30],"rotation":q});
    v["assemblies"][0]["instances"][1]["transform"] =
        json!({"translation":[3,-15,30],"rotation":q});
    let built = cad_build::build(&parse(v)).unwrap();
    let world = built.rigid_translation_groups["slide"].translation_world;
    assert!((world[0]).abs() < 1e-12 && (world[1] - 2.).abs() < 1e-12 && world[2].abs() < 1e-12);
}

#[test]
fn cancelling_ancestor_offsets_cannot_hide_motion_roundoff() {
    let mut v = value();
    v["assemblies"] = json!([
        {"id":"moving_definition","instances":[
            {"id":"b","part":"block","transform":{"translation":[-999999995.,7.,0.]}}]},
        {"id":"root","instances":[{"id":"a","part":"block"}],
            "assemblies":[{"id":"moving","assembly":"moving_definition",
                "transform":{"translation":[1000000000.,0.,0.]}}]}
    ]);
    v["rigid_translation_groups"][0]["instance_paths"] = json!([["moving", "b"]]);
    v["rigid_translation_groups"][0]["reference_instance_path"] = json!(["moving", "b"]);
    v["clearance_checks"][0]["right_instance_path"] = json!(["moving", "b"]);
    assert!(matches!(
        cad_build::build(&parse(v)),
        Err(cad_build::BuildError::TranslationGroupFailed { .. })
    ));
}
