use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/constrained_slope_fixture.star");
fn recipe(span: f64, left: f64, right: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "constrained_slope_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("span".into(), span),
            ("left_length".into(), left),
            ("right_length".into(), right),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6 + 1e-8 * b.abs(), "{a} != {b}")
}
#[test]
fn dimensions_solve_a_triangle_and_reposition_the_attached_deck() {
    let tolerance = cad_kernel::Tolerance::default();
    for span in [70_f64, 80., 100.] {
        for left in [55., 60., 80.] {
            for right in [60., 70., 85.] {
                // Independent circle-circle intersection, positive orientation branch.
                let x = (left * left + span * span - right * right) / (2. * span);
                let y = (left * left - x * x).sqrt();
                let built = cad_build::build(&recipe(span, left, right)).unwrap();
                let solve = &built.sketch_reports["support_sketch"];
                assert!(solve.is_fully_constrained());
                assert_eq!(solve.degrees_of_freedom, 0);
                near(solve.points["c"][0], x);
                near(solve.points["c"][1], y);
                let support = &built.outputs["support"].result.model;
                let actual = support.mass_properties(tolerance).unwrap();
                let v = 20. * span * y;
                near(actual.volume, v);
                for (a, b) in
                    actual
                        .centroid
                        .unwrap()
                        .into_iter()
                        .zip([(span + x) / 3., 20., y / 3.])
                {
                    near(a, b)
                }
                let var_x = (span * span + x * x + span * x) / 6. - ((span + x) / 3.).powi(2);
                let var_y = y * y / 18.;
                let cov_xy = y * (span + 2. * x) / 12. - (span + x) * y / 9.;
                let expected = [
                    [v * (var_y + 1600. / 12.), 0., -v * cov_xy],
                    [0., v * (var_x + var_y), 0.],
                    [-v * cov_xy, 0., v * (var_x + 1600. / 12.)],
                ];
                // Moment sensitivity scales with volume × length × coordinate error.
                // The sketch is a numerical solve, so nominally zero products need an
                // absolute allowance as well as relative accuracy on diagonal entries.
                let moment_allowance = 20. * tolerance.linear * v * span.max(40.);
                for (a, b) in actual
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(expected.into_iter().flatten())
                {
                    assert!(
                        (a - b).abs() < moment_allowance,
                        "tensor {a} != {b}, allowance {moment_allowance}"
                    )
                }
                let assembly = &built.assemblies["constrained_slope_fixture"];
                assert_eq!(assembly.instances.len(), 9);
                assert_eq!(assembly.parts.len(), 5);
                let base_v = 360. * span + 6336.;
                let bolt_v = 85.5 * std::f64::consts::PI;
                near(assembly.material_volume(), base_v + v + 4176. + 4. * bolt_v);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., 0.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([span + 20., 60., y + 6.]) {
                    near(a, b)
                }
                let n = [y / right, 0., (span - x) / right];
                let axis = [n[2], 0., -n[0]];
                let seat = [10. + (span + x) / 2., 30., 6. + y / 2.];
                let deck = assembly.instances.iter().find(|i| i.id == "deck").unwrap();
                for a in 0..3 {
                    near(
                        deck.transform.translation[a],
                        seat[a] - 12. * axis[a] - if a == 1 { 24. } else { 0. },
                    )
                }
                let pieces = [
                    (base_v * 2700e-9, [(span + 20.) / 2., 30., 3.]),
                    (v * 2700e-9, [10. + (span + x) / 3., 30., 6. + y / 3.]),
                    (
                        3456. * 7800e-9,
                        std::array::from_fn(|i| seat[i] + 1.5 * n[i]),
                    ),
                    (
                        720. * 7800e-9,
                        std::array::from_fn(|i| seat[i] + 5.5 * n[i]),
                    ),
                    (
                        4. * bolt_v * 7800e-9,
                        [(span + 20.) / 2., 30., 472.5 / 85.5],
                    ),
                ];
                let mass: f64 = pieces.iter().map(|p| p.0).sum();
                let properties = assembly.mass_properties(tolerance).unwrap();
                near(properties.mass_kg.unwrap(), mass);
                for (axis, value) in properties.center_of_mass.unwrap().into_iter().enumerate() {
                    near(
                        value,
                        pieces.iter().map(|p| p.0 * p.1[axis]).sum::<f64>() / mass,
                    )
                }
            }
        }
    }
}
#[test]
fn dimension_edits_reuse_unaffected_parts_and_resolve_the_seat_again() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(80., 60., 70.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(80., 60., 70.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let edited = engine.build(&recipe(80., 62., 70.)).unwrap();
    assert_eq!(edited.stats.computed_solids, 2); // extrude + baked rigid transform
    assert_ne!(
        original.face_datums["support"]["seat"].frame,
        edited.face_datums["support"]["seat"].frame
    );
    assert_eq!(
        original.feature_hashes["deck_body"],
        edited.feature_hashes["deck_body"]
    );
    assert_eq!(
        original.feature_hashes["base_body"],
        edited.feature_hashes["base_body"]
    );
    assert_ne!(
        original.feature_hashes["support_sketch"],
        edited.feature_hashes["support_sketch"]
    );
}
