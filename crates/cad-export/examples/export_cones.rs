//! Signed conical surfaces and inward countersinks for independent exchange checks.
use cad_kernel::{revolve_polygon, transform, Tolerance};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let tapered = revolve_polygon(&[[2., 0.], [8., 0.], [4., 12.], [3., 12.]], t)?.model;
    let flared = revolve_polygon(&[[1., 0.], [4., 0.], [8., 12.], [1., 12.]], t)?.model;
    let moved = transform(&tapered, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, model) in [
        ("tapered_sleeve", tapered),
        ("flared_sleeve", flared),
        ("oblique_tapered_sleeve", moved),
        (
            "upper_apex",
            revolve_polygon(&[[0., 0.], [8., 0.], [0., 12.]], t)?.model,
        ),
        (
            "upper_apex_oblique",
            transform(
                &revolve_polygon(&[[0., 0.], [8., 0.], [0., 12.]], t)?.model,
                [12., -7., 4.],
                [0.5, 0.5, 0., 0.5_f64.sqrt()],
                t,
            )?
            .model,
        ),
        (
            "lower_apex",
            revolve_polygon(&[[0., 0.], [8., 12.], [0., 12.]], t)?.model,
        ),
        (
            "double_apex",
            revolve_polygon(&[[0., 0.], [8., 6.], [0., 12.]], t)?.model,
        ),
        (
            "blind_apex",
            revolve_polygon(&[[0., 0.], [10., 0.], [10., 20.], [6., 20.], [0., 8.]], t)?.model,
        ),
    ] {
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
