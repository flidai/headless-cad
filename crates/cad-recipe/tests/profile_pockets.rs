use cad_build::{BuildEngine, BuildError, Feature, Recipe};
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/motor_mount_housing.star");
fn recipe(parameters: &[(&str, f64)]) -> Recipe {
    cad_recipe::from_starlark_with_params(
        "motor_mount_housing.star",
        SOURCE,
        &parameters
            .iter()
            .map(|(name, value)| ((*name).into(), *value))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap()
}

#[test]
fn motor_housing_keeps_serial_floor_identity_and_native_measurements() {
    let mut engine = BuildEngine::default();
    for (side, height, width, first, second) in [
        (1., 24., 52., 12., 18.),
        (0., 24., 52., 12., 18.),
        (1., 22., 46., 10., 19.),
        (0., 28., 58., 14., 16.),
    ] {
        let source = recipe(&[
            ("pocket_side", side),
            ("height", height),
            ("recess_width", width),
            ("recess_depth", first),
            ("cable_depth", second),
        ]);
        let built = engine.build(&source).unwrap();
        assert_eq!(built.assemblies["motor_mount_housing"].instances.len(), 2);
        let floor = &built.face_assertions["motor_floor_survives"].witnesses[0];
        assert_eq!(floor.target_face.0, floor.anchor_faces[0].0 + 8);
        assert_eq!(
            built.face_assertions["motor_walls_survive"].witnesses.len(),
            8
        );
        for (id, expected) in [
            ("motor_depth", first),
            ("cable_depth", second),
            ("remaining_web", height - second),
        ] {
            assert!((built.datum_dimensions[id].value_mm - expected).abs() < 1e-7);
        }
        assert_eq!(built.section_views.len(), 3);
        assert_eq!(built.drawing_sheets.len(), 1);
        assert_eq!(built.property_assertions.len(), 2);
        assert_eq!(engine.build(&source).unwrap().stats.computed_solids, 0);
    }
}

#[test]
fn profile_only_revision_reuses_stock_and_cover_but_rebuilds_both_pocket_stages() {
    let mut engine = BuildEngine::default();
    let old = engine.build(&recipe(&[])).unwrap();
    let revised = engine.build(&recipe(&[("recess_width", 54.)])).unwrap();
    assert_eq!(revised.stats.computed_solids, 2);
    for feature in ["housing_stock", "mounting_slots", "cover_body"] {
        assert_eq!(old.feature_hashes[feature], revised.feature_hashes[feature]);
    }
    for feature in ["motor_recess", "cable_pocket"] {
        assert_ne!(old.feature_hashes[feature], revised.feature_hashes[feature]);
    }
    let restored = engine.build(&recipe(&[])).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
}

#[test]
fn profile_frames_and_native_floor_obligations_are_not_bypassed_by_cache() {
    let mut engine = BuildEngine::default();
    let good = recipe(&[]);
    let selected = ["cover".into()];
    engine.build_outputs(&good, &selected).unwrap();
    let mut bad = good.clone();
    for f in &mut bad.features {
        if let Feature::BlindProfile {
            id, frame_origin, ..
        } = f
        {
            if id == "motor_recess" {
                frame_origin[2] += 0.1;
            }
        }
    }
    assert!(matches!(
        engine.build_outputs(&bad, &selected),
        Err(BuildError::Kernel { .. })
    ));
    let mut bad = good.clone();
    bad.datum_dimensions[0].min = Some(100.);
    bad.datum_dimensions[0].max = Some(101.);
    assert!(matches!(
        engine.build_outputs(&bad, &selected),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
    assert_eq!(
        engine
            .build_outputs(&good, &selected)
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}

#[test]
fn strict_profile_feature_schema_requires_valid_references_and_rejects_extra_fields() {
    let source = recipe(&[]);
    let mut value = serde_json::to_value(&source).unwrap();
    let index = source
        .features
        .iter()
        .position(|f| f.id() == "motor_recess")
        .unwrap();
    value["features"][index]["unexpected"] = true.into();
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    let mut wrong = source.clone();
    if let Feature::BlindProfile { profile, .. } = &mut wrong.features[index] {
        *profile = "housing_stock".into();
    }
    assert!(matches!(
        BuildEngine::default().build(&wrong),
        Err(BuildError::InputType { .. })
    ));
    let mut wrong = source;
    if let Feature::BlindProfile { body, .. } = &mut wrong.features[index] {
        *body = "motor_profile".into();
    }
    assert!(matches!(
        BuildEngine::default().build(&wrong),
        Err(BuildError::InputType { .. })
    ));
}

#[test]
fn alternate_profile_dependencies_preserve_geometry_and_reject_holes() {
    use serde_json::json;
    let points = [
        ("a", [0., 0.]),
        ("b", [8., 0.]),
        ("c", [8., 12.]),
        ("d", [0., 12.]),
    ];
    let sketch = json!({"op":"sketch","id":"cut_profile", "loops":[["a","b","c","d"]],
        "sketch":{"points":points.iter().map(|(id,p)|json!({"id":id,"position":p})).collect::<Vec<_>>(),
        "constraints":points.iter().map(|(id,p)|json!({"id":id,"kind":"fixed","point":id,"position":p})).collect::<Vec<_>>()}});
    for profile in [
        json!({"op":"rectangle","id":"cut_profile","width":8,"depth":12}),
        json!({"op":"polygon","id":"cut_profile","points":[[0,0],[8,0],[8,12],[0,12]]}),
        sketch,
    ] {
        let value = json!({"schema_version":2,"units":"mm","features":[
            {"op":"rectangle","id":"outer","width":50,"depth":50},
            {"op":"extrude","id":"stock","profile":"outer","height":20},profile,
            {"op":"blind_profile","id":"pocket","body":"stock","profile":"cut_profile",
             "face":{"from_feature":"stock","role":"end_cap"},"depth":5,"frame_origin":[10,10,20]}
        ],"outputs":[{"name":"pocket","feature":"pocket"}]});
        let recipe: Recipe = serde_json::from_value(value.clone()).unwrap();
        let mut engine = BuildEngine::default();
        let built = engine.build(&recipe).unwrap();
        assert!(
            (built.outputs["pocket"].result.model.volume() - (50. * 50. * 20. - 8. * 12. * 5.))
                .abs()
                < 1e-7
        );
        assert_eq!(engine.build(&recipe).unwrap().stats.computed_solids, 0);
        let mut with_hole = value;
        with_hole["features"][2] = json!({"op":"polygon","id":"cut_profile","points":[[0,0],[8,0],[8,12],[0,12]],
            "circular_holes":[{"center":[4,6],"radius":1}]});
        let bad: Recipe = serde_json::from_value(with_hole).unwrap();
        assert!(matches!(engine.build(&bad), Err(BuildError::Kernel { .. })));
    }
}
