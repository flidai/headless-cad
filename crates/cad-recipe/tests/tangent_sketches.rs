use cad_build::{BuildEngine, Recipe};
const SOURCE: &str = include_str!("../../../examples/tangent_bore_fixture.star");
#[test]
fn tangent_circle_recipe_solves_independent_wedge_geometry() {
    for (width, radius, thickness) in [
        (30., 4., 8.),
        (20., 6., 6.),
        (35., 3., 10.),
        (26.125, 4.375, 7.25),
    ] {
        let r = cad_recipe::from_starlark_with_params(
            "tangent.star",
            SOURCE,
            &std::collections::BTreeMap::from([
                ("half_width".into(), width),
                ("bore_radius".into(), radius),
                ("thickness".into(), thickness),
            ]),
        )
        .unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let decoded: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(json, serde_json::to_string(&decoded).unwrap());
        let b = BuildEngine::default().build(&decoded).unwrap();
        let report = &b.sketch_reports["bore_layout"];
        assert!(report.is_fully_constrained());
        assert_eq!(report.rank, 28);
        let offset = radius * width.hypot(60.) / width;
        for (name, y) in [
            ("lower_center", 12. + offset),
            ("upper_center", 68. - offset),
        ] {
            assert!((report.points[name][0] - 50.).abs() < 1e-6);
            assert!((report.points[name][1] - y).abs() < 1e-6);
        }
        let volume = (8000. - 2. * std::f64::consts::PI * radius * radius) * thickness;
        assert!((b.outputs["plate"].result.model.volume() - volume).abs() < 1e-4);
    }
}
#[test]
fn tangent_starlark_schema_rejects_missing_and_ambiguous_side() {
    for bad in [
        SOURCE.replace(",\"side\":\"right\"", ""),
        SOURCE.replace("\"side\":\"right\"", "\"side\":\"either\""),
        SOURCE.replace(
            "\"circle\":[\"lower_center\",\"lower_rim\"]",
            "\"circle\":\"lower_center\"",
        ),
        SOURCE.replace("\"side\":\"right\"", "\"side\":\"right\",\"radius\":4"),
    ] {
        assert!(cad_recipe::from_starlark("bad.star", &bad).is_err());
    }
}
