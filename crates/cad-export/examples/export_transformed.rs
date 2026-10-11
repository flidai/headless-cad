//! Exact transformed convex-solid interoperability fixture. Expected volume 120,
//! bounds [0,15,-18]..[6,20,-10], five planar faces, nine edges, six vertices.
use cad_kernel::{extrude_polygon, transform, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tolerance = Tolerance::default();
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let prism = extrude_polygon(&[[0., 0.], [8., 0.], [0., 6.]], 5., tolerance)?;
    let first = transform(&prism.model, [10., 0., 0.], [q, 0., 0., q], tolerance)?;
    let result = transform(&first.model, [0., 20., 0.], [0., q, 0., q], tolerance)?;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join("transformed.step"),
        cad_export::to_step(&result.model, tolerance)?,
    )?;
    std::fs::write(
        directory.join("transformed.stl"),
        cad_export::to_stl(&result.model, tolerance)?,
    )?;
    Ok(())
}
