//! Independent rectangle/semicircle/cylinder integrals, not native geometry queries
//! reused as their own expectations. Assembly sums do not certify fit or loading.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/machined_bearing_support.star");
fn recipe(r: f64, d: f64, b: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "machined_bearing_support.star",
        SOURCE,
        &BTreeMap::from([
            ("seat_radius".into(), r),
            ("depth".into(), d),
            ("bore_radius".into(), b),
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
fn expectations(r: f64, d: f64, b: f64) -> (f64, f64, [f64; 3], f64, [f64; 3], f64, [f64; 3]) {
    let pi = std::f64::consts::PI;
    let area = 5000. - pi * r * r / 2. - 4. * pi * b * b;
    let cy =
        (125000. - pi * r * r / 2. * (50. - 4. * r / (3. * pi)) - 2. * pi * b * b * (12. + 40.))
            / area;
    // Rectangle minus downward semicircle and four independent disk integrals.
    let raw_y2 = 100. * 50_f64.powi(3) / 3.
        - (2500. * pi * r * r / 2. - 200. * r.powi(3) / 3. + pi * r.powi(4) / 8.)
        - pi * b.powi(4)
        - 2. * pi * b * b * (144. + 1600.);
    let central_y2 = raw_y2 - area * cy * cy;
    let central_x2 = 50. * 100_f64.powi(3) / 12.
        - pi * r.powi(4) / 8.
        - 4. * (pi * b.powi(4) / 4. + pi * b * b * 1444.);
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
    let sleeve_v = (2. * b - 1.) * pi * (d + 4.);
    let radial = b * b + (b - 1.).powi(2);
    let sleeve_ix = sleeve_v * (3. * radial + (d + 4.).powi(2)) / 12.;
    let sleeve_i = [sleeve_ix, sleeve_ix, sleeve_v * radial / 2.];
    (jaw_v, cy, jaw_i, shaft_v, shaft_i, sleeve_v, sleeve_i)
}
#[test]
fn sequential_drills_match_independent_moments_in_twenty_seven_configurations() {
    for r in [14., 18., 22.] {
        for d in [12., 18., 24.] {
            for b in [2., 3., 4.] {
                let build = cad_build::build(&recipe(r, d, b)).unwrap();
                let tol = cad_kernel::Tolerance::default();
                let (jaw_v, cy, jaw_i, shaft_v, shaft_i, sleeve_v, sleeve_i) =
                    expectations(r, d, b);
                let model = &build.outputs["drilled_jaw"].result.model;
                assert!(model.validate(tol).valid);
                let props = model.mass_properties(tol).unwrap();
                near(props.volume, jaw_v);
                for (a, b) in props.centroid.unwrap().into_iter().zip([50., cy, d / 2.]) {
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
                assert_eq!(walls.len(), 5);
                assert!(walls.iter().all(|c| c.inward));
                assert_eq!(model.edges.iter().filter(|e| matches!(&e.curve,cad_kernel::Curve::CircularArc(a) if a.sweep_radians<0.)).count(),2);
                let assembly = &build.assemblies["machined_bearing_support"];
                assert_eq!(assembly.instances.len(), 11);
                assert_eq!(assembly.parts.len(), 3);
                let bounds = assembly.bounds().unwrap();
                for (a, b) in bounds.min.into_iter().zip([0., 0., -12.]) {
                    near(a, b)
                }
                for (a, b) in bounds.max.into_iter().zip([100., 102., d + 12.]) {
                    near(a, b)
                }
                let p = assembly.mass_properties(tol).unwrap();
                near(p.volume, 2. * jaw_v + shaft_v + 8. * sleeve_v);
                near(
                    p.mass_kg.unwrap(),
                    2. * jaw_v * 2700e-9 + (shaft_v + 8. * sleeve_v) * 7800e-9,
                );
                for centroid in [p.volume_centroid.unwrap(), p.center_of_mass.unwrap()] {
                    for (a, b) in centroid.into_iter().zip([50., 51., d / 2.]) {
                        near(a, b)
                    }
                }
                let jaw_delta = 51. - cy;
                let jaws = [
                    2. * (jaw_i[0] + jaw_v * jaw_delta * jaw_delta),
                    2. * jaw_i[1],
                    2. * (jaw_i[2] + jaw_v * jaw_delta * jaw_delta),
                ];
                let sleeves = [
                    8. * sleeve_i[0] + 4. * sleeve_v * (39_f64.powi(2) + 11_f64.powi(2)),
                    8. * (sleeve_i[1] + sleeve_v * 1444.),
                    8. * sleeve_i[2]
                        + 8. * sleeve_v * 1444.
                        + 4. * sleeve_v * (39_f64.powi(2) + 11_f64.powi(2)),
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
fn machining_edits_reuse_unaffected_stock_and_tools() {
    let mut engine = cad_build::BuildEngine::default();
    let original = engine.build(&recipe(18., 18., 3.)).unwrap();
    assert_eq!(
        engine
            .build(&recipe(18., 18., 3.))
            .unwrap()
            .stats
            .computed_solids,
        0
    );
    let resized = engine.build(&recipe(22., 18., 3.)).unwrap();
    assert_eq!(resized.stats.computed_solids, 6);
    let old = &original.assemblies["machined_bearing_support"];
    let new = &resized.assemblies["machined_bearing_support"];
    assert_eq!(
        old.parts["sleeve"].content_hash,
        new.parts["sleeve"].content_hash
    );
    let bored = engine.build(&recipe(22., 18., 4.)).unwrap();
    assert_eq!(bored.stats.computed_solids, 10);
    assert_eq!(
        resized.outputs["stock"].content_hash,
        bored.outputs["stock"].content_hash
    );
    assert_eq!(
        new.parts["shaft"].content_hash,
        bored.assemblies["machined_bearing_support"].parts["shaft"].content_hash
    );
    assert_eq!(
        engine
            .build(&recipe(22., 24., 4.))
            .unwrap()
            .stats
            .computed_solids,
        12
    );
}
