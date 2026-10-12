//! Sequential through-cut exchange fixtures, including oblique input operands.
use cad_kernel::{
    boolean, extrude_circle, extrude_path, extrude_rectangle, transform, BooleanOp, PathSegment,
    PlanarPath, Tolerance,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let t = Tolerance::default();
    let path = PlanarPath {
        start: [20., 0.],
        segments: vec![
            PathSegment::LineTo { end: [60., 0.] },
            PathSegment::ArcTo {
                center: [60., 20.],
                end: [60., 40.],
                clockwise: false,
            },
            PathSegment::LineTo { end: [20., 40.] },
            PathSegment::ArcTo {
                center: [20., 20.],
                end: [20., 0.],
                clockwise: false,
            },
        ],
    };
    let stocks = [
        ("rectangle", extrude_rectangle(80., 40., 8., t)?.model),
        ("capsule", extrude_path(&path, &[], 8., t)?.model),
    ];
    for (name, stock) in stocks {
        for oblique in [false, true] {
            let frame=|m:&cad_kernel::ModelSnapshot|->Result<cad_kernel::ModelSnapshot,cad_kernel::KernelError>{if oblique{Ok(transform(m,[12.,-7.,4.],[0.5,0.5,0.,0.5_f64.sqrt()],t)?.model)}else{Ok(m.clone())}};
            let mut model = frame(&stock)?;
            for (x, r) in [(20., 3.), (60., 4.)] {
                let local = transform(
                    &extrude_circle(r, 12., t)?.model,
                    [x, 20., -2.],
                    [0., 0., 0., 1.],
                    t,
                )?
                .model;
                model = boolean(&model, &frame(&local)?, BooleanOp::Difference, t)?.model;
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
