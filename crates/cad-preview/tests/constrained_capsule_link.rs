use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/constrained_capsule_link.star");
fn recipe(span: f64, radius: f64, bore: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "constrained_capsule_link.star",
        SOURCE,
        &BTreeMap::from([
            ("span".into(), span),
            ("outer_radius".into(), radius),
            ("bore_radius".into(), bore),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 2e-6 + 2e-8 * b.abs(), "{a} != {b}")
}
#[test]
fn independent_rectangle_semicircle_integrals_check_twenty_seven_linkages() {
    for span in [40_f64, 60., 80.] {
        for r in [8_f64, 10., 14.] {
            for bore in [3_f64, 4., 5.] {
                let build = cad_build::build(&recipe(span, r, bore)).unwrap();
                let report = &build.sketch_reports["capsule_layout"];
                assert!(report.is_fully_constrained());
                for (id, p) in [
                    ("a", [0., -r]),
                    ("b", [span, -r]),
                    ("c", [span, r]),
                    ("d", [0., r]),
                ] {
                    for (a, b) in report.points[id].into_iter().zip(p) {
                        near(a, b)
                    }
                }
                let pi = std::f64::consts::PI;
                let v = 8. * (2. * r * span + pi * r * r - 2. * pi * bore * bore);
                let ix = 2. * span * r.powi(3) / 3. + pi * r.powi(4) / 4. - pi * bore.powi(4) / 2.;
                let iy = r * span.powi(3) / 6.
                    + pi * r.powi(4) / 4.
                    + 4. * span * r.powi(3) / 3.
                    + pi * r * r * span * span / 4.
                    - pi * bore.powi(4) / 2.
                    - pi * bore * bore * span * span / 2.;
                let props = build.outputs["link"]
                    .result
                    .model
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.volume, v);
                for (a, b) in props.centroid.unwrap().into_iter().zip([span / 2., 0., 4.]) {
                    near(a, b)
                }
                let tensor = [
                    [8. * ix + v * 64. / 12., 0., 0.],
                    [0., 8. * iy + v * 64. / 12., 0.],
                    [0., 0., 8. * (ix + iy)],
                ];
                for (a, b) in props
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(tensor.into_iter().flatten())
                {
                    assert!((a - b).abs() < 2e-4 * v, "{a} != {b}")
                }
                let assembly = &build.assemblies["constrained_capsule_link"];
                assert_eq!(assembly.instances.len(), 6);
                assert_eq!(assembly.parts.len(), 3);
                let spacers = 20. * pi * ((r - 2.).powi(2) - bore * bore);
                let heads = 12. * pi * (bore + 2.).powi(2);
                let shafts = 56. * pi * (bore - 0.5).powi(2);
                near(
                    assembly.material_volume(),
                    2. * v + spacers + heads + shafts,
                );
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([-r, -r, 0.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([span + r, r, 34.]) {
                    near(a, b)
                }
                let mass = 2. * v * 2700e-9 + (spacers + heads + shafts) * 7800e-9;
                let z = (2. * v * 2700e-9 * 19.
                    + 7800e-9 * (spacers * 19. + heads * 3. + shafts * 20.))
                    / mass;
                let props = assembly
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.mass_kg.unwrap(), mass);
                for (a, b) in props
                    .center_of_mass
                    .unwrap()
                    .into_iter()
                    .zip([span / 2., 0., z])
                {
                    near(a, b)
                }
            }
        }
    }
}
#[test]
fn dimension_changes_rebuild_only_affected_shared_parts() {
    let mut engine = cad_build::BuildEngine::default();
    let a = engine.build(&recipe(60., 10., 4.)).unwrap();
    let warm = engine.build(&recipe(60., 10., 4.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(a.sketch_reports, warm.sketch_reports);
    let length = engine.build(&recipe(70., 10., 4.)).unwrap();
    assert_eq!(length.stats.computed_solids, 1);
    let radius = engine.build(&recipe(70., 12., 4.)).unwrap();
    assert_eq!(radius.stats.computed_solids, 2);
    let bore = engine.build(&recipe(70., 12., 5.)).unwrap();
    assert_eq!(bore.stats.computed_solids, 3);
}
