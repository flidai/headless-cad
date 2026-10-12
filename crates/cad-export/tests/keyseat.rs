use cad_kernel::{
    boolean, extrude_circle, extrude_rectangle, transform, translate, BooleanOp, Tolerance,
};

#[test]
fn finite_keyseat_exports_analytic_surfaces_and_preserves_round_journals() {
    let t = Tolerance::default();
    for (r, a, b, height, start, length) in [
        (6., 4., 2., 40., 10., 14.),
        (7., 5., 2., 48., 9., 18.),
        (10., 7., 3., 60., 12., 20.),
    ] {
        let stock = extrude_circle(r, height, t).unwrap().model;
        let tool = translate(
            &extrude_rectangle(r + 2. - a, 2. * b, length, t)
                .unwrap()
                .model,
            [a, -b, start],
            t,
        )
        .unwrap()
        .model;
        for oblique in [false, true] {
            let placed = |model: &cad_kernel::ModelSnapshot| {
                if oblique {
                    transform(model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)
                        .unwrap()
                        .model
                } else {
                    model.clone()
                }
            };
            let cut = boolean(&placed(&stock), &placed(&tool), BooleanOp::Difference, t).unwrap();
            assert!(cut.model.validate(t).valid);
            let removed_area = b * (r * r - b * b).sqrt() + r * r * (b / r).asin() - 2. * a * b;
            let expected = std::f64::consts::PI * r * r * height - removed_area * length;
            assert!((cut.model.mass_properties(t).unwrap().volume - expected).abs() < 1e-7);
            let step = cad_export::to_step(&cut.model, t).unwrap();
            assert!(step.contains("CYLINDRICAL_SURFACE("));
            assert!(step.contains("CIRCLE("));
            assert!(step.contains("PLANE("));
            assert_eq!(step, cad_export::to_step(&cut.model, t).unwrap());
            let stl = cad_export::to_stl(&cut.model, t).unwrap();
            assert!(stl.contains("facet normal"));
            assert!(!stl.contains("NaN"));
        }
    }
}
