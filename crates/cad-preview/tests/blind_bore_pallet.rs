//! Independent signed box/cylinder integrals for a mixed machining sequence.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/blind_bore_pallet.star");
const TOP: [[f64; 2]; 6] = [
    [20., 20.],
    [60., 20.],
    [100., 20.],
    [20., 60.],
    [60., 60.],
    [100., 60.],
];
const BOTTOM: [[f64; 2]; 2] = [[40., 40.], [80., 40.]];
const THROUGH: [[f64; 2]; 4] = [[8., 8.], [112., 8.], [8., 72.], [112., 72.]];
fn recipe(r: f64, h: f64, d: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "blind_bore_pallet.star",
        SOURCE,
        &BTreeMap::from([
            ("socket_radius".into(), r),
            ("socket_depth".into(), h),
            ("plate_height".into(), d),
        ]),
    )
    .unwrap()
}
#[derive(Clone)]
struct Integral {
    volume: f64,
    center: [f64; 3],
    variance: [f64; 3],
    density: f64,
}
fn cylinder(r: f64, h: f64, center: [f64; 3], sign: f64, density: f64) -> Integral {
    Integral {
        volume: sign * std::f64::consts::PI * r * r * h,
        center,
        variance: [r * r / 4., r * r / 4., h * h / 12.],
        density,
    }
}
fn stock_and_removals(r: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut items = vec![Integral {
        volume: 9600. * d,
        center: [60., 40., d / 2.],
        variance: [1200., 6400. / 12., d * d / 12.],
        density: 2700e-9,
    }];
    for [x, y] in TOP {
        items.push(cylinder(r, h, [x, y, d - h / 2.], -1., 2700e-9));
    }
    for [x, y] in BOTTOM {
        items.push(cylinder(4., 6., [x, y, 3.], -1., 2700e-9));
    }
    for [x, y] in THROUGH {
        items.push(cylinder(2.5, d, [x, y, d / 2.], -1., 2700e-9));
    }
    items
}
fn assembly_integrals(r: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut items = stock_and_removals(r, h, d);
    for [x, y] in TOP {
        items.push(cylinder(
            r - 0.5,
            h + 10.,
            [x, y, d - h + (h + 10.) / 2.],
            1.,
            7800e-9,
        ));
        items.push(cylinder(r + 2., 4., [x, y, d + 12.], 1., 7800e-9));
    }
    for [x, y] in BOTTOM {
        items.push(cylinder(3.5, 16., [x, y, -2.], 1., 7800e-9));
    }
    for [x, y] in THROUGH {
        items.push(cylinder(2.5, d + 2., [x, y, d / 2.], 1., 7800e-9));
        items.push(cylinder(1.5, d + 2., [x, y, d / 2.], -1., 7800e-9));
    }
    let shifted: Vec<_> = items
        .iter()
        .map(|i| {
            let mut i = i.clone();
            i.center[0] += 150.;
            i
        })
        .collect();
    items.extend(shifted);
    items
}
fn expected(items: &[Integral], mass: bool) -> (f64, [f64; 3], [[f64; 3]; 3]) {
    let weight = |i: &Integral| i.volume * if mass { i.density } else { 1. };
    let total = items.iter().map(weight).sum::<f64>();
    let center =
        std::array::from_fn(|j| items.iter().map(|i| weight(i) * i.center[j]).sum::<f64>() / total);
    let mut tensor = [[0.; 3]; 3];
    for i in items {
        let q: [f64; 3] = std::array::from_fn(|j| i.center[j] - center[j]);
        for a in 0..3 {
            for b in 0..3 {
                tensor[a][b] += weight(i)
                    * (if a == b {
                        i.variance.iter().sum::<f64>() - i.variance[a]
                            + q.iter().map(|x| x * x).sum::<f64>()
                    } else {
                        0.
                    } - q[a] * q[b]);
            }
        }
    }
    (total, center, tensor)
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6 + 1e-8 * b.abs(), "{a} != {b}");
}
fn compare(
    volume: f64,
    centroid: [f64; 3],
    tensor: [[f64; 3]; 3],
    expected: (f64, [f64; 3], [[f64; 3]; 3]),
) {
    near(volume, expected.0);
    for (a, b) in centroid.into_iter().zip(expected.1) {
        near(a, b);
    }
    for (actual, expected) in tensor.into_iter().zip(expected.2) {
        for (a, b) in actual.into_iter().zip(expected) {
            near(a, b);
        }
    }
}
#[test]
fn mixed_blind_and_through_machining_matches_twenty_seven_independent_integrals() {
    let t = cad_kernel::Tolerance::default();
    for r in [6., 8., 10.] {
        for h in [8., 12., 16.] {
            for d in [24., 28., 32.] {
                let build = cad_build::build(&recipe(r, h, d)).unwrap();
                let model = &build.outputs["pallet"].result.model;
                assert!(model.validate(t).valid);
                let p = model.mass_properties(t).unwrap();
                compare(
                    p.volume,
                    p.centroid.unwrap(),
                    p.central_inertia.unwrap(),
                    expected(&stock_and_removals(r, h, d), false),
                );
                assert_eq!(
                    model
                        .faces
                        .iter()
                        .filter(
                            |f| matches!(&f.surface,cad_kernel::Surface::Cylinder(c) if c.inward)
                        )
                        .count(),
                    12
                );
                let a = &build.assemblies["blind_bore_pallet"];
                assert_eq!(a.instances.len(), 26);
                assert_eq!(a.parts.len(), 4);
                let bounds = a.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., -10.]) {
                    near(a, b);
                }
                for (a, b) in bounds.max.into_iter().zip([270., 80., d + 14.]) {
                    near(a, b);
                }
                let p = a.mass_properties(t).unwrap();
                let items = assembly_integrals(r, h, d);
                compare(
                    p.volume,
                    p.volume_centroid.unwrap(),
                    p.central_volume_inertia.unwrap(),
                    expected(&items, false),
                );
                compare(
                    p.mass_kg.unwrap(),
                    p.center_of_mass.unwrap(),
                    p.central_mass_inertia.unwrap(),
                    expected(&items, true),
                );
            }
        }
    }
}
#[test]
fn socket_depth_revision_preserves_stock_and_unaffected_tools_and_parts() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(8., 12., 28.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(8., 12., 28.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let revised = engine.build(&recipe(8., 16., 28.)).unwrap();
    assert_eq!(
        original.outputs["stock"].content_hash,
        revised.outputs["stock"].content_hash
    );
    let a = &original.assemblies["blind_bore_pallet"].parts;
    let b = &revised.assemblies["blind_bore_pallet"].parts;
    for id in ["sleeve", "foot"] {
        assert_eq!(a[id].content_hash, b[id].content_hash);
    }
    for id in ["pin", "pallet"] {
        assert_ne!(a[id].content_hash, b[id].content_hash);
    }
    assert_eq!(
        engine
            .build(&recipe(8., 16., 28.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}
