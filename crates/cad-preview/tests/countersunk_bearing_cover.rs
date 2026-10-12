//! Independent cylinder and Gauss-integrated frustum moments for machined annular stock and its cartridge.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/countersunk_bearing_cover.star");
fn recipe(p: f64, h: f64, d: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "countersunk_bearing_cover.star",
        SOURCE,
        &BTreeMap::from([
            ("bolt_pitch".into(), p),
            ("seat_depth".into(), h),
            ("flange_height".into(), d),
        ]),
    )
    .unwrap()
}
fn centers(p: f64) -> Vec<[f64; 2]> {
    (0..6)
        .map(|i| {
            let a = i as f64 * std::f64::consts::PI / 3.;
            [p * a.cos(), p * a.sin()]
        })
        .collect()
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
fn frustum(
    r0: f64,
    r1: f64,
    low: f64,
    high: f64,
    xy: [f64; 2],
    sign: f64,
    density: f64,
) -> Integral {
    let h = high - low;
    let mut m = [0.; 4];
    // Three-point Gauss quadrature is exact for these degree <=4 slice integrands.
    for (node, weight) in [
        (-(3_f64 / 5.).sqrt(), 5. / 9.),
        (0., 8. / 9.),
        ((3_f64 / 5.).sqrt(), 5. / 9.),
    ] {
        let z = (node + 1.) * h / 2.;
        let r = r0 + (r1 - r0) * z / h;
        for (a, value) in m
            .iter_mut()
            .zip([r * r, z * r * r, z * z * r * r, r.powi(4)])
        {
            *a += weight * h / 2. * value;
        }
    }
    Integral {
        volume: sign * std::f64::consts::PI * m[0],
        center: [xy[0], xy[1], low + m[1] / m[0]],
        variance: [
            m[3] / (4. * m[0]),
            m[3] / (4. * m[0]),
            m[2] / m[0] - (m[1] / m[0]).powi(2),
        ],
        density,
    }
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

fn flange_integrals(p: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut items = vec![
        cylinder(60., d, [0., 0., d / 2.], 1., 2700e-9),
        cylinder(20., d, [0., 0., d / 2.], -1., 2700e-9),
    ];
    for [x, y] in centers(p) {
        items.push(cylinder(3.2, d - h, [x, y, (d - h) / 2.], -1., 2700e-9));
        items.push(frustum(3.2, 3.2 + h, d - h, d, [x, y], -1., 2700e-9));
    }
    for [x, y] in centers(28.).into_iter().step_by(2) {
        items.push(cylinder(3., 5., [x, y, d - 2.5], -1., 2700e-9));
    }
    items
}
fn assembly_integrals(p: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut module = flange_integrals(p, h, d);
    let length = d - h + 7.;
    for [x, y] in centers(p) {
        module.push(cylinder(3., length, [x, y, -7. + length / 2.], 1., 7800e-9));
        module.push(frustum(3., 3. + h, d - h, d, [x, y], 1., 7800e-9));
    }
    module.push(cylinder(19.8, d - 2., [0., 0., d / 2.], 1., 7800e-9));
    module.push(cylinder(10., d - 2., [0., 0., d / 2.], -1., 7800e-9));
    let mut items = module.clone();
    // Rx(pi) preserves each axial cylinder's diagonal central tensor.
    items.extend(module.into_iter().map(|mut i| {
        i.center = [i.center[0], -i.center[1], 2. * d + 40. - i.center[2]];
        i
    }));
    for [x, y] in centers(28.).into_iter().step_by(2) {
        items.push(cylinder(2.8, 50., [x, y, d + 20.], 1., 7800e-9));
    }
    items.push(cylinder(9.8, 2. * d + 60., [0., 0., d + 20.], 1., 7800e-9));
    items
}
#[test]
fn annular_machining_and_cartridge_match_twenty_seven_independent_integrals() {
    let t = cad_kernel::Tolerance::default();
    for p in [40., 42., 44.] {
        for h in [2., 3., 4.] {
            for d in [18., 21., 24.] {
                let build = cad_build::build(&recipe(p, h, d)).unwrap();
                let model = &build.outputs["flange"].result.model;
                assert!(model.validate(t).valid);
                let properties = model.mass_properties(t).unwrap();
                compare(
                    properties.volume,
                    properties.centroid.unwrap(),
                    properties.central_inertia.unwrap(),
                    expected(&flange_integrals(p, h, d), false),
                );
                assert_eq!(
                    model
                        .faces
                        .iter()
                        .filter(
                            |f| matches!(&f.surface,cad_kernel::Surface::Cylinder(c) if c.inward)
                        )
                        .count(),
                    10
                );
                assert_eq!(model.faces.iter().filter(|face| matches!(&face.surface, cad_kernel::Surface::Cone(cone) if cone.inward)).count(), 6);
                let assembly = &build.assemblies["countersunk_bearing_cover"];
                assert_eq!(assembly.instances.len(), 20);
                assert_eq!(assembly.parts.len(), 5);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([-60., -60., -10.]) {
                    near(a, b);
                }
                for (a, b) in bounds.max.into_iter().zip([60., 60., 2. * d + 50.]) {
                    near(a, b);
                }
                for (i, [x, y]) in centers(p).into_iter().enumerate() {
                    let datum = &assembly.parts["flange"].datums[&format!("seat_{i}")];
                    for (a, b) in datum.translation.into_iter().zip([x, y, d]) {
                        near(a, b);
                    }
                }
                let properties = assembly.mass_properties(t).unwrap();
                let items = assembly_integrals(p, h, d);
                compare(
                    properties.volume,
                    properties.volume_centroid.unwrap(),
                    properties.central_volume_inertia.unwrap(),
                    expected(&items, false),
                );
                compare(
                    properties.mass_kg.unwrap(),
                    properties.center_of_mass.unwrap(),
                    properties.central_mass_inertia.unwrap(),
                    expected(&items, true),
                );
            }
        }
    }
}
#[test]
fn seat_edits_preserve_round_stock_and_rod_placement() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(42., 3., 21.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(42., 3., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let changed = engine.build(&recipe(42., 4., 21.)).unwrap();
    for name in ["stock", "sockets", "piloted"] {
        assert_eq!(
            original.outputs[name].content_hash,
            changed.outputs[name].content_hash
        );
    }
    let a = &original.assemblies["countersunk_bearing_cover"];
    let b = &changed.assemblies["countersunk_bearing_cover"];
    for id in ["bearing", "rod", "shaft"] {
        assert_eq!(a.parts[id].content_hash, b.parts[id].content_hash);
    }
    for id in ["flange", "bolt"] {
        assert_ne!(a.parts[id].content_hash, b.parts[id].content_hash);
    }
    let position = |assembly: &cad_assembly::BuiltAssembly, id: &str| {
        assembly
            .instances
            .iter()
            .find(|i| i.id == id)
            .unwrap()
            .transform
            .translation
    };
    assert_eq!(position(b, "lower_bolt_0"), position(a, "lower_bolt_0"));
    assert_eq!(position(b, "upper_bolt_0"), position(a, "upper_bolt_0"));
    assert_eq!(position(b, "rod_0"), position(a, "rod_0"));
    assert_eq!(
        engine
            .build(&recipe(42., 4., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}
