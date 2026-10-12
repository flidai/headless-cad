#[path = "support/countersink.rs"]
mod countersink;

#[test]
fn countersinks_export_exact_cones_with_shared_cylinder_junctions() {
    let t = cad_kernel::Tolerance::default();
    for name in countersink::CASES {
        for oblique in [false, true] {
            let model = countersink::fixture(name, oblique).unwrap();
            let stock = if name.starts_with("box") {
                60. * 40. * 12.
            } else {
                std::f64::consts::PI
                    * (900. - if name.starts_with("annulus") { 64. } else { 0. })
                    * 12.
            };
            let removed = std::f64::consts::PI
                * if name.contains("crossing") {
                    9. * 6.5 + 5.5 * (9. + 3. * 7.4 + 7.4 * 7.4) / 3.
                } else {
                    9. * 8. + 4. * (9. + 21. + 49.) / 3.
                };
            assert!(
                (model.mass_properties(t).unwrap().volume - (stock - removed)).abs() < 1e-7,
                "{name}"
            );
            let step = cad_export::to_step(&model, t).unwrap();
            assert_eq!(step.matches("CONICAL_SURFACE(").count(), 1, "{name}");
            // The touching cone and pilot share an edge, not an annular face.
            assert_eq!(
                step.matches("PLANE(").count(),
                if name.starts_with("box") { 6 } else { 2 },
                "{name}"
            );
            assert!(cad_export::to_stl(&model, t)
                .unwrap()
                .contains("facet normal"));
        }
    }
}
