//! Line/arc profiles for independent STEP and signed-area moment checks.
use cad_kernel::{extrude_path, transform, CircleHole, PathSegment, PlanarPath, Tolerance};
fn line(end: [f64; 2]) -> PathSegment {
    PathSegment::LineTo { end }
}
fn arc(center: [f64; 2], end: [f64; 2]) -> PathSegment {
    PathSegment::ArcTo {
        center,
        end,
        clockwise: false,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let t = Tolerance::default();
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let fixtures = [
        (
            "capsule",
            PlanarPath {
                start: [-10., -5.],
                segments: vec![
                    line([10., -5.]),
                    arc([10., 0.], [10., 5.]),
                    line([-10., 5.]),
                    arc([-10., 0.], [-10., -5.]),
                ],
            },
            CircleHole {
                center: [0., 0.],
                radius: 2.,
            },
        ),
        (
            "rounded",
            PlanarPath {
                start: [-16., -15.],
                segments: vec![
                    line([16., -15.]),
                    arc([16., -11.], [20., -11.]),
                    line([20., 11.]),
                    arc([16., 11.], [16., 15.]),
                    line([-16., 15.]),
                    arc([-16., 11.], [-20., 11.]),
                    line([-20., -11.]),
                    arc([-16., -11.], [-16., -15.]),
                ],
            },
            CircleHole {
                center: [7., 3.],
                radius: 2.,
            },
        ),
        (
            "asymmetric",
            PlanarPath {
                start: [-6., -10.],
                segments: vec![
                    line([0., -10.]),
                    arc([0., 0.], [0., 10.]),
                    line([-6., 10.]),
                    line([-6., -10.]),
                ],
            },
            CircleHole {
                center: [-3., 2.],
                radius: 1.5,
            },
        ),
    ];
    for (name, path, hole) in fixtures {
        let model = extrude_path(&path, &[hole], 8., t)?.model;
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
