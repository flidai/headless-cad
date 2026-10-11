//! Analytic annular STEP interoperability fixtures, including an oblique frame.
use cad_kernel::{extrude_annulus, transform, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let tube = extrude_annulus(12., 8., 40., t)?.model;
    let moved = transform(&tube, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, model) in [("tube", tube), ("oblique_tube", moved)] {
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
                "{{\"volume\":{},\"volume_centroid\":{:?},\"central_volume_inertia\":{:?}}}",
                p.volume,
                p.centroid.unwrap(),
                p.central_inertia.unwrap()
            ),
        )?;
    }
    Ok(())
}
