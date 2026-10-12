use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/rounded_clevis.star");
fn recipe(length: f64, gap: f64, radius: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "rounded_clevis.star",
        SOURCE,
        &BTreeMap::from([
            ("link_length".into(), length),
            ("clevis_gap".into(), gap),
            ("corner_radius".into(), radius),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8 * b.abs().max(1.), "{a} != {b}")
}
fn link_oracle(length: f64, height: f64) -> (f64, [f64; 3], [[f64; 3]; 3]) {
    let pi = std::f64::consts::PI;
    let r = 8_f64;
    let area = 2. * r * length + pi * r * r;
    let xx = r * length.powi(3) / 6.
        + pi * r * r * length * length / 4.
        + 4. * length * r.powi(3) / 3.
        + pi * r.powi(4) / 4.;
    let yy = 2. * length * r.powi(3) / 3. + pi * r.powi(4) / 4.;
    let v = area * height;
    let mut pieces = vec![(
        v,
        [length / 2., 0., height / 2.],
        [
            yy * height + v * height * height / 12.,
            xx * height + v * height * height / 12.,
            (xx + yy) * height,
        ],
    )];
    for (x, r) in [(0., 5.5_f64), (length, 4_f64)] {
        let v = -pi * r * r * height;
        pieces.push((
            v,
            [x, 0., height / 2.],
            [
                v * (3. * r * r + height * height) / 12.,
                v * (3. * r * r + height * height) / 12.,
                v * r * r / 2.,
            ],
        ));
    }
    let volume = pieces.iter().map(|p| p.0).sum::<f64>();
    let center = std::array::from_fn(|i| pieces.iter().map(|p| p.0 * p.1[i]).sum::<f64>() / volume);
    let inertia = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            pieces
                .iter()
                .map(|(v, c, diag)| {
                    let d: [f64; 3] = std::array::from_fn(|k| c[k] - center[k]);
                    if i == j {
                        diag[i] + v * (d[(i + 1) % 3].powi(2) + d[(i + 2) % 3].powi(2))
                    } else {
                        -v * d[i] * d[j]
                    }
                })
                .sum()
        })
    });
    (volume, center, inertia)
}
#[test]
fn rounded_clevis_native_shapes_and_placements_match_independent_expectations() {
    let mut engine = cad_build::BuildEngine::default();
    let pi = std::f64::consts::PI;
    for length in [32., 45., 64.] {
        for gap in [12., 16., 24.] {
            for radius in [4., 8., 10.] {
                let built = engine.build(&recipe(length, gap, radius)).unwrap();
                let link = &built.outputs["link"].result.model;
                let actual = link
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                let (v, c, tensor) = link_oracle(length, gap - 6.);
                near(actual.volume, v);
                for (a, b) in actual.centroid.unwrap().into_iter().zip(c) {
                    near(a, b)
                }
                for (a, b) in actual
                    .central_inertia
                    .unwrap()
                    .into_iter()
                    .flatten()
                    .zip(tensor.into_iter().flatten())
                {
                    near(a, b)
                }
                assert_eq!(link.faces.len(), 8);
                assert_eq!(
                    link.edges
                        .iter()
                        .filter(|e| matches!(e.curve, cad_kernel::Curve::CircularArc(_)))
                        .count(),
                    4
                );
                let frame = link
                    .face_frame(
                        cad_kernel::FaceId(1),
                        [1., 0., 0.],
                        [0.; 3],
                        cad_kernel::Tolerance::default(),
                    )
                    .unwrap();
                for (a, b) in frame.origin.into_iter().zip([c[0], 0., gap - 6.]) {
                    near(a, b)
                }
                let base = (80. * 60. - (4. - pi) * radius * radius - 36. * pi) * 6.;
                let cheek = (24. * 28. + 41.75 * pi) * 6.;
                let pin = 25. * pi * (gap + 16.);
                let mass = (base + 2. * cheek) * 2700e-9 + (v + pin) * 7800e-9;
                let assembly = &built.assemblies["rounded_clevis"];
                assert_eq!(assembly.instances.len(), 5);
                assert_eq!(assembly.parts.len(), 4);
                near(assembly.material_volume(), base + 2. * cheek + v + pin);
                let native = assembly
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(native.mass_kg.unwrap(), mass);
                let cheek_z = 6. + (10560. + 1169. * pi) / (672. + 41.75 * pi);
                let expected_center = [
                    40. + v * 7800e-9 * c[0] / mass,
                    30.,
                    (base * 2700e-9 * 3.
                        + 2. * cheek * 2700e-9 * cheek_z
                        + (v + pin) * 7800e-9 * 34.)
                        / mass,
                ];
                for (a, b) in native
                    .center_of_mass
                    .unwrap()
                    .into_iter()
                    .zip(expected_center)
                {
                    near(a, b)
                }
                let bounds = assembly.bounds().unwrap();
                for (a, b) in
                    bounds
                        .min
                        .into_iter()
                        .zip([0., 0., 0.])
                        .chain(
                            bounds
                                .max
                                .into_iter()
                                .zip([(48. + length).max(80.), 60., 46.]),
                        )
                {
                    near(a, b)
                }
                for (id, point) in [
                    ("link", [40., 27. + gap / 2., 34.]),
                    ("pin", [40., 38. + gap / 2., 34.]),
                ] {
                    let instance = assembly.instances.iter().find(|i| i.id == id).unwrap();
                    for (a, b) in instance.transform.point([0.; 3]).into_iter().zip(point) {
                        near(a, b)
                    }
                }
            }
        }
    }
}
#[test]
fn rounded_profile_edits_rebuild_only_affected_native_definitions() {
    let mut engine = cad_build::BuildEngine::default();
    assert_eq!(
        engine
            .build(&recipe(45., 16., 8.))
            .unwrap()
            .stats
            .computed_solids,
        4
    );
    assert_eq!(
        engine
            .build(&recipe(46., 16., 8.))
            .unwrap()
            .stats
            .computed_solids,
        1
    );
    assert_eq!(
        engine
            .build(&recipe(46., 18., 8.))
            .unwrap()
            .stats
            .computed_solids,
        2
    );
    assert_eq!(
        engine
            .build(&recipe(46., 18., 9.))
            .unwrap()
            .stats
            .computed_solids,
        1
    );
    let selected = engine
        .build_outputs(&recipe(46., 18., 9.), &["link".into()])
        .unwrap();
    assert_eq!(selected.outputs.len(), 1);
    assert_eq!(selected.property_assertions.len(), 3);
    assert!(selected
        .property_assertions
        .values()
        .all(|c| c.status == cad_build::PropertyAssertionStatus::Passed));
    for (length, gap, radius) in [
        (31., 16., 8.),
        (65., 16., 8.),
        (45., 11., 8.),
        (45., 25., 8.),
        (45., 16., 3.),
        (45., 16., 11.),
    ] {
        assert!(cad_recipe::from_starlark_with_params(
            "rounded_clevis.star",
            SOURCE,
            &BTreeMap::from([
                ("link_length".into(), length),
                ("clevis_gap".into(), gap),
                ("corner_radius".into(), radius)
            ])
        )
        .is_err());
    }
}
