//! Independent exchange fixtures: transform operands before the curved Boolean.
use cad_kernel::{boolean, extrude_circle, transform, BooleanOp, Tolerance};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let t = Tolerance::default();
    let a = extrude_circle(22., 8., t)?.model;
    let b = transform(
        &extrude_circle(18., 8., t)?.model,
        [30., 0., 0.],
        [0., 0., 0., 1.],
        t,
    )?
    .model;
    for oblique in [false, true] {
        let (a, b) = if oblique {
            let q = [0.5, 0.5, 0., 0.5_f64.sqrt()];
            (
                transform(&a, [12., -7., 4.], q, t)?.model,
                transform(&b, [12., -7., 4.], q, t)?.model,
            )
        } else {
            (a.clone(), b.clone())
        };
        for (name, op) in [
            ("union", BooleanOp::Union),
            ("intersection", BooleanOp::Intersection),
            ("difference", BooleanOp::Difference),
        ] {
            let model = boolean(&a, &b, op, t)?.model;
            let name = format!("{name}{}", if oblique { "_oblique" } else { "" });
            std::fs::write(
                directory.join(format!("{name}.step")),
                cad_export::to_step(&model, t)?,
            )?;
            std::fs::write(
                directory.join(format!("{name}.stl")),
                cad_export::to_stl(&model, t)?,
            )?;
            let p = model.mass_properties(t)?;
            std::fs::write(
                directory.join(format!("{name}.properties.json")),
                format!(
                    "{{\"volume\":{},\"centroid\":{:?},\"inertia\":{:?}}}",
                    p.volume,
                    p.centroid.unwrap(),
                    p.central_inertia.unwrap()
                ),
            )?;
        }
    }
    Ok(())
}
