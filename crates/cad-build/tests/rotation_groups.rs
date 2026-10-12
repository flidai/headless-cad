use cad_build::{BuildEngine, Recipe};
use serde_json::{json, Value};
fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":2,"depth":3},{"op":"extrude","id":"body","profile":"p","height":4}],"outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"block","feature":"body"}],"assemblies":[{"id":"root","instances":[{"id":"a","part":"block"},{"id":"b","part":"block","transform":{"translation":[5,7,0]}}]}],"assembly_outputs":[{"name":"pair","assembly":"root"}],"rigid_rotation_groups":[{"id":"rotor","output":"pair","instance_paths":[["a"],["b"]],"reference_instance_path":["a"],"pivot":[0,0,0],"axis":[0,0,1],"angle_radians":std::f64::consts::TAU}],"clearance_checks":[{"id":"gap","output":"pair","left_instance_path":["a"],"right_instance_path":["b"],"min_clearance":4.9,"rotation_group":"rotor"}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn common_map_keeps_distance_and_global_checks_on_cached_selected_output() {
    let mut r = parse(value());
    let mut engine = BuildEngine::default();
    let a = engine.build_outputs(&r, &["body".into()]).unwrap();
    let gap = &a.clearance_checks["gap"];
    assert_eq!(gap.motion_proof.as_deref(), Some("common_rigid_motion"));
    assert!(gap.rotation_sweep.is_none());
    assert!(gap.lower_bound_mm < 5. && gap.upper_bound_mm > 5.);
    assert_eq!(a.rigid_rotation_groups["rotor"].members.len(), 2);
    r.rigid_rotation_groups[0].angle_radians = -1.;
    let b = engine.build_outputs(&r, &["body".into()]).unwrap();
    assert_eq!(b.stats.computed_solids, 0);
    assert_eq!(gap.lower_bound_mm, b.clearance_checks["gap"].lower_bound_mm);
    r.clearance_checks[0].min_clearance = 5.1;
    assert!(engine.build_outputs(&r, &["body".into()]).is_err());
}
#[test]
fn invalid_group_authority_is_rejected_before_geometry() {
    let mut cases = vec![];
    let mut v = value();
    v["rigid_rotation_groups"][0]["reference_instance_path"] = json!(["missing"]);
    cases.push(v);
    let mut v = value();
    v["rigid_rotation_groups"][0]["instance_paths"] = json!([["a"], ["a"]]);
    cases.push(v);
    let mut v = value();
    v["clearance_checks"] = json!([]);
    cases.push(v);
    let mut v = value();
    v["clearance_checks"][0]["rotation_group"] = json!("missing");
    cases.push(v);
    let mut v = value();
    v["clearance_checks"][0]["right_translation"] = json!([1, 0, 0]);
    cases.push(v);
    let mut v = value();
    let mut g = v["rigid_rotation_groups"][0].clone();
    g["id"] = json!("other");
    v["rigid_rotation_groups"].as_array_mut().unwrap().push(g);
    cases.push(v);
    let mut v = value();
    v["rigid_rotation_groups"][0]["angle_radians"] = json!(0);
    cases.push(v);
    let mut v = value();
    v["rigid_rotation_groups"][0]["axis"] = json!([0, 0, 2]);
    cases.push(v);
    for v in cases {
        assert!(cad_build::build(&parse(v)).is_err());
    }
    for field in [json!(null), json!({}), json!([{"unexpected":true}])] {
        let mut v = value();
        v["rigid_rotation_groups"] = field;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut v = value();
    v["schema_version"] = json!(1);
    assert!(serde_json::from_value::<Recipe>(v).is_err());
}
#[test]
fn remote_unqueried_members_cannot_hide_unresolved_common_motion() {
    let mut v = value();
    v["assemblies"][0]["instances"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"remote","part":"block","transform":{"translation":[1e12,0,0]}}));
    v["rigid_rotation_groups"][0]["instance_paths"]
        .as_array_mut()
        .unwrap()
        .push(json!(["remote"]));
    assert!(cad_build::build(&parse(v)).is_err());
}

#[test]
fn common_world_path_is_reference_resolved_and_motion_budget_is_global() {
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
    v["rigid_rotation_groups"][0]["pivot"] = json!([1, 0, 0]);
    let built = cad_build::build(&parse(v)).unwrap();
    let group = &built.rigid_rotation_groups["rotor"];
    for (actual, expected) in group.rotation_world.pivot.into_iter().zip([10., -19., 30.]) {
        assert!((actual - expected).abs() < 1e-12);
    }
    assert!(built.clearance_checks["gap"].lower_bound_mm < 5.);
    let mut v = value();
    let checks = (0..17)
        .map(|i| {
            let mut c = v["clearance_checks"][0].clone();
            c["id"] = json!(format!("c{i}"));
            c
        })
        .collect::<Vec<_>>();
    v["clearance_checks"] = json!(checks);
    assert!(matches!(
        cad_build::build(&parse(v)),
        Err(cad_build::BuildError::ResourceLimit { .. })
    ));
}
