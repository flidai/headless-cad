use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/modular_bore_plate.star");
fn recipe(pitch: f64, radius: f64, t: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "modular_bore_plate.star",
        SOURCE,
        &BTreeMap::from([
            ("pitch".into(), pitch),
            ("bore_radius".into(), radius),
            ("thickness".into(), t),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 3e-6 + 2e-8 * b.abs(), "{a} != {b}")
}
#[test]
fn thirty_bore_array_has_independent_analytic_moments_and_shared_guides() {
    for pitch in [22_f64, 25., 28.] {
        for r in [3_f64, 4., 5.] {
            for t in [6_f64, 8., 10.] {
                let build = cad_build::build(&recipe(pitch, r, t)).unwrap();
                let report = &build.sketch_reports["modular_layout"];
                assert!(report.is_fully_constrained());
                assert_eq!(report.schema_version, 3);
                assert_eq!(report.points.len(), 64);
                assert_eq!(report.islands.len(), 34);
                assert_eq!(report.rank, 128);
                assert!(report
                    .islands
                    .iter()
                    .all(|i| i.status == cad_build::SolveStatus::FullyConstrained));
                assert_eq!(
                    report.rank,
                    report.islands.iter().map(|i| i.rank).sum::<usize>()
                );
                let pi = std::f64::consts::PI;
                let v = (27000. - 30. * pi * r * r) * t;
                let ix = 180. * 150_f64.powi(3) / 12.
                    - 30. * pi * r.powi(4) / 4.
                    - 60. * pi * r * r * pitch * pitch;
                let iy = 150. * 180_f64.powi(3) / 12.
                    - 30. * pi * r.powi(4) / 4.
                    - 87.5 * pi * r * r * pitch * pitch;
                let p = build.outputs["plate"]
                    .result
                    .model
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(p.volume, v);
                for (a, b) in p.centroid.unwrap().into_iter().zip([90., 75., t / 2.]) {
                    near(a, b)
                }
                let expected = [
                    [t * ix + v * t * t / 12., 0., 0.],
                    [0., t * iy + v * t * t / 12., 0.],
                    [0., 0., t * (ix + iy)],
                ];
                for (a, b) in p
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(expected.into_iter().flatten())
                {
                    assert!((a - b).abs() <= 2e-4 * v, "{a} != {b}")
                }
                let asm = &build.assemblies["modular_bore_plate"];
                assert_eq!(asm.instances.len(), 33);
                assert_eq!(asm.parts.len(), 3);
                let guides = 30. * pi * (2. * r - 1.) * (t + 4.);
                near(asm.material_volume(), v + 34560. + guides);
                let bounds = asm.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., 0.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([180., 150., 16. + t]) {
                    near(a, b)
                }
                let pm = v * 2700e-9;
                let fm = 34560. * 2700e-9;
                let gm = guides * 7800e-9;
                let mass = pm + fm + gm;
                let z = (pm * (12. + t / 2.) + fm * 6. + gm * (12. + (t + 4.) / 2.)) / mass;
                let p = asm
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(p.mass_kg.unwrap(), mass);
                for (a, b) in p.center_of_mass.unwrap().into_iter().zip([90., 75., z]) {
                    near(a, b)
                }
            }
        }
    }
}
#[test]
fn spacing_edit_changes_one_shared_plate_and_thirty_placements() {
    let mut engine = cad_build::BuildEngine::default();
    let a = engine.build(&recipe(25., 4., 8.)).unwrap();
    let warm = engine.build(&recipe(25., 4., 8.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    assert_eq!(a.sketch_reports, warm.sketch_reports);
    let b = engine.build(&recipe(28., 4., 8.)).unwrap();
    assert_eq!(b.stats.computed_solids, 1);
    near(
        a.assemblies["modular_bore_plate"].material_volume(),
        b.assemblies["modular_bore_plate"].material_volume(),
    );
    let c = engine.build(&recipe(28., 5., 8.)).unwrap();
    assert_eq!(c.stats.computed_solids, 2);
    for i in 0..30 {
        let name = format!("guide_{i}");
        let get = |b: &cad_build::BuildResult| {
            b.assemblies["modular_bore_plate"]
                .instances
                .iter()
                .find(|p| p.id == name)
                .unwrap()
                .transform
        };
        assert_ne!(get(&a), get(&b));
    }
}
