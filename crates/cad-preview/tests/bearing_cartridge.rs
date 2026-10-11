use std::collections::BTreeMap;
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7_f64.max(b.abs() * 1e-11), "{a} != {b}");
}
#[test]
fn bore_edits_rebuild_dependents_and_preserve_annular_datums() {
    let source = include_str!("../../../examples/bearing_cartridge.star");
    let mut engine = cad_build::BuildEngine::default();
    for (index, (r, h)) in [(4., 20.), (6., 40.), (8., 80.)].into_iter().enumerate() {
        let recipe = cad_recipe::from_starlark_with_params(
            "bearing_cartridge.star",
            source,
            &BTreeMap::from([("bore_radius".into(), r), ("sleeve_length".into(), h)]),
        )
        .unwrap();
        let result = engine.build(&recipe).unwrap();
        assert_eq!(result.stats.computed_solids, if index == 0 { 3 } else { 2 });
        let model = &result.outputs["sleeve"].result.model;
        assert_eq!(
            (model.vertices.len(), model.edges.len(), model.faces.len()),
            (4, 6, 4)
        );
        let props = model
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        let volume = std::f64::consts::PI * (100. - r * r) * h;
        close(props.volume, volume);
        close(
            props.central_inertia.unwrap()[2][2],
            volume * (100. + r * r) / 2.,
        );
        close(
            props.central_inertia.unwrap()[0][0],
            volume * ((100. + r * r) / 4. + h * h / 12.),
        );
        let assembly = &result.assemblies["bearing_cartridge"];
        let rear = assembly
            .instances
            .iter()
            .find(|i| i.id == "rear_retainer")
            .unwrap();
        close(rear.transform.translation[2], h - 6.);
        let front = assembly
            .instances
            .iter()
            .find(|i| i.id == "front_retainer")
            .unwrap();
        close(front.transform.translation[2], 0.);
        let shaft = assembly.instances.iter().find(|i| i.id == "shaft").unwrap();
        close(shaft.transform.translation[2], -15.);
        let shaft_volume = std::f64::consts::PI * (r - 1.).powi(2) * (h + 30.);
        let retainer_volume = std::f64::consts::PI * (256. - 100.) * 6.;
        close(
            assembly.material_volume(),
            volume + shaft_volume + 2. * retainer_volume,
        );
        let report = assembly
            .bill_of_materials(recipe.tolerance.unwrap_or_default())
            .unwrap();
        close(
            report.total_mass_kg.unwrap(),
            volume * 8.8e-6 + (shaft_volume + 2. * retainer_volume) * 7.8e-6,
        );
        let properties = assembly
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        close(properties.center_of_mass.unwrap()[2], h / 2.);
        close(properties.volume_centroid.unwrap()[2], h / 2.);
        let sleeve_ix = volume * ((100. + r * r) / 4. + h * h / 12.);
        let shaft_ix = shaft_volume * ((r - 1.).powi(2) / 4. + (h + 30.).powi(2) / 12.);
        let ring_ix = retainer_volume * ((256. + 100.) / 4. + 3. + (h / 2. - 3.).powi(2));
        close(
            properties.central_volume_inertia.unwrap()[0][0],
            sleeve_ix + shaft_ix + 2. * ring_ix,
        );
        close(
            properties.central_mass_inertia.unwrap()[0][0],
            sleeve_ix * 8.8e-6 + (shaft_ix + 2. * ring_ix) * 7.8e-6,
        );
        let html = cad_preview::to_html(model).unwrap();
        assert!(html.contains("semantic_face_id\":3"));
        let again = engine.build(&recipe).unwrap();
        assert_eq!(again.stats.computed_solids, 0);
    }
}
#[test]
fn optional_bore_preserves_solid_circle_and_rejects_bad_profiles() {
    let make = |extra: &str| {
        cad_recipe::from_starlark("circle.star",&format!("p=circle(id='p',radius=10{extra})\nb=extrude(id='b',profile=p,height=6)\noutput(name='b',feature=b)")).unwrap()
    };
    let plain = make("");
    let zero = make(",inner_radius=0");
    assert_eq!(
        serde_json::to_string(&plain).unwrap(),
        serde_json::to_string(&zero).unwrap()
    );
    let mut engine = cad_build::BuildEngine::default();
    let a = engine.build(&plain).unwrap();
    let b = engine.build(&zero).unwrap();
    assert_eq!(a.outputs["b"].content_hash, b.outputs["b"].content_hash);
    assert_eq!(b.stats.computed_solids, 0);
    for extra in [
        ",inner_radius=-1",
        ",inner_radius=10",
        ",inner_radius=11",
        ",inner_radius=0.00000001",
        ",inner_radius=9.99999999",
    ] {
        assert!(cad_build::build(&make(extra)).is_err(), "{extra}");
    }
    let json = r#"{"op":"circle","id":"p","radius":10,"inner_radius":4,"inner_radius":5}"#;
    assert!(serde_json::from_str::<cad_build::Feature>(json).is_err());
}
