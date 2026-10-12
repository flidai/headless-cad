use cad_kernel::{
    boolean, extrude_circle, extrude_rectangle, transform, BooleanOp, ModelSnapshot, Tolerance,
};

pub const CASES: [&str; 5] = ["top", "top_reverse", "bottom", "bottom_reverse", "opposed"];

pub fn fixture(name: &str, oblique: bool) -> Result<ModelSnapshot, cad_kernel::KernelError> {
    let t = Tolerance::default();
    let frame = |model: &ModelSnapshot| {
        if oblique {
            Ok(transform(model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model)
        } else {
            Ok(model.clone())
        }
    };
    let through = (3., -2., 16.);
    let top = (7., 8., 8.);
    let bottom = (6., -2., 6.);
    let tools = match name {
        "top" => vec![through, top],
        "top_reverse" => vec![top, through],
        "bottom" => vec![through, bottom],
        "bottom_reverse" => vec![bottom, through],
        "opposed" => vec![through, top, bottom],
        _ => panic!("unknown fixture"),
    };
    let mut model = frame(&extrude_rectangle(80., 40., 12., t)?.model)?;
    for (r, z, h) in tools {
        let tool = transform(
            &extrude_circle(r, h, t)?.model,
            [23., 17., z],
            [0., 0., 0., 1.],
            t,
        )?
        .model;
        model = boolean(&model, &frame(&tool)?, BooleanOp::Difference, t)?.model;
    }
    Ok(model)
}
