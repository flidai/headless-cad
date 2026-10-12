//! Independent rectangle/semicircle/cylinder integrals, not native geometry queries
//! reused as their own expectations. Assembly sums do not certify fit or loading.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/split_bearing_block.star");
fn recipe(r: f64, d: f64, g: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "split_bearing_block.star",
        SOURCE,
        &BTreeMap::from([
            ("seat_radius".into(), r),
            ("depth".into(), d),
            ("jaw_gap".into(), g),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-6 + 1e-8 * b.abs(), "{a} != {b}");
}
fn tensor(actual: [[f64; 3]; 3], diagonal: [f64; 3]) {
    for (i, row) in actual.into_iter().enumerate() {
        for (j, value) in row.into_iter().enumerate() {
            near(value, if i == j { diagonal[i] } else { 0. });
        }
    }
}
fn expectations(r: f64, d: f64) -> (f64, f64, [f64; 3], f64, [f64; 3], f64, [f64; 3]) {
    let pi = std::f64::consts::PI;
    let area = 3200. - pi * r * r / 2. - 18. * pi;
    let cy = (64000. - pi * r * r / 2. * (40. - 4. * r / (3. * pi)) - 18. * pi * 12.) / area;
    // Integrate y² and (x−40)² over the removed downward semicircle.
    let raw_y2 = 80. * 40_f64.powi(3) / 3.
        - (1600. * pi * r * r / 2. - 160. * r.powi(3) / 3. + pi * r.powi(4) / 8.)
        - 2. * (pi * 81. / 4. + 9. * pi * 144.);
    let central_y2 = raw_y2 - area * cy * cy;
    let central_x2 =
        40. * 80_f64.powi(3) / 12. - pi * r.powi(4) / 8. - 2. * (pi * 81. / 4. + 9. * pi * 900.);
    let jaw_v = area * d;
    let jaw_i = [
        d * central_y2 + jaw_v * d * d / 12.,
        d * central_x2 + jaw_v * d * d / 12.,
        d * (central_y2 + central_x2),
    ];
    let head_r = r + 3.;
    let neck_r = r - 2.;
    let neck_l = d + 16.;
    let head_v = pi * head_r * head_r * 4.;
    let neck_v = pi * neck_r * neck_r * neck_l;
    let shaft_v = 2. * head_v + neck_v;
    let shaft_ix = 2. * head_v * ((3. * head_r * head_r + 16.) / 12. + (d / 2. + 10.).powi(2))
        + neck_v * (3. * neck_r * neck_r + neck_l * neck_l) / 12.;
    let shaft_i = [
        shaft_ix,
        shaft_ix,
        head_v * head_r * head_r + neck_v * neck_r * neck_r / 2.,
    ];
    let sleeve_v = 5. * pi * (d + 4.);
    let sleeve_ix = sleeve_v * (39. + (d + 4.).powi(2)) / 12.;
    let sleeve_i = [sleeve_ix, sleeve_ix, sleeve_v * 13. / 2.];
    (jaw_v, cy, jaw_i, shaft_v, shaft_i, sleeve_v, sleeve_i)
}
#[test]
fn concave_seats_match_independent_moments_in_twenty_seven_configurations() {
    for r in [10., 14., 18.] {
        for d in [12., 16., 24.] {
            for g in [0., 2., 3.] {
                let build = cad_build::build(&recipe(r, d, g)).unwrap();
                let tol = cad_kernel::Tolerance::default();
                let (jaw_v, cy, jaw_i, shaft_v, shaft_i, sleeve_v, sleeve_i) = expectations(r, d);
                let model = &build.outputs["jaw"].result.model;
                assert!(model.validate(tol).valid);
                let props = model.mass_properties(tol).unwrap();
                near(props.volume, jaw_v);
                for (a, b) in props.centroid.unwrap().into_iter().zip([40., cy, d / 2.]) {
                    near(a, b)
                }
                tensor(props.central_inertia.unwrap(), jaw_i);
                let walls: Vec<_> = model
                    .faces
                    .iter()
                    .filter_map(|f| {
                        if let cad_kernel::Surface::Cylinder(c) = &f.surface {
                            Some(c)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(walls.len(), 3);
                assert!(walls.iter().all(|c| c.inward));
                assert_eq!(model.edges.iter().filter(|e| matches!(&e.curve,cad_kernel::Curve::CircularArc(a) if a.sweep_radians<0.)).count(),2);
                let assembly = &build.assemblies["split_bearing_block"];
                assert_eq!(assembly.instances.len(), 7);
                assert_eq!(assembly.parts.len(), 3);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., -12.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([80., 80. + g, d + 12.]) {
                    near(a, b)
                }
                let p = assembly.mass_properties(tol).unwrap();
                near(p.volume, 2. * jaw_v + shaft_v + 4. * sleeve_v);
                near(
                    p.mass_kg.unwrap(),
                    2. * jaw_v * 2700e-9 + (shaft_v + 4. * sleeve_v) * 7800e-9,
                );
                for centroid in [p.volume_centroid.unwrap(), p.center_of_mass.unwrap()] {
                    for (a, b) in centroid.into_iter().zip([40., 40. + g / 2., d / 2.]) {
                        near(a, b)
                    }
                }
                let jaw_delta = 40. + g / 2. - cy;
                let sleeve_delta = 28. + g / 2.;
                let jaws = [
                    2. * (jaw_i[0] + jaw_v * jaw_delta * jaw_delta),
                    2. * jaw_i[1],
                    2. * (jaw_i[2] + jaw_v * jaw_delta * jaw_delta),
                ];
                let sleeves = [
                    4. * (sleeve_i[0] + sleeve_v * sleeve_delta * sleeve_delta),
                    4. * (sleeve_i[1] + sleeve_v * 900.),
                    4. * (sleeve_i[2] + sleeve_v * (900. + sleeve_delta * sleeve_delta)),
                ];
                tensor(
                    p.central_volume_inertia.unwrap(),
                    std::array::from_fn(|i| jaws[i] + shaft_i[i] + sleeves[i]),
                );
                tensor(
                    p.central_mass_inertia.unwrap(),
                    std::array::from_fn(|i| {
                        jaws[i] * 2700e-9 + (shaft_i[i] + sleeves[i]) * 7800e-9
                    }),
                );
            }
        }
    }
}
#[test]
fn separation_edits_only_place_parts_and_geometry_edits_rebuild_shared_definitions() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(14., 16., 2.)).unwrap();
    let warm = engine.build(&recipe(14., 16., 2.)).unwrap();
    assert_eq!(warm.stats.computed_solids, 0);
    let separated = engine.build(&recipe(14., 16., 3.)).unwrap();
    assert_eq!(separated.stats.computed_solids, 0);
    let old = &original.assemblies["split_bearing_block"];
    let new = &separated.assemblies["split_bearing_block"];
    assert_ne!(old.content_hash, new.content_hash);
    for name in ["jaw", "shaft", "sleeve"] {
        assert_eq!(old.parts[name].content_hash, new.parts[name].content_hash);
    }
    near(old.material_volume(), new.material_volume());
    let resized = engine.build(&recipe(18., 16., 3.)).unwrap();
    assert_eq!(resized.stats.computed_solids, 2);
    let deeper = engine.build(&recipe(18., 24., 3.)).unwrap();
    assert_eq!(deeper.stats.computed_solids, 3);
}
