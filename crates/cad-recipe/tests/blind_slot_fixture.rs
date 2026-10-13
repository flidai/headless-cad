use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../../examples/blind_slot_fixture.star");

#[test]
fn source_owned_blind_fixture_builds_both_cap_sides_with_native_floor_depths() {
    for side in [0., 1.] {
        let recipe = cad_recipe::from_starlark_with_modules(
            "blind_slot_fixture.star",
            SOURCE,
            &BTreeMap::from([("pocket_side".into(), side)]),
            &BTreeMap::new(),
        )
        .unwrap();
        let build = cad_build::BuildEngine::default()
            .build_outputs(&recipe, &["fixture".into()])
            .unwrap();
        let area = 120. * 80. - 4. * std::f64::consts::PI * 9.;
        let pocket = 2. * 6. * 24. + std::f64::consts::PI * 36.;
        let expected = area * 12. - 2. * pocket * 4.;
        assert!((build.outputs["fixture"].result.model.volume() - expected).abs() < 1e-6);
        assert!(
            (build.section_views["inside_pockets"].query.section.area - (area - 2. * pocket)).abs()
                < 1e-6
        );
        assert!((build.section_views["intact_web"].query.section.area - area).abs() < 1e-6);
        for name in ["left_pocket_depth", "right_pocket_depth"] {
            assert!((build.datum_dimensions[name].value_mm - 4.).abs() < 1e-6);
        }
        assert!((build.datum_dimensions["remaining_web"].value_mm - 8.).abs() < 1e-6);
        assert_eq!(
            build.drawing_sheets["blind_fixture_machining"].status,
            "layout_validated"
        );
    }
}
