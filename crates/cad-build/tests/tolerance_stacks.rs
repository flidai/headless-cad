use cad_build::{BuildEngine, BuildError, Recipe};
use serde_json::{json, Value};
fn recipe_value() -> Value {
    json!({"schema_version":2,"units":"mm","features":[{"op":"circle","id":"pa","radius":3},{"op":"extrude","id":"a","profile":"pa","height":8},{"op":"circle","id":"pb","radius":2.5},{"op":"extrude","id":"b","profile":"pb","height":8}],"outputs":[{"name":"a","feature":"a"},{"name":"b","feature":"b"}],"section_views":[
 {"id":"av","output":"a","origin":[0,0,4],"circle_dimensions":[{"id":"dia","anchor_feature":"a","role":"wall","kind":"diameter","label_offset":[10,10],"tolerance":{"nominal":6,"lower":-0.125,"upper":0.125}}]},
 {"id":"bv","output":"b","origin":[0,0,4],"circle_dimensions":[{"id":"dia","anchor_feature":"b","role":"wall","kind":"diameter","label_offset":[10,10],"tolerance":{"nominal":5,"lower":-0.0625,"upper":0.0625}}]}],
 "tolerance_stacks":[{"id":"clearance","terms":[{"view":"av","dimension":"dia","coefficient":1},{"view":"bv","dimension":"dia","coefficient":-1}],"min":0.8,"max":1.2}]})
}
fn parse(v: Value) -> Recipe {
    serde_json::from_value(v).unwrap()
}
#[test]
fn signed_worst_case_envelope_is_global_and_cached_edits_preserve_geometry() {
    let mut r = parse(recipe_value());
    let mut e = BuildEngine::default();
    let a = e.build_outputs(&r, &["a".into()]).unwrap();
    let s = &a.tolerance_stacks["clearance"];
    assert_eq!(s.status, "passed");
    assert_eq!(s.nominal_mm, 1.);
    assert!(s.lower_limit_mm < 0.8125 && s.lower_limit_mm > 0.81249);
    assert!(s.upper_limit_mm > 1.1875 && s.upper_limit_mm < 1.18751);
    assert_eq!(s.terms[1].native_value_mm, 5.);
    assert_eq!(s.terms[1].tolerance.nominal_mm, 5.);
    assert!(s.terms[1].lower_limit_mm < -5.0625);
    r.tolerance_stacks[0].max = Some(1.3);
    let b = e.build_outputs(&r, &["a".into()]).unwrap();
    assert_eq!(b.stats.computed_solids, 0);
    assert_eq!(a.feature_hashes, b.feature_hashes);
    r.tolerance_stacks[0].min = Some(0.8125);
    assert!(
        matches!(e.build_outputs(&r,&["a".into()]),Err(BuildError::ToleranceStackFailed{lower_limit_mm:Some(lo),upper_limit_mm:Some(hi),..}) if lo<0.8125&&hi>1.1875)
    );
}
#[test]
fn negative_requirements_and_cancellation_remain_conservative() {
    let mut r = parse(recipe_value());
    let stack = &mut r.tolerance_stacks[0];
    stack.terms[0].coefficient = -1e6;
    stack.terms[1].coefficient = 1e6;
    stack.min = Some(-1.2e6);
    stack.max = Some(-0.8e6);
    let b = cad_build::build(&r).unwrap();
    let s = &b.tolerance_stacks["clearance"];
    assert_eq!(s.nominal_mm, -1e6);
    assert!(s.lower_limit_mm < -1_187_500.);
    assert!(s.upper_limit_mm > -812_500.);
    // Distinct source dimensions can share nominal values; the declared interval
    // relationship remains independent rather than cancelling their uncertainty.
    r.features[2] = serde_json::from_value(json!({"op":"circle","id":"pb","radius":3})).unwrap();
    r.section_views[1].circle_dimensions[0]
        .tolerance
        .as_mut()
        .unwrap()
        .nominal = 6.;
    r.tolerance_stacks[0].min = Some(-200_000.);
    r.tolerance_stacks[0].max = Some(200_000.);
    let b = cad_build::build(&r).unwrap();
    let s = &b.tolerance_stacks["clearance"];
    assert_eq!(s.nominal_mm, 0.);
    assert!(s.lower_limit_mm < -187_500. && s.upper_limit_mm > 187_500.);
}
#[test]
fn structural_invalidity_precedes_geometry_and_unknown_fields_are_strict() {
    for (field, v) in [
        ("coefficient", json!(0)),
        ("coefficient", json!(1000001)),
        ("view", json!("missing")),
        ("dimension", json!("missing")),
    ] {
        let mut r = recipe_value();
        r["tolerance_stacks"][0]["terms"][0][field] = v;
        r["features"][0]["radius"] = json!(-1);
        assert!(matches!(
            cad_build::build(&parse(r)),
            Err(BuildError::InvalidToleranceStack { .. })
        ));
    }
    let mut r = recipe_value();
    r["section_views"][0]["circle_dimensions"][0]
        .as_object_mut()
        .unwrap()
        .remove("tolerance");
    assert!(matches!(
        cad_build::build(&parse(r)),
        Err(BuildError::InvalidToleranceStack { .. })
    ));
    for field in ["min", "max"] {
        let mut r = recipe_value();
        r["tolerance_stacks"][0][field] = Value::Null;
        assert!(cad_build::build(&parse(r)).is_ok());
    }
    let mut r = recipe_value();
    r["tolerance_stacks"][0]["min"] = Value::Null;
    r["tolerance_stacks"][0]["max"] = Value::Null;
    assert!(cad_build::build(&parse(r)).is_err());
    let mut r = recipe_value();
    let term = r["tolerance_stacks"][0]["terms"][0].clone();
    r["tolerance_stacks"][0]["terms"]
        .as_array_mut()
        .unwrap()
        .push(term);
    assert!(cad_build::build(&parse(r)).is_err());
    for invalid in [
        json!(null),
        json!([["bad", [], 0, 1]]),
        json!([{"id":"s","terms":[],"min":0,"extra":true}]),
        json!([{"id":"s","terms":[["av","dia",1]],"min":0}]),
    ] {
        let mut r = recipe_value();
        r["tolerance_stacks"] = invalid;
        assert!(serde_json::from_value::<Recipe>(r).is_err());
    }
    let mut r = recipe_value();
    r["schema_version"] = json!(1);
    r.as_object_mut().unwrap().remove("section_views");
    r["tolerance_stacks"] = json!([]);
    assert!(serde_json::from_value::<Recipe>(r).is_err());
}
#[test]
fn declaration_resource_limits_are_checked() {
    let mut r = parse(recipe_value());
    let template = r.tolerance_stacks[0].clone();
    r.tolerance_stacks = (0..65)
        .map(|i| {
            let mut s = template.clone();
            s.id = format!("s{i}");
            s
        })
        .collect();
    assert!(matches!(
        cad_build::build(&r),
        Err(BuildError::ResourceLimit {
            resource: "tolerance stacks",
            ..
        })
    ));
    let mut r = parse(recipe_value());
    r.tolerance_stacks[0].terms = vec![r.tolerance_stacks[0].terms[0].clone(); 33];
    assert!(cad_build::build(&r).is_err());
    let mut r = parse(recipe_value());
    let dimension = r.section_views[0].circle_dimensions[0].clone();
    r.section_views[0].circle_dimensions = (0..32)
        .map(|i| {
            let mut d = dimension.clone();
            d.id = format!("d{i}");
            d
        })
        .collect();
    let terms = (0..32)
        .map(|i| cad_build::ToleranceStackTerm {
            datum_dimension: None,
            view: "av".into(),
            dimension: format!("d{i}"),
            coefficient: 1.,
        })
        .collect::<Vec<_>>();
    r.tolerance_stacks = (0..33)
        .map(|i| cad_build::ToleranceStack {
            id: format!("s{i}"),
            terms: terms.clone(),
            min: Some(0.),
            max: None,
        })
        .collect();
    assert!(matches!(
        cad_build::build(&r),
        Err(BuildError::ResourceLimit {
            resource: "tolerance stack terms",
            ..
        })
    ));
}
