use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/serpentine_manifold.star");

#[derive(Default)]
struct Moments {
    weight: f64,
    first: [f64; 3],
    pieces: Vec<(f64, [f64; 3], [f64; 3])>,
}
impl Moments {
    fn add(&mut self, weight: f64, center: [f64; 3], central: [f64; 3]) {
        self.weight += weight;
        for (i, coordinate) in center.into_iter().enumerate() {
            self.first[i] += weight * coordinate;
        }
        self.pieces.push((weight, center, central));
    }
    fn cuboid(&mut self, min: [f64; 3], size: [f64; 3], coefficient: f64) {
        let weight = size.iter().product::<f64>() * coefficient;
        let center = std::array::from_fn(|i| min[i] + size[i] / 2.);
        let central = std::array::from_fn(|i| {
            weight * (size[(i + 1) % 3].powi(2) + size[(i + 2) % 3].powi(2)) / 12.
        });
        self.add(weight, center, central);
    }
    fn cylinder(&mut self, xy: [f64; 2], z: f64, radius: f64, height: f64, coefficient: f64) {
        let weight = std::f64::consts::PI * radius.powi(2) * height * coefficient;
        let transverse = weight * (3. * radius.powi(2) + height.powi(2)) / 12.;
        self.add(
            weight,
            [xy[0], xy[1], z + height / 2.],
            [transverse, transverse, weight * radius.powi(2) / 2.],
        );
    }
    fn center(&self) -> [f64; 3] {
        self.first.map(|v| v / self.weight)
    }
    fn inertia(&self) -> [[f64; 3]; 3] {
        let c = self.center();
        std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                self.pieces
                    .iter()
                    .map(|(weight, center, diagonal)| {
                        let d: [f64; 3] = std::array::from_fn(|k| center[k] - c[k]);
                        if i == j {
                            diagonal[i] + weight * (d[(i + 1) % 3].powi(2) + d[(i + 2) % 3].powi(2))
                        } else {
                            -weight * d[i] * d[j]
                        }
                    })
                    .sum()
            })
        })
    }
}
const BOLTS: [[f64; 2]; 4] = [[6., 6.], [90., 6.], [90., 58.], [6., 58.]];
fn plate(m: &mut Moments, z: f64, height: f64, coefficient: f64) {
    m.cuboid([0., 0., z], [96., 64., height], coefficient);
    for xy in BOLTS {
        m.cylinder(xy, z, 2.5, height, -coefficient);
    }
}
fn remove_channel(m: &mut Moments, w: f64, h: f64, z: f64, coefficient: f64) {
    // Disjoint rectangular prisms: three horizontal legs and two connectors.
    // No polygon integration or tessellation from the production kernel is used.
    for (xy, size) in [
        ([12., 12.], [72., w]),
        ([84. - w, 12. + w], [w, 20. - w]),
        ([12., 32.], [72., w]),
        ([12., 32. + w], [w, 12. - w]),
        ([12., 44.], [72., w]),
    ] {
        m.cuboid([xy[0], xy[1], z], [size[0], size[1], h], -coefficient);
    }
}
fn assembly_oracle(w: f64, h: f64, material: bool) -> Moments {
    let aluminum = if material { 2700e-9 } else { 1. };
    let steel = if material { 7800e-9 } else { 1. };
    let mut m = Moments::default();
    plate(&mut m, 0., 3., aluminum);
    plate(&mut m, 3., h, aluminum);
    remove_channel(&mut m, w, h, 3., aluminum);
    plate(&mut m, h + 3., 3., aluminum);
    for xy in [[12. + w / 2., 12. + w / 2.], [84. - w / 2., 44. + w / 2.]] {
        m.cylinder(xy, h + 3., 2.5, 3., -aluminum);
        m.cylinder(xy, h + 6., 4., 10., steel);
        m.cylinder(xy, h + 6., 2.5, 10., -steel);
    }
    for xy in BOLTS {
        m.cylinder(xy, 0., 2., h + 6., steel);
        m.cylinder(xy, h + 6., 4., 3., steel);
    }
    m
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-8 * b.abs().max(1.), "{a} != {b}");
}
fn check(m: &Moments, weight: f64, center: [f64; 3], inertia: [[f64; 3]; 3]) {
    near(weight, m.weight);
    for (actual, expected) in center.into_iter().zip(m.center()) {
        near(actual, expected);
    }
    for (actual, expected) in inertia.into_iter().zip(m.inertia()) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            near(actual, expected);
        }
    }
}
fn recipe(w: f64, h: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "serpentine_manifold.star",
        SOURCE,
        &BTreeMap::from([("channel_width".into(), w), ("channel_depth".into(), h)]),
    )
    .unwrap()
}
#[test]
fn serpentine_native_properties_and_assembly_match_independent_box_cylinder_oracles() {
    let mut engine = cad_build::BuildEngine::default();
    for w in [6., 8., 10.] {
        for h in [6., 10., 18.] {
            let recipe = recipe(w, h);
            let result = engine.build(&recipe).unwrap();
            let model = &result.outputs["channel_core"].result.model;
            let properties = model
                .mass_properties(cad_kernel::Tolerance::default())
                .unwrap();
            let mut native = Moments::default();
            plate(&mut native, 0., h, 1.);
            remove_channel(&mut native, w, h, 0., 1.);
            check(
                &native,
                properties.volume,
                properties.centroid.unwrap(),
                properties.central_inertia.unwrap(),
            );
            assert_eq!(model.faces.len(), 22); // two caps, 4 outer, 12 slot, 4 round walls
            assert_eq!(result.face_assertions["channel_walls"].witnesses.len(), 12);
            let assembly = &result.assemblies["serpentine_manifold"];
            assert_eq!(assembly.parts.len(), 5);
            assert_eq!(assembly.instances.len(), 9);
            let p = assembly
                .mass_properties(cad_kernel::Tolerance::default())
                .unwrap();
            check(
                &assembly_oracle(w, h, true),
                p.mass_kg.unwrap(),
                p.center_of_mass.unwrap(),
                p.central_mass_inertia.unwrap(),
            );
            near(
                result.property_assertions["assembly_material"]
                    .actual
                    .unwrap(),
                assembly_oracle(w, h, false).weight,
            );
            for (a, b) in assembly
                .bounds()
                .unwrap()
                .min
                .into_iter()
                .zip([0., 0., 0.])
                .chain(
                    assembly
                        .bounds()
                        .unwrap()
                        .max
                        .into_iter()
                        .zip([96., 64., h + 16.]),
                )
            {
                near(a, b);
            }
            let open = &result.assemblies["open_channel"];
            assert_eq!(open.parts.len(), 2);
            assert_eq!(open.instances.len(), 2);
        }
    }
}
#[test]
fn serpentine_edits_reuse_unaffected_definitions_and_keep_global_obligations() {
    let mut engine = cad_build::BuildEngine::default();
    let first = engine.build(&recipe(8., 10.)).unwrap();
    assert_eq!(first.stats.computed_solids, 5);
    let widened = engine.build(&recipe(9., 10.)).unwrap();
    assert_eq!(widened.stats.computed_solids, 2);
    let deepened = engine.build(&recipe(9., 11.)).unwrap();
    assert_eq!(deepened.stats.computed_solids, 2);
    let selected = engine
        .build_outputs(&recipe(9., 11.), &["channel_core".into()])
        .unwrap();
    assert_eq!(selected.outputs.len(), 1);
    assert_eq!(selected.property_assertions.len(), 3);
    assert!(selected
        .property_assertions
        .values()
        .all(|p| p.status == cad_build::PropertyAssertionStatus::Passed));
    for (w, h) in [(5.9, 10.), (10.1, 10.), (8., 5.9), (8., 18.1)] {
        assert!(cad_recipe::from_starlark_with_params(
            "serpentine_manifold.star",
            SOURCE,
            &BTreeMap::from([("channel_width".into(), w), ("channel_depth".into(), h)])
        )
        .is_err());
    }
}
