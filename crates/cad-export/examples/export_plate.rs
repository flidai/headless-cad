//! Small standalone exporter for interoperability checks.
use cad_kernel::{extrude_rectangle, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tolerance = Tolerance::default();
    let model = extrude_rectangle(80.0, 40.0, 6.0, tolerance)?.model;
    let directory = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory)?;
    std::fs::write(
        directory.join("plate.step"),
        cad_export::to_step(&model, tolerance)?,
    )?;
    std::fs::write(
        directory.join("plate.stl"),
        cad_export::to_stl(&model, tolerance)?,
    )?;
    Ok(())
}
