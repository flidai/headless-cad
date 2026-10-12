use std::collections::BTreeMap;

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-8 * expected.abs().max(1.),
        "{actual} != {expected}"
    );
}

#[test]
fn between_centers_native_properties_and_parameter_edits() {
    let source = include_str!("../../../examples/centering_fixture.star");
    let mut engine = cad_build::BuildEngine::default();
    let pi = std::f64::consts::PI;
    for length in [40., 80., 120.] {
        for clearance in [0.05, 0.2, 1.] {
            let mut previous_volume = None;
            for (index, depth) in [6., 12., 16.].into_iter().enumerate() {
                let recipe = cad_recipe::from_starlark_with_params(
                    "centering_fixture.star",
                    source,
                    &BTreeMap::from([
                        ("work_length".into(), length),
                        ("tip_depth".into(), depth),
                        ("bore_clearance".into(), clearance),
                    ]),
                )
                .unwrap();
                let built = engine.build(&recipe).unwrap();
                // Within one parameter sweep only the pin and drilled workpiece change.
                // Definitions already seen in earlier sweeps may be cache hits too.
                if index > 0 {
                    assert!(built.stats.computed_solids <= 2);
                }
                let assembly = &built.assemblies["centering_fixture"];
                assert_eq!(assembly.parts.len(), 4);
                assert_eq!(assembly.instances.len(), 6);
                let v_pin = pi * (1280. + 12. * depth);
                let v_work = pi * (256. * length - 24. * depth);
                let v_holder = 25200. - 20. * pi * (8. + clearance).powi(2);
                let v_base = 480. * (length + 60.);
                let volume = 2. * v_pin + v_work + 2. * v_holder + v_base;
                let mass = (2. * (v_pin + v_holder) * 7800. + (v_work + v_base) * 2700.) * 1e-9;
                let native = assembly
                    .mass_properties(recipe.tolerance.unwrap_or_default())
                    .unwrap();
                near(native.mass_kg.unwrap(), mass);
                near(native.center_of_mass.unwrap()[0], length / 2.);
                near(
                    built.property_assertions["material_envelope"]
                        .actual
                        .unwrap(),
                    volume,
                );
                if let Some(previous) = previous_volume {
                    near(volume, previous);
                }
                previous_volume = Some(volume);
                near(built.outputs["center_pin"].result.model.volume(), v_pin);
                near(built.outputs["workpiece"].result.model.volume(), v_work);
                assert!(!built.outputs["center_pin"]
                    .result
                    .history
                    .generated_faces
                    .contains_key("end_cap"));
                assert_eq!(
                    built.outputs["center_pin"].result.history.generated_faces["conical_faces"]
                        .len(),
                    1
                );
                assert_eq!(
                    built.outputs["workpiece"].result.history.generated_faces["inner_walls"].len(),
                    2
                );
                // Check seating transforms independently of symmetric mass sums.
                for (id, origin, tip, axis) in [
                    (
                        "left_center",
                        [-20., 0., 40.],
                        [depth, 0., 40.],
                        [1., 0., 0.],
                    ),
                    (
                        "right_center",
                        [length + 20., 0., 40.],
                        [length - depth, 0., 40.],
                        [-1., 0., 0.],
                    ),
                ] {
                    let instance = assembly.instances.iter().find(|i| i.id == id).unwrap();
                    for (a, b) in instance
                        .transform
                        .point([0.; 3])
                        .into_iter()
                        .zip(origin)
                        .chain(
                            instance
                                .transform
                                .point([0., 0., 20. + depth])
                                .into_iter()
                                .zip(tip),
                        )
                        .chain(
                            instance
                                .transform
                                .vector([0., 0., 1.])
                                .into_iter()
                                .zip(axis),
                        )
                    {
                        near(a, b);
                    }
                }
                let bounds = assembly.bounds().unwrap();
                for (actual, expected) in bounds
                    .min
                    .into_iter()
                    .zip([-30., -30., 0.])
                    .chain(bounds.max.into_iter().zip([length + 30., 30., 56.]))
                {
                    near(actual, expected);
                }
                for report in built.property_assertions.values() {
                    assert_eq!(report.target_content_hash, assembly.content_hash);
                }
            }
        }
    }
    for (name, value) in [
        ("work_length", 39.),
        ("work_length", 121.),
        ("tip_depth", 5.),
        ("tip_depth", 17.),
        ("bore_clearance", 0.01),
        ("bore_clearance", 1.1),
    ] {
        assert!(cad_recipe::from_starlark_with_params(
            "bad.star",
            source,
            &BTreeMap::from([(name.into(), value)])
        )
        .is_err());
    }
}

#[test]
fn changing_only_tip_depth_rebuilds_two_solids_and_preserves_global_checks() {
    let source = include_str!("../../../examples/centering_fixture.star");
    let mut engine = cad_build::BuildEngine::default();
    let mut volume = None;
    for depth in [12., 13.] {
        let recipe = cad_recipe::from_starlark_with_params(
            "centering_fixture.star",
            source,
            &BTreeMap::from([("tip_depth".into(), depth)]),
        )
        .unwrap();
        let built = engine.build(&recipe).unwrap();
        assert_eq!(
            built.stats.computed_solids,
            if depth == 12. { 4 } else { 2 }
        );
        let actual = built.property_assertions["material_envelope"]
            .actual
            .unwrap();
        if let Some(previous) = volume {
            near(actual, previous);
        }
        volume = Some(actual);
        let selected = engine
            .build_outputs(&recipe, &["center_pin".into()])
            .unwrap();
        assert_eq!(selected.stats.computed_solids, 0);
        assert!(selected.assemblies.is_empty());
        assert_eq!(
            serde_json::to_value(selected.property_assertions).unwrap(),
            serde_json::to_value(built.property_assertions).unwrap()
        );
    }
}
