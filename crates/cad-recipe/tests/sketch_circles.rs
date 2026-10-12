use cad_build::{BuildEngine, Feature, Recipe};

const SOURCE: &str = include_str!("../../../examples/constrained_drill_jig.star");

#[test]
fn solved_circle_references_roundtrip_and_drive_drill_jig() {
    let recipe = cad_recipe::from_starlark("drill.star", SOURCE).unwrap();
    let Feature::Sketch { circular_holes, .. } = &recipe.features[0] else {
        panic!("expected sketch")
    };
    assert_eq!(circular_holes.len(), 3);
    assert_eq!(circular_holes[0].center, "left");
    assert_eq!(circular_holes[0].rim, "left_rim");
    let encoded = serde_json::to_string(&recipe).unwrap();
    let decoded: Recipe = serde_json::from_str(&encoded).unwrap();
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
    let built = BuildEngine::default().build(&decoded).unwrap();
    let expected = (6000. - 3. * std::f64::consts::PI * 16.) * 8.;
    assert!((built.outputs["plate"].result.model.volume() - expected).abs() < 1e-4);
    let report = &built.sketch_reports["guide_layout"];
    assert!(report.is_fully_constrained());
    assert_eq!(report.rank, 20);
    for (name, x) in [("left", 25.), ("middle", 50.), ("right", 75.)] {
        assert!((report.points[name][0] - x).abs() < 1e-6);
        assert!((report.points[name][1] - 30.).abs() < 1e-6);
    }
}

#[test]
fn circular_reference_fields_are_strictly_typed() {
    let marker = "{\"center\":center,\"rim\":center+\"_rim\"}";
    for malformed in [
        "{\"center\":center,\"rim\":1}",
        "{\"center\":center}",
        "{\"center\":center,\"rim\":center+\"_rim\",\"radius\":4}",
        "{\"center\":[0,0],\"rim\":center+\"_rim\"}",
    ] {
        let error = cad_recipe::from_starlark("bad.star", &SOURCE.replace(marker, malformed));
        assert!(error.is_err(), "accepted {malformed}");
    }
}

#[test]
fn fractional_radius_and_pitch_survive_starlark_json() {
    let recipe = cad_recipe::from_starlark_with_params(
        "fractional.star",
        SOURCE,
        &std::collections::BTreeMap::from([
            ("pitch".into(), 23.125),
            ("bore_radius".into(), 3.375),
            ("thickness".into(), 7.25),
        ]),
    )
    .unwrap();
    let built = BuildEngine::default().build(&recipe).unwrap();
    let expected = (6000. - 3. * std::f64::consts::PI * 3.375_f64.powi(2)) * 7.25;
    assert!((built.outputs["plate"].result.model.volume() - expected).abs() < 1e-4);
}
