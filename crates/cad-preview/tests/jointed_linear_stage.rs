use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/jointed_linear_stage.star");
fn recipe(travel: f64, angle: f64, pose: f64) -> cad_build::Recipe {
    let modules = BTreeMap::from([
        (
            "recipe_modules/stock.star".into(),
            include_str!("../../../examples/recipe_modules/stock.star").into(),
        ),
        (
            "recipe_modules/drive_parts.star".into(),
            include_str!("../../../examples/recipe_modules/drive_parts.star").into(),
        ),
    ]);
    cad_recipe::from_starlark_with_modules(
        "jointed_linear_stage.star",
        SOURCE,
        &BTreeMap::from([
            ("travel".into(), travel),
            ("angle".into(), angle),
            ("pose".into(), pose),
        ]),
        &modules,
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}")
}
fn world(p: [f64; 3], pose: usize) -> [f64; 3] {
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let r = match pose {
        0 => p,
        1 => [-p[1], p[0], p[2]],
        _ => [
            (p[0] + p[1]) / 2. + q * p[2],
            (p[0] + p[1]) / 2. - q * p[2],
            q * (p[1] - p[0]),
        ],
    };
    [r[0] + 7., r[1] - 4., r[2] + 2.]
}
#[test]
fn prescribed_motion_matches_independent_positions_mass_and_semantic_sections() {
    let mut engine = cad_build::BuildEngine::default();
    let mut first = true;
    for travel in [20., 50., 100.] {
        for angle in [
            -std::f64::consts::FRAC_PI_2,
            0.,
            std::f64::consts::FRAC_PI_2,
        ] {
            for pose in 0..3 {
                let build = engine.build(&recipe(travel, angle, pose as f64)).unwrap();
                if !first {
                    assert_eq!(build.stats.computed_solids, 0)
                }
                first = false;
                let a = &build.assemblies["jointed_linear_stage"];
                assert_eq!(a.parts.len(), 4);
                assert_eq!(a.instances.len(), 5);
                let arm = a
                    .instances
                    .iter()
                    .find(|i| a.instance_paths[&i.id] == ["stage", "head", "arm"])
                    .unwrap();
                for (actual, expected) in arm
                    .transform
                    .point([5., 5., 0.])
                    .into_iter()
                    .zip(world([travel + 20., 45., 36.], pose))
                {
                    near(actual, expected)
                }
                for (actual, expected) in arm.transform.point([50., 5., 4.]).into_iter().zip(world(
                    [
                        travel + 20. + 45. * angle.cos(),
                        45. + 45. * angle.sin(),
                        40.,
                    ],
                    pose,
                )) {
                    near(actual, expected)
                }
                let pi = std::f64::consts::PI;
                let rv = 160. * 396.;
                let cv = (3600. - 36. * pi) * 8.;
                let hv = 864. * pi;
                let av = 2000. - 16. * pi;
                let rz = (64000. * 10. - 640. * (20. - 2. / 3.)) / rv;
                let arm_offset = (2000. * 25. - 16. * pi * 5.) / av - 5.;
                let masses = [
                    2. * rv * 7800. / 1e9,
                    cv * 2700. / 1e9,
                    hv * 7800. / 1e9,
                    av * 2700. / 1e9,
                ];
                let centers = [
                    [80., 45., rz],
                    [travel + 20., 45., 24.],
                    [travel + 20., 45., 31.],
                    [
                        travel + 20. + arm_offset * angle.cos(),
                        45. + arm_offset * angle.sin(),
                        38.,
                    ],
                ];
                let mass: f64 = masses.iter().sum();
                let center = std::array::from_fn(|i| {
                    masses
                        .iter()
                        .zip(centers)
                        .map(|(m, p)| m * p[i])
                        .sum::<f64>()
                        / mass
                });
                let properties = a.mass_properties(cad_kernel::Tolerance::default()).unwrap();
                near(properties.mass_kg.unwrap(), mass);
                for (actual, expected) in properties
                    .center_of_mass
                    .unwrap()
                    .into_iter()
                    .zip(world(center, pose))
                {
                    near(actual, expected)
                }
                near(
                    build.section_views["moving_carriage"].query.section.area,
                    3600. - 36. * pi,
                );
                let reports = a.joint_reports(cad_kernel::Tolerance::default()).unwrap();
                assert_eq!(reports.len(), 2);
                for report in reports {
                    assert_eq!(report.intrinsic_dof, 1);
                    assert_eq!(report.remaining_dof, 0);
                    assert!(report.residual_translation_norm_mm < 1e-7);
                    assert!(report.residual_angle_rad < 1e-7);
                }
            }
        }
    }
}
