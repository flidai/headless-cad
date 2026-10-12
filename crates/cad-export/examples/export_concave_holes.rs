//! Concave inner boundaries with independent box/cylinder subtraction oracles.
use cad_kernel::{extrude_polygon_with_circular_holes, transform, CircleHole, Tolerance};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let outer = [[0., 0.], [40., 0.], [40., 30.], [0., 30.]];
    let hole = vec![
        [5., 5.],
        [5., 23.],
        [9., 23.],
        [9., 9.],
        [25., 9.],
        [25., 23.],
        [29., 23.],
        [29., 5.],
    ];
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    for (name, holes, circles) in [
        (
            "circle_bay",
            vec![hole.clone()],
            vec![CircleHole {
                center: [17., 17.],
                radius: 3.,
            }],
        ),
        (
            "rectangle_bay",
            vec![hole, vec![[13., 13.], [13., 21.], [21., 21.], [21., 13.]]],
            vec![],
        ),
    ] {
        let model = extrude_polygon_with_circular_holes(&outer, &holes, &circles, 6., t)?.model;
        for (suffix, model) in [
            ("", model.clone()),
            (
                "_oblique",
                transform(&model, [12., -7., 4.], [0.5, 0.5, 0., 0.5_f64.sqrt()], t)?.model,
            ),
        ] {
            let name = format!("{name}{suffix}");
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
    }
    Ok(())
}
