use cad_kernel::{
    boolean, extrude_annulus, extrude_rectangle, transform, translate, BooleanOp, Tolerance,
};

#[test]
fn hub_keyway_exchange_preserves_analytic_bore_and_outer_wall() {
    let t = Tolerance::default();
    for (outer, bore, roof, half_width, height) in [
        (24., 6.15, 8.05, 2.02, 14.),
        (24., 7.15, 9.05, 2.02, 14.),
        (12., 3.075, 4.025, 1.01, 7.),
    ] {
        let stock = extrude_annulus(outer, bore, height, t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(roof, 2. * half_width, height + 2., t)
                .unwrap()
                .model,
            [0., -half_width, -1.],
            t,
        )
        .unwrap()
        .model;
        for oblique in [false, true] {
            let place = |model: &cad_kernel::ModelSnapshot| {
                if oblique {
                    transform(model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                        .unwrap()
                        .model
                } else {
                    model.clone()
                }
            };
            let cut = boolean(&place(&stock), &place(&tool), BooleanOp::Difference, t)
                .unwrap()
                .model;
            assert!(cut.validate(t).valid);
            let removed_area = 2. * roof * half_width
                - half_width * (bore * bore - half_width * half_width).sqrt()
                - bore * bore * (half_width / bore).asin();
            let expected =
                (std::f64::consts::PI * (outer * outer - bore * bore) - removed_area) * height;
            assert!((cut.mass_properties(t).unwrap().volume - expected).abs() < 1e-7);
            let step = cad_export::to_step(&cut, t).unwrap();
            assert!(step.contains("CYLINDRICAL_SURFACE("));
            assert!(step.contains("CIRCLE("));
            assert!(step.contains("PLANE("));
            assert_eq!(step, cad_export::to_step(&cut, t).unwrap());
            let stl = cad_export::to_stl(&cut, t).unwrap();
            assert!(stl.contains("facet normal"));
            assert!(!stl.contains("NaN"));
        }
    }
}
