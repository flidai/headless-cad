use std::collections::BTreeMap;

const SMALL: &str = include_str!("../../../examples/slotted_clamp_adjustment.star");
const DRIVE: &str = include_str!("../../../examples/drive_module_adjustment.star");
const RELEASE: &str = include_str!("../../../examples/drive_module_rotation.star");

#[test]
fn isolated_slot_travel_is_authored_and_globally_checked() {
    let recipe = cad_recipe::from_starlark("slotted_clamp_adjustment.star", SMALL).unwrap();
    let replay: cad_build::Recipe =
        serde_json::from_value(serde_json::to_value(&recipe).unwrap()).unwrap();
    assert_eq!(replay.rigid_translation_groups.len(), 1);
    let built = cad_build::BuildEngine::default()
        .build_outputs(&replay, &["adjustment_bolt".into()])
        .unwrap();
    let group = &built.rigid_translation_groups["adjustment"];
    assert_eq!(group.translation_world, [6., 0., 0.]);
    assert_eq!(
        group.clearance_check_ids,
        vec!["slot_shank_running_gap".to_string()]
    );
    let check = &built.clearance_checks["slot_shank_running_gap"];
    assert_eq!(check.motion_proof.as_deref(), Some("slot_translation"));
    assert_eq!(check.moving_endpoint.as_deref(), Some("right"));
    assert!(check.lower_bound_mm >= 0.49);
    assert!(check.sweep.as_ref().unwrap().slot_motion_proof.is_some());
}

#[test]
fn loaded_drive_study_reuses_upper_parts_and_proves_four_named_slots() {
    let modules = BTreeMap::from([("drive_module_rotation.star".into(), RELEASE.into())]);
    let recipe = cad_recipe::from_starlark_with_modules(
        "drive_module_adjustment.star",
        DRIVE,
        &BTreeMap::new(),
        &modules,
    )
    .unwrap();
    let built = cad_build::BuildEngine::default()
        .build_outputs(&recipe, &["drive_module_adjustment".into()])
        .unwrap();
    let group = &built.rigid_translation_groups["upper_slot_travel"];
    assert_eq!(group.members.len(), 29);
    assert_eq!(group.translation_world, [6., 0., 0.]);
    assert_eq!(group.clearance_check_ids.len(), 4);
    assert_eq!(
        built.assemblies["drive_module_adjustment"].instances.len(),
        30
    );
    for i in 0..4 {
        let check = &built.clearance_checks[&format!("mounting_slot_{i}_shank_gap")];
        assert_eq!(check.motion_proof.as_deref(), Some("slot_translation"));
        assert_eq!(check.moving_endpoint.as_deref(), Some("right"));
        assert!(check.lower_bound_mm >= 0.49);
    }
}
