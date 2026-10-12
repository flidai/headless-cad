use cad_kernel::{
    boolean, extrude_annulus, extrude_circle, extrude_rectangle, revolve_polygon, transform,
    BooleanOp, ModelSnapshot, Tolerance,
};
pub const CASES: [&str; 8] = [
    "box_top",
    "box_bottom",
    "box_crossing_top",
    "box_crossing_bottom",
    "disk_top",
    "disk_bottom",
    "annulus_top",
    "annulus_bottom",
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
    let stock = if name.starts_with("box") {
        extrude_rectangle(60., 40., 12., t)?
    } else if name.starts_with("annulus") {
        extrude_annulus(30., 8., 12., t)?
    } else {
        extrude_circle(30., 12., t)?
    };
    let xy = if name.starts_with("box") {
        [23., 17.]
    } else {
        [18., 0.]
    };
    let pilot = transform(
        &extrude_circle(3., 16., t)?.model,
        [xy[0], xy[1], -2.],
        [0., 0., 0., 1.],
        t,
    )?
    .model;
    let profile = if name == "box_crossing_top" {
        vec![[0., 4.], [1., 4.], [9., 14.], [0., 14.]]
    } else if name == "box_crossing_bottom" {
        vec![[0., -2.], [9., -2.], [1., 8.], [0., 8.]]
    } else if name.ends_with("top") {
        vec![[0., 8.], [3., 8.], [9., 14.], [0., 14.]]
    } else {
        vec![[0., -2.], [9., -2.], [3., 4.], [0., 4.]]
    };
    let sink = transform(
        &revolve_polygon(&profile, t)?.model,
        [xy[0], xy[1], 0.],
        [0., 0., 0., 1.],
        t,
    )?
    .model;
    let drilled = boolean(
        &frame(&stock.model)?,
        &frame(&pilot)?,
        BooleanOp::Difference,
        t,
    )?
    .model;
    Ok(boolean(&drilled, &frame(&sink)?, BooleanOp::Difference, t)?.model)
}
