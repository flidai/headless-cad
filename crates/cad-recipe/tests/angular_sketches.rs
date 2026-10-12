use cad_build::{BuildEngine, ConstraintKind, Feature, Recipe};
const SOURCE: &str = r#"
theta = param(name = "angle", default = 30.25)
profile = sketch(id = "layout", points = [
 {"id":"a","position":[0,0]}, {"id":"b","position":[20.5,0]}, {"id":"c","position":[10.3,6.125]}],
 constraints = [
 {"id":"a_fixed","kind":"fixed","point":"a","position":[0,0]},
 {"id":"b_fixed","kind":"fixed","point":"b","position":[20.5,0]},
 {"id":"equal","kind":"equal_length","a":["a","c"],"b":["b","c"]},
 {"id":"angle","kind":"angle","a":["a","b"],"b":["a","c"],"degrees":theta}
 ], outline = ["a","b","c"])
output(name = "part", feature = extrude(id = "body", profile = profile, height = 3.25))
"#;
#[test]
fn fractional_angular_source_roundtrips_without_losing_constraint_types() {
    let mut engine = BuildEngine::default();
    for angle in [30.25, 45.125, 60.75] {
        let r = cad_recipe::from_starlark_with_params(
            "angular.star",
            SOURCE,
            &std::collections::BTreeMap::from([("angle".into(), angle)]),
        )
        .unwrap();
        let Feature::Sketch { sketch, .. } = &r.features[0] else {
            panic!("expected sketch")
        };
        assert!(
            matches!(&sketch.constraints[3].kind,ConstraintKind::Angle{degrees,..}if *degrees==angle)
        );
        assert!(matches!(
            &sketch.constraints[2].kind,
            ConstraintKind::EqualLength { .. }
        ));
        let json = serde_json::to_string(&r).unwrap();
        let decoded: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&decoded).unwrap());
        let built = engine.build(&decoded).unwrap();
        let expected_height = 10.25 * angle.to_radians().tan();
        assert!(
            (built.outputs["part"].result.model.volume() - 20.5 * expected_height * 3.25 / 2.)
                .abs()
                < 1e-4
        );
        let report = &built.sketch_reports["layout"];
        assert_eq!(report.schema_version, 3);
        assert!(report.is_fully_constrained());
        assert!(report.max_angular_residual_radians <= 1e-9);
    }
}
#[test]
fn malformed_new_constraint_fields_fail_during_compilation() {
    for bad in [
        SOURCE.replace("\"degrees\":theta", "\"degrees\":theta,\"unexpected\":1"),
        SOURCE.replace("\"a\":[\"a\",\"b\"]", "\"a\":\"a\""),
        SOURCE.replace("\"kind\":\"angle\"", "\"kind\":\"parallel\""),
    ] {
        assert!(cad_recipe::from_starlark("bad.star", &bad).is_err());
    }
}
