use std::collections::BTreeMap;

const STUDY: &str = include_str!("../../../examples/drive_module_slot_positions.star");
const RELEASE: &str = include_str!("../../../examples/drive_module_rotation.star");

fn study_recipe() -> cad_build::Recipe {
    cad_recipe::from_starlark_with_modules(
        "drive_module_slot_positions.star",
        STUDY,
        &BTreeMap::new(),
        &BTreeMap::from([("drive_module_rotation.star".into(), RELEASE.into())]),
    )
    .unwrap()
}

#[test]
fn four_native_slot_center_datums_drive_six_guarded_machining_positions() {
    let recipe = study_recipe();
    assert_eq!(recipe.slot_center_datums.len(), 4);
    let build = cad_build::BuildEngine::default()
        .build_outputs(&recipe, &["housing".into()])
        .unwrap();
    let centers = &build.slot_center_datums["machining_base"];
    for (name, expected) in [
        ("front_minus", [33., -28., 0.]),
        ("front_plus", [33., 28., 0.]),
        ("rear_minus", [93., -28., 0.]),
        ("rear_plus", [93., 28., 0.]),
    ] {
        let witness = &centers[name];
        for (actual, nominal) in witness.frame.translation.into_iter().zip(expected) {
            assert!((actual - nominal).abs() < 1e-8, "{name}");
        }
        assert_eq!(witness.walls.witnesses.len(), 4);
        assert_eq!(witness.cap.witnesses.len(), 1);
        assert!(witness.native.arithmetic_uncertainty_mm > 0.);
        assert_eq!(
            witness.target_content_hash,
            build.feature_hashes["base_slotted"]
        );
    }
    for (view, id, expected) in [
        ("slot_positions", "front_x", 33.),
        ("slot_positions", "rear_x", 93.),
        ("slot_positions", "lower_y", 12.),
        ("slot_positions", "upper_y", 68.),
        ("slot_pitches", "pitch_x", 60.),
        ("slot_pitches", "pitch_y", 56.),
    ] {
        let dimension = build.section_views[view]
            .dimensions
            .iter()
            .find(|d| d.declaration.id == id)
            .unwrap();
        assert!((dimension.value_mm - expected).abs() < 1e-8, "{id}");
        assert!(dimension.arithmetic_uncertainty_mm.unwrap() > 0.);
        assert!(
            dimension.from_witness.slot_datum.is_some()
                || dimension.to_witness.slot_datum.is_some()
        );
    }
}

#[test]
fn selected_warm_cache_rechecks_role_bounds_and_face_only_datum_contract() {
    let mut recipe = study_recipe();
    let mut engine = cad_build::BuildEngine::default();
    engine.build_outputs(&recipe, &["housing".into()]).unwrap();
    recipe.slot_center_datums[0].role = "missing_slot_walls".into();
    assert!(matches!(
        engine.build_outputs(&recipe, &["housing".into()]),
        Err(cad_build::BuildError::SlotCenterDatumFailed { .. })
    ));
    recipe = study_recipe();
    let front = recipe
        .section_views
        .iter_mut()
        .find(|v| v.id == "slot_positions")
        .unwrap();
    let narrowed = front
        .dimensions
        .iter_mut()
        .find(|d| d.id == "front_x")
        .unwrap();
    narrowed.min = Some(32.8);
    narrowed.max = Some(32.9);
    assert!(matches!(
        engine.build_outputs(&recipe, &["housing".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
    recipe = study_recipe();
    let front = recipe
        .section_views
        .iter_mut()
        .find(|v| v.id == "slot_positions")
        .unwrap();
    let exact_edge = front
        .dimensions
        .iter_mut()
        .find(|d| d.id == "front_x")
        .unwrap();
    exact_edge.min = Some(32.9);
    exact_edge.max = Some(33.);
    assert!(matches!(
        engine.build_outputs(&recipe, &["housing".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { id, .. }) if id == "front_x"
    ));
    recipe = study_recipe();
    let front = recipe
        .section_views
        .iter_mut()
        .find(|v| v.id == "slot_positions")
        .unwrap();
    let narrowly_certified = front
        .dimensions
        .iter_mut()
        .find(|d| d.id == "front_x")
        .unwrap();
    narrowly_certified.min = Some(33. - 1e-12);
    narrowly_certified.max = Some(33. + 1e-12);
    assert!(matches!(
        engine.build_outputs(&recipe, &["housing".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
    recipe = study_recipe();
    recipe.datum_dimensions.push(
        serde_json::from_value(serde_json::json!({
            "id":"invalid_slot_seating_like_plane", "output":"base_machining_layout",
            "from":{"instance_path":["machining_leaf"],"datum":"left_edge"},
            "to":{"instance_path":["machining_leaf"],"datum":"front_minus"},
            "min":1
        }))
        .unwrap(),
    );
    let face_only = engine
        .build_outputs(&recipe, &["housing".into()])
        .unwrap_err();
    assert!(matches!(
        face_only,
        cad_build::BuildError::DatumDimensionFailed { .. }
    ));
    assert!(face_only.to_string().contains("native semantic face datum"));
}

#[test]
fn canceled_ancestor_placement_and_anchored_section_plane_refuse() {
    let mut recipe = study_recipe();
    let view = recipe
        .section_views
        .iter_mut()
        .find(|v| v.id == "slot_positions")
        .unwrap();
    view.anchor = Some(
        serde_json::from_value(serde_json::json!({
            "instance_path":["machining_leaf"], "datum":"front_minus"
        }))
        .unwrap(),
    );
    view.origin = [0., 0., -3.];
    let anchored = cad_build::BuildEngine::default().build_outputs(&recipe, &["housing".into()]);
    assert!(matches!(
        anchored,
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));

    let mut recipe = study_recipe();
    let leaf = recipe
        .assemblies
        .iter_mut()
        .find(|a| a.id == "base_machining_layout")
        .unwrap();
    leaf.instances[0].transform.translation = [-100_000_000_000., 0., 0.];
    recipe.assemblies.push(
        serde_json::from_value(serde_json::json!({
            "id":"nested_machining_layout",
            "assemblies":[{"id":"outer_machining","assembly":"base_machining_layout",
                "transform":{"translation":[100000000000.0,0,0],"rotation":[0,0,0,1]}}]
        }))
        .unwrap(),
    );
    recipe
        .assembly_outputs
        .iter_mut()
        .find(|o| o.name == "base_machining_layout")
        .unwrap()
        .assembly = "nested_machining_layout".into();
    for view in &mut recipe.section_views {
        if view.output == "base_machining_layout" {
            for dimension in &mut view.dimensions {
                dimension
                    .from
                    .instance_path
                    .insert(0, "outer_machining".into());
                dimension
                    .to
                    .instance_path
                    .insert(0, "outer_machining".into());
            }
        }
    }
    let canceled = cad_build::BuildEngine::default().build_outputs(&recipe, &["housing".into()]);
    assert!(matches!(
        canceled,
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
    assert!(canceled
        .unwrap_err()
        .to_string()
        .contains("placement exceeds dimension precision budget"));
}
