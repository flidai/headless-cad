use std::{collections::BTreeMap, path::Path};

#[test]
fn nozzle_edits_match_independent_signed_frustum_integrals() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/tapered_nozzle.star"),
    )
    .unwrap();
    let close =
        |a: f64, b: f64| assert!((a - b).abs() < 1e-8_f64.max(b.abs() * 1e-11), "{a} != {b}");
    for length in [16., 28., 48.] {
        let recipe = cad_recipe::from_starlark_with_params(
            "tapered_nozzle.star",
            &source,
            &BTreeMap::from([("neck_length".into(), length)]),
        )
        .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let p = built.outputs["nozzle"]
            .result
            .model
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        let mut volume = 0.;
        let mut first = 0.;
        let mut axial = 0.;
        let mut transverse_origin = 0.;
        // Independent solid-frustum moments, subtracting the bore afterwards.
        for (sign, a, b, z, h) in [
            (1., 20., 24., 0., 4.),
            (1., 24., 24., 4., 6.),
            (1., 24., 18., 10., 6.),
            (1., 18., 14., 16., length),
            (1., 14., 12., 16. + length, 2.),
            (-1., 6., 6., 0., 14. + length),
            (-1., 6., 8., 14. + length, 4.),
        ] {
            let delta = b - a;
            let pi = std::f64::consts::PI;
            let v = pi * h * (a * a + a * b + b * b) / 3.;
            let q = pi * h * h * (a * a / 2. + 2. * a * delta / 3. + delta * delta / 4.);
            let zz = pi * h * h * h * (a * a / 3. + a * delta / 2. + delta * delta / 5.);
            let radial4 = pi
                * h
                * (a * a * a * a + a * a * a * b + a * a * b * b + a * b * b * b + b * b * b * b)
                / 5.;
            volume += sign * v;
            first += sign * (z * v + q);
            axial += sign * radial4 / 2.;
            transverse_origin += sign * (radial4 / 4. + z * z * v + 2. * z * q + zz);
        }
        let z = first / volume;
        let transverse = transverse_origin - volume * z * z;
        close(p.volume, volume);
        for (a, b) in p.centroid.unwrap().into_iter().zip([0., 0., z]) {
            close(a, b);
        }
        for (i, row) in p.central_inertia.unwrap().iter().enumerate() {
            for (j, &a) in row.iter().enumerate() {
                close(
                    a,
                    if i != j {
                        0.
                    } else if i == 2 {
                        axial
                    } else {
                        transverse
                    },
                );
            }
        }
        close(
            built.assemblies["tapered_nozzle"].material_volume(),
            volume + 2588. / 3. * std::f64::consts::PI,
        );
    }
}
