//! Independent cylinder integrals for machined annular stock and its cartridge.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/round_bearing_cartridge.star");
fn recipe(p: f64, h: f64, d: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "round_bearing_cartridge.star",
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
        items.push(cylinder(6., h, [x, y, d - h / 2.], -1., 2700e-9));
    }
    for [x, y] in centers(30.).into_iter().step_by(2) {
        items.push(cylinder(3., 5., [x, y, d - 2.5], -1., 2700e-9));
    }
    items
}
fn assembly_integrals(p: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut module = flange_integrals(p, h, d);
    let length = d - h + 7.;
    for [x, y] in centers(p) {
        module.push(cylinder(5.7, 1., [x, y, d - h + 0.5], 1., 7800e-9));
        module.push(cylinder(3.1, 1., [x, y, d - h + 0.5], -1., 7800e-9));
        module.push(cylinder(3., length, [x, y, -6. + length / 2.], 1., 7800e-9));
        module.push(cylinder(5.3, 3., [x, y, d - h + 2.5], 1., 7800e-9));
    }
    module.push(cylinder(19.8, d - 2., [0., 0., d / 2.], 1., 7800e-9));
    module.push(cylinder(10., d - 2., [0., 0., d / 2.], -1., 7800e-9));
    let mut items = module.clone();
    // Rx(pi) preserves each axial cylinder's diagonal central tensor.
    items.extend(module.into_iter().map(|mut i| {
        i.center = [i.center[0], -i.center[1], 2. * d + 40. - i.center[2]];
        i
    }));
    for [x, y] in centers(30.).into_iter().step_by(2) {
        items.push(cylinder(2.8, 50., [x, y, d + 20.], 1., 7800e-9));
    }
    items.push(cylinder(9.8, 2. * d + 60., [0., 0., d + 20.], 1., 7800e-9));
    items
}
#[test]
fn annular_machining_and_cartridge_match_twenty_seven_independent_integrals() {
    let t = cad_kernel::Tolerance::default();
    for p in [40., 42., 44.] {
        for h in [4., 6., 8.] {
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
                    16
                );
                let assembly = &build.assemblies["round_bearing_cartridge"];
                assert_eq!(assembly.instances.len(), 32);
                assert_eq!(assembly.parts.len(), 6);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([-60., -60., -10.]) {
                    near(a, b);
                }
                for (a, b) in bounds.max.into_iter().zip([60., 60., 2. * d + 50.]) {
                    near(a, b);
                }
                for (i, [x, y]) in centers(p).into_iter().enumerate() {
                    let datum = &assembly.parts["flange"].datums[&format!("seat_{i}")];
                    for (a, b) in datum.translation.into_iter().zip([x, y, d - h]) {
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
    let original = engine.build(&recipe(42., 6., 21.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(42., 6., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let changed = engine.build(&recipe(42., 8., 21.)).unwrap();
    for name in ["stock", "sockets", "piloted"] {
        assert_eq!(
            original.outputs[name].content_hash,
            changed.outputs[name].content_hash
        );
    }
    let a = &original.assemblies["round_bearing_cartridge"];
    let b = &changed.assemblies["round_bearing_cartridge"];
    for id in ["bearing", "rod", "shaft", "washer"] {
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
    near(
        position(b, "lower_washer_0")[2] - position(a, "lower_washer_0")[2],
        -2.,
    );
    near(
        position(b, "upper_washer_0")[2] - position(a, "upper_washer_0")[2],
        2.,
    );
    assert_eq!(position(b, "rod_0"), position(a, "rod_0"));
    assert_eq!(
        engine
            .build(&recipe(42., 8., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}
