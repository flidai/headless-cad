//! Independent disjoint axial-slice integrals and shoulder-driven placements.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/counterbored_fixture.star");
const CENTERS: [[f64; 2]; 6] = [
    [20., 20.],
    [60., 20.],
    [100., 20.],
    [20., 60.],
    [60., 60.],
    [100., 60.],
];
fn recipe(r: f64, h: f64, d: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "counterbored_fixture.star",
        SOURCE,
        &BTreeMap::from([
            ("counterbore_radius".into(), r),
            ("counterbore_depth".into(), h),
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

fn plate_integrals(r: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut items = vec![Integral {
        volume: 9600. * d,
        center: [60., 40., d / 2.],
        variance: [1200., 6400. / 12., d * d / 12.],
        density: 2700e-9,
    }];
    for [x, y] in CENTERS {
        // Three NON-overlapping axial slices; no repeated drill-volume subtraction.
        items.push(cylinder(4.5, 3., [x, y, 1.5], -1., 2700e-9));
        items.push(cylinder(
            3.2,
            d - h - 3.,
            [x, y, (d - h + 3.) / 2.],
            -1.,
            2700e-9,
        ));
        items.push(cylinder(r, h, [x, y, d - h / 2.], -1., 2700e-9));
    }
    items
}
fn ring(items: &mut Vec<Integral>, outer: f64, inner: f64, h: f64, c: [f64; 3]) {
    items.push(cylinder(outer, h, c, 1., 7800e-9));
    items.push(cylinder(inner, h, c, -1., 7800e-9));
}
fn assembly_integrals(r: f64, h: f64, d: f64) -> Vec<Integral> {
    let mut items = vec![];
    for item in plate_integrals(r, h, d) {
        let mut upper = item.clone();
        upper.center[2] += d + 30.;
        items.push(upper);
        let mut lower = item;
        lower.center = [lower.center[0], 80. - lower.center[1], d - lower.center[2]];
        items.push(lower);
    }
    let length = 2. * d + 30. - 2. * h + 7.;
    for [x, y] in CENTERS {
        ring(&mut items, r - 0.3, 3.1, 1., [x, y, 2. * d + 30. - h + 0.5]);
        ring(&mut items, r - 0.3, 3.1, 1., [x, y, h - 0.5]);
        ring(&mut items, r - 1., 3.1, 4., [x, y, h - 3.]);
        ring(&mut items, 4.2, 3.2, 36., [x, y, d + 15.]);
        items.push(cylinder(
            3.,
            length,
            [x, y, h - 6. + length / 2.],
            1.,
            7800e-9,
        ));
        items.push(cylinder(
            r - 1.,
            3.,
            [x, y, 2. * d + 30. - h + 2.5],
            1.,
            7800e-9,
        ));
    }
    items
}
#[test]
fn opposed_counterbores_and_fastening_stack_match_twenty_seven_independent_integrals() {
    let t = cad_kernel::Tolerance::default();
    for r in [6., 7., 8.] {
        for h in [4., 6., 8.] {
            for d in [18., 21., 24.] {
                let build = cad_build::build(&recipe(r, h, d)).unwrap();
                let model = &build.outputs["plate"].result.model;
                assert!(model.validate(t).valid);
                let p = model.mass_properties(t).unwrap();
                compare(
                    p.volume,
                    p.centroid.unwrap(),
                    p.central_inertia.unwrap(),
                    expected(&plate_integrals(r, h, d), false),
                );
                assert_eq!(
                    model
                        .faces
                        .iter()
                        .filter(
                            |f| matches!(&f.surface,cad_kernel::Surface::Cylinder(c) if c.inward)
                        )
                        .count(),
                    18
                );
                assert_eq!(
                    model
                        .faces
                        .iter()
                        .filter(|f| f.try_plane().is_some() && f.loops.len() == 2)
                        .count(),
                    12
                );
                let a = &build.assemblies["counterbored_fixture"];
                assert_eq!(a.instances.len(), 32);
                assert_eq!(a.parts.len(), 5);
                let bounds = a.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., (h - 6.).min(0.)]) {
                    near(a, b);
                }
                for (a, b) in bounds.max.into_iter().zip([120., 80., 2. * d + 30.]) {
                    near(a, b);
                }
                for (i, [x, y]) in CENTERS.into_iter().enumerate() {
                    for (side, z) in [("top", d - h), ("bottom", 3.)] {
                        let datum = &a.parts["plate"].datums[&format!("{side}_seat_{i}")];
                        for (a, b) in datum.translation.into_iter().zip([x, y, z]) {
                            near(a, b);
                        }
                    }
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
fn shoulder_depth_edits_move_attached_hardware_and_reuse_independent_parts() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(7., 6., 21.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(7., 6., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let changed = engine.build(&recipe(7., 8., 21.)).unwrap();
    assert_eq!(
        original.outputs["stock"].content_hash,
        changed.outputs["stock"].content_hash
    );
    assert_eq!(
        original.outputs["piloted"].content_hash,
        changed.outputs["piloted"].content_hash
    );
    let a = &original.assemblies["counterbored_fixture"];
    let b = &changed.assemblies["counterbored_fixture"];
    for id in ["washer", "nut", "spacer"] {
        assert_eq!(a.parts[id].content_hash, b.parts[id].content_hash);
    }
    for id in ["plate", "bolt"] {
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
    // The semantic upper seat moves down2mm and lower seat moves up2mm.
    near(
        position(b, "top_washer_0")[2] - position(a, "top_washer_0")[2],
        -2.,
    );
    near(
        position(b, "bottom_washer_0")[2] - position(a, "bottom_washer_0")[2],
        2.,
    );
    near(position(b, "spacer_0")[2], position(a, "spacer_0")[2]);
    assert_eq!(
        engine
            .build(&recipe(7., 8., 21.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
}
