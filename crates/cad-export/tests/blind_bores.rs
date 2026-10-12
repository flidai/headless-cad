use cad_kernel::{boolean, extrude_circle, extrude_rectangle, transform, BooleanOp, Tolerance};

#[test]
fn top_bottom_and_through_bores_export_distinct_walls_and_blind_floors() {
    let t = Tolerance::default();
    for oblique in [false, true] {
        let frame = |m: &cad_kernel::ModelSnapshot| {
            if oblique {
                transform(m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                    .unwrap()
                    .model
            } else {
                m.clone()
            }
        };
        let mut model = frame(&extrude_rectangle(80., 40., 12., t).unwrap().model);
        for (x, y, r, z, h) in [
            (20., 13., 3., 8., 8.),
            (57., 26., 4., -2., 7.),
            (40., 20., 2., -2., 16.),
        ] {
            let tool = transform(
                &extrude_circle(r, h, t).unwrap().model,
                [x, y, z],
                [0., 0., 0., 1.],
                t,
            )
            .unwrap()
            .model;
            model = boolean(&model, &frame(&tool), BooleanOp::Difference, t)
                .unwrap()
                .model;
        }
        let step = cad_export::to_step(&model, t).unwrap();
        assert_eq!(step.matches("SEAM_CURVE(").count(), 3);
        assert_eq!(step.matches("PCURVE(").count(), 6);
        assert_eq!(step.matches("CYLINDRICAL_SURFACE(").count(), 3);
        assert_eq!(step.matches("PLANE(").count(), 8);
        assert_eq!(
            step.lines()
                .filter(|l| l.contains("ADVANCED_FACE(") && l.ends_with(",.F.);"))
                .count(),
            3
        );
        let expected = 38400. - std::f64::consts::PI * (9. * 4. + 16. * 5. + 4. * 12.);
        assert!((model.mass_properties(t).unwrap().volume - expected).abs() < 1e-6);
        assert!(cad_export::to_stl(&model, t)
            .unwrap()
            .contains("facet normal"));
    }
}
