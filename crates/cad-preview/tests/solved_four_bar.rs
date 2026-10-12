use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/solved_four_bar.star");
fn recipe(angle: f64, length: f64, pose: usize) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "solved_four_bar.star",
        SOURCE,
        &BTreeMap::from([
            ("angle".into(), angle),
            ("coupler_length".into(), length),
            ("pose".into(), pose as f64),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}")
}
fn world(p: [f64; 3], pose: usize) -> [f64; 3] {
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let r = match pose {
        0 => p,
        1 => [p[0], -p[2], p[1]],
        _ => [
            (p[0] + p[1]) / 2. + q * p[2],
            (p[0] + p[1]) / 2. - q * p[2],
            q * (p[1] - p[0]),
        ],
    };
    [r[0] + 10., r[1] - 5., r[2] + 3.]
}
// Independent circle-circle intersection, never calling the mechanism solver.
fn pins(angle: f64, length: f64) -> [[f64; 2]; 4] {
    let b = [30. * angle.cos(), 30. * angle.sin()];
    let delta = [80. - b[0], -b[1]];
    let d = delta[0].hypot(delta[1]);
    let u = [delta[0] / d, delta[1] / d];
    let along = (length * length - 2500. + d * d) / (2. * d);
    let h = (length * length - along * along).sqrt();
    [
        [0., 0.],
        b,
        [
            b[0] + along * u[0] - h * u[1],
            b[1] + along * u[1] + h * u[0],
        ],
        [80., 0.],
    ]
}
#[test]
fn closed_linkage_matches_independent_closure_mass_sections_and_reuses_geometry() {
    let mut engine = cad_build::BuildEngine::default();
    for (li, length) in [65., 70., 75.].into_iter().enumerate() {
        for (ai, angle) in [0.5, 0.9, 1.3].into_iter().enumerate() {
            for pose in 0..3 {
                let build = engine.build(&recipe(angle, length, pose)).unwrap();
                if ai != 0 || pose != 0 {
                    assert_eq!(build.stats.computed_solids, 0)
                } else if li > 0 {
                    assert_eq!(build.stats.computed_solids, 1)
                }
                let a = &build.assemblies["solved_four_bar"];
                assert_eq!(a.parts.len(), 5);
                assert_eq!(a.instances.len(), 8);
                let p = pins(angle, length);
                let specs = [
                    (vec!["ground"], 0, 3, 80., 0., 6.),
                    (vec!["crank_unit", "link"], 0, 1, 30., 8., 4.),
                    (vec!["coupler"], 1, 2, length, 14., 4.),
                    (vec!["rocker"], 3, 2, 50., 8., 4.),
                ];
                let mut total_mass = 0.;
                let mut moment = [0.; 3];
                for (path, from, to, l, z, height) in specs {
                    let i = a
                        .instances
                        .iter()
                        .find(|i| a.instance_paths[&i.id] == path)
                        .unwrap();
                    for (x, pin) in [(0., from), (l, to)] {
                        let expected = world([p[pin][0], p[pin][1], z], pose);
                        for (actual, expected) in
                            i.transform.point([x, 0., 0.]).into_iter().zip(expected)
                        {
                            near(actual, expected)
                        }
                    }
                    let mass = (12. * l + 28. * std::f64::consts::PI) * height * 2700. / 1e9;
                    let center = world(
                        [
                            (p[from][0] + p[to][0]) / 2.,
                            (p[from][1] + p[to][1]) / 2.,
                            z + height / 2.,
                        ],
                        pose,
                    );
                    total_mass += mass;
                    for axis in 0..3 {
                        moment[axis] += mass * center[axis]
                    }
                }
                let pin_mass = std::f64::consts::PI * 1.8 * 1.8 * 20. * 7800. / 1e9;
                for pin in p {
                    let center = world([pin[0], pin[1], 10.], pose);
                    total_mass += pin_mass;
                    for axis in 0..3 {
                        moment[axis] += pin_mass * center[axis]
                    }
                }
                let properties = a.mass_properties(cad_kernel::Tolerance::default()).unwrap();
                near(properties.mass_kg.unwrap(), total_mass);
                for (actual, expected) in properties
                    .center_of_mass
                    .unwrap()
                    .into_iter()
                    .zip(moment.map(|v| v / total_mass))
                {
                    near(actual, expected)
                }
                near(
                    build.section_views["moving_coupler"].query.section.area,
                    12. * length + (28. + 4. * 1.8 * 1.8) * std::f64::consts::PI,
                );
                assert_eq!(build.mechanism_reports.len(), 1);
                assert_eq!(build.mechanism_bindings.len(), 4);
            }
        }
    }
}
