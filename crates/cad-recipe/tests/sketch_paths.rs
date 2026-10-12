use cad_build::{BuildEngine, Feature, Recipe};
const SOURCE: &str = include_str!("../../../examples/constrained_capsule_link.star");
#[test]
fn solved_curved_outline_roundtrips_and_builds_analytic_capsule() {
    for (span, radius, bore) in [
        (60., 10., 4.),
        (40., 8., 5.),
        (80., 14., 3.),
        (57.125, 10.375, 3.625),
    ] {
        let r = cad_recipe::from_starlark_with_params(
            "capsule.star",
            SOURCE,
            &std::collections::BTreeMap::from([
                ("span".into(), span),
                ("outer_radius".into(), radius),
                ("bore_radius".into(), bore),
            ]),
        )
        .unwrap();
        let Feature::Sketch {
            curve_outline,
            loops,
            ..
        } = &r.features[0]
        else {
            panic!("expected sketch")
        };
        assert!(loops.is_empty());
        assert_eq!(curve_outline.as_ref().unwrap().segments.len(), 4);
        let json = serde_json::to_string(&r).unwrap();
        let decoded: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&decoded).unwrap());
        let b = BuildEngine::default()
            .build(&decoded)
            .unwrap_or_else(|e| panic!("{span} {radius} {bore}: {e:?}"));
        let report = &b.sketch_reports["capsule_layout"];
        assert!(report.is_fully_constrained());
        assert_eq!(report.rank, 16);
        for (name, p) in [
            ("a", [0., -radius]),
            ("b", [span, -radius]),
            ("c", [span, radius]),
            ("d", [0., radius]),
        ] {
            for (actual, expected) in report.points[name].iter().zip(p) {
                assert!((actual - expected).abs() < 1e-6);
            }
        }
        let model = &b.outputs["link"].result.model;
        let props = model.mass_properties(Default::default()).unwrap();
        let volume =
            8. * (2. * radius * span + std::f64::consts::PI * (radius * radius - 2. * bore * bore));
        assert!((props.volume - volume).abs() < 1e-4);
        for (actual, expected) in props.centroid.unwrap().iter().zip([span / 2., 0., 4.]) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }
}
#[test]
fn curved_sketch_schema_requires_exactly_one_strict_outline() {
    for bad in [
        SOURCE.replace(
            "curve_outline =",
            "outline = [\"a\",\"b\",\"c\",\"d\"], curve_outline =",
        ),
        SOURCE.replace(
            "\"kind\":\"line_to\",\"end\":\"b\"",
            "\"kind\":\"line_to\",\"end\":1",
        ),
        SOURCE.replace(
            "\"center\":\"right_center\",\"end\":\"c\"",
            "\"center\":\"right_center\",\"end\":\"c\",\"clockwise\":\"false\"",
        ),
        SOURCE.replace(
            "\"kind\":\"line_to\",\"end\":\"b\"",
            "\"kind\":\"line_to\",\"end\":\"b\",\"radius\":10",
        ),
        SOURCE.replace(
            "curve_outline =",
            "holes = [[\"a\",\"b\",\"c\"]], curve_outline =",
        ),
    ] {
        assert!(cad_recipe::from_starlark("bad.star", &bad).is_err());
    }
    let missing = "p=sketch(id='p',points=[],constraints=[])";
    assert!(cad_recipe::from_starlark("missing.star", missing).is_err());
}

#[test]
fn asymmetric_initial_guesses_reach_a_native_compatible_curved_profile() {
    for shift in [0.125, -0.25, 0.5] {
        let mut recipe = cad_recipe::from_starlark_with_params(
            "capsule.star",
            SOURCE,
            &std::collections::BTreeMap::from([
                ("span".into(), 47.5),
                ("outer_radius".into(), 11.25),
                ("bore_radius".into(), 3.75),
            ]),
        )
        .unwrap();
        let Feature::Sketch { sketch, .. } = &mut recipe.features[0] else {
            panic!("expected sketch")
        };
        for (i, p) in sketch.points.iter_mut().enumerate() {
            p.position[0] += shift * (i + 1) as f64 / 8.;
            p.position[1] += shift * ((i % 3) + 1) as f64;
        }
        let built = BuildEngine::default().build(&recipe).unwrap();
        let report = &built.sketch_reports["capsule_layout"];
        assert!(report.is_fully_constrained());
        let expected = 8.
            * (2. * 11.25 * 47.5
                + std::f64::consts::PI * (11.25_f64.powi(2) - 2. * 3.75_f64.powi(2)));
        assert!((built.outputs["link"].result.model.volume() - expected).abs() < 1e-4);
    }
}
