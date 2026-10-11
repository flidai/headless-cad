use std::collections::BTreeMap;

#[test]
fn mounted_module_engineering_gates_preserve_cached_geometry_and_reject_overbudget_designs() {
    let source = include_str!("../../../examples/mounted_drive_module.star");
    let mut engine = cad_build::BuildEngine::default();
    for (index, spacing) in [80., 100., 120.].into_iter().enumerate() {
        let recipe = cad_recipe::from_starlark_with_params(
            "mounted_drive_module.star",
            source,
            &BTreeMap::from([("module_spacing".into(), spacing)]),
        )
        .unwrap();
        let built = engine.build(&recipe).unwrap();
        assert_eq!(built.stats.computed_solids, if index == 0 { 13 } else { 0 });
        assert_eq!(built.property_assertions.len(), 3);
        let assembly = &built.assemblies["mounted_drive_module"];
        let native = assembly
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        // Independent density-weighted sums for both modules, sensor, mount and
        // slotted platform. Units are mm³, kg and kg·mm.
        let mass = 0.2288736 + 0.132242 * std::f64::consts::PI;
        let x = (13.32288
            + (0.065905 * std::f64::consts::PI + 0.001296 + 0.000432 * std::f64::consts::PI)
                * spacing)
            / mass;
        assert!((native.mass_kg.unwrap() - mass).abs() < 1e-10);
        assert!((native.center_of_mass.unwrap()[0] - x).abs() < 1e-9);
        for (id, expected) in [
            ("material_envelope", 84768. + 26414. * std::f64::consts::PI),
            ("mass_budget", mass),
            ("balance_envelope", x),
        ] {
            let report = &built.property_assertions[id];
            assert!((report.actual.unwrap() - expected).abs() < 1e-7);
            assert_eq!(report.target_content_hash, assembly.content_hash);
        }
        // A body-only selection still enforces the assembly's global gates.
        let selected = engine.build_outputs(&recipe, &["housing".into()]).unwrap();
        assert_eq!(selected.stats.computed_solids, 0);
        assert_eq!(selected.property_assertions.len(), 3);
        assert_eq!(
            serde_json::to_value(selected.property_assertions).unwrap(),
            serde_json::to_value(built.property_assertions).unwrap()
        );
    }
    let too_heavy = source.replace(
        "part_number = \"MD-BASE\", material = \"Aluminum\", density_kg_per_m3 = 2700",
        "part_number = \"MD-BASE\", material = \"Steel\", density_kg_per_m3 = 7800",
    );
    assert_ne!(too_heavy, source);
    let recipe = cad_recipe::from_starlark("heavy.star", &too_heavy).unwrap();
    let error = engine
        .build_outputs(&recipe, &["housing".into()])
        .unwrap_err();
    assert!(error.to_string().contains("mass_budget"), "{error}");
    let expanded = source.replace("module_spacing > 120", "module_spacing > 150");
    let recipe = cad_recipe::from_starlark_with_params(
        "unbalanced.star",
        &expanded,
        &BTreeMap::from([("module_spacing".into(), 140.)]),
    )
    .unwrap();
    let error = engine.build(&recipe).unwrap_err();
    assert!(error.to_string().contains("balance_envelope"), "{error}");
    let oversized = source.replace("width = 190, depth = 80", "width = 200, depth = 80");
    let recipe = cad_recipe::from_starlark("oversized.star", &oversized).unwrap();
    assert!(engine.build(&recipe).is_err());
}
