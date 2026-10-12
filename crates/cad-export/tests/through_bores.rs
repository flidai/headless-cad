use cad_kernel::{boolean, extrude_circle, extrude_rectangle, transform, BooleanOp, Tolerance};
#[test]
fn sequential_bores_export_distinct_inward_periodic_walls_after_oblique_input_transform() {
    let t = Tolerance::default();
    let q = [0.5, 0.5, 0., 0.5_f64.sqrt()];
    let mut stock = transform(
        &extrude_rectangle(80., 40., 8., t).unwrap().model,
        [12., -7., 4.],
        q,
        t,
    )
    .unwrap()
    .model;
    for (x, r) in [(20., 3.), (60., 4.)] {
        let tool = transform(
            &extrude_circle(r, 12., t).unwrap().model,
            [x, 20., -2.],
            [0., 0., 0., 1.],
            t,
        )
        .unwrap()
        .model;
        let tool = transform(&tool, [12., -7., 4.], q, t).unwrap().model;
        stock = boolean(&stock, &tool, BooleanOp::Difference, t)
            .unwrap()
            .model;
    }
    let step = cad_export::to_step(&stock, t).unwrap();
    assert_eq!(step.matches("SEAM_CURVE(").count(), 2);
    assert_eq!(step.matches("PCURVE(").count(), 4);
    assert_eq!(
        step.lines()
            .filter(|l| l.contains("ADVANCED_FACE(") && l.ends_with(",.F.);"))
            .count(),
        2
    );
    assert!((stock.volume() - (3200. - 25. * std::f64::consts::PI) * 8.).abs() < 1e-5);
    assert!(cad_export::to_stl(&stock, t)
        .unwrap()
        .contains("facet normal"));
}
