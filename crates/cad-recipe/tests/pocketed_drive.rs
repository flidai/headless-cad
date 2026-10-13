use cad_build::{BuildEngine, BuildError, Recipe};
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/drive_module_rotation.star");
fn recipe(depth: f64) -> Recipe {
    cad_recipe::from_starlark_with_params(
        "drive_module_rotation.star",
        SOURCE,
        &BTreeMap::from([("base_pocket_depth".into(), depth)]),
    )
    .unwrap()
}

#[test]
fn reliefs_change_only_actual_base_and_preserve_existing_drive_obligations() {
    let mut engine = BuildEngine::default();
    let plain = engine.build(&recipe(0.)).unwrap();
    let cut = engine.build(&recipe(1.)).unwrap();
    let assembly = &cut.assemblies["drive_module_release"];
    assert_eq!(assembly.instances.len(), 30);
    let changed: Vec<_> = assembly
        .parts
        .iter()
        .filter_map(|(id, part)| {
            (part.content_hash != plain.assemblies["drive_module_release"].parts[id].content_hash)
                .then_some(id.as_str())
        })
        .collect();
    assert_eq!(changed, ["base"]);
    assert_eq!(
        cut.feature_hashes["base_slotted"],
        plain.feature_hashes["base_slotted"]
    );
    let removed = 3. * (108. + 9. * std::f64::consts::PI);
    assert!(
        (plain.outputs["base"].result.model.volume()
            - cut.outputs["base"].result.model.volume()
            - removed)
            .abs()
            < 1e-6
    );
    assert_eq!(cut.clearance_checks.len(), 13);
    assert_eq!(cut.nominal_seating_checks.len(), 6);
    assert_eq!(cut.tolerance_stacks.len(), 8);
    assert_eq!(cut.datum_dimensions.len(), 29);
    assert_eq!(cut.property_assertions.len(), 7);
    assert_eq!(
        cut.clearance_checks.keys().collect::<Vec<_>>(),
        plain.clearance_checks.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        cut.nominal_seating_checks.keys().collect::<Vec<_>>(),
        plain.nominal_seating_checks.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        cut.tolerance_stacks.keys().collect::<Vec<_>>(),
        plain.tolerance_stacks.keys().collect::<Vec<_>>()
    );
    for id in plain.datum_dimensions.keys() {
        assert!(cut.datum_dimensions.contains_key(id));
    }
    for id in plain.face_assertions.keys() {
        assert!(cut.face_assertions.contains_key(id));
    }
    for (id, value) in [
        ("base_shallow_depth", 1.),
        ("base_deep_depth", 2.),
        ("base_remaining_web", 4.),
    ] {
        assert!((cut.datum_dimensions[id].value_mm - value).abs() < 1e-8);
    }
    let old_floor = &cut.face_assertions["base_shallow_floor_survives"].witnesses[0];
    assert_eq!(old_floor.target_face.0, old_floor.anchor_faces[0].0 + 4);
    let restored = engine.build(&recipe(0.)).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
    assert_eq!(restored.feature_hashes, plain.feature_hashes);
}

#[test]
fn selected_warm_drive_builds_enforce_new_floor_and_existing_clearance_obligations() {
    let mut engine = BuildEngine::default();
    let source = recipe(1.);
    engine.build_outputs(&source, &["washer".into()]).unwrap();
    let mut wrong_floor = source.clone();
    let dimension = wrong_floor
        .datum_dimensions
        .iter_mut()
        .find(|d| d.id == "base_shallow_depth")
        .unwrap();
    dimension.min = Some(1.1);
    dimension.max = Some(1.2);
    assert!(matches!(
        engine.build_outputs(&wrong_floor, &["washer".into()]),
        Err(BuildError::DatumDimensionFailed { .. })
    ));
    let wrong_clearance = cad_recipe::from_starlark_with_params(
        "drive_module_rotation.star",
        SOURCE,
        &BTreeMap::from([
            ("base_pocket_depth".into(), 1.),
            ("required_clearance".into(), 0.3),
        ]),
    )
    .unwrap();
    assert!(matches!(
        engine.build_outputs(&wrong_clearance, &["washer".into()]),
        Err(BuildError::ClearanceCheckFailed { .. })
    ));
    let restored = engine.build_outputs(&source, &["washer".into()]).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
    for depth in [-1., 0.25, 1.75] {
        assert!(cad_recipe::from_starlark_with_params(
            "drive_module_rotation.star",
            SOURCE,
            &BTreeMap::from([("base_pocket_depth".into(), depth)]),
        )
        .is_err());
    }
}
