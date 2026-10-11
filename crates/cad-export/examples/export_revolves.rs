//! Native full-turn stepped solids for independent STEP/property checks.
use cad_kernel::{revolve_polygon, transform, Tolerance};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let shaft = revolve_polygon(
        &[
            [0., 0.],
            [8., 0.],
            [8., 20.],
            [15., 20.],
            [15., 26.],
            [6., 26.],
            [6., 50.],
            [0., 50.],
        ],
        t,
    )?
    .model;
    let housing = revolve_polygon(
        &[
            [8., 0.],
            [25., 0.],
            [25., 8.],
            [20., 8.],
            [20., 32.],
            [12., 32.],
            [12., 12.],
            [8., 12.],
        ],
        t,
    )?
    .model;
    let blind = revolve_polygon(
        &[
            [0., 0.],
            [20., 0.],
            [20., 30.],
            [8., 30.],
            [8., 10.],
            [0., 10.],
        ],
        t,
    )?
    .model;
    let moved = transform(&housing, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, model) in [
        ("stepped_shaft", shaft),
        ("counterbored_housing", housing),
        ("blind_bore", blind),
        ("oblique_housing", moved),
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
