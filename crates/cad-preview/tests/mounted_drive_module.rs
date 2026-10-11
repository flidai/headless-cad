use std::collections::BTreeMap;

#[test]
fn whole_modules_attach_by_nested_datums_and_preserve_internal_geometry() {
    let source = include_str!("../../../examples/mounted_drive_module.star");
    let mut engine = cad_build::BuildEngine::default();
    let mut initial_hashes = None;
    for (index, spacing) in [80., 100., 120.].into_iter().enumerate() {
        let recipe = cad_recipe::from_starlark_with_params(
            "mounted_drive_module.star",
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
        let scene = &built.assemblies["mounted_drive_module"];
        assert_eq!(scene.instances.len(), 11);
        assert_eq!(scene.parts.len(), 7);
        let resolved = cad_assembly::resolve_hierarchy_placements(
            &scene.parts,
            &scene.hierarchy,
            recipe.tolerance.unwrap_or_default(),
        )
        .unwrap();
        let root = &resolved[&scene.assembly_id];
        let placement = &root
            .assemblies
            .iter()
            .find(|i| i.id == "module_b")
            .unwrap()
            .transform;
        // Internal module frame is Rx90 at [5,-3,2]; mating its housing mount
        // to the Rz90 dock yields Rz90*Rx(-90) and [spacing+2,-5,-3].
        for (actual, expected) in placement
            .translation
            .into_iter()
            .zip([spacing + 2., -5., -3.])
        {
            assert!((actual - expected).abs() < 1e-10);
        }
        for p in [[0., 0., 0.], [1., 2., 3.]] {
            let expected = [spacing + 2. - p[2], -5. + p[0], -3. - p[1]];
            for (actual, expected) in placement.point(p).into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }

        for (leaf, z) in [
            ("housing", 0.),
            ("sleeve", 10.),
            ("shaft", -10.),
            ("retainer", 30.),
        ] {
            let id = scene
                .instance_paths
                .iter()
                .find(|(_, p)| p.as_slice() == ["module_b", "cartridge", leaf])
                .unwrap()
                .0;
            let item = scene.instances.iter().find(|i| &i.id == id).unwrap();
            for (actual, expected) in
                item.transform
                    .point([1., 2., 3.])
                    .into_iter()
                    .zip([spacing - 2., 1., z + 3.])
            {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
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
