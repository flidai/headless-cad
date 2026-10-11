use std::collections::BTreeMap;
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7_f64.max(b.abs() * 1e-11), "{a} != {b}");
}
#[test]
fn circular_recipe_edits_preserve_datums_native_geometry_and_exchange() {
    let source = include_str!("../../../examples/shaft_stack.star");
    let mut engine = cad_build::BuildEngine::default();
    for (r, h) in [(4., 30.), (8., 60.), (12., 100.)] {
        let recipe = cad_recipe::from_starlark_with_params(
            "shaft_stack.star",
            source,
            &BTreeMap::from([("shaft_radius".into(), r), ("shaft_length".into(), h)]),
        )
        .unwrap();
        let result = engine.build(&recipe).unwrap();
        let model = &result.outputs["shaft"].result.model;
        assert_eq!(
            (model.vertices.len(), model.edges.len(), model.faces.len()),
            (2, 3, 3)
        );
        let mass = model
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        let volume = std::f64::consts::PI * r * r * h;
        close(mass.volume, volume);
        close(mass.centroid.unwrap()[2], h / 2.);
        close(
            mass.central_inertia.unwrap()[0][0],
            volume * (r * r / 4. + h * h / 12.),
        );
        close(mass.central_inertia.unwrap()[2][2], volume * r * r / 2.);
        let assembly = &result.assemblies["shaft_stack"];
        let cap = assembly.instances.iter().find(|i| i.id == "cap").unwrap();
        close(cap.transform.translation[2], 6. + h);
        close(
            assembly.material_volume(),
            volume + std::f64::consts::PI * (18. * 18. * 6. + 12. * 12. * 4.),
        );
        let html = cad_preview::to_html(model).unwrap();
        assert!(html.contains("cad-scene-data-v3"));
        assert!(html.contains("semantic_face_id"));
        assert!(html.contains("tessellation_error_mm"));
        let json = serde_json::to_string(&recipe).unwrap();
        let restored: cad_build::Recipe = serde_json::from_str(&json).unwrap();
        let warm = engine.build(&restored).unwrap();
        assert_eq!(warm.stats.computed_solids, 0);
        close(warm.outputs["shaft"].result.model.volume(), volume);
    }
}
#[test]
fn circle_recipe_rejects_unknown_fields_invalid_dimensions_and_solid_use() {
    for source in ["p=circle(id='p',radius=0)\nb=extrude(id='b',profile=p,height=4)\noutput(name='b',feature=b)","p=circle(id='p',radius=2)\noutput(name='p',feature=p)"] {
        let recipe=cad_recipe::from_starlark("bad.star",source).unwrap();
        assert!(cad_build::build(&recipe).is_err());
    }
    for json in [
        r#"{"op":"circle","id":"p","radius":2,"width":4}"#,
        r#"{"op":"circle","id":"p","radius":2,"radius":3}"#,
    ] {
        assert!(serde_json::from_str::<cad_build::Feature>(json).is_err());
    }
}
