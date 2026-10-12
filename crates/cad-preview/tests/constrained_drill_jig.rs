use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/constrained_drill_jig.star");
fn recipe(pitch: f64, radius: f64, thickness: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "constrained_drill_jig.star",
        SOURCE,
        &BTreeMap::from([
            ("pitch".into(), pitch),
            ("bore_radius".into(), radius),
            ("thickness".into(), thickness),
        ]),
    )
    .unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-6 + 2e-8 * expected.abs(),
        "{actual} != {expected}"
    );
}
#[test]
fn independent_circle_subtraction_and_material_rollup_across_twenty_seven_designs() {
    for pitch in [20., 25., 30.] {
        for radius in [3_f64, 4., 5.] {
            for t in [6_f64, 8., 10.] {
                let build = cad_build::build(&recipe(pitch, radius, t)).unwrap();
                for report in build.sketch_reports.values() {
                    assert!(report.is_fully_constrained());
                    assert!(report.max_residual_mm <= report.tolerance_mm);
                }
                let report = &build.sketch_reports["guide_layout"];
                for (id, x) in [
                    ("left", 50. - pitch),
                    ("middle", 50.),
                    ("right", 50. + pitch),
                ] {
                    let center = report.points[id];
                    let rim = report.points[&format!("{id}_rim")];
                    near(center[0], x);
                    near(center[1], 30.);
                    near((rim[0] - center[0]).hypot(rim[1] - center[1]), radius);
                }
                let pi = std::f64::consts::PI;
                let area = 6000. - 3. * pi * radius.powi(2);
                let volume = t * area;
                let props = build.outputs["plate"]
                    .result
                    .model
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.volume, volume);
                for (a, b) in props.centroid.unwrap().into_iter().zip([50., 30., t / 2.]) {
                    near(a, b)
                }
                let ix = 100. * 60_f64.powi(3) / 12. - 3. * pi * radius.powi(4) / 4.;
                let iy = 60. * 100_f64.powi(3) / 12.
                    - 3. * pi * radius.powi(4) / 4.
                    - 2. * pi * radius.powi(2) * pitch * pitch;
                let expected = [
                    [t * ix + volume * t * t / 12., 0., 0.],
                    [0., t * iy + volume * t * t / 12., 0.],
                    [0., 0., t * (ix + iy)],
                ];
                for (a, b) in props
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(expected.into_iter().flatten())
                {
                    assert!((a - b).abs() <= 2e-4 * volume, "{a} != {b}");
                }
                let assembly = &build.assemblies["constrained_drill_jig"];
                assert_eq!(assembly.instances.len(), 6);
                assert_eq!(assembly.parts.len(), 3);
                let guides = 3. * pi * (2. * radius - 1.) * (t + 6.);
                let feet = 19200.;
                near(assembly.material_volume(), volume + feet + guides);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., 0.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([100., 60., 18. + t]) {
                    near(a, b)
                }
                let plate_mass = volume * 2700e-9;
                let feet_mass = feet * 2700e-9;
                let guide_mass = guides * 7800e-9;
                let mass = plate_mass + feet_mass + guide_mass;
                let expected_z = (plate_mass * (12. + t / 2.)
                    + feet_mass * 6.
                    + guide_mass * (12. + (t + 6.) / 2.))
                    / mass;
                let properties = assembly
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(properties.mass_kg.unwrap(), mass);
                for (a, b) in properties
                    .center_of_mass
                    .unwrap()
                    .into_iter()
                    .zip([50., 30., expected_z])
                {
                    near(a, b)
                }
            }
        }
    }
}
#[test]
fn pitch_edits_move_holes_without_rebuilding_guides_or_feet() {
    let mut engine = cad_build::BuildEngine::default();
    let first = engine.build(&recipe(25., 4., 8.)).unwrap();
    let warm = engine.build(&recipe(25., 4., 8.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(first.sketch_reports, warm.sketch_reports);
    let moved = engine.build(&recipe(28., 4., 8.)).unwrap();
    assert_eq!(moved.stats.computed_solids, 1);
    let resized = engine.build(&recipe(28., 4.5, 8.)).unwrap();
    assert_eq!(resized.stats.computed_solids, 2);
    near(
        first.assemblies["constrained_drill_jig"].material_volume(),
        moved.assemblies["constrained_drill_jig"].material_volume(),
    );
    assert_ne!(
        first.outputs["plate"].content_hash,
        moved.outputs["plate"].content_hash
    );
}
