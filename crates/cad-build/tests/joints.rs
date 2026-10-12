use cad_build::*;
use serde_json::{json, Value};
fn recipe(position: f64) -> Recipe {
    serde_json::from_value(json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":4,"depth":3},{"op":"extrude","id":"body","profile":"p","height":2}],"outputs":[{"name":"body","feature":"body"}],"parts":[{"id":"part","feature":"body","datums":{"origin":{},"top":{"translation":[0,0,2]}}}],"assemblies":[
{"id":"module","instances":[{"id":"base","part":"part"},{"id":"slide","part":"part","transform":{"translation":[0,0,1]},"attachment":{"instance":"base","datum":"origin","target_datum":"top","joint":{"kind":"prismatic","position":position,"min":0,"max":10}}},{"id":"turn","part":"part","attachment":{"instance":"slide","datum":"origin","target_datum":"top","joint":{"kind":"revolute","position":std::f64::consts::FRAC_PI_2,"min":-2,"max":2}}}]},
{"id":"root","assemblies":[{"id":"left","assembly":"module"},{"id":"right","assembly":"module","transform":{"translation":[30,0,0]}}]}],"assembly_outputs":[{"name":"scene","assembly":"root"}]})).unwrap()
}
fn occurrence<'a>(a: &'a cad_assembly::BuiltAssembly, path: &[&str]) -> &'a cad_assembly::Instance {
    let p: Vec<_> = path.iter().map(|s| s.to_string()).collect();
    a.instances
        .iter()
        .find(|i| a.instance_paths[&i.id] == p)
        .unwrap()
}
#[test]
fn prescribed_motion_composes_chains_repeated_modules_and_reuses_part_solids() {
    let mut engine = BuildEngine::default();
    let first = engine.build(&recipe(3.)).unwrap();
    let a = &first.assemblies["scene"];
    assert_eq!(a.instances.len(), 6);
    assert_eq!(a.parts.len(), 1);
    for (side, x) in [("left", 0.), ("right", 30.)] {
        let slide = occurrence(a, &[side, "slide"]);
        assert_eq!(slide.transform.translation, [x, 0., 6.]);
        let turn = occurrence(a, &[side, "turn"]);
        assert_eq!(turn.transform.translation, [x, 0., 8.]);
        let q = turn.transform.rotation;
        assert!((q[2] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((q[3] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
    }
    let second = engine.build(&recipe(5.)).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(
        first.outputs["body"].content_hash,
        second.outputs["body"].content_hash
    );
    assert_ne!(a.content_hash, second.assemblies["scene"].content_hash);
    assert_eq!(
        occurrence(&second.assemblies["scene"], &["right", "turn"])
            .transform
            .translation,
        [30., 0., 10.]
    );
    let r = recipe(3.);
    let bytes = serde_json::to_string(&r).unwrap();
    assert_eq!(
        bytes,
        serde_json::to_string(&serde_json::from_str::<Recipe>(&bytes).unwrap()).unwrap()
    );
}
#[test]
fn joint_limits_and_schema_are_global_even_for_selected_body() {
    let good = recipe(3.);
    let value = serde_json::to_value(&good).unwrap();
    for change in [
        json!({"kind":"prismatic","position":11,"min":0,"max":10}),
        json!({"kind":"revolute","position":0,"min":2,"max":-2}),
    ] {
        let mut v = value.clone();
        v["assemblies"][0]["instances"][1]["attachment"]["joint"] = change;
        let r: Recipe = serde_json::from_value(v).unwrap();
        assert!(BuildEngine::default()
            .build_outputs(&r, &["body".into()])
            .is_err());
    }
    for change in [
        json!({"kind":"free","position":1,"min":0,"max":2}),
        json!({"kind":"prismatic","position":1,"min":0,"max":2,"axis":[1,0,0]}),
        json!({"kind":"revolute","position":0,"min":-1}),
    ] {
        let mut v = value.clone();
        v["assemblies"][0]["instances"][1]["attachment"]["joint"] = change;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut bad = good;
    bad.assemblies[0].instances[1]
        .attachment
        .as_mut()
        .unwrap()
        .joint = Some(PrescribedJoint::Prismatic {
        position: f64::NAN,
        min: 0.,
        max: 10.,
    });
    assert!(BuildEngine::default()
        .build_outputs(&bad, &["body".into()])
        .is_err());
    let mut legacy: Value = value;
    legacy["assemblies"][0]["instances"][1]["attachment"]
        .as_object_mut()
        .unwrap()
        .remove("joint");
    let r: Recipe = serde_json::from_value(legacy).unwrap();
    assert!(build(&r).is_ok());
}

#[test]
fn aggregate_joint_budget_rejects_unused_assembly_before_selected_body_build() {
    let mut r = recipe(3.);
    let template = r.assemblies[0].instances[1].clone();
    r.assemblies[0].instances.truncate(1);
    for i in 0..=MAX_PRESCRIBED_JOINTS {
        let mut instance = template.clone();
        instance.id = format!("joint_{i}");
        r.assemblies[0].instances.push(instance);
    }
    let over_budget = r.assemblies[0].instances.pop().unwrap();
    assert!(BuildEngine::default()
        .build_outputs(&r, &["body".into()])
        .is_ok());
    r.assemblies[0].instances.push(over_budget);
    let error = BuildEngine::default()
        .build_outputs(&r, &["body".into()])
        .unwrap_err()
        .to_string();
    assert!(error.contains("budget"), "{error}");
}
