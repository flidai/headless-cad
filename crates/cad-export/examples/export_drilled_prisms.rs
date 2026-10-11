//! Mixed analytic STEP interoperability fixtures, including an oblique frame.
use cad_kernel::{extrude_polygon_with_circular_holes, transform, CircleHole, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let tube = extrude_polygon_with_circular_holes(
        &[[0., 0.], [80., 0.], [80., 50.], [0., 50.]],
        &[],
        &[
            CircleHole {
                center: [20., 25.],
                radius: 5.,
            },
            CircleHole {
                center: [60., 25.],
                radius: 8.,
            },
        ],
        12.,
        t,
    )?
    .model;
    let moved = transform(&tube, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model;
    let mixed = extrude_polygon_with_circular_holes(
        &[
            [0., 0.],
            [80., 0.],
            [80., 60.],
            [55., 60.],
            [55., 25.],
            [25., 25.],
            [25., 60.],
            [0., 60.],
        ],
        &[vec![[5., 5.], [5., 15.], [15., 15.], [15., 5.]]],
        &[CircleHole {
            center: [65., 40.],
            radius: 5.,
        }],
        8.,
        t,
    )?
    .model;
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, model) in [
        ("drilled_prism", tube),
        ("oblique_drilled_prism", moved),
        ("mixed_concave_prism", mixed),
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
