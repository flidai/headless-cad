use std::collections::BTreeMap;

#[test]
fn external_instrument_tracks_one_reused_nested_cartridge_without_rebuilding_geometry() {
    let source = include_str!("../../../examples/instrumented_cartridge.star");
    let mut engine = cad_build::BuildEngine::default();
    let mut initial_hashes = None;
    for (index, spacing) in [80., 100., 120.].into_iter().enumerate() {
        let recipe = cad_recipe::from_starlark_with_params(
            "instrumented_cartridge.star",
            source,
            &BTreeMap::from([("module_spacing".into(), spacing)]),
        )
        .unwrap();
        let built = engine.build(&recipe).unwrap();
        assert_eq!(built.stats.computed_solids, if index == 0 { 13 } else { 0 });
        if let Some(hashes) = &initial_hashes {
            assert_eq!(&built.feature_hashes, hashes);
        }
        initial_hashes = Some(built.feature_hashes.clone());
        let scene = &built.assemblies["instrumented_cartridge"];
        assert_eq!(scene.instances.len(), 11);
        assert_eq!(scene.parts.len(), 7);
        // Independent cylinder/annulus volume sums; the rectangular platform and
        // instrument mount contribute 84,768 mm³ independent of module spacing.
        let volume = 84768. + 26414. * std::f64::consts::PI;
        assert!((scene.material_volume() - volume).abs() < 1e-7);
        let mount = scene
            .instances
            .iter()
            .find(|i| scene.instance_paths[&i.id] == ["sensor_mount"])
            .unwrap();
        let sensor = scene
            .instances
            .iter()
            .find(|i| scene.instance_paths[&i.id] == ["sensor"])
            .unwrap();
        for (actual, expected) in
            mount
                .transform
                .translation
                .into_iter()
                .zip([spacing + 5., 18., 26.])
        {
            assert!((actual - expected).abs() < 1e-10);
        }
        for (actual, expected) in sensor
            .transform
            .translation
            .into_iter()
            .zip([spacing, 22., 20.])
        {
            assert!((actual - expected).abs() < 1e-10);
        }
        for (actual, expected) in sensor
            .transform
            .point([0., 0., 16.])
            .into_iter()
            .zip([spacing, 38., 20.])
        {
            assert!((actual - expected).abs() < 1e-10);
        }
        let first = scene
            .instance_paths
            .iter()
            .find(|(_, p)| {
                p == &&vec![
                    "module_a".to_owned(),
                    "cartridge".to_owned(),
                    "housing".to_owned(),
                ]
            })
            .unwrap()
            .0;
        assert_eq!(
            scene
                .instances
                .iter()
                .find(|i| &i.id == first)
                .unwrap()
                .transform
                .translation,
            [0., 0., 0.]
        );
    }
}
