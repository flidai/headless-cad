//! Independent circular-segment integrals and material accounting for curved Booleans.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/twin_lobe_fixture.star");
fn recipe(r: f64, d: f64, h: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "twin_lobe_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("left_radius".into(), r),
            ("spacing".into(), d),
            ("depth".into(), h),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 2e-6 + 2e-8 * b.abs(), "{a} != {b}");
}
// [area, first x moment, second x moment, second y moment].
fn cap(r: f64, t: f64) -> [f64; 4] {
    let s = (r * r - t * t).sqrt();
    let a = (t / r).acos();
    [
        r * r * a - t * s,
        2. * s.powi(3) / 3.,
        r.powi(4) * a / 4. - t * s * (2. * t * t - r * r) / 4.,
        r.powi(4) * a / 4. - t * s * (5. * r * r - 2. * t * t) / 12.,
    ]
}
fn disk(r: f64, x: f64) -> [f64; 4] {
    let a = std::f64::consts::PI * r * r;
    let j = a * r * r / 4.;
    [a, a * x, j + a * x * x, j]
}
fn regions(r: f64, d: f64) -> [[f64; 4]; 4] {
    let chord = (d * d + r * r - 324.) / (2. * d);
    let a = cap(r, chord);
    let b = cap(18., d - chord);
    let lens = [
        a[0] + b[0],
        a[1] + d * b[0] - b[1],
        a[2] + d * d * b[0] - 2. * d * b[1] + b[2],
        a[3] + b[3],
    ];
    let left = disk(r, 0.);
    let right = disk(18., d);
    [
        std::array::from_fn(|i| left[i] - lens[i]),
        lens,
        std::array::from_fn(|i| right[i] - lens[i]),
        std::array::from_fn(|i| left[i] + right[i] - lens[i]),
    ]
}
#[derive(Clone, Copy)]
struct Properties {
    volume: f64,
    center: [f64; 3],
    inertia: [f64; 3],
}
fn prism(p: [f64; 4], h: f64, z: f64) -> Properties {
    let volume = p[0] * h;
    let xx = p[2] - p[1] * p[1] / p[0];
    Properties {
        volume,
        center: [p[1] / p[0], 0., z + h / 2.],
        inertia: [
            h * p[3] + volume * h * h / 12.,
            h * xx + volume * h * h / 12.,
            h * (xx + p[3]),
        ],
    }
}
fn tensor(actual: [[f64; 3]; 3], expected: [[f64; 3]; 3]) {
    for (a, b) in actual
        .into_iter()
        .flatten()
        .zip(expected.into_iter().flatten())
    {
        near(a, b)
    }
}
fn aggregate(parts: &[(Properties, f64)]) -> (f64, [f64; 3], [[f64; 3]; 3]) {
    let total = parts.iter().map(|(p, w)| p.volume * w).sum::<f64>();
    let center = std::array::from_fn(|i| {
        parts
            .iter()
            .map(|(p, w)| p.volume * w * p.center[i])
            .sum::<f64>()
            / total
    });
    let inertia = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            parts
                .iter()
                .map(|(p, w)| {
                    let delta: [f64; 3] = std::array::from_fn(|k| p.center[k] - center[k]);
                    let shift = if i == j {
                        delta
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| *k != i)
                            .map(|(_, x)| x * x)
                            .sum::<f64>()
                    } else {
                        -delta[i] * delta[j]
                    };
                    w * ((if i == j { p.inertia[i] } else { 0. }) + p.volume * shift)
                })
                .sum::<f64>()
        })
    });
    (total, center, inertia)
}
#[test]
fn curved_boolean_partition_matches_independent_moments_and_materials() {
    for r in [20., 22., 24.] {
        for d in [26., 30., 34.] {
            for h in [6., 8., 10.] {
                let build = cad_build::build(&recipe(r, d, h)).unwrap();
                let areas = regions(r, d);
                let tol = cad_kernel::Tolerance::default();
                let base = prism(areas[3], 4., 0.);
                let left = prism(areas[0], h, 4.);
                let lens = prism(areas[1], h, 4.);
                let right = prism(areas[2], h, 4.);
                for (name, region, height) in [
                    ("left_lobe", areas[0], h),
                    ("overlap", areas[1], h),
                    ("union_base", areas[3], 4.),
                ] {
                    let model = &build.outputs[name].result.model;
                    assert!(model.validate(tol).valid);
                    let expected = prism(region, height, 0.);
                    let p = model.mass_properties(tol).unwrap();
                    near(p.volume, expected.volume);
                    for (a, b) in p.centroid.unwrap().into_iter().zip(expected.center) {
                        near(a, b)
                    }
                    tensor(
                        p.central_inertia.unwrap(),
                        std::array::from_fn(|i| {
                            std::array::from_fn(|j| if i == j { expected.inertia[i] } else { 0. })
                        }),
                    );
                }
                let assembly = &build.assemblies["twin_lobe_fixture"];
                assert_eq!(assembly.instances.len(), 6);
                assert_eq!(assembly.parts.len(), 5);
                let vb = 48. * std::f64::consts::PI;
                let boss = Properties {
                    volume: vb,
                    center: [0., 0., h + 6.],
                    inertia: [vb * 76. / 12., vb * 76. / 12., vb * 10.],
                };
                let boss2 = Properties {
                    center: [d, 0., h + 6.],
                    ..boss
                };
                let parts = [
                    (base, 2700e-9),
                    (left, 2700e-9),
                    (lens, 7800e-9),
                    (right, 2700e-9),
                    (boss, 7800e-9),
                    (boss2, 7800e-9),
                ];
                let (mass, com, mi) = aggregate(&parts);
                let unweighted: Vec<_> = parts.iter().map(|(p, _)| (*p, 1.)).collect();
                let (volume, centroid, vi) = aggregate(&unweighted);
                let actual = assembly.mass_properties(tol).unwrap();
                near(actual.volume, volume);
                near(actual.mass_kg.unwrap(), mass);
                for (a, b) in actual.center_of_mass.unwrap().into_iter().zip(com) {
                    near(a, b)
                }
                for (a, b) in actual.volume_centroid.unwrap().into_iter().zip(centroid) {
                    near(a, b)
                }
                tensor(actual.central_volume_inertia.unwrap(), vi);
                tensor(actual.central_mass_inertia.unwrap(), mi);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([-r, -r, 0.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([d + 18., r, h + 8.]) {
                    near(a, b)
                }
                near(left.volume + lens.volume + right.volume, areas[3][0] * h);
            }
        }
    }
}
#[test]
fn shared_boolean_definitions_invalidate_only_affected_features() {
    let mut engine = cad_build::BuildEngine::default();
    let a = engine.build(&recipe(22., 30., 8.)).unwrap();
    let warm = engine.build(&recipe(22., 30., 8.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    let b = engine.build(&recipe(22., 34., 8.)).unwrap();
    assert_eq!(b.stats.computed_solids, 6);
    let c = engine.build(&recipe(24., 34., 8.)).unwrap();
    assert_eq!(c.stats.computed_solids, 6);
    let e = engine.build(&recipe(24., 34., 10.)).unwrap();
    assert_eq!(e.stats.computed_solids, 6);
    for build in [&b, &c, &e] {
        assert_eq!(
            a.assemblies["twin_lobe_fixture"].parts["boss"].content_hash,
            build.assemblies["twin_lobe_fixture"].parts["boss"].content_hash
        );
    }
}
