use cad_kernel::{
    boolean, extrude_annulus, extrude_circle, transform, BooleanOp, ModelSnapshot, Tolerance,
};

pub const CASES: [&str; 6] = [
    "disk_through",
    "disk_blind",
    "disk_stepped",
    "annulus_through",
    "annulus_blind",
    "annulus_stepped",
];

pub fn fixture(name: &str, oblique: bool) -> Result<ModelSnapshot, cad_kernel::KernelError> {
    let t = Tolerance::default();
    let frame = |model: &ModelSnapshot| {
        if oblique {
            Ok(transform(model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model)
        } else {
            Ok(model.clone())
        }
    };
    let stock = if name.starts_with("annulus") {
        extrude_annulus(30., 8., 16., t)?
    } else {
        extrude_circle(30., 16., t)?
    };
    let mut model = frame(&stock.model)?;
    let cuts = if name.ends_with("through") {
        vec![(18., 3., -2., 20.)]
    } else if name.ends_with("blind") {
        vec![(-18., 2., 11., 7.)]
    } else {
        vec![(18., 3., -2., 20.), (18., 5., 12., 6.), (-18., 2., 11., 7.)]
    };
    for (x, r, z, h) in cuts {
        let tool = transform(
            &extrude_circle(r, h, t)?.model,
            [x, 0., z],
            [0., 0., 0., 1.],
            t,
        )?
        .model;
        model = boolean(&model, &frame(&tool)?, BooleanOp::Difference, t)?.model;
    }
    Ok(model)
}
