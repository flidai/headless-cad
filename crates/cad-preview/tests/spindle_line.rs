use std::collections::BTreeMap;

#[test]
fn spindle_line_has_independent_material_totals_and_cached_spacing_edits() {
    let mut engine = cad_build::BuildEngine::default();
    for (index, spacing) in [100., 130.].into_iter().enumerate() {
        let recipe = cad_recipe::from_starlark_with_params(
            "spindle_line.star",
            include_str!("../../../examples/spindle_line.star"),
            &BTreeMap::from([("spacing".into(), spacing)]),
        )
        .unwrap();
        let built = engine.build(&recipe).unwrap();
        assert_eq!(built.stats.computed_solids, if index == 0 { 6 } else { 0 });
        let line = &built.assemblies["spindle_line"];
        assert_eq!(line.instances.len(), 48);
        assert_eq!(line.parts.len(), 6);
        let pi = std::f64::consts::PI;
        let base = 70. * 70. * 6. - pi * 9_f64.powi(2) * 6.;
        let shaft = pi * (7_f64.powi(2) * 48. + 5_f64.powi(2) * 12.);
        let retainer = pi * (14_f64.powi(2) - 9_f64.powi(2)) * 3.;
        let mut volume = 0.;
        let mut mass = 0.;
        for variant in 0..3 {
            let r = 22. + variant as f64 * 0.5;
            let t = 14. + variant as f64 * 0.5;
            // Separate cylinder, conical frustum and neck, minus the through bore.
            let housing = pi
                * (r * r * 8. + 12. / 3. * (r * r + r * t + t * t) + t * t * 16.
                    - 9_f64.powi(2) * 36.);
            volume += 4. * (base + shaft + retainer + housing);
            mass += 4. * ((base + housing) * 2700. + (shaft + retainer) * 7800.) * 1e-9;
        }
        let properties = line
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        assert!((line.material_volume() - volume).abs() < 1e-7);
        assert!((properties.mass_kg.unwrap() - mass).abs() < 1e-10);
        let bounds = line.bounds().unwrap();
        assert_eq!(bounds.min, [-35., -35., 0.]);
        assert_eq!(bounds.max, [3. * spacing + 35., 2. * spacing + 35., 60.]);
        assert_eq!(built.property_assertions.len(), 1);
        // Default portable rendering remains bounded while larger parametric arrays
        // use the shared GPU/streaming path.
        cad_preview::to_assembly_scene_json(line).unwrap();
    }
}

#[test]
fn spindle_line_rejects_unbounded_or_fractional_configuration() {
    for (name, value) in [
        ("stations", 1001.),
        ("stations", 1.5),
        ("variants", 13.),
        ("columns", 0.),
        ("spacing", 79.),
    ] {
        assert!(cad_recipe::from_starlark_with_params(
            "spindle_line.star",
            include_str!("../../../examples/spindle_line.star"),
            &BTreeMap::from([(name.into(), value)])
        )
        .is_err());
    }
}
