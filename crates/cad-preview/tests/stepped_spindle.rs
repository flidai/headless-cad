use std::{collections::BTreeMap, path::Path};

#[test]
fn spindle_edits_match_independent_cylinder_moments() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/stepped_spindle.star");
    let source = std::fs::read_to_string(path).unwrap();
    let close =
        |a: f64, b: f64| assert!((a - b).abs() < 1e-9_f64.max(b.abs() * 1e-11), "{a} != {b}");
    for (length, radius) in [(40., 12.), (50., 15.), (80., 18.)] {
        let recipe = cad_recipe::from_starlark_with_params(
            "stepped_spindle.star",
            &source,
            &BTreeMap::from([("length".into(), length), ("flange_radius".into(), radius)]),
        )
        .unwrap();
        let built = cad_build::build(&recipe).unwrap();
        let properties = built.outputs["shaft"]
            .result
            .model
            .mass_properties(recipe.tolerance.unwrap_or_default())
            .unwrap();
        let cylinders = [
            (8., 20., 10.),
            (radius, 6., 23.),
            (6., length - 26., (length + 26.) / 2.),
        ];
        let volume: f64 = cylinders
            .iter()
            .map(|(r, h, _)| std::f64::consts::PI * r * r * h)
            .sum();
        let z = cylinders
            .iter()
            .map(|(r, h, z)| std::f64::consts::PI * r * r * h * z)
            .sum::<f64>()
            / volume;
        let axial: f64 = cylinders
            .iter()
            .map(|(r, h, _)| std::f64::consts::PI * r * r * h * r * r / 2.)
            .sum();
        let transverse: f64 = cylinders
            .iter()
            .map(|(r, h, c)| {
                std::f64::consts::PI * r * r * h * ((3. * r * r + h * h) / 12. + (c - z) * (c - z))
            })
            .sum();
        close(properties.volume, volume);
        for (a, b) in properties.centroid.unwrap().into_iter().zip([0., 0., z]) {
            close(a, b);
        }
        let tensor = properties.central_inertia.unwrap();
        for (i, row) in tensor.iter().enumerate() {
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
            built.assemblies["stepped_spindle"].material_volume(),
            volume + (10952. + 10720.) * std::f64::consts::PI,
        );
        assert_eq!(built.assemblies["stepped_spindle"].instances.len(), 3);
    }
}
