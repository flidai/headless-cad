use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{Value, json};
fn value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"rectangle","id":"p","width":20,"depth":12},{"op":"extrude","id":"stock","profile":"p","height":10},{"op":"fillet","id":"rounded","body":"stock","faces":[{"from_feature":"stock","role":"wall_x_min"},{"from_feature":"stock","role":"wall_y_min"}],"radius":2}],"outputs":[{"name":"finished","feature":"rounded"},{"name":"stock","feature":"stock"}],"section_views":[{"id":"mid","output":"finished","origin":[0,0,5],"circle_dimensions":[{"id":"corner","anchor_feature":"rounded","role":"fillet_wall","kind":"radius","coverage":"single_arc","label_offset":[-4,-4],"min":1.9,"max":2.1}]}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn radius_witness_and_bounds_remain_global_on_selected_outputs_and_cache() {
    let mut r = parse(value());
    let mut engine = BuildEngine::default();
    let first = engine.build_outputs(&r, &["stock".into()]).unwrap();
    let d = &first.section_views["mid"].circle_dimensions[0];
    let a = d.arc.as_ref().unwrap();
    assert_eq!(d.value_mm, 2.);
    assert_eq!(a.target_face, d.selection.witnesses[0].target_face);
    assert!((a.sweep_angle.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    for x in a.tip {
        assert!((x - (2. - 2_f64.sqrt())).abs() < 1e-12);
    }
    assert!(a.arithmetic_uncertainty_mm > 0.);
    assert_eq!(
        engine
            .build_outputs(&r, &["stock".into()])
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    r.section_views[0].circle_dimensions[0].max = Some(1.99);
    assert!(matches!(
        engine.build_outputs(&r, &["stock".into()]),
        Err(BuildError::SectionDimensionFailed { .. })
    ));
    r.section_views[0].circle_dimensions[0].max = Some(2.1);
    if let cad_build::Feature::Fillet { radius, .. } = &mut r.features[2] {
        *radius = 3.;
    }
    assert!(matches!(
        engine.build_outputs(&r, &["stock".into()]),
        Err(BuildError::SectionDimensionFailed { .. })
    ));
}
#[test]
fn partial_radius_is_opt_in_and_rejects_wrong_kind_support_or_precision() {
    for mutation in 0..4 {
        let mut v = value();
        let d = &mut v["section_views"][0]["circle_dimensions"][0];
        match mutation {
            0 => {
                d.as_object_mut().unwrap().remove("coverage");
            }
            1 => d["kind"] = json!("diameter"),
            2 => {
                d["anchor_feature"] = json!("stock");
                d["role"] = json!("wall_x_min");
            }
            _ => d["coverage"] = json!("guess_arc"),
        };
        let r = serde_json::from_value::<Recipe>(v);
        assert!(r.is_err() || cad_build::build(&r.unwrap()).is_err());
    }
    let mut v = value();
    v["parts"] = json!([{"id":"part","feature":"rounded"}]);
    v["assemblies"] = json!([{"id":"root","instances":[{"id":"far","part":"part","transform":{"translation":[50000,0,0]}}]}]);
    v["assembly_outputs"] = json!([{"name":"machine","assembly":"root"}]);
    v["section_views"][0]["output"] = json!("machine");
    v["section_views"][0]["circle_dimensions"][0]["instance_path"] = json!(["far"]);
    v["section_views"][0]["origin"] = json!([50000, 0, 5]);
    let result = cad_build::build(&parse(v));
    assert!(
        matches!(result, Err(BuildError::Assembly { ref message, .. }) if message.contains("modeling precision")),
        "{result:?}"
    );
}
#[test]
fn transformed_native_support_keeps_local_drawing_radius_and_tip() {
    let mut v = value();
    v["features"].as_array_mut().unwrap().push(json!({"op":"transform","id":"posed","body":"rounded","offset":[7,-4,2],"rotation":[0.5,0.5,0.5,0.5]}));
    v["outputs"][0]["feature"] = json!("posed");
    v["section_views"][0]["origin"] = json!([12, -4, 2]);
    v["section_views"][0]["normal"] = json!([1, 0, 0]);
    v["section_views"][0]["x_direction"] = json!([0, 1, 0]);
    let built = cad_build::build(&parse(v)).unwrap();
    let d = &built.section_views["mid"].circle_dimensions[0];
    assert_eq!(d.value_mm, 2.);
    for x in d.arc.as_ref().unwrap().tip {
        assert!((x - (2. - 2_f64.sqrt())).abs() < 1e-12);
    }
}
