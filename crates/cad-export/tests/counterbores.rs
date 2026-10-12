#[path = "support/counterbore.rs"]
mod counterbore;

#[test]
fn stepped_bores_export_periodic_walls_and_annular_shoulders_in_either_order() {
    let t = cad_kernel::Tolerance::default();
    for name in counterbore::CASES {
        for oblique in [false, true] {
            let model = counterbore::fixture(name, oblique).unwrap();
            let walls = if name == "opposed" { 3 } else { 2 };
            let step = cad_export::to_step(&model, t).unwrap();
            assert_eq!(step.matches("SEAM_CURVE(").count(), walls, "{name}");
            assert_eq!(step.matches("PCURVE(").count(), walls * 2, "{name}");
            assert_eq!(
                step.matches("CYLINDRICAL_SURFACE(").count(),
                walls,
                "{name}"
            );
            assert_eq!(step.matches("PLANE(").count(), 6 + walls - 1, "{name}");
            assert_eq!(
                step.lines()
                    .filter(|l| l.contains("ADVANCED_FACE(") && l.ends_with(",.F.);"))
                    .count(),
                walls,
                "{name}"
            );
            let removed = match name {
                "opposed" => 9. * 4. + 49. * 4. + 36. * 4.,
                "top" | "top_reverse" => 9. * 8. + 49. * 4.,
                _ => 9. * 8. + 36. * 4.,
            };
            assert!(
                (model.mass_properties(t).unwrap().volume
                    - (38400. - std::f64::consts::PI * removed))
                    .abs()
                    < 1e-6,
                "{name}"
            );
            assert!(cad_export::to_stl(&model, t)
                .unwrap()
                .contains("facet normal"));
        }
    }
}
