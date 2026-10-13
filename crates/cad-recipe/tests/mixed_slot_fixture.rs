use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/mixed_slot_fixture.star");

fn source_recipe(side: f64, first_depth: f64, second_depth: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_modules(
        "mixed_slot_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("pocket_side".into(), side),
            ("first_depth".into(), first_depth),
            ("second_depth".into(), second_depth),
        ]),
        &BTreeMap::new(),
    )
    .unwrap()
}

#[test]
fn source_owned_mixed_fixture_keeps_four_through_slots_and_two_serial_floors() {
    for side in [0., 1.] {
        let recipe = source_recipe(side, 3., 7.);
        let mut engine = cad_build::BuildEngine::default();
        let built = engine.build_outputs(&recipe, &["fixture".into()]).unwrap();
        let stock_area = 120. * 80. - 4. * std::f64::consts::PI * 9.;
        let through_area = 4. * (48. + std::f64::consts::PI * 9.);
        let first_area = 200. + std::f64::consts::PI * 25.;
        let second_area = 264. + std::f64::consts::PI * 36.;
        assert!(
            (built.outputs["fixture"].result.model.volume()
                - ((stock_area - through_area) * 12. - first_area * 3. - second_area * 7.))
                .abs()
                < 1e-6
        );
        for (view, expected) in [
            (
                "both_pockets",
                stock_area - through_area - first_area - second_area,
            ),
            ("deep_pocket_only", stock_area - through_area - second_area),
            ("through_slots_only", stock_area - through_area),
        ] {
            assert!(
                (built.section_views[view].query.section.area - expected).abs() < 1e-6,
                "{view}"
            );
        }
        assert_eq!(
            built.face_assertions["first_floor_survives"]
                .witnesses
                .len(),
            1
        );
        assert_eq!(
            built.face_assertions["first_walls_survive"].witnesses.len(),
            4
        );
        for name in ["front_lower", "front_upper", "rear_lower", "rear_upper"] {
            assert_eq!(
                built.face_assertions[&format!("{name}_walls_survive")]
                    .witnesses
                    .len(),
                4
            );
        }
        for (name, expected) in [
            ("locating_depth", 3.),
            ("relief_depth", 7.),
            ("remaining_web", 5.),
        ] {
            assert!((built.datum_dimensions[name].value_mm - expected).abs() < 1e-6);
        }
        let sizes = &built.section_views["through_sizes"].slot_dimensions;
        assert_eq!(sizes.len(), 2);
        assert!((sizes[0].value_mm - 6.).abs() < 1e-6);
        assert!((sizes[1].value_mm - 14.).abs() < 1e-6);
        assert_eq!(sizes[0].selection.witnesses.len(), 4);
        assert!(
            (built.section_views["through_positions"].dimensions[0].value_mm - 80.).abs() < 1e-6
        );
        assert_eq!(
            built.slot_center_datums["mixed_fixture_part"]["front_lower"]
                .walls
                .witnesses
                .len(),
            4
        );
        assert_eq!(
            built.drawing_sheets["mixed_fixture_machining"].status,
            "layout_validated"
        );
        assert_eq!(
            built.drawing_sheets["mixed_mounting_references"].status,
            "layout_validated"
        );
        let warm = engine
            .build_outputs(&recipe, &["through_stage".into()])
            .unwrap();
        assert_eq!(warm.stats.computed_solids, 0);
        assert_eq!(warm.section_views["through_sizes"].slot_dimensions.len(), 2);
        let changed = engine
            .build_outputs(&source_recipe(side, 3., 8.), &["through_stage".into()])
            .unwrap();
        assert_eq!(
            changed.feature_hashes["mounting_slots"],
            built.feature_hashes["mounting_slots"]
        );
        assert_eq!(
            changed.feature_hashes["locating_pocket"],
            built.feature_hashes["locating_pocket"]
        );
        assert_ne!(
            changed.feature_hashes["relief_pocket"],
            built.feature_hashes["relief_pocket"]
        );
        assert!((changed.datum_dimensions["locating_depth"].value_mm - 3.).abs() < 1e-6);
        assert!((changed.datum_dimensions["relief_depth"].value_mm - 8.).abs() < 1e-6);
    }
}

#[test]
fn source_owned_selected_warm_mixed_checks_still_refuse_changed_native_bounds() {
    let mut engine = cad_build::BuildEngine::default();
    let recipe = source_recipe(1., 3., 7.);
    engine
        .build_outputs(&recipe, &["through_stage".into()])
        .unwrap();
    let mut wrong_size = recipe.clone();
    let dimension = wrong_size
        .section_views
        .iter_mut()
        .find(|v| v.id == "through_sizes")
        .unwrap()
        .slot_dimensions
        .iter_mut()
        .find(|d| d.id == "mounting_width")
        .unwrap();
    dimension.min = Some(6.1);
    dimension.max = Some(6.2);
    assert!(matches!(
        engine.build_outputs(&wrong_size, &["through_stage".into()]),
        Err(cad_build::BuildError::SectionDimensionFailed { .. })
    ));
    let mut wrong_floor = recipe.clone();
    let depth = wrong_floor
        .datum_dimensions
        .iter_mut()
        .find(|d| d.id == "locating_depth")
        .unwrap();
    depth.min = Some(3.1);
    depth.max = Some(3.2);
    assert!(matches!(
        engine.build_outputs(&wrong_floor, &["through_stage".into()]),
        Err(cad_build::BuildError::DatumDimensionFailed { .. })
    ));
    let mut wrong_center = recipe;
    wrong_center
        .slot_center_datums
        .iter_mut()
        .find(|d| d.name == "front_lower")
        .unwrap()
        .role = "slot_missing_walls".into();
    assert!(matches!(
        engine.build_outputs(&wrong_center, &["through_stage".into()]),
        Err(cad_build::BuildError::SlotCenterDatumFailed { .. })
    ));
}
