use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/serial_blind_fixture.star");

fn source_recipe(side: f64, first_depth: f64, second_depth: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_modules(
        "serial_blind_fixture.star",
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
fn source_owned_serial_pockets_keep_both_native_floors_and_sections_on_both_caps() {
    for side in [0., 1.] {
        let recipe = source_recipe(side, 3., 7.);
        let mut engine = cad_build::BuildEngine::default();
        let built = engine.build_outputs(&recipe, &["fixture".into()]).unwrap();
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
        let first_area = 200. + std::f64::consts::PI * 25.;
        let second_area = 264. + std::f64::consts::PI * 36.;
        let stock_area = 120. * 80. - 4. * std::f64::consts::PI * 9.;
        assert!(
            (built.outputs["fixture"].result.model.volume()
                - (stock_area * 12. - first_area * 3. - second_area * 7.))
                .abs()
                < 1e-6
        );
        for (name, expected) in [
            ("both_pockets", stock_area - first_area - second_area),
            ("deep_pocket_only", stock_area - second_area),
            ("intact_web", stock_area),
        ] {
            assert!((built.section_views[name].query.section.area - expected).abs() < 1e-6);
        }
        for (name, expected) in [
            ("locating_depth", 3.),
            ("relief_depth", 7.),
            ("remaining_web", 5.),
        ] {
            assert!((built.datum_dimensions[name].value_mm - expected).abs() < 1e-6);
        }
        assert_eq!(
            built.drawing_sheets["serial_fixture_machining"].status,
            "layout_validated"
        );
        assert_eq!(
            engine
                .build_outputs(&recipe, &["fixture".into()])
                .unwrap()
                .stats
                .computed_solids,
            0
        );
        let edited = source_recipe(side, 3., 8.);
        let changed = engine.build_outputs(&edited, &["fixture".into()]).unwrap();
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
fn source_owned_serial_floor_obligation_survives_selected_warm_build() {
    let mut engine = cad_build::BuildEngine::default();
    let recipe = source_recipe(1., 3., 7.);
    engine
        .build_outputs(&recipe, &["first_stage".into()])
        .unwrap();
    let mut bad = recipe;
    let dimension = bad
        .datum_dimensions
        .iter_mut()
        .find(|d| d.id == "locating_depth")
        .unwrap();
    dimension.min = Some(3.1);
    dimension.max = Some(3.2);
    assert!(matches!(
        engine.build_outputs(&bad, &["first_stage".into()]),
        Err(cad_build::BuildError::DatumDimensionFailed { .. })
    ));
}
