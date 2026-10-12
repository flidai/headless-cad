use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/constrained_angle_fixture.star");
fn recipe(span: f64, angle: f64, top: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "constrained_angle_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("span".into(), span),
            ("tilt_degrees".into(), angle),
            ("deck_top_width".into(), top),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-6 + 2e-8 * b.abs(), "{a} != {b}")
}
fn rotate(p: [f64; 3], c: f64, s: f64) -> [f64; 3] {
    [c * p[0] - s * p[2], p[1], s * p[0] + c * p[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn moments(model: &cad_kernel::ModelSnapshot, v: f64, center: [f64; 3], tensor: [[f64; 3]; 3]) {
    let actual = model
        .mass_properties(cad_kernel::Tolerance::default())
        .unwrap();
    near(actual.volume, v);
    for (a, b) in actual.centroid.unwrap().into_iter().zip(center) {
        near(a, b)
    }
    // Bounds propagated from dimensional/angular solve accuracy into moments.
    for (a, b) in actual
        .central_inertia
        .unwrap()
        .into_iter()
        .flatten()
        .zip(tensor.into_iter().flatten())
    {
        assert!((a - b).abs() < 2e-4 * v, "{a} != {b}");
    }
}
#[test]
fn independent_trigonometry_and_trapezoid_integrals_check_twenty_seven_designs() {
    for span in [50_f64, 60., 80.] {
        for angle in [15_f64, 30., 45.] {
            for top in [18_f64, 24., 30.] {
                let radians = angle.to_radians();
                let (c, s) = (radians.cos(), radians.sin());
                let height = span * radians.tan();
                let k = (36. - top) / 2.;
                let h = (44_f64.powi(2) - k * k).sqrt();
                let area = (36. + top) * h / 2.;
                let cy = h * (36. + 2. * top) / (3. * (36. + top));
                let built = cad_build::build(&recipe(span, angle, top)).unwrap();
                for report in built.sketch_reports.values() {
                    assert_eq!(report.schema_version, 2);
                    assert!(report.is_fully_constrained());
                    assert!(report.max_residual_mm <= report.tolerance_mm);
                    assert!(
                        report.max_angular_residual_radians <= report.angular_tolerance_radians
                    );
                    for residual in &report.constraint_residuals {
                        assert_ne!(
                            residual.residual_mm.is_some(),
                            residual.residual_radians.is_some()
                        )
                    }
                }
                for (a, b) in built.sketch_reports["support_sketch"].points["c"]
                    .into_iter()
                    .zip([span, height])
                {
                    near(a, b)
                }
                for (a, b) in built.sketch_reports["deck_sketch"].points["d"]
                    .into_iter()
                    .zip([k, h])
                {
                    near(a, b)
                }
                let v = 30. * span * height;
                let vx = span * span / 18.;
                let vy = height * height / 18.;
                let xz = -v * span * height / 36.;
                moments(
                    &built.outputs["support"].result.model,
                    v,
                    [2. * span / 3., 30., height / 3.],
                    [
                        [v * (vy + 300.), 0., xz],
                        [0., v * (vx + vy), 0.],
                        [xz, 0., v * (vx + 300.)],
                    ],
                );
                // Independent integration across horizontal trapezoid strips.
                let xx = h * (36. + top) * (36_f64.powi(2) + top * top) / 48.;
                let yy = h.powi(3) * (36. + 3. * top) / 12. - area * cy * cy;
                let dv = area * 4.;
                moments(
                    &built.outputs["deck"].result.model,
                    dv,
                    [18., cy, 2.],
                    [
                        [4. * yy + dv * 16. / 12., 0., 0.],
                        [0., 4. * xx + dv * 16. / 12., 0.],
                        [0., 0., 4. * (xx + yy)],
                    ],
                );
                let assembly = &built.assemblies["constrained_angle_fixture"];
                assert_eq!(assembly.instances.len(), 9);
                assert_eq!(assembly.parts.len(), 5);
                let seat = [10. + span / 2., 40., 6. + height / 2.];
                let origin = add(seat, rotate([-18., -cy, 0.], c, s));
                let deck = assembly.instances.iter().find(|i| i.id == "deck").unwrap();
                for (a, b) in deck.transform.translation.into_iter().zip(origin) {
                    near(a, b)
                }
                let mut vertices = vec![
                    [0., 0., 0.],
                    [span + 20., 80., 6.],
                    [10. + span, 10., height + 6.],
                ];
                for x in [0., 36.] {
                    for y in [0., h] {
                        for z in [0., 4.] {
                            vertices.push(add(origin, rotate([x, y, z], c, s)))
                        }
                    }
                }
                let mut pad_centers = vec![];
                for (name, x, n) in [
                    ("left_pad", k / 2., [-h / 44., k / 44., 0.]),
                    ("right_pad", 36. - k / 2., [h / 44., k / 44., 0.]),
                ] {
                    let center = [x, h / 2., 2.];
                    let tangent = [n[1], -n[0], 0.];
                    let local_origin = add(center, [-4. * tangent[0], -4. * tangent[1], -1.]);
                    let actual = &assembly
                        .instances
                        .iter()
                        .find(|i| i.id == name)
                        .unwrap()
                        .transform;
                    for (a, b) in actual
                        .translation
                        .into_iter()
                        .zip(add(origin, rotate(local_origin, c, s)))
                    {
                        near(a, b)
                    }
                    for px in [0., 2.] {
                        for py in [0., 8.] {
                            for pz in [0., 5.] {
                                let p = add(
                                    local_origin,
                                    [py * tangent[0] + pz * n[0], py * tangent[1] + pz * n[1], px],
                                );
                                vertices.push(add(origin, rotate(p, c, s)));
                            }
                        }
                    }
                    pad_centers.push(add(
                        origin,
                        rotate(add(center, [2.5 * n[0], 2.5 * n[1], 0.]), c, s),
                    ));
                }
                let bounds = assembly.bounds().unwrap();
                for i in 0..3 {
                    near(
                        bounds.min[i],
                        vertices.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min),
                    );
                    near(
                        bounds.max[i],
                        vertices
                            .iter()
                            .map(|p| p[i])
                            .fold(f64::NEG_INFINITY, f64::max),
                    )
                }
                let bv = 480. * span + 8736.;
                let bolts = 342. * std::f64::consts::PI;
                near(assembly.material_volume(), bv + v + dv + 160. + bolts);
                let mut pieces = vec![
                    (bv * 2700e-9, [(span + 20.) / 2., 40., 3.]),
                    (v * 2700e-9, [10. + 2. * span / 3., 40., 6. + height / 3.]),
                    (dv * 7800e-9, add(seat, rotate([0., 0., 2.], c, s))),
                    (bolts * 7800e-9, [(span + 20.) / 2., 40., 472.5 / 85.5]),
                ];
                pieces.extend(pad_centers.into_iter().map(|p| (80. * 7800e-9, p)));
                let mass: f64 = pieces.iter().map(|p| p.0).sum();
                let props = assembly
                    .mass_properties(cad_kernel::Tolerance::default())
                    .unwrap();
                near(props.mass_kg.unwrap(), mass);
                for (i, value) in props.center_of_mass.unwrap().into_iter().enumerate() {
                    near(
                        value,
                        pieces.iter().map(|p| p.0 * p.1[i]).sum::<f64>() / mass,
                    )
                }
            }
        }
    }
}
#[test]
fn angle_and_taper_edits_rebuild_only_their_geometry_and_update_attachments() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(60., 30., 24.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(60., 30., 24.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let tilted = engine.build(&recipe(60., 35., 24.)).unwrap();
    assert_eq!(tilted.stats.computed_solids, 2);
    assert_eq!(
        original.feature_hashes["deck_body"],
        tilted.feature_hashes["deck_body"]
    );
    let tapered = engine.build(&recipe(60., 35., 28.)).unwrap();
    assert_eq!(tapered.stats.computed_solids, 1);
    assert_eq!(
        tilted.feature_hashes["support_body"],
        tapered.feature_hashes["support_body"]
    );
    let pad = |b: &cad_build::BuildResult| {
        b.assemblies["constrained_angle_fixture"]
            .instances
            .iter()
            .find(|i| i.id == "left_pad")
            .unwrap()
            .transform
    };
    assert_ne!(pad(&original), pad(&tilted));
    assert_ne!(pad(&tilted), pad(&tapered));
}
