//! Independent sliced-domain quadrature for six interacting planar edge chamfers.
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../../../examples/chamfered_guide_block.star");
fn recipe(d: f64, e: f64, h: f64) -> cad_build::Recipe {
    cad_recipe::from_starlark_with_params(
        "chamfered_guide_block.star",
        SOURCE,
        &BTreeMap::from([
            ("corner".into(), d),
            ("bevel".into(), e),
            ("height".into(), h),
        ]),
    )
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7 + 1e-9 * b.abs(), "{a} != {b}");
}
fn gauss(low: f64, high: f64) -> [(f64, f64); 3] {
    let root = (3_f64 / 5.).sqrt();
    [(-root, 5. / 9.), (0., 8. / 9.), (root, 5. / 9.)].map(|(node, weight)| {
        (
            (low + high) / 2. + node * (high - low) / 2.,
            weight * (high - low) / 2.,
        )
    })
}
fn expected(d: f64, e: f64, h: f64) -> (f64, [f64; 3], [[f64; 3]; 3]) {
    let mut m = [0.; 5];
    // Face setbacks define x/y integration limits directly; no kernel geometry is sampled.
    for (lo, hi) in [(0., h - e), (h - e, h)] {
        for (z, wz) in gauss(lo, hi) {
            let s = (z - (h - e)).max(0.);
            for (a, b) in [(s, d), (d, 40. - d), (40. - d, 40. - s)] {
                for (y, wy) in gauss(a, b) {
                    let cut = (d - y).max(0.).max(y - (40. - d));
                    let width = 60. - 2. * cut;
                    let weight = wz * wy;
                    for (sum, value) in m.iter_mut().zip([
                        width,
                        width * z,
                        width * z * z,
                        width.powi(3) / 12.,
                        width * (y - 20.).powi(2),
                    ]) {
                        *sum += weight * value;
                    }
                }
            }
        }
    }
    let z = m[1] / m[0];
    let iz = m[2] - m[0] * z * z;
    (
        m[0],
        [30., 20., z],
        [
            [m[4] + iz, 0., 0.],
            [0., m[3] + iz, 0.],
            [0., 0., m[3] + m[4]],
        ],
    )
}
#[test]
fn twenty_seven_chamfer_chains_match_independent_volume_centroid_and_inertia() {
    let mut engine = cad_build::BuildEngine::default();
    for d in [3., 4., 5.] {
        for e in [0.75, 1.5, 2.25] {
            for h in [16., 20., 24.] {
                let built = engine.build(&recipe(d, e, h)).unwrap();
                let model = &built.outputs["block"].result.model;
                let t = cad_kernel::Tolerance::default();
                assert!(model.validate(t).valid);
                assert_eq!(model.faces.len(), 12);
                assert_eq!(model.solids.len(), 1);
                let p = model.mass_properties(t).unwrap();
                let (v, c, i) = expected(d, e, h);
                near(p.volume, v);
                near(
                    v,
                    (2400. - 2. * d * d) * h - (60. - 2. * d) * e * e - 2. * e.powi(3) / 3.,
                );
                for (a, b) in p.centroid.unwrap().into_iter().zip(c) {
                    near(a, b);
                }
                for (ar, br) in p.central_inertia.unwrap().into_iter().zip(i) {
                    for (a, b) in ar.into_iter().zip(br) {
                        near(a, b);
                    }
                }
                for assertion in built.face_assertions.values() {
                    assert_eq!(assertion.status, cad_build::FaceResolutionStatus::Resolved);
                    assert_eq!(assertion.witnesses.len(), 1);
                }
                near(
                    built.section_views["mid"].query.section.area,
                    2400. - 2. * d * d,
                );
                near(
                    built.section_views["bevel"].query.section.area,
                    2400. - 2. * d * d - (60. - 2. * d) * e - e * e / 2.,
                );
                let frame = built.face_datums["block"]["wear_seat"].frame;
                let a = 60. - 2. * d;
                let cy = e * (a / 2. + 2. * e / 3.) / (a + e);
                for (actual, expected) in frame.translation.into_iter().zip([30., cy, h - e + cy]) {
                    near(actual, expected);
                }
                for (actual, expected) in frame.vector([0., 0., 1.]).into_iter().zip([
                    0.,
                    -std::f64::consts::FRAC_1_SQRT_2,
                    std::f64::consts::FRAC_1_SQRT_2,
                ]) {
                    near(actual, expected);
                }
                let assembly = &built.assemblies["chamfered_guide_block"];
                assert_eq!(assembly.instances.len(), 4);
                assert_eq!(assembly.parts.len(), 2);
                for instance in &assembly.instances {
                    if instance.part == "pad" {
                        let side = &assembly.instance_paths[&instance.id][0];
                        let shift = if side == "right" { 90. } else { 0. };
                        for (actual, expected) in instance
                            .transform
                            .point([20., 0.25, 0.])
                            .into_iter()
                            .zip([30. + shift, cy, h - e + cy])
                        {
                            near(actual, expected);
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn identical_recipe_reuses_native_chamfers_and_topology_witnesses() {
    let mut engine = cad_build::BuildEngine::default();
    let r = recipe(4., 1.5, 20.);
    let first = engine.build(&r).unwrap();
    let second = engine.build(&r).unwrap();
    assert_eq!(second.stats.computed_solids, 0);
    assert_eq!(
        first.outputs["block"].content_hash,
        second.outputs["block"].content_hash
    );
    assert_eq!(
        serde_json::to_value(first.face_datums).unwrap(),
        serde_json::to_value(second.face_datums).unwrap()
    );
}
