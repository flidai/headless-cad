use std::collections::BTreeMap;

const STUDY: &str = include_str!("../../../examples/drive_module_slot_drawing.star");
const RELEASE: &str = include_str!("../../../examples/drive_module_rotation.star");

fn recipe(source: &str) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_modules(
        "drive_module_slot_drawing.star",
        source,
        &BTreeMap::new(),
        &BTreeMap::from([("drive_module_rotation.star".into(), RELEASE.into())]),
    )
    .unwrap()
}

fn nested_assembly_study() -> cad_build::Recipe {
    let mut study = recipe(STUDY);
    study.assemblies.extend(serde_json::from_value::<Vec<_>>(serde_json::json!([
        {"id":"slot_leaf_definition","instances":[{"id":"base_leaf","part":"base","transform":{"translation":[2,-3,0],"rotation":[0,0,0,1]}}]},
        {"id":"slot_probe_definition","assemblies":[{"id":"outer","assembly":"slot_leaf_definition","transform":{"translation":[10,-5,0],"rotation":[0,0,0.3826834323650898,0.9238795325112867]}}]}
    ])).unwrap());
    study.assembly_outputs.push(
        serde_json::from_value(serde_json::json!({
            "name":"slot_probe","assembly":"slot_probe_definition"
        }))
        .unwrap(),
    );
    let view = study
        .section_views
        .iter_mut()
        .find(|v| v.id == "base_slot_plan")
        .unwrap();
    view.output = "slot_probe".into();
    for dimension in &mut view.slot_dimensions {
        dimension.instance_path = Some(vec!["outer".into(), "base_leaf".into()]);
    }
    study
}

#[test]
fn four_native_slots_supply_three_source_bound_dimensions_each() {
    let recipe = recipe(STUDY);
    assert_eq!(
        recipe
            .section_views
            .iter()
            .find(|view| view.id == "base_slot_plan")
            .unwrap()
            .slot_dimensions
            .len(),
        12
    );
    let built = cad_build::BuildEngine::default()
        .build_outputs(&recipe, &["base".into()])
        .unwrap();
    let view = &built.section_views["base_slot_plan"];
    assert_eq!(view.slot_dimensions.len(), 12);
    for slot in ["front_minus", "front_plus", "rear_minus", "rear_plus"] {
        for (suffix, expected) in [("width", 6.), ("overall", 12.), ("centerline", 6.)] {
            let id = format!("{slot}_{suffix}");
            let report = view
                .slot_dimensions
                .iter()
                .find(|dimension| dimension.declaration.id == id)
                .unwrap();
            assert_eq!(report.status, "passed");
            assert!((report.value_mm - expected).abs() < 1e-8, "{id}");
            assert_eq!(report.measurement.segments.len(), 4);
            assert_eq!(report.selection.witnesses.len(), 4);
            assert_eq!(
                report.target_content_hash,
                built.feature_hashes["base_slotted"]
            );
        }
    }
}

#[test]
fn changed_slot_bounds_and_missing_wall_role_fail_even_for_other_selected_output() {
    let mut engine = cad_build::BuildEngine::default();
    engine
        .build_outputs(&recipe(STUDY), &["housing".into()])
        .unwrap();
    let too_narrow = STUDY.replace(
        "min = base_slot_length-0.000001, max = base_slot_length+0.000001",
        "min = base_slot_length-0.2, max = base_slot_length-0.1",
    );
    assert!(matches!(
        engine.build_outputs(&recipe(&too_narrow), &["housing".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
    let wrong_role = STUDY.replacen(
        "role = role, kind = \"width\"",
        "role = \"missing_slot_walls\", kind = \"width\"",
        1,
    );
    assert!(matches!(
        engine.build_outputs(&recipe(&wrong_role), &["housing".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
}

#[test]
fn oblique_occurrence_keeps_native_slot_witnesses_and_large_canceled_ancestors_refuse() {
    let mut study = nested_assembly_study();
    let mut engine = cad_build::BuildEngine::default();
    let valid = engine.build_outputs(&study, &["housing".into()]).unwrap();
    let view = &valid.section_views["base_slot_plan"];
    assert_eq!(view.slot_dimensions.len(), 12);
    for report in &view.slot_dimensions {
        assert_eq!(
            report.instance_path.as_deref(),
            Some(&["outer".into(), "base_leaf".into()][..])
        );
        assert!(report.placement_uncertainty_mm > 0.);
        assert!(
            report.total_arithmetic_uncertainty_mm > report.measurement.arithmetic_uncertainty_mm
        );
        assert_eq!(report.measurement.segments.len(), 4);
    }
    let width = view
        .slot_dimensions
        .iter()
        .find(|d| d.declaration.id == "front_minus_width")
        .unwrap();
    assert!((width.value_mm - 6.).abs() < 1e-8);
    let leaf = study
        .assemblies
        .iter_mut()
        .find(|a| a.id == "slot_leaf_definition")
        .unwrap();
    leaf.instances[0].transform.translation = [-100_000_000_000., 0., 0.];
    let root = study
        .assemblies
        .iter_mut()
        .find(|a| a.id == "slot_probe_definition")
        .unwrap();
    root.assemblies[0].transform.translation = [100_000_000_000., 0., 0.];
    root.assemblies[0].transform.rotation = [0., 0., 0., 1.];
    let error = engine
        .build_outputs(&study, &["housing".into()])
        .unwrap_err();
    assert!(matches!(
        error,
        cad_build::BuildError::SectionDimensionFailed { .. }
    ));
    assert!(error
        .to_string()
        .contains("placement exceeds measurement precision budget"));
}
