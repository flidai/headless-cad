#[path = "support/round_stock.rs"]
mod round_stock;

// Signed cylinder integrals are independent of the model's topology and mass evaluator.
fn expected(name: &str, oblique: bool) -> (f64, [f64; 3], [[f64; 3]; 3]) {
    let mut pieces = vec![(30_f64, 16_f64, [0_f64, 0_f64, 8_f64], 1_f64)];
    if name.starts_with("annulus") {
        pieces.push((8., 16., [0., 0., 8.], -1.));
    }
    if name.ends_with("through") {
        pieces.push((3., 16., [18., 0., 8.], -1.));
    } else if name.ends_with("blind") {
        pieces.push((2., 5., [-18., 0., 13.5], -1.));
    } else {
        pieces.extend([
            (3., 12., [18., 0., 6.], -1.),
            (5., 4., [18., 0., 14.], -1.),
            (2., 5., [-18., 0., 13.5], -1.),
        ]);
    }
    let integrals: Vec<_> = pieces
        .into_iter()
        .map(|(r, h, c, sign)| {
            let v = sign * std::f64::consts::PI * r * r * h;
            (
                v,
                c,
                [
                    v * (3. * r * r + h * h) / 12.,
                    v * (3. * r * r + h * h) / 12.,
                    v * r * r / 2.,
                ],
            )
        })
        .collect();
    let volume: f64 = integrals.iter().map(|p| p.0).sum();
    let center: [f64; 3] =
        std::array::from_fn(|i| integrals.iter().map(|(v, c, _)| v * c[i]).sum::<f64>() / volume);
    let tensor: [[f64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            integrals
                .iter()
                .map(|(v, c, diag)| {
                    if i == j {
                        diag[i]
                            + v * (0..3)
                                .filter(|&k| k != i)
                                .map(|k| (c[k] - center[k]).powi(2))
                                .sum::<f64>()
                    } else {
                        -v * (c[i] - center[i]) * (c[j] - center[j])
                    }
                })
                .sum()
        })
    });
    if !oblique {
        return (volume, center, tensor);
    }
    let q = 0.5_f64.sqrt();
    let rotation = [[0.5, 0.5, q], [0.5, 0.5, -q], [-q, q, 0.]];
    let world_center = std::array::from_fn(|i| {
        (0..3).map(|j| rotation[i][j] * center[j]).sum::<f64>() + [12., -7., 4.][i]
    });
    let world_tensor = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (0..3)
                .map(|a| {
                    (0..3)
                        .map(|b| rotation[i][a] * tensor[a][b] * rotation[j][b])
                        .sum::<f64>()
                })
                .sum()
        })
    });
    (volume, world_center, world_tensor)
}

#[test]
fn drilled_round_stock_preserves_exact_curved_surfaces_and_volume() {
    let t = cad_kernel::Tolerance::default();
    for name in round_stock::CASES {
        for oblique in [false, true] {
            let model = round_stock::fixture(name, oblique).unwrap();
            let inner = usize::from(name.starts_with("annulus"));
            let (walls, removed) = if name.ends_with("through") {
                (1, 9. * 16.)
            } else if name.ends_with("blind") {
                (1, 4. * 5.)
            } else {
                (3, 9. * 12. + 25. * 4. + 4. * 5.)
            };
            let step = cad_export::to_step(&model, t).unwrap();
            assert_eq!(
                step.matches("CYLINDRICAL_SURFACE(").count(),
                2 + inner + walls,
                "{name}"
            );
            assert_eq!(step.matches("SEAM_CURVE(").count(), inner + walls, "{name}");
            let nominal_volume =
                std::f64::consts::PI * ((900. - inner as f64 * 64.) * 16. - removed);
            let (volume, center, tensor) = expected(name, oblique);
            assert!((volume - nominal_volume).abs() < 1e-6);
            let actual = model.mass_properties(t).unwrap();
            assert!((actual.volume - volume).abs() < 1e-6, "{name}");
            for (i, row) in tensor.iter().enumerate() {
                assert!(
                    (actual.centroid.unwrap()[i] - center[i]).abs() < 1e-7,
                    "{name} centroid"
                );
                for (j, value) in row.iter().enumerate() {
                    assert!(
                        (actual.central_inertia.unwrap()[i][j] - value).abs() < 1e-5,
                        "{name} full inertia"
                    );
                }
            }
            assert!(cad_export::to_stl(&model, t)
                .unwrap()
                .contains("facet normal"));
        }
    }
}
