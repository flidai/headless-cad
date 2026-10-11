//! Operands/result for the independent OCP common-operation oracle.
//! Pass an output directory and optional `skew` for two independently oriented cubes.
use cad_kernel::{boolean, extrude_rectangle, transform, BooleanOp, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tolerance = Tolerance::default();
    let (left, right) = if std::env::args().nth(2).as_deref() == Some("skew") {
        let cube = extrude_rectangle(4., 4., 4., tolerance)?;
        let centered = cad_kernel::translate(&cube.model, [-2., -2., -2.], tolerance)?;
        let rotation = |axis: [f64; 3], angle: f64| {
            let length = axis[0].hypot(axis[1]).hypot(axis[2]);
            let factor = (angle / 2.).sin() / length;
            [
                axis[0] * factor,
                axis[1] * factor,
                axis[2] * factor,
                (angle / 2.).cos(),
            ]
        };
        (
            transform(
                &centered.model,
                [0.; 3],
                rotation([1., 2., 3.], 0.62),
                tolerance,
            )?,
            transform(
                &centered.model,
                [0.7, 0.4, 0.2],
                rotation([2., -1., 1.], 0.8),
                tolerance,
            )?,
        )
    } else {
        let left = extrude_rectangle(4., 4., 4., tolerance)?;
        let right = extrude_rectangle(4., 8., 8., tolerance)?;
        let right = transform(
            &right.model,
            [2., -4., -2.],
            [0., 0., 0.3826834323650898, 0.9238795325112867],
            tolerance,
        )?;
        (left, right)
    };
    let result = boolean(
        &left.model,
        &right.model,
        BooleanOp::Intersection,
        tolerance,
    )?;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, model) in [
        ("left", left.model),
        ("right", right.model),
        ("common", result.model),
    ] {
        std::fs::write(
            directory.join(format!("{name}.step")),
            cad_export::to_step(&model, tolerance)?,
        )?;
        std::fs::write(
            directory.join(format!("{name}.stl")),
            cad_export::to_stl(&model, tolerance)?,
        )?;
    }
    Ok(())
}
