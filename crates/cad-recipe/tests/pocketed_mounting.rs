use cad_build::{BuildEngine, BuildError, Recipe};
use std::collections::BTreeMap;

const DRIVE: &str = include_str!("../../../examples/drive_module_rotation.star");
const TIGHTENED: &str = include_str!("../../../examples/drive_module_tightened.star");
const ADJUSTMENT: &str = include_str!("../../../examples/drive_module_adjustment.star");
fn recipe(tightened: bool, depth: f64, endpoint: f64, drop: f64) -> Recipe {
    let mut parameters = BTreeMap::from([("base_pocket_depth".into(), depth)]);
    if tightened {
        parameters.insert("mounting_endpoint".into(), endpoint);
        parameters.insert("nut_drop".into(), drop);
    }
    cad_recipe::from_starlark_with_modules(
        if tightened {
            "drive_module_tightened.star"
        } else {
            "drive_module_adjustment.star"
        },
        if tightened { TIGHTENED } else { ADJUSTMENT },
        &parameters,
        &BTreeMap::from([("drive_module_rotation.star".into(), DRIVE.into())]),
    )
    .unwrap()
}

#[test]
fn actual_pocketed_base_keeps_mounting_seats_and_slot_witnesses_at_both_ends() {
    let mut engine = BuildEngine::default();
    for (depth, endpoint) in [(0.5, -1.), (1.5, 1.)] {
        let built = engine
            .build_outputs(&recipe(true, depth, endpoint, 0.), &["washer".into()])
            .unwrap();
        assert_eq!(built.nominal_seating_checks.len(), 14);
        assert_eq!(built.clearance_checks.len(), 17);
        assert!((built.datum_dimensions["base_deep_depth"].value_mm - depth - 1.).abs() < 1e-8);
        for i in 0..4 {
            let seat = &built.nominal_seating_checks[&format!("tight_nut_{i}_base_seat")];
            assert_eq!(seat.status, "passed");
            assert_eq!(seat.right.instance_path, ["fixed_base"]);
            assert_eq!(seat.right.part_id, "base");
            assert!(seat.disk_radius_lower_mm >= 0.4);
            let gap = &built.clearance_checks[&format!("tight_slot_{i}_shank_gap")];
            assert_eq!(gap.status, "passed");
            assert_eq!(gap.left.instance_path, ["fixed_base"]);
            assert!(gap.lower_bound_mm >= 0.49 && gap.upper_bound_mm <= 0.500001);
        }
        assert!(matches!(
            engine.build_outputs(&recipe(true, depth, endpoint, 0.05), &["washer".into()]),
            Err(BuildError::NominalSeatingCheckFailed { .. })
        ));
        let restored = engine
            .build_outputs(&recipe(true, depth, endpoint, 0.), &["washer".into()])
            .unwrap();
        assert_eq!(restored.stats.computed_solids, 0);
    }
}

#[test]
fn mixed_stock_translation_has_whole_path_proofs_and_warm_overtravel_refusal() {
    let mut engine = BuildEngine::default();
    let source = recipe(false, 1., 0., 0.);
    let built = engine.build_outputs(&source, &["washer".into()]).unwrap();
    let group = &built.rigid_translation_groups["upper_slot_travel"];
    assert_eq!(group.members.len(), 29);
    assert_eq!(group.clearance_check_ids.len(), 4);
    for i in 0..4 {
        let check = &built.clearance_checks[&format!("mounting_slot_{i}_shank_gap")];
        assert_eq!(check.status, "passed");
        assert!(check.lower_bound_mm >= 0.49 && check.upper_bound_mm <= 0.500001);
        let sweep = check.sweep.as_ref().unwrap();
        assert_eq!(sweep.query_count, 2);
        let proof = sweep.slot_motion_proof.as_ref().unwrap();
        assert!(proof.complete_path_lower_bound_mm >= 0.49);
        assert_eq!(proof.stock_slot_wall_faces.len(), 4);
    }
    let mut too_far = source.clone();
    too_far.rigid_translation_groups[0].translation[0] += 1.;
    assert!(matches!(
        engine.build_outputs(&too_far, &["washer".into()]),
        Err(BuildError::ClearanceCheckFailed { .. })
    ));
    let restored = engine.build_outputs(&source, &["washer".into()]).unwrap();
    assert_eq!(restored.stats.computed_solids, 0);
}
