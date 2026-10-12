//! Conical countersink machining fixtures for the independent OCP exchange oracle.
#[path = "../tests/support/countersink.rs"]
mod countersink;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let t = cad_kernel::Tolerance::default();
    for name in countersink::CASES {
        for oblique in [false, true] {
            let model = countersink::fixture(name, oblique)?;
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
