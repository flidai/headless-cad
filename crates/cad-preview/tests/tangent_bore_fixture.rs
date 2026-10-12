use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/tangent_bore_fixture.star");
fn recipe(width: f64, radius: f64, thickness: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "tangent_bore_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("half_width".into(), width),
            ("bore_radius".into(), radius),
            ("thickness".into(), thickness),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 2e-6 + 2e-8 * b.abs(), "{a} != {b}")
}
#[test]
fn independent_wedge_incircle_geometry_and_moments_across_twenty_seven_designs() {
    for width in [20_f64, 30., 35.] {
        for radius in [3_f64, 4., 6.] {
            for t in [6_f64, 8., 10.] {
                let b = cad_build::build(&recipe(width, radius, t)).unwrap();
                let report = &b.sketch_reports["bore_layout"];
                assert!(report.is_fully_constrained());
                let distance = radius * width.hypot(60.) / width;
                for (id, y) in [
                    ("lower_center", 12. + distance),
                    ("upper_center", 68. - distance),
                ] {
                    let p = report.points[id];
                    near(p[0], 50.);
                    near(p[1], y);
                }
                let pi = std::f64::consts::PI;
                let v = (8000. - 2. * pi * radius.powi(2)) * t;
                let ix = 100. * 80_f64.powi(3) / 12.
                    - 2. * (pi * radius.powi(4) / 4.
                        + pi * radius.powi(2) * (28. - distance).powi(2));
                let iy = 80. * 100_f64.powi(3) / 12. - 2. * pi * radius.powi(4) / 4.;
                let props = b.outputs["plate"]
                    .result
                    .model
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.volume, v);
                for (a, e) in props.centroid.unwrap().into_iter().zip([50., 40., t / 2.]) {
                    near(a, e)
                }
                let tensor = [
                    [t * ix + v * t * t / 12., 0., 0.],
                    [0., t * iy + v * t * t / 12., 0.],
                    [0., 0., t * (ix + iy)],
                ];
                for (a, e) in props
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(tensor.into_iter().flatten())
                {
                    assert!((a - e).abs() < 2e-4 * v, "{a} != {e}")
                }
                let asm = &b.assemblies["tangent_bore_fixture"];
                assert_eq!(asm.instances.len(), 7);
                assert_eq!(asm.parts.len(), 3);
                near(asm.material_volume(), v + 19200. + 768.);
                let bounds = asm.bounds().unwrap();
                for (a, e) in bounds.min.into_iter().zip([0., 0., 0.]) {
                    near(a, e)
                }
                for (a, e) in bounds.max.into_iter().zip([100., 80., 16. + t]) {
                    near(a, e)
                }
                let pm = v * 2700e-9;
                let fm = 19200. * 2700e-9;
                let cm = 768. * 7800e-9;
                let mass = pm + fm + cm;
                let props = asm
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.mass_kg.unwrap(), mass);
                let z = (pm * (12. + t / 2.) + fm * 6. + cm * (14. + t)) / mass;
                for (a, e) in props.center_of_mass.unwrap().into_iter().zip([50., 40., z]) {
                    near(a, e)
                }
            }
        }
    }
}
#[test]
fn wedge_edits_move_bores_preserving_material_volume_and_other_definitions() {
    let mut engine = cad_build::BuildEngine::default();
    let a = engine.build(&recipe(30., 4., 8.)).unwrap();
    let warm = engine.build(&recipe(30., 4., 8.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(a.sketch_reports, warm.sketch_reports);
    let b = engine.build(&recipe(25., 4., 8.)).unwrap();
    assert_eq!(b.stats.computed_solids, 1);
    near(
        a.assemblies["tangent_bore_fixture"].material_volume(),
        b.assemblies["tangent_bore_fixture"].material_volume(),
    );
    assert_ne!(
        a.outputs["plate"].content_hash,
        b.outputs["plate"].content_hash
    );
    assert_ne!(
        a.sketch_reports["bore_layout"].points["lower_center"],
        b.sketch_reports["bore_layout"].points["lower_center"]
    );
    let c = engine.build(&recipe(25., 5., 8.)).unwrap();
    assert_eq!(c.stats.computed_solids, 1);
}
