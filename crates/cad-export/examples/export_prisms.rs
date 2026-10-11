//! Polygon interoperability fixtures; planar caps remain polygons in STEP.
//!
//! Run with `cargo run -p cad-export --example export_prisms -- /tmp/prisms`.
//! Feed outputs to tools/check_step_ocp.py with independent expectations:
//! triangle: volume120, bounds0,0,0,8,6,5, faces5/edges9/vertices6;
//! trapezoid: volume96, bounds0,0,0,10,4,3, faces6/edges12/vertices8;
//! pentagon: volume66, bounds0,0,0,6,7,2, faces7/edges15/vertices10.
//! Rotated assembly: volume120, bounds20,5,4,28,10,10; one part/occurrence.
use cad_kernel::{extrude_polygon, Tolerance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&directory)?;
    let cases: &[(&str, &[[f64; 2]], f64)] = &[
        ("triangle", &[[0., 0.], [8., 0.], [0., 6.]], 5.),
        ("trapezoid", &[[0., 0.], [10., 0.], [8., 4.], [2., 4.]], 3.),
        (
            "pentagon",
            &[[0., 0.], [6., 0.], [6., 4.], [3., 7.], [0., 4.]],
            2.,
        ),
    ];
    for &(name, points, height) in cases {
        let model = extrude_polygon(points, height, Tolerance::default())?.model;
        std::fs::write(
            directory.join(format!("{name}.step")),
            cad_export::to_step(&model, Tolerance::default())?,
        )?;
        std::fs::write(
            directory.join(format!("{name}.stl")),
            cad_export::to_stl(&model, Tolerance::default())?,
        )?;
    }
    let result = std::sync::Arc::new(extrude_polygon(
        &[[0., 0.], [8., 0.], [0., 6.]],
        5.,
        Tolerance::default(),
    )?);
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let assembly = cad_assembly::BuiltAssembly::new(
        "rotated_prism".into(),
        std::collections::BTreeMap::from([(
            "wedge".into(),
            cad_assembly::BuiltPart {
                feature_id: "extrude".into(),
                content_hash: "triangle".into(),
                metadata: Default::default(),
                result,
            },
        )]),
        vec![cad_assembly::Instance {
            id: "turned".into(),
            part: "wedge".into(),
            transform: cad_assembly::RigidTransform {
                translation: [20., 10., 4.],
                rotation: [q, 0., 0., q],
            },
        }],
        Tolerance::default(),
    )?;
    std::fs::write(
        directory.join("rotated_prism.step"),
        cad_export::assembly_to_step(&assembly, Tolerance::default())?,
    )?;
    std::fs::write(
        directory.join("rotated_prism.stl"),
        cad_export::assembly_to_stl(&assembly, Tolerance::default())?,
    )?;
    Ok(())
}
