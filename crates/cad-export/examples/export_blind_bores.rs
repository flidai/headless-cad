//! Blind and mixed-depth bore exchange fixtures with independently checkable dimensions.
use cad_kernel::{
    boolean, extrude_circle, extrude_path, extrude_rectangle, transform, BooleanOp, PathSegment,
    PlanarPath, Tolerance,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let t = Tolerance::default();
    for (name, tools) in [
        ("arc", vec![(0., 0., 3., 8., 8.)]),
        ("top", vec![(20., 13., 3., 8., 8.)]),
        ("bottom", vec![(57., 26., 4., -2., 7.)]),
        (
            "mixed",
            vec![
                (20., 13., 3., 8., 8.),
                (57., 26., 4., -2., 7.),
                (40., 20., 2., -2., 16.),
            ],
        ),
    ] {
        for oblique in [false, true] {
            let frame = |m: &cad_kernel::ModelSnapshot| -> Result<cad_kernel::ModelSnapshot, cad_kernel::KernelError> {
                if oblique { Ok(transform(m, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model) }
                else { Ok(m.clone()) }
            };
            let stock = if name == "arc" {
                extrude_path(
                    &PlanarPath {
                        start: [20., 0.],
                        segments: vec![
                            PathSegment::ArcTo {
                                center: [0., 0.],
                                end: [-20., 0.],
                                clockwise: false,
                            },
                            PathSegment::ArcTo {
                                center: [0., 0.],
                                end: [20., 0.],
                                clockwise: false,
                            },
                        ],
                    },
                    &[],
                    12.,
                    t,
                )?
                .model
            } else {
                extrude_rectangle(80., 40., 12., t)?.model
            };
            let mut model = frame(&stock)?;
            for &(x, y, r, z, h) in &tools {
                let tool = transform(
                    &extrude_circle(r, h, t)?.model,
                    [x, y, z],
                    [0., 0., 0., 1.],
                    t,
                )?
                .model;
                model = boolean(&model, &frame(&tool)?, BooleanOp::Difference, t)?.model;
            }
            let stem = format!("{name}{}", if oblique { "_oblique" } else { "" });
            std::fs::write(
                directory.join(format!("{stem}.step")),
                cad_export::to_step(&model, t)?,
            )?;
            std::fs::write(
                directory.join(format!("{stem}.stl")),
                cad_export::to_stl(&model, t)?,
            )?;
            let p = model.mass_properties(t)?;
            std::fs::write(
                directory.join(format!("{stem}.properties.json")),
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
